//! 基于 manifold-rust 的网格布尔运算。
//!
//! truck（B-rep 内核）目前没有布尔 API，因此布尔运算在网格层面完成
//! （与 Super3D 原型的 three-bvh-csg 方案一致）。`GeometryKernel` 接口
//! 保持 OCCT 语义，未来替换真 B-rep 内核时无需改动上层。

use crate::mesh::MeshData;
use crate::{KernelError, Result};
use manifold_rust::linalg::{Mat3x4, Vec2, Vec3};
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{Error as MfError, MeshGL, Polygons};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

fn to_meshgl(m: &MeshData) -> MeshGL {
    let mut gl = MeshGL::default();
    gl.num_prop = 3;
    for p in &m.positions {
        gl.vert_properties.extend_from_slice(p);
    }
    gl.tri_verts.extend_from_slice(&m.indices);
    gl
}

fn from_meshgl(gl: &MeshGL) -> MeshData {
    let positions: Vec<[f32; 3]> = gl
        .vert_properties
        .chunks_exact(gl.num_prop as usize)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let indices = gl.tri_verts.clone();
    MeshData::new(positions, indices)
}

/// 网格布尔运算。输入 mesh 需为闭合流形；输出为新 MeshData。
/// 运算前先焊接重复顶点（truck tessellation 的边界重复点）。
pub fn boolean(a: &MeshData, b: &MeshData, op: BoolOp) -> Result<MeshData> {
    if a.is_empty() || b.is_empty() {
        return Err(KernelError::Boolean("empty operand mesh".into()));
    }
    let mut a = a.clone();
    let mut b = b.clone();
    a.weld(1e-4);
    b.weld(1e-4);
    let ma = Manifold::from_mesh_gl(&to_meshgl(&a));
    let mb = Manifold::from_mesh_gl(&to_meshgl(&b));
    if ma.status() != MfError::NoError {
        return Err(KernelError::Boolean(format!(
            "first operand not manifold: {:?}",
            ma.status()
        )));
    }
    if mb.status() != MfError::NoError {
        return Err(KernelError::Boolean(format!(
            "second operand not manifold: {:?}",
            mb.status()
        )));
    }
    let res = match op {
        BoolOp::Union => ma.union(&mb),
        BoolOp::Subtract => ma.difference(&mb),
        BoolOp::Intersect => ma.intersection(&mb),
    };
    if res.status() != MfError::NoError {
        return Err(KernelError::Boolean(format!(
            "boolean failed: {:?}",
            res.status()
        )));
    }
    let gl = res.get_mesh_gl(-1);
    if gl.tri_verts.is_empty() || gl.vert_properties.is_empty() {
        return Err(KernelError::Boolean(
            "boolean result is empty; operands may not overlap or the target was fully removed"
                .into(),
        ));
    }
    let out = from_meshgl(&gl);
    if out
        .positions
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(KernelError::Boolean(
            "boolean result contains non-finite geometry".into(),
        ));
    }
    Ok(out)
}

/// 把位于子午面内的闭合截面绕任意轴旋转 360°，生成单个连续环形网格。
///
/// `profile` 中每个点为 `[rho, z]`：`rho` 是到旋转轴的距离（必须 > 0），
/// `z` 是沿轴方向相对 `axis_origin` 的坐标。截面须为 CCW，首点无需重复。
/// 显式传入环向段数，避免全局 quality 配置影响小半径圆边的平滑度。
pub fn revolve_profile(
    profile: &[[f64; 2]],
    axis_origin: [f64; 3],
    axis_dir: [f64; 3],
    radial_hint: [f64; 3],
    circular_segments: usize,
) -> Result<MeshData> {
    if profile.len() < 3 {
        return Err(KernelError::Geometry(
            "revolve profile needs at least three points".into(),
        ));
    }
    if profile
        .iter()
        .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] <= 1e-6)
    {
        return Err(KernelError::Geometry(
            "revolve profile must be finite and stay clear of the axis".into(),
        ));
    }

    let axis = glam::DVec3::from_array(axis_dir);
    if axis.length_squared() < 1e-18 {
        return Err(KernelError::Geometry("revolve axis has zero length".into()));
    }
    let axis = axis.normalize();
    let hint = glam::DVec3::from_array(radial_hint);
    let radial = hint - axis * hint.dot(axis);
    if radial.length_squared() < 1e-18 {
        return Err(KernelError::Geometry(
            "revolve radial hint is parallel to the axis".into(),
        ));
    }
    let radial = radial.normalize();
    let tangent = axis.cross(radial).normalize();

    let polygons: Polygons = vec![profile.iter().map(|p| Vec2::new(p[0], p[1])).collect()];
    let revolved = Manifold::revolve(&polygons, circular_segments.max(3) as i32, 360.0);
    if revolved.status() != MfError::NoError {
        return Err(KernelError::Geometry(format!(
            "revolve cutter failed: {:?}",
            revolved.status()
        )));
    }

    let mf = |v: glam::DVec3| Vec3::new(v.x, v.y, v.z);
    // canonical revolve: X=radial, Y=tangent, Z=axis
    let transform = Mat3x4::from_cols(
        mf(radial),
        mf(tangent),
        mf(axis),
        Vec3::new(axis_origin[0], axis_origin[1], axis_origin[2]),
    );
    let revolved = revolved.transform(&transform);
    if revolved.status() != MfError::NoError {
        return Err(KernelError::Geometry(format!(
            "revolve cutter transform failed: {:?}",
            revolved.status()
        )));
    }
    Ok(from_meshgl(&revolved.get_mesh_gl(-1)))
}

/// 沿经过 `origin`、法线为 `normal` 的平面镜像网格。
///
/// 镜像会改变坐标系手性，因此同时翻转每个三角形的绕序，保持外法线朝外。
pub fn mirror(m: &MeshData, origin: [f64; 3], normal: [f64; 3]) -> Result<MeshData> {
    let origin = glam::DVec3::from_array(origin);
    let normal = glam::DVec3::from_array(normal);
    if !origin.is_finite() || !normal.is_finite() || normal.length_squared() <= 1e-20 {
        return Err(KernelError::Geometry("invalid mirror plane".into()));
    }
    let normal = normal.normalize();
    let positions = m
        .positions
        .iter()
        .map(|position| {
            let point =
                glam::DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64);
            let mirrored = point - 2.0 * (point - origin).dot(normal) * normal;
            [mirrored.x as f32, mirrored.y as f32, mirrored.z as f32]
        })
        .collect();
    let mut indices = m.indices.clone();
    for triangle in indices.chunks_exact_mut(3) {
        triangle.swap(1, 2);
    }
    Ok(MeshData::new(positions, indices))
}

/// 沿经过 `origin`、法线为 `normal` 的平面切分闭合网格。
///
/// 返回 `(法线正向侧, 反向侧)` 两个闭合网格；两侧的切口都带平面封盖
/// （manifold 的 `split_by_plane` 语义）。若某侧为空则返回空 `MeshData`。
pub fn split_by_plane(
    m: &MeshData,
    origin: [f64; 3],
    normal: [f64; 3],
) -> Result<(MeshData, MeshData)> {
    let origin = glam::DVec3::from_array(origin);
    let normal = glam::DVec3::from_array(normal);
    if !origin.is_finite() || !normal.is_finite() || normal.length_squared() <= 1e-20 {
        return Err(KernelError::Geometry("invalid split plane".into()));
    }
    if m.is_empty() {
        return Err(KernelError::Boolean("empty operand mesh".into()));
    }
    let normal = normal.normalize();
    let offset = origin.dot(normal);

    let mut welded = m.clone();
    welded.weld(1e-4);
    let manifold = Manifold::from_mesh_gl(&to_meshgl(&welded));
    if manifold.status() != MfError::NoError {
        return Err(KernelError::Boolean(format!(
            "split operand not manifold: {:?}",
            manifold.status()
        )));
    }

    let (positive, negative) =
        manifold.split_by_plane(Vec3::new(normal.x, normal.y, normal.z), offset);
    let positive = checked_half_mesh(positive, "positive half")?;
    let negative = checked_half_mesh(negative, "negative half")?;
    if positive.is_empty() && negative.is_empty() {
        return Err(KernelError::Boolean(
            "split produced no geometry on either side of the plane".into(),
        ));
    }
    Ok((positive, negative))
}

/// 把 `split_by_plane` 的某一半转成 MeshData 并校验数值。
fn checked_half_mesh(half: Manifold, name: &str) -> Result<MeshData> {
    if half.is_empty() {
        return Ok(MeshData::default());
    }
    if half.status() != MfError::NoError {
        return Err(KernelError::Boolean(format!(
            "split {name} failed: {:?}",
            half.status()
        )));
    }
    let gl = half.get_mesh_gl(-1);
    if gl.tri_verts.is_empty() || gl.vert_properties.is_empty() {
        return Ok(MeshData::default());
    }
    let out = from_meshgl(&gl);
    if out
        .positions
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(KernelError::Boolean(format!(
            "split {name} contains non-finite geometry"
        )));
    }
    Ok(out)
}

/// 沿平面切分网格，再把两侧结果分解为独立连通件。
///
/// 用于 Split 建模操作：一个 object 可能被切成两个或更多（多连通体 /
/// 平面同时切到多个分离部分）独立 3D objects。返回顺序为先法线正向侧
/// 再反向侧（各自内部为 manifold decompose 顺序）。
pub fn split_mesh_components(
    m: &MeshData,
    origin: [f64; 3],
    normal: [f64; 3],
) -> Result<Vec<MeshData>> {
    let (positive, negative) = split_by_plane(m, origin, normal)?;
    let mut raw_components: Vec<(MeshData, f64)> = Vec::new();
    for half in [positive, negative] {
        if half.is_empty() {
            continue;
        }
        let manifold = Manifold::from_mesh_gl(&to_meshgl(&half));
        if manifold.status() != MfError::NoError {
            return Err(KernelError::Boolean(format!(
                "split component not manifold: {:?}",
                manifold.status()
            )));
        }
        for part in manifold.decompose() {
            if part.is_empty() {
                continue;
            }
            if part.status() != MfError::NoError {
                return Err(KernelError::Boolean(format!(
                    "split decomposed component failed: {:?}",
                    part.status()
                )));
            }
            let volume = part.volume().abs();
            let gl = part.get_mesh_gl(-1);
            if gl.tri_verts.is_empty() || gl.vert_properties.is_empty() {
                continue;
            }
            let mesh = from_meshgl(&gl);
            if mesh
                .positions
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(KernelError::Boolean(
                    "split component contains non-finite geometry".into(),
                ));
            }
            raw_components.push((mesh, volume));
        }
    }
    // Plane/triangle near-coincidence can make manifold emit microscopic closed
    // slivers. They appear as leaking or extra faces in the viewport and should
    // not become standalone Objects. Use a scale-relative threshold so normal
    // small features remain intact.
    let total_volume: f64 = raw_components.iter().map(|(_, volume)| volume).sum();
    let minimum_component_volume = (total_volume * 1e-10).max(1e-15);
    let components: Vec<MeshData> = raw_components
        .into_iter()
        .filter(|(_, volume)| *volume > minimum_component_volume)
        .map(|(mesh, _)| mesh)
        .collect();
    if components.is_empty() {
        return Err(KernelError::Boolean(
            "split produced no solid components".into(),
        ));
    }
    Ok(components)
}

/// 返回网格中位于切平面内的三角形索引（剖面封盖面）。
///
/// `split_by_plane` 的切口封盖位于切平面内，几何法线与平面法线平行
/// （manifold 绕序使封盖法线指向所保留半侧的内部，即方向与切平面法线
/// 相反或相同取决于半侧）。这里按「三个顶点共面 + 法线与平面法线平行」
/// 做方向无关识别，避免把与平面平行但不相交的原有面（如立方体远端的
/// x=1 面）误判为封盖。渲染端按需翻转绕序。
pub fn cap_triangle_indices(m: &MeshData, origin: [f64; 3], normal: [f64; 3]) -> Vec<u32> {
    let origin = glam::DVec3::from_array(origin);
    let n = glam::DVec3::from_array(normal);
    if n.length_squared() <= 1e-20 {
        return Vec::new();
    }
    let n = n.normalize();
    let (min, max) = match m.bounds() {
        Some(bounds) => bounds,
        None => return Vec::new(),
    };
    let plane_eps = (((max - min).length() * 1e-5).max(1e-6)) as f64;
    let mut caps = Vec::new();
    for tri in m.indices.chunks_exact(3) {
        let points = [
            glam::DVec3::from_array(m.positions[tri[0] as usize].map(|c| c as f64)),
            glam::DVec3::from_array(m.positions[tri[1] as usize].map(|c| c as f64)),
            glam::DVec3::from_array(m.positions[tri[2] as usize].map(|c| c as f64)),
        ];
        let face_normal = (points[1] - points[0]).cross(points[2] - points[0]);
        if face_normal.length_squared() <= 1e-24 {
            continue;
        }
        if face_normal.normalize().dot(n).abs() <= 0.995 {
            continue;
        }
        if points.iter().any(|p| (p - origin).dot(n).abs() > plane_eps) {
            continue;
        }
        caps.extend_from_slice(tri);
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_cube_mesh(translate_x: f32) -> MeshData {
        let v: [[f32; 3]; 8] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let tris: [[u32; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let mut m = MeshData::new(
            v.iter().map(|p| [p[0] + translate_x, p[1], p[2]]).collect(),
            tris.iter().flatten().copied().collect(),
        );
        m.bounds();
        m
    }

    #[test]
    fn boolean_union_volume() {
        let a = unit_cube_mesh(0.0);
        let b = unit_cube_mesh(0.5);
        let u = boolean(&a, &b, BoolOp::Union).unwrap();
        assert_eq!(u.triangle_count() * 3, u.indices.len());
        let m = Manifold::from_mesh_gl(&to_meshgl(&u));
        assert_eq!(m.status(), MfError::NoError);
        // 1 + 1 - 0.5 = 1.5
        assert!((m.volume() - 1.5).abs() < 0.05, "volume={}", m.volume());
    }

    #[test]
    fn boolean_subtract_volume() {
        let a = unit_cube_mesh(0.0);
        let b = unit_cube_mesh(0.5);
        let d = boolean(&a, &b, BoolOp::Subtract).unwrap();
        let m = Manifold::from_mesh_gl(&to_meshgl(&d));
        assert_eq!(m.status(), MfError::NoError);
        assert!((m.volume() - 0.5).abs() < 0.05, "volume={}", m.volume());
    }

    #[test]
    fn boolean_intersect_volume() {
        let a = unit_cube_mesh(0.0);
        let b = unit_cube_mesh(0.5);
        let i = boolean(&a, &b, BoolOp::Intersect).unwrap();
        let m = Manifold::from_mesh_gl(&to_meshgl(&i));
        assert_eq!(m.status(), MfError::NoError);
        assert!((m.volume() - 0.5).abs() < 0.05, "volume={}", m.volume());
    }

    #[test]
    fn disjoint_intersection_reports_empty_result() {
        let a = unit_cube_mesh(0.0);
        let b = unit_cube_mesh(2.0);
        let error = boolean(&a, &b, BoolOp::Intersect).unwrap_err();
        assert!(error.to_string().contains("empty"), "error={error}");
    }

    #[test]
    fn mirror_uses_offset_plane_and_preserves_manifold_orientation() {
        let cube = unit_cube_mesh(0.0);
        let mirrored = mirror(&cube, [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        let (min, max) = mirrored.bounds().unwrap();
        assert!((min.x - 3.0).abs() < 1e-6, "min={min:?}");
        assert!((max.x - 4.0).abs() < 1e-6, "max={max:?}");

        let manifold = Manifold::from_mesh_gl(&to_meshgl(&mirrored));
        assert_eq!(manifold.status(), MfError::NoError);
        assert!((manifold.volume() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_rejects_degenerate_plane_normal() {
        let error = mirror(&unit_cube_mesh(0.0), [0.0; 3], [0.0; 3]).unwrap_err();
        assert!(error.to_string().contains("invalid mirror plane"));
    }

    #[test]
    fn split_by_plane_cuts_cube_into_two_closed_halves() {
        let cube = unit_cube_mesh(0.0);
        let (pos, neg) = split_by_plane(&cube, [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert!(!pos.is_empty() && !neg.is_empty());
        for half in [&pos, &neg] {
            let manifold = Manifold::from_mesh_gl(&to_meshgl(half));
            assert_eq!(manifold.status(), MfError::NoError);
            assert!(
                (manifold.volume() - 0.5).abs() < 0.05,
                "volume={}",
                manifold.volume()
            );
            assert!(half
                .positions
                .iter()
                .flatten()
                .all(|value| value.is_finite()));
        }
        let (min, max) = pos.bounds().unwrap();
        assert!((max.x - 1.0).abs() < 1e-4);
        assert!((min.x - 0.5).abs() < 1e-4);
        let (min, max) = neg.bounds().unwrap();
        assert!((max.x - 0.5).abs() < 1e-4);
        assert!((min.x - 0.0).abs() < 1e-4);
    }

    #[test]
    fn split_plane_outside_returns_whole_mesh_on_one_side() {
        let cube = unit_cube_mesh(0.0);
        let (pos, neg) = split_by_plane(&cube, [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert!(!neg.is_empty());
        assert!(pos.is_empty());
        let manifold = Manifold::from_mesh_gl(&to_meshgl(&neg));
        assert!((manifold.volume() - 1.0).abs() < 0.05);
    }

    #[test]
    fn split_mesh_components_returns_two_pieces_for_one_cut() {
        let cube = unit_cube_mesh(0.0);
        let parts = split_mesh_components(&cube, [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(parts.len(), 2);
        for part in &parts {
            let manifold = Manifold::from_mesh_gl(&to_meshgl(part));
            assert_eq!(manifold.status(), MfError::NoError);
        }
    }

    #[test]
    fn cap_triangle_indices_find_planar_cap_on_positive_half() {
        let cube = unit_cube_mesh(0.0);
        let (pos, _neg) = split_by_plane(&cube, [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        let caps = cap_triangle_indices(&pos, [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]);
        assert!(!caps.is_empty());
        assert_eq!(caps.len() % 3, 0);
        for tri in caps.chunks_exact(3) {
            for &index in tri {
                let x = pos.positions[index as usize][0];
                assert!((x - 0.5).abs() < 1e-4, "cap vertex x={x}");
            }
        }
    }
}
