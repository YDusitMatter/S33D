//! `GeometryKernel` 的 truck 实现。
//!
//! - B-rep 建模（拉伸/旋转）与网格化由 truck 承担。
//! - 原有网格布尔保留；涉及精确曲面的布尔、变换与分割交给 OCCT。
//! - STEP 导入保留原始 B-Rep，精确对象通过 OCCT 导出。
//! - 旧网格先缝合为合法的平面 B-Rep 后导出，不宣称恢复原设计曲面。

use crate::mesh::MeshData;
use crate::mesh_bool::{
    boolean, mirror as mesh_mirror, revolve_profile, split_mesh_components, BoolOp,
};
use crate::region::{signed_area, Axis2D, PlaneDef, Point2, Region2D};
use crate::{GeometryKernel, KernelError, Result, ShapeId};
use glam::{DVec3, Mat4};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use truck_meshalgo::prelude::*;
use truck_modeling::builder;
use truck_modeling::topology::{Solid, Wire};
use truck_modeling::*;

/// 内核内部形状表示。
enum KernelShape {
    Exact(crate::occt::ExactShape),
    /// 参数化 B-rep 实体（拉伸/旋转产物）。
    Brep(Solid),
    /// 网格体（布尔/镜像/STEP 导入产物）。
    Mesh(MeshData),
}

/// 倒角/圆角棱链的一段：世界坐标端点与两相邻面的外法线。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeCutterSegment {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub n1: [f64; 3],
    pub n2: [f64; 3],
}

/// truck 实现的几何内核。
#[derive(Default, Clone)]
pub struct TruckKernel {
    // Shapes are immutable: every operation allocates a new ID. Snapshots
    // share geometry without copying large meshes on the UI thread.
    shapes: HashMap<ShapeId, Arc<KernelShape>>,
    next_id: u64,
}

/// Remove numerical debris produced by dense/intersecting Sketch arrangements
/// before asking truck to attach planar wires. Consecutive duplicates and
/// straight-through collinear vertices do not change the profile but can make
/// `try_attach_plane` reject an otherwise closed complex region.
fn clean_region_ring(ring: &[[f64; 2]]) -> Result<Vec<[f64; 2]>> {
    if ring.iter().flatten().any(|value| !value.is_finite()) {
        return Err(KernelError::Geometry(
            "region ring contains a non-finite point".into(),
        ));
    }
    if ring.len() < 3 {
        return Err(KernelError::Geometry(
            "a region ring needs at least 3 points".into(),
        ));
    }
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for point in ring {
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let scale = (max[0] - min[0]).hypot(max[1] - min[1]).max(1.0);
    let tolerance = (scale * 1e-9).max(1e-10);
    let mut clean = Vec::with_capacity(ring.len());
    for &point in ring {
        if clean.last().is_none_or(|previous: &[f64; 2]| {
            (point[0] - previous[0]).hypot(point[1] - previous[1]) > tolerance
        }) {
            clean.push(point);
        }
    }
    if clean.len() > 1
        && (clean[0][0] - clean[clean.len() - 1][0]).hypot(clean[0][1] - clean[clean.len() - 1][1])
            <= tolerance
    {
        clean.pop();
    }

    let mut changed = true;
    while changed && clean.len() > 3 {
        changed = false;
        for index in 0..clean.len() {
            let previous = clean[(index + clean.len() - 1) % clean.len()];
            let point = clean[index];
            let next = clean[(index + 1) % clean.len()];
            let a = [point[0] - previous[0], point[1] - previous[1]];
            let b = [next[0] - point[0], next[1] - point[1]];
            let chord = [next[0] - previous[0], next[1] - previous[1]];
            let chord_len = chord[0].hypot(chord[1]);
            let distance = if chord_len > tolerance {
                (chord[0] * (point[1] - previous[1]) - chord[1] * (point[0] - previous[0])).abs()
                    / chord_len
            } else {
                0.0
            };
            let straight_through = a[0] * b[0] + a[1] * b[1] >= -tolerance * tolerance;
            if distance <= tolerance && straight_through {
                clean.remove(index);
                changed = true;
                break;
            }
        }
    }
    if clean.len() < 3 || signed_area(&clean).abs() <= tolerance * tolerance {
        return Err(KernelError::Geometry(
            "region ring is degenerate after cleanup".into(),
        ));
    }
    Ok(clean)
}

impl TruckKernel {
    /// Build the selected portions of a straight-walled hole's planar rim.
    /// Two selected neighbors share the cross-section on the angle bisector.
    pub fn mitered_hole_chamfer_cutter(
        &self,
        rim: &[EdgeCutterSegment],
        selected: &[bool],
        distance: f64,
    ) -> Result<MeshData> {
        let invalid = || KernelError::Geometry("cannot form a continuous hole-rim chamfer".into());
        if rim.len() < 3 || selected.len() != rim.len() || !distance.is_finite() || distance <= 0.0
        {
            return Err(invalid());
        }
        let first = &rim[0];
        let pair = |edge: &EdgeCutterSegment| {
            [
                DVec3::from_array(edge.n1).normalize(),
                DVec3::from_array(edge.n2).normalize(),
            ]
        };
        let [first_normal, second_normal] = pair(first);
        let normal = [first_normal, second_normal]
            .into_iter()
            .find(|normal| {
                rim.iter()
                    .all(|edge| pair(edge).into_iter().any(|n| n.dot(*normal) > 0.9999))
            })
            .ok_or_else(invalid)?;
        let side_normals = rim
            .iter()
            .map(|edge| {
                let [a, b] = pair(edge);
                let side = if a.dot(normal) > b.dot(normal) { b } else { a };
                (side.dot(normal).abs() < 1e-3)
                    .then(|| (side - normal * side.dot(normal)).normalize())
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(invalid)?;
        let n = rim.len();
        let points: Vec<_> = rim.iter().map(|edge| DVec3::from_array(edge.a)).collect();
        if rim.iter().enumerate().any(|(i, edge)| {
            let end = DVec3::from_array(edge.b);
            !points[i].is_finite()
                || !end.is_finite()
                || end.distance_squared(points[(i + 1) % n]) > 1e-10
                || end.distance_squared(points[i]) < 1e-16
        }) {
            return Err(invalid());
        }
        let mut offsets = vec![DVec3::ZERO; n];
        for i in 0..n {
            let previous = (i + n - 1) % n;
            offsets[i] = match (selected[previous], selected[i]) {
                (true, true) => {
                    // The intersection of the two unit-offset lines.
                    let bisector = side_normals[previous] + side_normals[i];
                    let length_squared = bisector.length_squared();
                    if length_squared < 1e-12 {
                        return Err(invalid());
                    }
                    -2.0 * bisector / length_squared
                }
                (true, false) => -side_normals[previous],
                (false, true) => -side_normals[i],
                (false, false) => DVec3::ZERO,
            };
        }
        let too_wide = || {
            KernelError::Geometry("chamfer distance makes neighboring hole-rim cuts cross".into())
        };
        let projected: Vec<_> = points
            .iter()
            .zip(&offsets)
            .map(|(&p, &m)| p + m * distance)
            .collect();
        let mut pad = (distance * 0.1).clamp(0.01, 0.25);
        for i in (0..n).filter(|&i| selected[i]) {
            let j = (i + 1) % n;
            let edge = points[j] - points[i];
            let length = edge.length_squared();
            if (projected[j] - projected[i]).dot(edge) <= 1e-9 * length {
                return Err(too_wide());
            }
            let extension = (offsets[j] - offsets[i]).dot(edge);
            if extension > 0.0 {
                pad = pad.min(length / (4.0 * extension));
            }
            for k in (i + 1..n).filter(|&k| selected[k]) {
                let l = (k + 1) % n;
                if j == k || l == i {
                    continue;
                }
                let side = |a: usize, b: usize, c: usize| {
                    (projected[b] - projected[a])
                        .cross(projected[c] - projected[a])
                        .dot(normal)
                };
                if side(i, j, k) * side(i, j, l) < 0.0 && side(k, l, i) * side(k, l, j) < 0.0 {
                    return Err(too_wide());
                }
            }
        }
        let mut positions = Vec::new();
        let mut sections = vec![0u32; n];
        for i in 0..n {
            if !selected[(i + n - 1) % n] && !selected[i] {
                continue;
            }
            sections[i] = positions.len() as u32;
            let point = points[i];
            positions.extend(
                [
                    point + (normal - offsets[i]) * pad,
                    projected[i],
                    point - normal * distance,
                ]
                .map(|point| point.as_vec3().to_array()),
            );
        }
        let mut indices = Vec::new();
        for i in 0..n {
            if !selected[i] {
                continue;
            }
            let next = (i + 1) % n;
            let (a, b) = (sections[i], sections[next]);
            for j in 0..3 {
                let k = (j + 1) % 3;
                indices.extend_from_slice(&[a + j, b + j, b + k, a + j, b + k, a + k]);
            }
            if !selected[(i + n - 1) % n] {
                indices.extend_from_slice(&[a, a + 1, a + 2]);
            }
            if !selected[next] {
                indices.extend_from_slice(&[b, b + 2, b + 1]);
            }
        }
        let origin = DVec3::from_array(first.a);
        let signed_volume = indices
            .chunks_exact(3)
            .map(|triangle| {
                let [a, b, c] = [0, 1, 2].map(|i| {
                    DVec3::from_array(positions[triangle[i] as usize].map(f64::from)) - origin
                });
                a.dot(b.cross(c)) / 6.0
            })
            .sum::<f64>();
        if !signed_volume.is_finite() || signed_volume.abs() < 1e-12 {
            return Err(invalid());
        }
        if signed_volume < 0.0 {
            for triangle in indices.chunks_exact_mut(3) {
                triangle.swap(1, 2);
            }
        }
        Ok(MeshData::new(positions, indices))
    }

    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> ShapeId {
        let id = ShapeId(self.next_id);
        self.next_id += 1;
        id
    }

    fn insert(&mut self, shape: KernelShape) -> ShapeId {
        let id = self.alloc_id();
        self.shapes.insert(id, Arc::new(shape));
        id
    }

    fn get(&self, id: ShapeId) -> Result<&KernelShape> {
        self.shapes
            .get(&id)
            .map(Arc::as_ref)
            .ok_or(KernelError::UnknownShape(id))
    }

    fn get_mesh(&self, id: ShapeId) -> Result<MeshData> {
        match self.get(id)? {
            KernelShape::Exact(shape) => Ok(shape.mesh.clone()),
            KernelShape::Brep(solid) => Ok(tessellate_solid(solid)),
            KernelShape::Mesh(m) => Ok(m.clone()),
        }
    }

    /// 从 truck Solid 构建（供内部/测试用）。
    pub fn insert_solid(&mut self, solid: Solid) -> ShapeId {
        self.insert(KernelShape::Brep(solid))
    }

    /// 从网格构建（供内部/测试用）。
    pub fn insert_mesh(&mut self, mesh: MeshData) -> ShapeId {
        self.insert(KernelShape::Mesh(mesh))
    }

    pub fn exact_shape(&self, id: ShapeId) -> Option<&crate::occt::ExactShape> {
        match self.shapes.get(&id).map(Arc::as_ref) {
            Some(KernelShape::Exact(shape)) => Some(shape),
            _ => None,
        }
    }

    pub fn restore_exact(&mut self, brep: &str) -> Result<ShapeId> {
        let shape = crate::occt::ExactShape::from_brep(brep)?;
        Ok(self.insert(KernelShape::Exact(shape)))
    }

    pub fn insert_exact(&mut self, shape: crate::occt::ExactShape) -> ShapeId {
        self.insert(KernelShape::Exact(shape))
    }

    pub fn offset_exact(
        &mut self,
        id: ShapeId,
        distance: f64,
        tolerance: f64,
        faces: &[u32],
        mode: crate::OffsetMode,
    ) -> Result<ShapeId> {
        let shape=self.exact_shape(id).ok_or_else(|| KernelError::Geometry("Exact offset needs original B-Rep surfaces; re-import the source STEP. Mesh-only objects cannot recover those surfaces.".into()))?;
        let output = shape.offset(distance, tolerance, faces, mode)?;
        Ok(self.insert(KernelShape::Exact(output)))
    }

    fn exact_boolean(&mut self, a: ShapeId, b: ShapeId, mode: i32) -> Result<Option<ShapeId>> {
        match (self.exact_shape(a), self.exact_shape(b)) {
            (Some(left), Some(right)) => {
                let output = left.boolean(right, mode)?;
                Ok(Some(self.insert(KernelShape::Exact(output))))
            }
            (None, None) => Ok(None),
            _ => {
                // Faceted tools stay faceted, while the exact operand's curved
                // surfaces remain native; never tessellate the exact operand.
                let left = self.exact_operand(a)?;
                let right = self.exact_operand(b)?;
                let output = left.boolean(&right, mode)?;
                Ok(Some(self.insert(KernelShape::Exact(output))))
            }
        }
    }

    fn exact_operand(&self, id: ShapeId) -> Result<crate::occt::ExactShape> {
        if let Some(shape) = self.exact_shape(id) {
            return Ok(shape.clone());
        }
        let mesh = self.get_mesh(id)?;
        crate::occt::ExactShape::from_faceted_mesh(&mesh)
    }

    /// Profile parts (especially thin text strokes) already produce a checked
    /// manifold mesh. Do not weld/round-trip it through faceted B-rep after
    /// every union: that can collapse short edges and open a valid shell.
    pub fn union_profile_parts(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId> {
        let out = boolean(&self.get_mesh(a)?, &self.get_mesh(b)?, BoolOp::Union)?;
        Ok(self.insert(KernelShape::Mesh(out)))
    }

    /// 用多边形环（世界坐标 3D 点列，逆时针）构造平面 face。
    fn ring_to_wire(&self, ring: &[[f64; 3]]) -> Result<Wire> {
        if ring.len() < 3 {
            return Err(KernelError::Geometry(
                "a ring needs at least 3 points".into(),
            ));
        }
        let pts: Vec<Point3> = ring.iter().map(|p| Point3::new(p[0], p[1], p[2])).collect();
        let verts: Vec<_> = pts.iter().map(|p| builder::vertex(*p)).collect();
        let mut wire = Wire::new();
        for i in 0..pts.len() {
            let v0 = &verts[i];
            let v1 = &verts[(i + 1) % pts.len()];
            wire.push_back(builder::line(v0, v1));
        }
        Ok(wire)
    }

    /// 区域（局部坐标，outer CCW / holes CW）→ 世界空间 wire 列表。
    fn region_wires(&self, region: &Region2D, plane: &PlaneDef) -> Result<Vec<Wire>> {
        let mut wires = Vec::new();
        // outer：确保从法线方向看为逆时针
        let outer_ring = clean_region_ring(&region.outer)?;
        let outer = plane.with_orientation(&outer_ring, true);
        let outer3: Vec<[f64; 3]> = outer.iter().map(|p| plane.to_world(p[0], p[1])).collect();
        wires.push(self.ring_to_wire(&outer3)?);
        // holes：顺时针
        for hole in &region.holes {
            let hole_ring = clean_region_ring(hole)?;
            let h = plane.with_orientation(&hole_ring, false);
            let h3: Vec<[f64; 3]> = h.iter().map(|p| plane.to_world(p[0], p[1])).collect();
            wires.push(self.ring_to_wire(&h3)?);
        }
        Ok(wires)
    }

    /// 区域 → 平面 face（带孔）。失败时返回该区域的错误。
    fn region_to_face(&self, region: &Region2D, plane: &PlaneDef) -> Result<Face> {
        if region.outer.len() < 3 {
            return Err(KernelError::Geometry("region has no outer loop".into()));
        }
        let wires = self.region_wires(region, plane)?;
        builder::try_attach_plane(&wires)
            .map_err(|e| KernelError::Geometry(format!("attach plane: {e}")))
    }
}

/// Promote a closed manifold triangle mesh to a faceted truck B-rep. Vertices
/// and edges are shared by topology identity, so `Solid::try_new` can verify
/// that every connected component is a closed manifold shell.
fn mesh_to_faceted_brep(mesh: &MeshData) -> Result<Solid> {
    let mut mesh = mesh.clone();
    mesh.weld(1e-5);
    if mesh.positions.is_empty() || mesh.indices.len() < 12 {
        return Err(KernelError::Geometry(
            "cannot create a B-rep from an empty mesh".into(),
        ));
    }

    let points: Vec<Point3> = mesh
        .positions
        .iter()
        .map(|point| Point3::new(point[0] as f64, point[1] as f64, point[2] as f64))
        .collect();
    let vertices: Vec<_> = points.iter().map(|point| builder::vertex(*point)).collect();
    let mut edges: HashMap<(u32, u32), Edge> = HashMap::new();
    let mut faces = Vec::with_capacity(mesh.indices.len() / 3);

    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
        if a == b || b == c || a == c {
            continue;
        }
        let pa = points[a as usize];
        let pb = points[b as usize];
        let pc = points[c as usize];
        let normal = (pb - pa).cross(pc - pa);
        if normal.magnitude2() <= 1e-20 {
            continue;
        }

        let mut directed_edge = |from: u32, to: u32| {
            let key = if from < to { (from, to) } else { (to, from) };
            let edge = edges
                .entry(key)
                .or_insert_with(|| {
                    builder::line(&vertices[key.0 as usize], &vertices[key.1 as usize])
                })
                .clone();
            if from < to {
                edge
            } else {
                edge.inverse()
            }
        };
        let wire = Wire::from(vec![
            directed_edge(a, b),
            directed_edge(b, c),
            directed_edge(c, a),
        ]);
        faces.push(Face::new(
            vec![wire],
            Surface::Plane(Plane::new(pa, pb, pc)),
        ));
    }

    if faces.is_empty() {
        return Err(KernelError::Geometry(
            "mesh contains no non-degenerate faces".into(),
        ));
    }
    let shell: Shell = faces.into_iter().collect();
    let boundaries = shell.connected_components();
    Solid::try_new(boundaries)
        .map_err(|error| KernelError::Geometry(format!("faceted B-rep validation: {error}")))
}

/// 拉伸方向由区域环方向（CCW 从法线看）+ 深度共同决定。
/// 返回沿 `plane.normal * depth` 方向的 B-rep Solid。
/// 注意：truck 的 tsweep 在负方向挤出时会产生内外翻转的体（面法线全反），
/// 导致布尔运算结果错乱——负深度改为正方向挤出后平移到负侧。
fn extrude_face(face: &Face, plane: &PlaneDef, depth: f64) -> Solid {
    let n = plane.normal();
    if depth >= 0.0 {
        let dir = n * depth;
        builder::tsweep(face, Vector3::new(dir.x, dir.y, dir.z))
    } else {
        let d = -depth;
        let pos = builder::tsweep(face, Vector3::new(n.x * d, n.y * d, n.z * d));
        builder::transformed(
            &pos,
            Matrix4::from_translation(Vector3::new(n.x * depth, n.y * depth, n.z * depth)),
        )
    }
}

/// 由棱点、切线和相邻面法线构造倒角/圆角刀具的二维截面。
/// 返回 `(u, v, outer)`，其中 `u × v = tangent`，`outer` 在 u-v 平面内为 CCW。
fn edge_cutter_profile(
    point: DVec3,
    tangent: DVec3,
    n1: DVec3,
    n2: DVec3,
    chamfer: Option<f64>,
    fillet: Option<f64>,
    padding: f64,
    fillet_segments: Option<usize>,
) -> Result<(DVec3, DVec3, Vec<[f64; 2]>)> {
    let normalize = |v: DVec3, what: &str| -> Result<DVec3> {
        if !v.is_finite() || v.length_squared() < 1e-18 {
            return Err(KernelError::Geometry(format!("degenerate {what}")));
        }
        Ok(v.normalize())
    };
    let e = normalize(tangent, "edge")?;
    let n1v = normalize(n1 - e * n1.dot(e), "face 1")?;
    let n2v = normalize(n2 - e * n2.dot(e), "face 2")?;

    // 面内垂直棱的方向（朝实体内部）。
    let mut t1 = normalize(e.cross(n1v), "face 1")?;
    if t1.dot(-n2v) < 0.0 {
        t1 = -t1;
    }
    let mut t2 = normalize(e.cross(n2v), "face 2")?;
    if t2.dot(-n1v) < 0.0 {
        t2 = -t2;
    }

    // 局部基：u = t1，v = e × u（normal = u × v = e）。
    let u = t1;
    let v = normalize(e.cross(u), "cutter profile")?;
    let to_local = |p: DVec3| -> [f64; 2] {
        let d = p - point;
        [d.dot(u), d.dot(v)]
    };

    let bisector = normalize(t1 + t2, "edge bisector")?;
    let cos_theta = n1v.dot(n2v).clamp(-1.0, 1.0);
    let theta = cos_theta.acos();
    if theta <= 1e-6 || (std::f64::consts::PI - theta) <= 1e-6 {
        return Err(KernelError::Geometry("invalid edge face angle".into()));
    }

    let p0 = to_local(point - (t1 + t2) * padding.max(1e-4));
    let mut outer: Vec<[f64; 2]> = Vec::new();
    match (chamfer, fillet) {
        (Some(d), None) if d.is_finite() && d > 0.0 => {
            let q1 = to_local(point + t1 * d);
            let q2 = to_local(point + t2 * d);
            outer = vec![p0, q2, q1];
        }
        (None, Some(r)) if r.is_finite() && r > 0.0 => {
            let half = theta / 2.0;
            // theta is the angle between OUTWARD normals. The material's
            // interior angle is PI - theta, not theta (equal only at 90 deg).
            let center = point + bisector * (r / half.cos());
            let tangent_distance = r * half.tan();
            let q1 = point + t1 * tangent_distance;
            let q2 = point + t2 * tangent_distance;
            let c_local = to_local(center);
            let q1_local = to_local(q1);
            let q2_local = to_local(q2);
            let start_ang = (q1_local[1] - c_local[1]).atan2(q1_local[0] - c_local[0]);
            let end_ang = (q2_local[1] - c_local[1]).atan2(q2_local[0] - c_local[0]);
            let mut delta = end_ang - start_ang;
            while delta > std::f64::consts::PI {
                delta -= 2.0 * std::f64::consts::PI;
            }
            while delta < -std::f64::consts::PI {
                delta += 2.0 * std::f64::consts::PI;
            }
            let segs = fillet_segments
                .unwrap_or_else(|| ((theta / 3.0f64.to_radians()).ceil() as usize).max(16))
                .max(4);
            outer.push(p0);
            for k in 0..=segs {
                let ang = end_ang - delta * (k as f64 / segs as f64);
                outer.push([c_local[0] + ang.cos() * r, c_local[1] + ang.sin() * r]);
            }
        }
        _ => {
            return Err(KernelError::Geometry(
                "exactly one positive chamfer/fillet value must be set".into(),
            ))
        }
    }

    let area2: f64 = outer
        .iter()
        .zip(outer.iter().cycle().skip(1))
        .take(outer.len())
        .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
        .sum();
    if area2.abs() < 1e-12 {
        return Err(KernelError::Geometry("degenerate cutter profile".into()));
    }
    if area2 < 0.0 {
        outer.reverse();
    }
    Ok((u, v, outer))
}

#[derive(Debug, Clone, Copy)]
struct CircularEdgeFrame {
    center: DVec3,
    axis: DVec3,
    point: DVec3,
    radial: DVec3,
    side_normal: DVec3,
    radius: f64,
    point_count: usize,
}

fn solve_3x3(mut m: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&a, &b| {
            m[a][col]
                .abs()
                .partial_cmp(&m[b][col].abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        if pivot != col {
            m.swap(pivot, col);
        }
        let div = m[col][col];
        for j in col..4 {
            m[col][j] /= div;
        }
        for row in 0..3 {
            if row == col {
                continue;
            }
            let factor = m[row][col];
            for j in col..4 {
                m[row][j] -= factor * m[col][j];
            }
        }
    }
    Some([m[0][3], m[1][3], m[2][3]])
}

/// 闭合棱链 → 圆与轴向 frame。乱序、反向段以及 n1/n2 交换均可识别。
fn fit_circular_edge_chain(segments: &[EdgeCutterSegment]) -> Option<CircularEdgeFrame> {
    if segments.len() < 6 {
        return None;
    }
    let raw_points: Vec<DVec3> = segments
        .iter()
        .flat_map(|s| [DVec3::from_array(s.a), DVec3::from_array(s.b)])
        .collect();
    if raw_points.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for p in &raw_points {
        min = min.min(*p);
        max = max.max(*p);
    }
    let scale = (max - min).length().max(1.0);
    let weld_tol = (scale * 1e-6).max(1e-6);
    let mut points: Vec<DVec3> = Vec::new();
    let mut degree: Vec<usize> = Vec::new();
    for p in raw_points {
        let id = points
            .iter()
            .position(|q| q.distance(p) <= weld_tol)
            .unwrap_or_else(|| {
                points.push(p);
                degree.push(0);
                points.len() - 1
            });
        degree[id] += 1;
    }
    if points.len() != segments.len() || degree.iter().any(|&d| d != 2) {
        return None;
    }

    let normalized_pair = |s: &EdgeCutterSegment| -> Option<(DVec3, DVec3)> {
        let a = DVec3::from_array(s.n1);
        let b = DVec3::from_array(s.n2);
        if !a.is_finite()
            || !b.is_finite()
            || a.length_squared() < 1e-18
            || b.length_squared() < 1e-18
        {
            return None;
        }
        Some((a.normalize(), b.normalize()))
    };
    let pairs: Vec<(DVec3, DVec3)> = segments
        .iter()
        .map(normalized_pair)
        .collect::<Option<_>>()?;
    // 圆边与平面的交线：每段的两个相邻面中，有一组法线在全环近似恒定。
    let mut best: Option<(DVec3, f64)> = None;
    for candidate in pairs.iter().flat_map(|(a, b)| [*a, *b]) {
        let score: f64 = pairs
            .iter()
            .map(|(a, b)| candidate.dot(*a).abs().max(candidate.dot(*b).abs()))
            .sum::<f64>()
            / pairs.len() as f64;
        if best.map(|(_, s)| score > s).unwrap_or(true) {
            best = Some((candidate, score));
        }
    }
    let (candidate, score) = best?;
    if score < 0.985 {
        return None;
    }
    let mut plane_sum = DVec3::ZERO;
    for (a, b) in &pairs {
        let mut plane = if a.dot(candidate).abs() >= b.dot(candidate).abs() {
            *a
        } else {
            *b
        };
        if plane.dot(candidate) < 0.0 {
            plane = -plane;
        }
        plane_sum += plane;
    }
    if plane_sum.length_squared() < 1e-18 {
        return None;
    }
    let axis = plane_sum.normalize();

    let centroid = points.iter().copied().sum::<DVec3>() / points.len() as f64;
    let plane_tol = (scale * 1e-4).max(1e-5);
    if points
        .iter()
        .any(|p| (*p - centroid).dot(axis).abs() > plane_tol)
    {
        return None;
    }
    let basis_u = points
        .iter()
        .map(|p| {
            let d = *p - centroid;
            d - axis * d.dot(axis)
        })
        .max_by(|a, b| {
            a.length_squared()
                .partial_cmp(&b.length_squared())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    if basis_u.length_squared() < 1e-18 {
        return None;
    }
    let basis_u = basis_u.normalize();
    let basis_v = axis.cross(basis_u).normalize();

    // 稳定的代数最小二乘圆拟合：x²+y²+A*x+B*y+C=0。
    let mut ata = [[0.0f64; 3]; 3];
    let mut atb = [0.0f64; 3];
    for p in &points {
        let d = *p - centroid;
        let row = [d.dot(basis_u), d.dot(basis_v), 1.0];
        let rhs = -(row[0] * row[0] + row[1] * row[1]);
        for i in 0..3 {
            atb[i] += row[i] * rhs;
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    let solution = solve_3x3([
        [ata[0][0], ata[0][1], ata[0][2], atb[0]],
        [ata[1][0], ata[1][1], ata[1][2], atb[1]],
        [ata[2][0], ata[2][1], ata[2][2], atb[2]],
    ])?;
    let cx = -solution[0] / 2.0;
    let cy = -solution[1] / 2.0;
    let radius_sq = cx * cx + cy * cy - solution[2];
    if !radius_sq.is_finite() || radius_sq <= 1e-12 {
        return None;
    }
    let radius = radius_sq.sqrt();
    // Vertices of a regular polygon also lie on a circle. Its long chords
    // must retain their corners instead of receiving a round cutter.
    if segments
        .iter()
        .any(|s| DVec3::from_array(s.a).distance(DVec3::from_array(s.b)) > radius * 0.4)
    {
        return None;
    }
    let center = centroid + basis_u * cx + basis_v * cy;
    let radial_tol = (radius * 1e-3).max(1e-5);
    if points.iter().any(|p| {
        (((*p - center) - axis * (*p - center).dot(axis)).length() - radius).abs() > radial_tol
    }) {
        return None;
    }

    let seg = &segments[0];
    let midpoint = (DVec3::from_array(seg.a) + DVec3::from_array(seg.b)) * 0.5;
    let radial = {
        let d = midpoint - center;
        let d = d - axis * d.dot(axis);
        if d.length_squared() < 1e-18 {
            return None;
        }
        d.normalize()
    };
    let (a, b) = pairs[0];
    let mut side_normal = if a.dot(axis).abs() < b.dot(axis).abs() {
        a
    } else {
        b
    };
    let tangent = axis.cross(radial).normalize();
    side_normal -= tangent * side_normal.dot(tangent);
    if side_normal.length_squared() < 1e-18 {
        return None;
    }
    side_normal = side_normal.normalize();
    Some(CircularEdgeFrame {
        center,
        axis,
        point: center + radial * radius,
        radial,
        side_normal,
        radius,
        point_count: points.len(),
    })
}

#[derive(Debug, Clone)]
struct PlanarEdgeLoop {
    /// 闭环顶点，按连接顺序排列；第 i 条段为 points[i] → points[(i+1)%n]。
    points: Vec<DVec3>,
    /// 与上述有向段对应的非平面侧外法线。
    side_normals: Vec<DVec3>,
    plane_normal: DVec3,
    scale: f64,
}

/// Recognize a closed planar lip with near-vertical sides. Sharp miter joins
/// are used on hole rims, where the cut widens towards the surrounding solid.
fn fit_planar_edge_loop(segments: &[EdgeCutterSegment]) -> Option<PlanarEdgeLoop> {
    if segments.len() < 3 {
        return None;
    }
    let raw_points: Vec<DVec3> = segments
        .iter()
        .flat_map(|s| [DVec3::from_array(s.a), DVec3::from_array(s.b)])
        .collect();
    if raw_points.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for p in &raw_points {
        min = min.min(*p);
        max = max.max(*p);
    }
    let scale = (max - min).length().max(1.0);
    let weld_tol = (scale * 1e-6).max(1e-6);
    let mut points: Vec<DVec3> = Vec::new();
    let mut endpoint_ids: Vec<(usize, usize)> = Vec::with_capacity(segments.len());
    let point_id = |p: DVec3, points: &mut Vec<DVec3>| -> usize {
        points
            .iter()
            .position(|q| q.distance(p) <= weld_tol)
            .unwrap_or_else(|| {
                points.push(p);
                points.len() - 1
            })
    };
    for segment in segments {
        let a = point_id(DVec3::from_array(segment.a), &mut points);
        let b = point_id(DVec3::from_array(segment.b), &mut points);
        if a == b {
            return None;
        }
        endpoint_ids.push((a, b));
    }
    if points.len() != segments.len() {
        return None;
    }
    let mut adjacency: Vec<Vec<(usize, usize)>> = vec![Vec::new(); points.len()];
    for (segment_id, &(a, b)) in endpoint_ids.iter().enumerate() {
        adjacency[a].push((b, segment_id));
        adjacency[b].push((a, segment_id));
    }
    if adjacency.iter().any(|neighbors| neighbors.len() != 2) {
        return None;
    }

    let normalized_pair = |s: &EdgeCutterSegment| -> Option<(DVec3, DVec3)> {
        let a = DVec3::from_array(s.n1);
        let b = DVec3::from_array(s.n2);
        if !a.is_finite()
            || !b.is_finite()
            || a.length_squared() < 1e-18
            || b.length_squared() < 1e-18
        {
            return None;
        }
        Some((a.normalize(), b.normalize()))
    };
    let pairs: Vec<(DVec3, DVec3)> = segments
        .iter()
        .map(normalized_pair)
        .collect::<Option<_>>()?;
    let mut best: Option<(DVec3, f64)> = None;
    for candidate in pairs.iter().flat_map(|(a, b)| [*a, *b]) {
        let score = pairs
            .iter()
            .map(|(a, b)| candidate.dot(*a).abs().max(candidate.dot(*b).abs()))
            .sum::<f64>()
            / pairs.len() as f64;
        if best.map(|(_, old)| score > old).unwrap_or(true) {
            best = Some((candidate, score));
        }
    }
    let (candidate, score) = best?;
    if score < 0.985 {
        return None;
    }
    let mut plane_sum = DVec3::ZERO;
    let mut side_by_segment = Vec::with_capacity(segments.len());
    for (a, b) in &pairs {
        let (mut plane, side) = if a.dot(candidate).abs() >= b.dot(candidate).abs() {
            (*a, *b)
        } else {
            (*b, *a)
        };
        if plane.dot(candidate) < 0.0 {
            plane = -plane;
        }
        plane_sum += plane;
        side_by_segment.push(side);
    }
    if plane_sum.length_squared() < 1e-18 {
        return None;
    }
    let plane_normal = plane_sum.normalize();
    // 当前连续 sweep 处理近似正交的侧壁；斜壁/锥面仍走原路径。
    if side_by_segment
        .iter()
        .any(|side| side.dot(plane_normal).abs() > 0.15)
    {
        return None;
    }
    let centroid = points.iter().copied().sum::<DVec3>() / points.len() as f64;
    let plane_tol = (scale * 1e-4).max(1e-5);
    if points
        .iter()
        .any(|point| (*point - centroid).dot(plane_normal).abs() > plane_tol)
    {
        return None;
    }

    // 从任意起点沿 degree=2 图走完整闭环，同时把侧法线重排到对应有向段。
    let mut ordered_points = Vec::with_capacity(points.len());
    let mut ordered_sides = Vec::with_capacity(points.len());
    let mut used_segments = vec![false; segments.len()];
    let start = 0usize;
    let mut current = start;
    let mut previous: Option<usize> = None;
    for step in 0..points.len() {
        ordered_points.push(points[current]);
        let &(next, segment_id) = adjacency[current]
            .iter()
            .find(|(neighbor, segment_id)| {
                !used_segments[*segment_id] && previous.map(|p| *neighbor != p).unwrap_or(true)
            })
            .or_else(|| {
                adjacency[current]
                    .iter()
                    .find(|(_, segment_id)| !used_segments[*segment_id])
            })?;
        used_segments[segment_id] = true;
        ordered_sides.push(side_by_segment[segment_id]);
        previous = Some(current);
        current = next;
        if current == start && step + 1 != points.len() {
            return None;
        }
    }
    if current != start || used_segments.iter().any(|used| !used) {
        return None;
    }

    let max_turn_cos = 25.0f64.to_radians().cos();
    let max_normal_turn_cos = 35.0f64.to_radians().cos();
    let n = ordered_points.len();
    let winding = (0..n)
        .map(|i| {
            (ordered_points[i] - centroid)
                .cross(ordered_points[(i + 1) % n] - centroid)
                .dot(plane_normal)
        })
        .sum::<f64>();
    let hole_rim = winding.abs() > 1e-10
        && (0..n).all(|i| {
            let tangent = (ordered_points[(i + 1) % n] - ordered_points[i]).normalize();
            ordered_sides[i].dot(plane_normal.cross(tangent) * winding.signum()) > 0.9
        });
    for i in 0..n {
        let previous_dir = (ordered_points[i] - ordered_points[(i + n - 1) % n]).normalize();
        let next_dir = (ordered_points[(i + 1) % n] - ordered_points[i]).normalize();
        if !hole_rim && previous_dir.dot(next_dir) < max_turn_cos {
            return None;
        }
        let previous_side = ordered_sides[(i + n - 1) % n];
        let next_side = ordered_sides[i];
        if !hole_rim && previous_side.dot(next_side) < max_normal_turn_cos {
            return None;
        }
    }
    Some(PlanarEdgeLoop {
        points: ordered_points,
        side_normals: ordered_sides,
        plane_normal,
        scale,
    })
}

impl TruckKernel {
    /// 沿棱的倒角/圆角刀具体（CSG 差集用，返回焊接后的 MeshData）。
    /// a/b：棱两端点；n1/n2：两相邻面外法线；chamfer：倒角距离；fillet：圆角半径。
    /// 一律按凸边处理：boolean_subtract(target, cutter) 得到倒角/圆角
    /// （圆孔口边缘、外棱等均为凸边）。
    pub fn edge_cutter(
        &mut self,
        a: [f64; 3],
        b: [f64; 3],
        n1: [f64; 3],
        n2: [f64; 3],
        chamfer: Option<f64>,
        fillet: Option<f64>,
    ) -> Result<MeshData> {
        self.edge_cutter_prism(a, b, n1, n2, chamfer, fillet, false)
    }

    /// Build an additive profile inside a re-entrant corner. Its caps stop at
    /// the original endpoints so a through-hole does not acquire raised tabs.
    pub fn concave_edge_cutter(
        &mut self,
        edge: &EdgeCutterSegment,
        chamfer: Option<f64>,
        fillet: Option<f64>,
    ) -> Result<MeshData> {
        self.edge_cutter_prism(
            edge.a,
            edge.b,
            edge.n1.map(|x| -x),
            edge.n2.map(|x| -x),
            chamfer,
            fillet,
            true,
        )
    }

    fn edge_cutter_prism(
        &mut self,
        a: [f64; 3],
        b: [f64; 3],
        n1: [f64; 3],
        n2: [f64; 3],
        chamfer: Option<f64>,
        fillet: Option<f64>,
        additive: bool,
    ) -> Result<MeshData> {
        let pa = DVec3::from_array(a);
        let pb = DVec3::from_array(b);
        let e = pb - pa;
        let len = e.length();
        if len < 1e-9 {
            return Err(KernelError::Geometry("degenerate edge".into()));
        }
        let e = e / len;
        let overrun = if additive { 0.0 } else { 2.0 };
        let padding = if additive {
            chamfer.or(fillet).unwrap_or(0.0) * 0.1
        } else {
            2.0
        };
        let (u, v, outer) = edge_cutter_profile(
            pa,
            e,
            DVec3::from_array(n1),
            DVec3::from_array(n2),
            chamfer,
            fillet,
            padding,
            None,
        )?;
        // 刀具平面：原点 = a - e*2（两端各突出 2mm），沿 e 挤出 len+4
        let origin = pa - e * overrun;
        // 沿 e 挤出
        let plane = PlaneDef {
            origin: [origin.x, origin.y, origin.z],
            x_axis: [u.x, u.y, u.z],
            y_axis: [v.x, v.y, v.z],
        };
        let region = Region2D {
            outer,
            holes: Vec::new(),
        };
        let face = self.region_to_face(&region, &plane)?;
        let solid = extrude_face(&face, &plane, len + 2.0 * overrun);
        let mesh = tessellate_solid(&solid);
        let mut mesh = mesh;
        mesh.weld(1e-5);
        Ok(mesh)
    }

    /// 若棱链是闭合圆，则生成一个连续的环形刀具；非圆链返回 `Ok(None)`。
    ///
    /// 圆孔口不再逐段挤出并反复布尔，而是把同一个子午截面一次旋转 360°，
    /// 从拓扑上消除相邻直段刀具重叠造成的交线、薄片和褶皱。
    pub fn circular_edge_cutter(
        &mut self,
        segments: &[EdgeCutterSegment],
        chamfer: Option<f64>,
        fillet: Option<f64>,
    ) -> Result<Option<MeshData>> {
        let Some(frame) = fit_circular_edge_chain(segments) else {
            return Ok(None);
        };
        let amount = chamfer.or(fillet).ok_or_else(|| {
            KernelError::Geometry("exactly one chamfer/fillet value must be set".into())
        })?;
        if !amount.is_finite() || amount <= 0.0 {
            return Err(KernelError::Geometry(
                "chamfer/fillet value must be positive".into(),
            ));
        }
        // 仅需略微穿出相邻面以避免共面；按参数/圆半径缩放，避免小孔被固定 2mm pad 穿轴。
        let padding = (amount * 0.1)
            .clamp(0.01, 0.25)
            .min((frame.radius * 0.1).max(0.005));
        let tangent = frame.axis.cross(frame.radial).normalize();
        let (u, v, outer) = edge_cutter_profile(
            frame.point,
            tangent,
            frame.axis,
            frame.side_normal,
            chamfer,
            fillet,
            padding,
            None,
        )?;

        let mut profile: Vec<[f64; 2]> = Vec::with_capacity(outer.len());
        for p in outer {
            let world = frame.point + u * p[0] + v * p[1];
            let relative = world - frame.center;
            if relative.dot(tangent).abs() > (frame.radius * 1e-6).max(1e-6) {
                return Err(KernelError::Geometry(
                    "circular cutter profile left the meridian plane".into(),
                ));
            }
            let rho = relative.dot(frame.radial);
            let z = relative.dot(frame.axis);
            if rho <= 1e-6 {
                return Err(KernelError::Geometry(
                    "chamfer/fillet is too large for this circular edge".into(),
                ));
            }
            profile.push([rho, z]);
        }
        let area2: f64 = profile
            .iter()
            .zip(profile.iter().cycle().skip(1))
            .take(profile.len())
            .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
            .sum();
        if area2.abs() < 1e-12 {
            return Err(KernelError::Geometry("degenerate circular cutter".into()));
        }
        if area2 < 0.0 {
            profile.reverse();
        }
        // 原草图圆通常是 72 段；环向至少 120 段（3°）保证小圆孔仍平滑。
        let circular_segments = frame.point_count.max(120).min(720);
        revolve_profile(
            &profile,
            frame.center.to_array(),
            frame.axis.to_array(),
            frame.radial.to_array(),
            circular_segments,
        )
        .map(Some)
    }

    /// 沿共面的平滑闭环或多边形孔缘生成一个共用截面。
    /// 相邻线段共用角部截面，避免逐段切削留下接缝。
    pub fn closed_planar_edge_cutter(
        &mut self,
        segments: &[EdgeCutterSegment],
        chamfer: Option<f64>,
        fillet: Option<f64>,
        additive: bool,
    ) -> Result<Option<MeshData>> {
        let Some(edge_loop) = fit_planar_edge_loop(segments) else {
            return Ok(None);
        };
        let amount = chamfer.or(fillet).ok_or_else(|| {
            KernelError::Geometry("exactly one chamfer/fillet value must be set".into())
        })?;
        if !amount.is_finite() || amount <= 0.0 {
            return Err(KernelError::Geometry(
                "chamfer/fillet value must be positive".into(),
            ));
        }
        let padding = (amount * 0.1)
            .clamp(0.01, 0.25)
            .min((edge_loop.scale * 0.01).max(0.005));
        let n = edge_loop.points.len();
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut profile_len: Option<usize> = None;

        for i in 0..n {
            let point = edge_loop.points[i];
            let previous_dir = (point - edge_loop.points[(i + n - 1) % n]).normalize();
            let next_dir = (edge_loop.points[(i + 1) % n] - point).normalize();
            let tangent = (previous_dir + next_dir).normalize_or_zero();
            if tangent.length_squared() < 1e-18 {
                return Ok(None);
            }
            let previous_side = edge_loop.side_normals[(i + n - 1) % n];
            let next_side = edge_loop.side_normals[i];
            let mut side = previous_side + next_side;
            side -= tangent * side.dot(tangent);
            if side.length_squared() < 1e-18 {
                return Ok(None);
            }
            side = side.normalize();
            let orientation = if additive { -1.0 } else { 1.0 };
            let (u, v, outer) = edge_cutter_profile(
                point,
                tangent,
                orientation * edge_loop.plane_normal,
                orientation * side,
                chamfer,
                fillet,
                padding,
                fillet.map(|_| 32),
            )?;
            if profile_len.map(|len| len != outer.len()).unwrap_or(false) {
                return Ok(None);
            }
            profile_len = Some(outer.len());

            // 多段曲线顶点使用 miter 比例，使距相邻两段的横向偏移都保持一致。
            let segment_plane_dir = |direction: DVec3, side_normal: DVec3| {
                let mut inward = direction.cross(edge_loop.plane_normal).normalize();
                if inward.dot(-side_normal) < 0.0 {
                    inward = -inward;
                }
                inward
            };
            let previous_inward = segment_plane_dir(previous_dir, previous_side);
            let next_inward = segment_plane_dir(next_dir, next_side);
            let denominator =
                ((u.dot(previous_inward).abs() + u.dot(next_inward).abs()) * 0.5).max(0.25);
            let miter_scale = 1.0 / denominator;
            if miter_scale > 2.0 + 1e-8 {
                return Ok(None);
            }
            for p in outer {
                let world = point + u * (p[0] * miter_scale) + v * p[1];
                if !world.is_finite() {
                    return Err(KernelError::Geometry(
                        "non-finite closed edge cutter vertex".into(),
                    ));
                }
                positions.push(world.as_vec3().to_array());
            }
        }

        let Some(profile_len) = profile_len else {
            return Ok(None);
        };
        let mut indices = Vec::with_capacity(n * profile_len * 6);
        for i in 0..n {
            let next_i = (i + 1) % n;
            for j in 0..profile_len {
                let next_j = (j + 1) % profile_len;
                let a = (i * profile_len + j) as u32;
                let b = (next_i * profile_len + j) as u32;
                let c = (next_i * profile_len + next_j) as u32;
                let d = (i * profile_len + next_j) as u32;
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        let signed_volume = indices
            .chunks_exact(3)
            .map(|tri| {
                let a = DVec3::from_array(positions[tri[0] as usize].map(f64::from));
                let b = DVec3::from_array(positions[tri[1] as usize].map(f64::from));
                let c = DVec3::from_array(positions[tri[2] as usize].map(f64::from));
                a.dot(b.cross(c)) / 6.0
            })
            .sum::<f64>();
        if !signed_volume.is_finite() || signed_volume.abs() < 1e-10 {
            return Ok(None);
        }
        if signed_volume < 0.0 {
            for triangle in indices.chunks_exact_mut(3) {
                triangle.swap(1, 2);
            }
        }
        let mesh = MeshData::new(positions, indices);
        if mesh.indices.chunks_exact(3).any(|tri| {
            let a = glam::Vec3::from_array(mesh.positions[tri[0] as usize]);
            let b = glam::Vec3::from_array(mesh.positions[tri[1] as usize]);
            let c = glam::Vec3::from_array(mesh.positions[tri[2] as usize]);
            (b - a).cross(c - a).length_squared() < 1e-14
        }) {
            return Ok(None);
        }
        Ok(Some(mesh))
    }
}

impl GeometryKernel for TruckKernel {
    fn extrude_region(
        &mut self,
        region: &Region2D,
        plane: &PlaneDef,
        depth: f64,
    ) -> Result<ShapeId> {
        if region.outer.len() < 3 {
            return Err(KernelError::Geometry(
                "region has no closed outer loop".into(),
            ));
        }
        let face = self.region_to_face(region, plane)?;
        let solid = extrude_face(&face, plane, depth);
        Ok(self.insert(KernelShape::Brep(solid)))
    }

    fn revolve_region(
        &mut self,
        region: &Region2D,
        plane: &PlaneDef,
        axis: &Axis2D,
        angle_deg: f64,
    ) -> Result<ShapeId> {
        if !axis.is_valid() {
            return Err(KernelError::Geometry("revolve axis has zero length".into()));
        }
        if region.outer.len() < 3 {
            return Err(KernelError::Geometry(
                "region has no closed outer loop".into(),
            ));
        }
        // 轴：平面局部 (point, dir) → 世界坐标
        let origin = plane.to_world(axis.point[0], axis.point[1]);
        let q = plane.to_world(axis.point[0] + axis.dir[0], axis.point[1] + axis.dir[1]);
        let axis_dir = DVec3::from_array(q) - DVec3::from_array(origin);
        let axis_dir = axis_dir.normalize();
        let angle = angle_deg.min(360.0).max(-360.0);
        let mut rings = Vec::new();
        for (i, ring) in std::iter::once(&region.outer)
            .chain(region.holes.iter())
            .enumerate()
        {
            let cleaned = clean_region_ring(ring)?;
            rings.push(
                plane
                    .with_orientation(&cleaned, i == 0)
                    .iter()
                    .map(|p| plane.to_world(p[0], p[1]))
                    .collect(),
            );
        }
        let solid = crate::occt::ExactShape::revolve(
            &rings,
            origin,
            axis_dir.to_array(),
            angle.to_radians(),
        )?;
        Ok(self.insert(KernelShape::Exact(solid)))
    }

    fn boolean_union(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId> {
        if let Some(id) = self.exact_boolean(a, b, 0)? {
            return Ok(id);
        }
        let ma = self.get_mesh(a)?;
        let mb = self.get_mesh(b)?;
        let out = boolean(&ma, &mb, BoolOp::Union)?;
        Ok(self.insert(KernelShape::Mesh(out)))
    }

    fn boolean_subtract(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId> {
        if let Some(id) = self.exact_boolean(a, b, 1)? {
            return Ok(id);
        }
        let ma = self.get_mesh(a)?;
        let mb = self.get_mesh(b)?;
        let out = boolean(&ma, &mb, BoolOp::Subtract)?;
        Ok(self.insert(KernelShape::Mesh(out)))
    }

    fn boolean_intersect(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId> {
        if let Some(id) = self.exact_boolean(a, b, 2)? {
            return Ok(id);
        }
        let ma = self.get_mesh(a)?;
        let mb = self.get_mesh(b)?;
        let out = boolean(&ma, &mb, BoolOp::Intersect)?;
        Ok(self.insert(KernelShape::Mesh(out)))
    }

    fn transform(&mut self, id: ShapeId, mat: &Mat4) -> Result<ShapeId> {
        let out = match self.get(id)? {
            KernelShape::Exact(shape) => KernelShape::Exact(shape.transform(mat)?),
            KernelShape::Brep(solid) => {
                // glam Mat4（列主序）→ cgmath Matrix4（行向量构造）
                let cgmat = Matrix4::from_cols(
                    Vector4::new(
                        mat.x_axis.x as f64,
                        mat.x_axis.y as f64,
                        mat.x_axis.z as f64,
                        0.0,
                    ),
                    Vector4::new(
                        mat.y_axis.x as f64,
                        mat.y_axis.y as f64,
                        mat.y_axis.z as f64,
                        0.0,
                    ),
                    Vector4::new(
                        mat.z_axis.x as f64,
                        mat.z_axis.y as f64,
                        mat.z_axis.z as f64,
                        0.0,
                    ),
                    Vector4::new(
                        mat.w_axis.x as f64,
                        mat.w_axis.y as f64,
                        mat.w_axis.z as f64,
                        1.0,
                    ),
                );
                let moved = builder::transformed(solid, cgmat);
                KernelShape::Brep(moved)
            }
            KernelShape::Mesh(m) => {
                let mut out = m.clone();
                out.transform(mat);
                KernelShape::Mesh(out)
            }
        };
        Ok(self.insert(out))
    }

    fn mirror(&mut self, id: ShapeId, plane: &PlaneDef) -> Result<ShapeId> {
        if self.exact_shape(id).is_some() {
            let n = glam::Vec3::from_array(plane.normal().to_array().map(|v| v as f32)).normalize();
            let origin = glam::Vec3::from_array(plane.origin.map(|v| v as f32));
            let linear =
                glam::Mat3::IDENTITY - 2.0 * glam::Mat3::from_cols(n * n.x, n * n.y, n * n.z);
            let mut matrix = Mat4::from_mat3(linear);
            matrix.w_axis = (2.0 * n * origin.dot(n)).extend(1.0);
            return self.transform(id, &matrix);
        }
        let m = self.get_mesh(id)?;
        let out = mesh_mirror(&m, plane.origin, plane.normal().to_array())?;
        Ok(self.insert(KernelShape::Mesh(out)))
    }

    fn split_by_plane(&mut self, id: ShapeId, plane: &PlaneDef) -> Result<Vec<ShapeId>> {
        if let Some(shape) = self.exact_shape(id) {
            let parts = shape.split(plane.origin, plane.normal().to_array())?;
            return Ok(parts
                .into_iter()
                .map(|part| self.insert_exact(part))
                .collect());
        }
        let m = self.get_mesh(id)?;
        let parts = split_mesh_components(&m, plane.origin, plane.normal().to_array())?;
        Ok(parts
            .into_iter()
            .map(|part| self.insert(KernelShape::Mesh(part)))
            .collect())
    }

    fn tessellate(&self, id: ShapeId) -> Result<MeshData> {
        self.get_mesh(id)
    }

    fn import_step(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let text = std::fs::read_to_string(path)?;
        // Place the complete assembly before its one display tessellation.
        let shapes = crate::occt::ExactShape::import_step_normalized(&text)?;
        Ok(shapes
            .into_iter()
            .map(|shape| self.insert(KernelShape::Exact(shape)))
            .collect())
    }

    fn export_step(&self, id: ShapeId, path: &str) -> Result<()> {
        let text = match self.get(id)? {
            KernelShape::Exact(shape) => shape.export_step_z_up()?,
            _ => crate::occt::ExactShape::export_mesh_step(&self.get_mesh(id)?)?,
        };
        crate::export_file::write(path, text.as_bytes())?;
        Ok(())
    }

    fn import_iges(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let shapes = crate::occt::ExactShape::import_iges(path, true)?;
        Ok(shapes
            .into_iter()
            .map(|shape| self.insert(KernelShape::Exact(shape)))
            .collect())
    }

    fn import_stl(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let mut meshes = crate::mesh_io::read_stl(path)?;
        normalize_imported(&mut meshes, false);
        Ok(meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| self.insert(KernelShape::Mesh(m)))
            .collect())
    }

    fn import_obj(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let mut meshes = crate::mesh_io::read_obj(path)?;
        normalize_imported(&mut meshes, false);
        Ok(meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| self.insert(KernelShape::Mesh(m)))
            .collect())
    }

    fn import_ply(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let mut meshes = crate::mesh_io::read_ply(path)?;
        normalize_imported(&mut meshes, false);
        Ok(meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| self.insert(KernelShape::Mesh(m)))
            .collect())
    }

    fn import_3mf(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        // Parse and expand the entire build before inserting any shapes.
        let mut meshes = crate::three_mf::read(path)?;
        normalize_imported(&mut meshes, false);
        Ok(meshes
            .into_iter()
            .map(|m| self.insert(KernelShape::Mesh(m)))
            .collect())
    }

    fn import_glb(&mut self, path: &str) -> Result<Vec<ShapeId>> {
        let mut meshes = crate::mesh_io::read_glb(path)?;
        normalize_imported(&mut meshes, false);
        Ok(meshes
            .into_iter()
            .filter(|m| !m.is_empty())
            .map(|m| self.insert(KernelShape::Mesh(m)))
            .collect())
    }

    fn export_stl(&self, id: ShapeId, path: &str) -> Result<()> {
        let m = self.get_mesh(id)?;
        crate::mesh_io::write_stl(&m, path)
    }

    fn export_obj(&self, id: ShapeId, path: &str) -> Result<()> {
        let m = self.get_mesh(id)?;
        crate::mesh_io::write_obj(&m, path)
    }

    fn export_ply(&self, id: ShapeId, path: &str) -> Result<()> {
        let m = self.get_mesh(id)?;
        crate::mesh_io::write_ply(&m, path)
    }

    fn export_glb(&self, id: ShapeId, path: &str) -> Result<()> {
        let m = self.get_mesh(id)?;
        crate::mesh_io::write_glb(&m, path)
    }

    fn delete(&mut self, id: ShapeId) {
        self.shapes.remove(&id);
    }

    fn retain_shapes(&mut self, live: &HashSet<ShapeId>) -> usize {
        let before = self.shapes.len();
        self.shapes.retain(|id, _| live.contains(id));
        before - self.shapes.len()
    }

    fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    fn bounds(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let mut all: Option<(glam::Vec3, glam::Vec3)> = None;
        for shape in self.shapes.values() {
            let b = match shape.as_ref() {
                KernelShape::Exact(shape) => shape.mesh.bounds(),
                KernelShape::Brep(s) => tessellate_solid(s).bounds(),
                KernelShape::Mesh(m) => m.bounds(),
            };
            if let Some((min, max)) = b {
                all = Some(match all {
                    None => (min, max),
                    Some((amin, amax)) => (amin.min(min), amax.max(max)),
                });
            }
        }
        all
    }
}

/// 导入规范化：
/// 1. `zup_to_yup=true`（STEP）：Z-up → Y-up 转换 `(x,y,z) → (x,z,-y)`
///    （Super3D 导入一致；winding 由后续渲染法线计算处理）。
/// 2. 整体平移到「原点上方中央」：合并包围盒的底面贴 Y=0 平面，
///    XZ 水平居中。同一文件的多个 mesh 保持相对位置。
fn normalize_imported(meshes: &mut [MeshData], zup_to_yup: bool) {
    if meshes.is_empty() {
        return;
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for m in meshes.iter_mut() {
        if zup_to_yup {
            for p in &mut m.positions {
                *p = [p[0], p[2], -p[1]];
            }
        }
        for p in &m.positions {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    if !min[0].is_finite() || !max[0].is_finite() {
        return;
    }
    let t = glam::Vec3::new(-(min[0] + max[0]) / 2.0, -min[1], -(min[2] + max[2]) / 2.0);
    if t.length_squared() < 1e-10 {
        return;
    }
    for m in meshes.iter_mut() {
        m.translate(t);
    }
}
/// 将 truck 的 PolygonMesh 转成 MeshData（拆三角）。
fn polygon_mesh_to_data(mesh: &truck_polymesh::PolygonMesh) -> MeshData {
    let positions: Vec<[f32; 3]> = mesh
        .positions()
        .iter()
        .map(|p| [p.x as f32, p.y as f32, p.z as f32])
        .collect();
    let mut indices = Vec::new();
    for face in mesh.face_iter() {
        // 扇形三角化（truck 网格通常是三角形/四边形）
        if face.len() >= 3 {
            let first = face[0].pos as u32;
            for i in 1..face.len() - 1 {
                let a = first as usize;
                let b = face[i].pos as usize;
                let c = face[i + 1].pos as usize;
                // 过滤退化三角形（凹多边形顶面三角化会产生共线点三角形，
                // 导致渲染乱面/怪异光照）
                let (pa, pb, pc) = (positions[a], positions[b], positions[c]);
                let n = glam::Vec3::from_array(pb) - glam::Vec3::from_array(pa);
                let m = glam::Vec3::from_array(pc) - glam::Vec3::from_array(pa);
                if n.cross(m).length_squared() < 1e-16 {
                    continue;
                }
                indices.push(a as u32);
                indices.push(b as u32);
                indices.push(c as u32);
            }
        }
    }
    MeshData::new(positions, indices)
}

/// 把 truck Solid 网格化（容差 0.1 mm，平面/简单曲面足够）。
/// 不做属性合并（truck 的 `put_together_same_attrs` 对旋转体法线有 NaN bug）；
/// 布尔路径自行焊接顶点（见 `mesh_bool::boolean`）。
fn tessellate_solid(solid: &Solid) -> MeshData {
    let mesh = solid.triangulation(0.1).to_polygon();
    polygon_mesh_to_data(&mesh)
}

/// 便捷：2D 点 → 3D（供测试）。
#[allow(dead_code)]
fn p2d(region: &Region2D, plane: &PlaneDef) -> Vec<[f64; 3]> {
    region
        .outer
        .iter()
        .map(|p| plane.to_world(p[0], p[1]))
        .collect()
}

#[allow(dead_code)]
fn area2(ring: &[Point2]) -> f64 {
    signed_area(ring)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_snapshot_shares_immutable_geometry_and_keeps_independent_ids_and_lifetimes() {
        let mut kernel = TruckKernel::new();
        let mesh = MeshData::new(
            vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            vec![0, 1, 2],
        );
        let id = kernel.insert_mesh(mesh.clone());
        let mut worker = kernel.clone();
        assert!(Arc::ptr_eq(&kernel.shapes[&id], &worker.shapes[&id]));
        let moved = worker
            .transform(id, &Mat4::from_translation(glam::vec3(5., 0., 0.)))
            .unwrap();
        worker.delete(id);
        assert_eq!(kernel.tessellate(id).unwrap().positions, mesh.positions);
        assert!(kernel.tessellate(moved).is_err());
        assert_eq!(worker.tessellate(moved).unwrap().positions[0], [5., 0., 0.]);
        kernel.delete(id);
        assert!(worker.tessellate(moved).is_ok());
    }

    fn mesh_volume(mesh: &MeshData) -> f64 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let a = DVec3::from_array(mesh.positions[t[0] as usize].map(f64::from));
                let b = DVec3::from_array(mesh.positions[t[1] as usize].map(f64::from));
                let c = DVec3::from_array(mesh.positions[t[2] as usize].map(f64::from));
                a.dot(b.cross(c)) / 6.0
            })
            .sum::<f64>()
            .abs()
    }

    fn assert_watertight(mesh: &MeshData) {
        let mut mesh = mesh.clone();
        mesh.weld(1e-4);
        let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
        for tri in mesh.indices.chunks_exact(3) {
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *counts.entry(edge).or_default() += 1;
            }
        }
        assert!(!counts.is_empty());
        assert!(
            counts.values().all(|&count| count == 2),
            "non-manifold edges: {}",
            counts.values().filter(|&&count| count != 2).count()
        );
    }

    fn circle_chain(
        center: DVec3,
        axis: DVec3,
        radius: f64,
        count: usize,
        inner: bool,
    ) -> Vec<EdgeCutterSegment> {
        let axis = axis.normalize();
        let seed = if axis.x.abs() < 0.8 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let u = (seed - axis * seed.dot(axis)).normalize();
        let v = axis.cross(u).normalize();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let a0 = std::f64::consts::TAU * i as f64 / count as f64;
            let a1 = std::f64::consts::TAU * (i + 1) as f64 / count as f64;
            let am = (a0 + a1) * 0.5;
            let p0 = center + (u * a0.cos() + v * a0.sin()) * radius;
            let p1 = center + (u * a1.cos() + v * a1.sin()) * radius;
            let radial = u * am.cos() + v * am.sin();
            let side = if inner { -radial } else { radial };
            // 模拟拾取输出的不稳定顺序：交替交换端点和 n1/n2。
            let (a, b) = if i % 2 == 0 { (p0, p1) } else { (p1, p0) };
            let (n1, n2) = if i % 3 == 0 {
                (side, axis)
            } else {
                (axis, side)
            };
            out.push(EdgeCutterSegment {
                a: a.to_array(),
                b: b.to_array(),
                n1: n1.to_array(),
                n2: n2.to_array(),
            });
        }
        out.rotate_left(count / 3);
        out
    }

    fn rounded_rect_points(
        width: f64,
        height: f64,
        radius: f64,
        arc_steps: usize,
    ) -> Vec<[f64; 2]> {
        let hx = width / 2.0;
        let hy = height / 2.0;
        let corners = [
            ([hx - radius, hy - radius], 0.0),
            ([-hx + radius, hy - radius], 90.0),
            ([-hx + radius, -hy + radius], 180.0),
            ([hx - radius, -hy + radius], 270.0),
        ];
        let mut points = Vec::new();
        for (corner_index, (center, start_deg)) in corners.iter().enumerate() {
            for step in 0..=arc_steps {
                if corner_index == corners.len() - 1 && step == arc_steps {
                    continue; // 360° 端点与首点重复。
                }
                let angle = (start_deg + 90.0 * step as f64 / arc_steps as f64).to_radians();
                points.push([
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ]);
            }
        }
        points
    }

    fn rounded_rect_chain(
        center: DVec3,
        width: f64,
        height: f64,
        radius: f64,
        arc_steps: usize,
    ) -> Vec<EdgeCutterSegment> {
        let points = rounded_rect_points(width, height, radius, arc_steps);
        let axis = DVec3::Z;
        let mut segments = Vec::with_capacity(points.len());
        for i in 0..points.len() {
            let p0 = center + DVec3::new(points[i][0], points[i][1], 0.0);
            let next = points[(i + 1) % points.len()];
            let p1 = center + DVec3::new(next[0], next[1], 0.0);
            let tangent = (p1 - p0).normalize();
            // CCW 内轮廓的左侧是空腔，侧壁外法线指向左侧。
            let side = axis.cross(tangent).normalize();
            let (a, b) = if i % 2 == 0 { (p0, p1) } else { (p1, p0) };
            let (n1, n2) = if i % 3 == 0 {
                (side, axis)
            } else {
                (axis, side)
            };
            segments.push(EdgeCutterSegment {
                a: a.to_array(),
                b: b.to_array(),
                n1: n1.to_array(),
                n2: n2.to_array(),
            });
        }
        let rotate = segments.len() / 4;
        segments.rotate_left(rotate);
        segments
    }

    fn rect_region() -> Region2D {
        // local coords (0,0)-(10,0)-(10,5)-(0,5), CCW
        Region2D::new(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]])
    }

    fn front_plane() -> PlaneDef {
        // Front (XY) plane: normal +Z
        PlaneDef::from_normal([0.0, 0.0, 1.0])
    }

    #[test]
    fn extrude_produces_brep_and_mesh() {
        let mut k = TruckKernel::new();
        let id = k
            .extrude_region(&rect_region(), &front_plane(), 20.0)
            .unwrap();
        let mesh = k.tessellate(id).unwrap();
        assert!(
            mesh.triangle_count() >= 10,
            "tris={}",
            mesh.triangle_count()
        );
        let (min, max) = mesh.bounds().unwrap();
        // extrude direction +Z: z in [0, 20]
        assert!((min.z - 0.0).abs() < 1e-3, "min={min:?}");
        assert!((max.z - 20.0).abs() < 1e-3, "max={max:?}");
        assert!((max.x - 10.0).abs() < 1e-3);
        assert!((max.y - 5.0).abs() < 1e-3);
    }

    #[test]
    fn complex_region_extrude_ignores_duplicate_and_collinear_boundary_points() {
        let region = Region2D {
            outer: vec![
                [0.0, 0.0],
                [5.0, 0.0],
                [10.0, 0.0],
                [10.0, 0.0],
                [14.0, 0.0],
                [14.0, 12.0],
                [8.0, 12.0],
                [8.0, 7.0],
                [6.0, 7.0],
                [6.0, 12.0],
                [0.0, 12.0],
                [0.0, 4.0],
                [0.0, 0.0],
            ],
            holes: vec![vec![
                [2.0, 2.0],
                [2.0, 5.0],
                [3.5, 5.0],
                [5.0, 5.0],
                [5.0, 2.0],
            ]],
        };
        let mut kernel = TruckKernel::new();
        let shape = kernel
            .extrude_region(&region, &front_plane(), 8.0)
            .expect("cleaned complex region should extrude");
        let mesh = kernel.tessellate(shape).unwrap();
        assert!(mesh.triangle_count() > 20);
        assert_watertight(&mesh);
    }

    #[test]
    fn revolve_produces_solid() {
        let mut k = TruckKernel::new();
        // revolve around local X axis ((0,0)-(1,0)) by 90 deg:
        // the 5mm-wide rect (v in [0,5]) maps to z in [0,5]
        let axis = Axis2D {
            point: [0.0, 0.0],
            dir: [1.0, 0.0],
        };
        let id = k
            .revolve_region(&rect_region(), &front_plane(), &axis, 90.0)
            .unwrap();
        let mesh = k.tessellate(id).unwrap();
        assert!(mesh.triangle_count() > 0);
        let (min, max) = mesh.bounds().unwrap();
        assert!(max.z > 4.0, "max.z={}", max.z);
        assert!(min.z >= -1e-3);
    }

    #[test]
    fn fillet_profile_is_tangent_for_acute_and_obtuse_edges() {
        for angle in [30.0_f64, 60.0, 90.0, 120.0, 150.0] {
            let alpha = angle.to_radians();
            let t1 = DVec3::X;
            let t2 = DVec3::new(alpha.cos(), alpha.sin(), 0.0);
            let n1 = -DVec3::Y;
            let n2 = DVec3::new(-alpha.sin(), alpha.cos(), 0.0);
            let r = 10.0;
            let center = (t1 + t2).normalize() * (r / (alpha * 0.5).sin());
            let q1 = t1 * (r / (alpha * 0.5).tan());
            let q2 = t2 * (r / (alpha * 0.5).tan());
            for tangent in [DVec3::Z, -DVec3::Z] {
                let (u, v, polygon) =
                    edge_cutter_profile(DVec3::ZERO, tangent, n1, n2, None, Some(r), 2.0, None)
                        .unwrap();
                let arc: Vec<_> = polygon
                    .iter()
                    .map(|p| u * p[0] + v * p[1])
                    .filter(|p| ((*p - center).length() - r).abs() < 1e-8)
                    .collect();
                assert_eq!(arc.len(), polygon.len() - 1, "angle {angle}");
                assert!(arc.iter().any(|p| (*p - q1).length() < 1e-8));
                assert!(arc.iter().any(|p| (*p - q2).length() < 1e-8));
                assert!((q1 - center).dot(t1).abs() < 1e-8);
                assert!((q2 - center).dot(t2).abs() < 1e-8);
                assert!(arc.iter().all(|p| p.dot(n1) <= 1e-8 && p.dot(n2) <= 1e-8));
            }
        }
    }

    #[test]
    fn slanted_prism_edge_treatments_remove_only_the_expected_wedge() {
        for angle in [30.0_f64, 60.0, 90.0, 120.0, 150.0] {
            let alpha = angle.to_radians();
            let side = DVec3::new(alpha.cos(), alpha.sin(), 0.0) * 120.0;
            let mut k = TruckKernel::new();
            let region = Region2D::new(vec![
                [0.0, 0.0],
                [120.0, 0.0],
                [120.0 + side.x, side.y],
                [side.x, side.y],
            ]);
            let target = k.extrude_region(&region, &front_plane(), 200.0).unwrap();
            let before = mesh_volume(&k.tessellate(target).unwrap());
            for fillet in [false, true] {
                let cutter = k
                    .edge_cutter(
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0, 200.0],
                        [0.0, -1.0, 0.0],
                        [-alpha.sin(), alpha.cos(), 0.0],
                        (!fillet).then_some(10.0),
                        fillet.then_some(10.0),
                    )
                    .unwrap();
                assert_watertight(&cutter);
                let cutter_id = k.insert_mesh(cutter);
                let result = k.boolean_subtract(target, cutter_id).unwrap();
                let mesh = k.tessellate(result).unwrap();
                assert_watertight(&mesh);
                let removed = before - mesh_volume(&mesh);
                let theta = std::f64::consts::PI - alpha;
                let expected = 200.0
                    * 100.0
                    * if fillet {
                        (theta / 2.0).tan() - theta / 2.0
                    } else {
                        0.5 * alpha.sin()
                    };
                assert!(
                    (removed - expected).abs() < expected * 0.012,
                    "angle={angle}, fillet={fillet}, removed={removed}, expected={expected}"
                );
            }
        }
    }

    #[test]
    fn circular_hole_fillet_is_one_watertight_ring_and_reusable_csg() {
        let mut k = TruckKernel::new();
        let center = DVec3::new(10.0, 10.0, 6.0);
        let segments = circle_chain(center, DVec3::Z, 2.5, 72, true);
        let cutter = k
            .circular_edge_cutter(&segments, None, Some(0.6))
            .unwrap()
            .expect("closed circular chain");
        assert_watertight(&cutter);
        assert!(cutter.triangle_count() > 120 * 16);

        let mut hole = Vec::new();
        for i in (0..72).rev() {
            let angle = std::f64::consts::TAU * i as f64 / 72.0;
            hole.push([10.0 + 2.5 * angle.cos(), 10.0 + 2.5 * angle.sin()]);
        }
        let block_with_hole = Region2D {
            outer: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]],
            holes: vec![hole],
        };
        let target = k
            .extrude_region(&block_with_hole, &front_plane(), 6.0)
            .unwrap();
        let before = mesh_volume(&k.tessellate(target).unwrap());
        let cutter_id = k.insert_mesh(cutter);
        let result = k.boolean_subtract(target, cutter_id).unwrap();
        let result_mesh = k.tessellate(result).unwrap();
        let after = mesh_volume(&result_mesh);
        assert!(after < before - 1.0, "before={before}, after={after}");
        assert_watertight(&result_mesh);

        // 结果仍是合法流形，可继续参与后续布尔建模。
        let probe = k
            .extrude_region(
                &Region2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
                &front_plane(),
                1.0,
            )
            .unwrap();
        assert!(k.boolean_subtract(result, probe).is_ok());
    }

    #[test]
    fn rounded_rect_inner_fillet_uses_one_continuous_watertight_sweep() {
        let center = DVec3::new(25.0, 20.0, 6.0);
        let segments = rounded_rect_chain(center, 24.0, 18.0, 4.0, 12);
        let mut k = TruckKernel::new();
        assert!(k
            .circular_edge_cutter(&segments, None, Some(1.5))
            .unwrap()
            .is_none());
        let cutter = k
            .closed_planar_edge_cutter(&segments, None, Some(1.5), false)
            .unwrap()
            .expect("rounded rectangle should use continuous sweep");
        assert_watertight(&cutter);

        let hole: Vec<[f64; 2]> = rounded_rect_points(24.0, 18.0, 4.0, 12)
            .into_iter()
            .map(|p| [p[0] + 25.0, p[1] + 20.0])
            .collect();
        let region = Region2D {
            outer: vec![[0.0, 0.0], [50.0, 0.0], [50.0, 40.0], [0.0, 40.0]],
            holes: vec![hole],
        };
        let target = k.extrude_region(&region, &front_plane(), 6.0).unwrap();
        let before = mesh_volume(&k.tessellate(target).unwrap());
        let cutter_id = k.insert_mesh(cutter);
        let result = k.boolean_subtract(target, cutter_id).unwrap();
        let result_mesh = k.tessellate(result).unwrap();
        assert_watertight(&result_mesh);
        let after = mesh_volume(&result_mesh);
        assert!(after < before - 1.0, "before={before}, after={after}");
    }

    #[test]
    fn sharp_outer_rectangle_does_not_use_smooth_sweep() {
        let points = [
            DVec3::new(-5.0, -4.0, 3.0),
            DVec3::new(5.0, -4.0, 3.0),
            DVec3::new(5.0, 4.0, 3.0),
            DVec3::new(-5.0, 4.0, 3.0),
        ];
        let mut segments = Vec::new();
        for i in 0..points.len() {
            let next = (i + 1) % points.len();
            let tangent = (points[next] - points[i]).normalize();
            let side = tangent.cross(DVec3::Z).normalize();
            segments.push(EdgeCutterSegment {
                a: points[i].to_array(),
                b: points[next].to_array(),
                n1: DVec3::Z.to_array(),
                n2: side.to_array(),
            });
        }
        let mut k = TruckKernel::new();
        assert!(k
            .closed_planar_edge_cutter(&segments, None, Some(1.0), false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn circular_cutter_accepts_normals_from_real_tessellated_hole_rim() {
        let mut hole = Vec::new();
        for i in (0..72).rev() {
            let angle = std::f64::consts::TAU * i as f64 / 72.0;
            hole.push([10.0 + 2.5 * angle.cos(), 10.0 + 2.5 * angle.sin()]);
        }
        let region = Region2D {
            outer: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]],
            holes: vec![hole],
        };
        let mut k = TruckKernel::new();
        let target = k.extrude_region(&region, &front_plane(), 6.0).unwrap();
        let mut mesh = k.tessellate(target).unwrap();
        mesh.weld(1e-5);
        let chains = mesh.sharp_edge_chains(25.0, 20.0);
        let chain = chains
            .iter()
            .find(|chain| {
                chain.len() >= 64
                    && chain.iter().all(|(a, b)| {
                        [*a, *b].iter().all(|&i| {
                            let p = DVec3::from_array(mesh.positions[i].map(f64::from));
                            (p.z - 6.0).abs() < 1e-4
                                && ((p.x - 10.0).hypot(p.y - 10.0) - 2.5).abs() < 1e-3
                        })
                    })
            })
            .expect("top inner circular rim");

        let mut segments = Vec::with_capacity(chain.len());
        for &(a, b) in chain {
            let mut normals = Vec::new();
            for tri in mesh.indices.chunks_exact(3) {
                if !(tri.contains(&(a as u32)) && tri.contains(&(b as u32))) {
                    continue;
                }
                let pa = DVec3::from_array(mesh.positions[tri[0] as usize].map(f64::from));
                let pb = DVec3::from_array(mesh.positions[tri[1] as usize].map(f64::from));
                let pc = DVec3::from_array(mesh.positions[tri[2] as usize].map(f64::from));
                let n = (pb - pa).cross(pc - pa);
                if n.length_squared() > 1e-18 {
                    normals.push(n.normalize());
                }
                if normals.len() == 2 {
                    break;
                }
            }
            assert_eq!(normals.len(), 2);
            segments.push(EdgeCutterSegment {
                a: mesh.positions[a].map(f64::from),
                b: mesh.positions[b].map(f64::from),
                n1: normals[0].to_array(),
                n2: normals[1].to_array(),
            });
        }
        let cutter = k
            .circular_edge_cutter(&segments, None, Some(0.6))
            .unwrap()
            .expect("real tessellated rim should fit a circle");
        assert_watertight(&cutter);
    }

    #[test]
    fn circular_chamfer_supports_arbitrary_axis_and_rejects_open_chain() {
        let mut k = TruckKernel::new();
        let axis = DVec3::new(1.0, 2.0, 3.0).normalize();
        let mut segments = circle_chain(DVec3::new(4.0, -2.0, 7.0), axis, 3.0, 48, true);
        let cutter = k
            .circular_edge_cutter(&segments, Some(0.5), None)
            .unwrap()
            .expect("arbitrary-axis circle");
        assert_watertight(&cutter);

        segments.pop();
        assert!(k
            .circular_edge_cutter(&segments, Some(0.5), None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn circular_outer_edge_rejects_profile_crossing_axis() {
        let mut k = TruckKernel::new();
        let segments = circle_chain(DVec3::ZERO, DVec3::Z, 1.0, 36, false);
        let error = k
            .circular_edge_cutter(&segments, Some(1.5), None)
            .unwrap_err();
        assert!(error.to_string().contains("too large"), "{error}");
    }

    #[test]
    fn boolean_union_of_two_extrusions() {
        let mut k = TruckKernel::new();
        let a = k
            .extrude_region(&rect_region(), &front_plane(), 20.0)
            .unwrap();
        let mat = Mat4::from_translation(glam::Vec3::new(5.0, 0.0, 0.0));
        let b = k.transform(a, &mat).unwrap();
        let u = k.boolean_union(a, b).unwrap();
        // Manifold's checked output remains a mesh; a lossy faceted B-rep
        // round-trip used to reject small edges in subsequent fillets.
        assert!(matches!(k.get(u).unwrap(), KernelShape::Mesh(_)));
        let mesh = k.tessellate(u).unwrap();
        assert!(mesh.triangle_count() > 0);
        let (min, max) = mesh.bounds().unwrap();
        assert!((max.x - 15.0).abs() < 0.1, "max.x={}", max.x);
        assert!((max.z - 20.0).abs() < 0.1);
    }

    #[test]
    fn boolean_mesh_remains_sectionable() {
        let mut k = TruckKernel::new();
        let a = k
            .extrude_region(&rect_region(), &front_plane(), 20.0)
            .unwrap();
        let b = k
            .transform(a, &Mat4::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)))
            .unwrap();
        let result = k.boolean_union(a, b).unwrap();
        let mesh = k.tessellate(result).unwrap();
        let (positive, negative) =
            crate::mesh_bool::split_by_plane(&mesh, [7.5, 0.0, 0.0], [1.0, 0.0, 0.0])
                .expect("faceted B-rep tessellation should remain a valid section operand");
        assert!(!positive.is_empty());
        assert!(!negative.is_empty());
        assert_watertight(&positive);
        assert_watertight(&negative);
    }

    #[test]
    fn step_export_roundtrip_text() {
        let mut k = TruckKernel::new();
        let id = k
            .extrude_region(&rect_region(), &front_plane(), 20.0)
            .unwrap();
        let path = std::env::temp_dir().join("s33d-test-export.step");
        let p = path.to_str().unwrap();
        k.export_step(id, p).unwrap();
        let text = std::fs::read_to_string(p).unwrap();
        assert!(text.contains("ISO-10303-21;"));
        assert!(text.contains("MANIFOLD_SOLID_BREP"));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn import_normalizes_above_plane() {
        // 偏移的 mesh：导入后应底面贴 Y=0、XZ 居中
        let mut meshes = vec![MeshData::new(
            vec![[10.0, -5.0, 20.0], [20.0, -5.0, 20.0], [10.0, 5.0, 30.0]],
            vec![0, 1, 2],
        )];
        normalize_imported(&mut meshes, false);
        let (min, max) = meshes[0].bounds().unwrap();
        assert!((min.y).abs() < 1e-3, "min.y={}", min.y);
        assert!(((min.x + max.x) / 2.0).abs() < 1e-3, "x center");
        assert!(((min.z + max.z) / 2.0).abs() < 1e-3, "z center");
        // 高度保持
        assert!((max.y - 10.0).abs() < 1e-3, "max.y={}", max.y);
    }

    #[test]
    fn step_zup_to_yup_conversion() {
        // Z-up 点集（底面 z=0）：(1,0,0),(0,2,0),(0,0,3) → Y-up
        let mut meshes = vec![MeshData::new(
            vec![[1.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 3.0]],
            vec![0, 1, 2],
        )];
        normalize_imported(&mut meshes, true);
        // 转换：(1,0,0)→(1,0,0)；(0,2,0)→(0,0,-2)；(0,0,3)→(0,3,0)
        // 即 Z-up 底面(z=0) → Y-up 底面(y=0)，高度 3 保持
        let (min, max) = meshes[0].bounds().unwrap();
        assert!((min.y).abs() < 1e-3, "min.y={}", min.y);
        assert!((max.y - 3.0).abs() < 1e-3, "max.y={}", max.y);
        // XZ 居中（转换后 z ∈ [-2,0]，x ∈ [0,1]）
        assert!(((min.x + max.x) / 2.0).abs() < 1e-3);
        assert!(((min.z + max.z) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn import_keeps_relative_positions() {
        // 两个 mesh 相对偏移：整体移动后相对位置不变
        let a = MeshData::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![0, 1, 2],
        );
        let b = MeshData::new(
            vec![[5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]],
            vec![0, 1, 2],
        );
        let mut meshes = vec![a, b];
        normalize_imported(&mut meshes, false);
        let ca = meshes[0].bounds().unwrap();
        let cb = meshes[1].bounds().unwrap();
        // 相对 X 偏移仍为 5（中心差）
        let dx = ((cb.0.x + cb.1.x) - (ca.0.x + ca.1.x)) / 2.0;
        assert!((dx - 5.0).abs() < 1e-3, "dx={dx}");
        // 两者底面都贴 Y=0
        assert!((ca.0.y).abs() < 1e-3);
        assert!((cb.0.y).abs() < 1e-3);
    }

    #[test]
    fn bounds_covers_all_shapes() {
        let mut k = TruckKernel::new();
        k.extrude_region(&rect_region(), &front_plane(), 20.0)
            .unwrap();
        let (min, max) = k.bounds().unwrap();
        assert!(min.z >= -1e-3);
        assert!((max.z - 20.0).abs() < 1e-3);
    }

    #[test]
    fn retain_shapes_reclaims_only_unreferenced_geometry() {
        let mut kernel = TruckKernel::new();
        let expired = kernel.insert_mesh(MeshData::default());
        let retained = kernel.insert_mesh(MeshData::default());
        assert_eq!(kernel.shape_count(), 2);

        let removed = kernel.retain_shapes(&HashSet::from([retained]));
        assert_eq!(removed, 1);
        assert_eq!(kernel.shape_count(), 1);
        assert!(kernel.tessellate(expired).is_err());
        assert!(kernel.tessellate(retained).is_ok());
    }
}
