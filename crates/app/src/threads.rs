//! Thread cutters and a shared desktop/MCP workflow. All geometry is millimeters.
use glam::{DMat3, DMat4, DVec3};
#[cfg(test)]
use s33d_kernel::GeometryKernel;
use s33d_kernel::{mesh_bool, MeshData, PlaneDef};
use serde::{Deserialize, Serialize};

#[path = "thread_config.rs"]
pub mod config;
#[path = "thread_ui.rs"]
mod editor;
pub use editor::panel;
#[path = "thread_surfaces.rs"]
mod surfaces;
pub(crate) use surfaces::{remaining_targets, split_band_boundaries};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub body_id: u64,
    pub face_id: u32,
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub length: f64,
    pub internal: bool,
}
impl Target {
    /// Same complete wall, regardless of axial direction or tessellation IDs.
    /// Prefer the caller's native/provenance target when merging mesh recovery.
    pub fn same_wall(&self, other: &Self) -> bool {
        let axis = DVec3::from_array(self.axis);
        let other_axis = DVec3::from_array(other.axis);
        let a = DVec3::from_array(self.origin);
        let b = DVec3::from_array(other.origin);
        let tolerance = crate::mesh_cylinders::fit_tolerance(self.radius)
            .max(crate::mesh_cylinders::fit_tolerance(other.radius));
        if self.internal != other.internal
            || [self.radius, other.radius, self.length, other.length]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.)
            || !axis.is_finite()
            || !other_axis.is_finite()
            || !a.is_finite()
            || !b.is_finite()
            || (axis.length() - 1.).abs() > 1e-5
            || (other_axis.length() - 1.).abs() > 1e-5
            || axis.dot(other_axis).abs() < 1. - 1e-5
            || (self.radius - other.radius).abs() > tolerance
            || (self.length - other.length).abs() > tolerance
        {
            return false;
        }
        let other_start = if axis.dot(other_axis) < 0. {
            b + other_axis * other.length
        } else {
            b
        };
        a.distance(other_start) <= tolerance
    }

    /// Carry a trusted cylindrical wall through an affine copy only while it
    /// remains circular. Axial scaling and reflections are valid; elliptical
    /// cross sections and sheared axes are not threadable cylinders.
    pub fn transformed(&self, body_id: u64, mat: &DMat4) -> Option<Self> {
        if mat.to_cols_array().iter().any(|v| !v.is_finite())
            || mat.x_axis.w.abs() > 1e-10
            || mat.y_axis.w.abs() > 1e-10
            || mat.z_axis.w.abs() > 1e-10
            || (mat.w_axis.w - 1.).abs() > 1e-10
            || !self.radius.is_finite()
            || !self.length.is_finite()
            || self.radius <= 0.
            || self.length <= 0.
        {
            return None;
        }
        let origin = DVec3::from_array(self.origin);
        let axis = DVec3::from_array(self.axis);
        if !origin.is_finite() || !axis.is_finite() || (axis.length() - 1.).abs() > 1e-5 {
            return None;
        }
        let axis = axis.normalize();
        let hint = if axis.x.abs() < 0.8 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let radial_u = axis.cross(hint).normalize();
        let radial_v = axis.cross(radial_u);
        let a = mat.transform_vector3(axis);
        let u = mat.transform_vector3(radial_u);
        let v = mat.transform_vector3(radial_v);
        let (axial_scale, radial_scale_u, radial_scale_v) = (a.length(), u.length(), v.length());
        if [axial_scale, radial_scale_u, radial_scale_v]
            .iter()
            .any(|s| !s.is_finite() || *s <= 1e-12)
        {
            return None;
        }
        // The fitted copy transform comes from f32 display vertices. Allow
        // their rounding error without admitting visible ellipticity/shear.
        let tolerance = 1e-5;
        if (radial_scale_u - radial_scale_v).abs() > tolerance * radial_scale_u.max(radial_scale_v)
            || a.dot(u).abs() > tolerance * axial_scale * radial_scale_u
            || a.dot(v).abs() > tolerance * axial_scale * radial_scale_v
            || u.dot(v).abs() > tolerance * radial_scale_u * radial_scale_v
        {
            return None;
        }
        let origin = mat.transform_point3(origin);
        let radius = self.radius * (radial_scale_u + radial_scale_v) * 0.5;
        let length = self.length * axial_scale;
        if !origin.is_finite()
            || !radius.is_finite()
            || !length.is_finite()
            || radius <= 0.
            || length <= 0.
        {
            return None;
        }
        Some(Self {
            body_id,
            // Keep separate inner/outer walls distinguishable. The caller may
            // assign synthetic IDs when these no longer name native faces.
            face_id: self.face_id
                ^ if (0xa000_0000..0xc000_0000).contains(&self.face_id) && mat.determinant() < 0. {
                    0x1000_0000
                } else {
                    0
                },
            origin: origin.to_array(),
            axis: (a / axial_scale).to_array(),
            radius,
            length,
            internal: self.internal,
        })
    }

    /// Match the lateral surface, excluding planar end caps and adjacent bores.
    pub fn contains_triangle(&self, mesh: &MeshData, triangle: &[u32]) -> bool {
        if triangle.len() != 3
            || !self.radius.is_finite()
            || !self.length.is_finite()
            || self.radius <= 0.
            || self.length <= 0.
        {
            return false;
        }
        let origin = DVec3::from_array(self.origin);
        let axis = DVec3::from_array(self.axis);
        if !origin.is_finite() || !axis.is_finite() || (axis.length() - 1.).abs() > 1e-5 {
            return false;
        }
        let axis = axis.normalize();
        let mut points = [DVec3::ZERO; 3];
        for (slot, &index) in triangle.iter().enumerate() {
            let Some(p) = mesh.positions.get(index as usize) else {
                return false;
            };
            let p = DVec3::from_array(p.map(f64::from));
            if !p.is_finite() {
                return false;
            }
            points[slot] = p;
        }
        // Display coordinates are f32. At a distant world origin their ULP
        // can exceed a fixed millimeter tolerance even for a perfect copy.
        // Bound that allowance by wall radius: unresolved tiny bores must not
        // absorb a neighboring wall merely because both are far from zero.
        let coordinate = points
            .iter()
            .map(|p| p.abs().max_element())
            .fold(origin.abs().max_element(), f64::max);
        let quantization = coordinate * f64::from(f32::EPSILON) * 2.;
        let mut tolerance = 1e-4_f64
            .max(self.radius * 1e-5)
            .max(quantization)
            .min(self.radius * 1e-3);
        let thread_band = (0xa000_0000..0xc000_0000).contains(&self.face_id);
        let mut axial_tolerance = tolerance;
        // Thread remnants contain Boolean-created points inside the original
        // polygon chords. Their relative chord error must survive scaling;
        // retain tighter matching for native walls and faceted imports.
        if thread_band {
            tolerance = tolerance.max(self.radius * 0.01);
        } else if (0x8000_0000..0xa000_0000).contains(&self.face_id) {
            tolerance = tolerance.max(crate::mesh_cylinders::fit_tolerance(self.radius));
            axial_tolerance = tolerance;
        }
        let normal = (points[1] - points[0]).cross(points[2] - points[0]);
        if thread_band {
            // The chord allowance must never merge a nearby inner/outer wall
            // of a thin tube with the retained band.
            let center = (points[0] + points[1] + points[2]) / 3. - origin;
            let radial = center - axis * center.dot(axis);
            let winding = if self.face_id >= 0xb000_0000 { -1. } else { 1. };
            let outward = normal.dot(radial) * winding;
            if if self.internal {
                outward >= 0.
            } else {
                outward <= 0.
            } {
                return false;
            }
        }
        let mut z_min = f64::INFINITY;
        let mut z_max = f64::NEG_INFINITY;
        for p in points {
            let relative = p - origin;
            let z = relative.dot(axis);
            z_min = z_min.min(z);
            z_max = z_max.max(z);
            let radial = relative - axis * z;
            if (radial.length() - self.radius).abs() > tolerance {
                return false;
            }
        }
        // Irregular tessellation of a narrow spline cylinder can tilt a
        // triangle's chord normal along the axis despite all vertices lying
        // on the wall. End caps are distinguished by constant axial position,
        // not by assuming the display triangle normal is exactly radial.
        z_max.min(self.length) - z_min.max(0.) > axial_tolerance && normal.length_squared() > 1e-20
    }

    /// Certify that a trusted primitive/copy seed still describes a complete
    /// lateral wall in the current mesh, rather than a lone coplanar facet.
    pub fn matches_full_wall(&self, mesh: &MeshData) -> bool {
        if !self.radius.is_finite()
            || !self.length.is_finite()
            || self.radius <= 0.
            || self.length <= 0.
        {
            return false;
        }
        let expected = 2. * std::f64::consts::PI * self.radius * self.length;
        if !expected.is_finite() || expected <= 0. {
            return false;
        }
        let triangles: Vec<_> = mesh
            .indices
            .chunks_exact(3)
            .filter(|triangle| self.contains_triangle(mesh, triangle))
            .collect();
        let area: f64 = triangles
            .iter()
            .map(|triangle| self.clipped_triangle_area(mesh, triangle))
            .sum();
        area.is_finite()
            && (area / expected - 1.).abs() < 0.02
            && self.covers_circumference(mesh, &triangles)
    }

    /// Area tolerance allows polygon chord error, not a missing strip or a
    /// small breakthrough. Check every axial slab bounded by wall vertices:
    /// a midpoint intersects the same triangle edges throughout that slab.
    fn covers_circumference(&self, mesh: &MeshData, triangles: &[&[u32]]) -> bool {
        use std::f64::consts::{PI, TAU};
        let axis = DVec3::from_array(self.axis);
        let origin = DVec3::from_array(self.origin);
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);
        let points: Vec<[_; 3]> = triangles
            .iter()
            .map(|tri| {
                [tri[0], tri[1], tri[2]]
                    .map(|i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)) - origin)
            })
            .collect();
        // A transformed f32 end ring is planar only up to coordinate ULPs.
        // Do not interpret that rounding strip as a physical breakthrough.
        let coordinate = points
            .iter()
            .flatten()
            .map(|p| (*p + origin).abs().max_element())
            .fold(origin.abs().max_element(), f64::max);
        let epsilon = 1e-4_f64
            .max(self.radius * 1e-5)
            .max(coordinate * f64::from(f32::EPSILON) * 2.)
            .min(self.radius * 1e-3);
        if self.length <= epsilon * 2. {
            return false;
        }
        let mut levels = vec![epsilon, self.length - epsilon];
        levels.extend(
            points
                .iter()
                .flatten()
                .map(|p| p.dot(axis).clamp(epsilon, self.length - epsilon)),
        );
        levels.sort_by(f64::total_cmp);
        levels.dedup_by(|a, b| (*a - *b).abs() <= epsilon);
        for slab in levels.windows(2).filter(|s| s[1] - s[0] > epsilon) {
            let z = (slab[0] + slab[1]) * 0.5;
            let mut intervals = Vec::new();
            for tri in &points {
                let mut angles = Vec::with_capacity(2);
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let za = tri[a].dot(axis);
                    let zb = tri[b].dot(axis);
                    if (za <= z && zb > z) || (zb <= z && za > z) {
                        let p = tri[a] + (tri[b] - tri[a]) * ((z - za) / (zb - za));
                        angles.push(p.dot(v).atan2(p.dot(u)));
                    }
                }
                if angles.len() != 2 {
                    continue;
                }
                let delta = (angles[1] - angles[0] + PI).rem_euclid(TAU) - PI;
                let start = (angles[0] + delta.min(0.)).rem_euclid(TAU);
                let end = start + delta.abs();
                if end > TAU {
                    intervals.push((0., end - TAU));
                }
                intervals.push((start, end.min(TAU)));
            }
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut end: f64 = 0.;
            for (start, next) in intervals {
                if start > end + 1e-4 {
                    return false;
                }
                end = end.max(next);
            }
            if end < TAU - 1e-4 {
                return false;
            }
        }
        true
    }

    /// A certified straight sub-band may begin inside a display triangle at
    /// a scalloped fillet seam. Count only the triangle's axial intersection,
    /// otherwise long facets beyond the selected band inflate the area.
    fn clipped_triangle_area(&self, mesh: &MeshData, triangle: &[u32]) -> f64 {
        let origin = DVec3::from_array(self.origin);
        let axis = DVec3::from_array(self.axis);
        let mut polygon: Vec<_> = triangle
            .iter()
            .map(|&i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)) - origin)
            .collect();
        for (plane, sign) in [(0., 1.), (self.length, -1.)] {
            let mut clipped = Vec::new();
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let da = sign * (a.dot(axis) - plane);
                let db = sign * (b.dot(axis) - plane);
                if da >= 0. {
                    clipped.push(a);
                }
                if (da >= 0.) != (db >= 0.) {
                    clipped.push(a + (b - a) * (da / (da - db)));
                }
            }
            polygon = clipped;
        }
        if polygon.len() < 3 {
            return 0.;
        }
        (1..polygon.len() - 1)
            .map(|i| {
                (polygon[i] - polygon[0])
                    .cross(polygon[i + 1] - polygon[0])
                    .length()
                    * 0.5
            })
            .sum()
    }

    /// Recover a reordered tessellation only when the caller already has
    /// trusted Cylinder primitive provenance. This is not an STL classifier:
    /// every triangle and the closed shell must certify one capped cylinder.
    pub fn recover_primitive_cylinder(&self, mesh: &MeshData) -> Option<Self> {
        if mesh.positions.len() < 6
            || mesh.indices.is_empty()
            || mesh.indices.len() % 3 != 0
            || mesh
                .indices
                .iter()
                .any(|&i| i as usize >= mesh.positions.len())
        {
            return None;
        }
        let points: Vec<_> = mesh
            .positions
            .iter()
            .map(|p| DVec3::from_array(p.map(f64::from)))
            .collect();
        if points.iter().any(|p| !p.is_finite()) {
            return None;
        }
        let preferred = DVec3::from_array(self.axis);
        if !preferred.is_finite() || preferred.length() < 1e-12 {
            return None;
        }
        let preferred = preferred.normalize();

        // Tessellation may duplicate positions at hard face seams. Require
        // geometric edges to close even if their vertex IDs were reordered.
        // Area alone would wrongly permit one missing 1/96 side strip.
        let mut positions = std::collections::HashMap::new();
        let mut remap = Vec::with_capacity(mesh.positions.len());
        for p in &mesh.positions {
            let key = p.map(|v| if v == 0. { 0 } else { v.to_bits() });
            let next = positions.len() as u32;
            remap.push(*positions.entry(key).or_insert(next));
        }
        let mut edges = std::collections::HashMap::<(u32, u32), usize>::new();
        let mut normals = Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            let p = |i: usize| points[tri[i] as usize];
            let cross = (p(1) - p(0)).cross(p(2) - p(0));
            let area_twice = cross.length();
            if area_twice <= 1e-20 {
                continue;
            }
            normals.push((area_twice, cross / area_twice));
            let ids = [tri[0], tri[1], tri[2]].map(|i| remap[i as usize]);
            for (a, b) in [(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[0])] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        if edges.is_empty() || edges.values().any(|&count| count != 2) {
            return None;
        }

        // Large cap triangles provide better-conditioned normals than small
        // slivers. Sign and ordering are independent of triangle winding.
        normals.sort_by(|a, b| b.0.total_cmp(&a.0));
        let oriented = |mut axis: DVec3| {
            let alignment = axis.dot(preferred);
            let first = [axis.x, axis.y, axis.z]
                .into_iter()
                .find(|v| v.abs() > 1e-8)
                .unwrap_or(1.);
            if alignment < -1e-6 || (alignment.abs() <= 1e-6 && first < 0.) {
                axis = -axis;
            }
            axis
        };
        let mut groups = Vec::<DVec3>::new();
        for (area_twice, normal) in normals {
            let axis = oriented(normal);
            if let Some(group) = groups
                .iter_mut()
                .find(|group| axis.dot(group.normalize()).abs() > 1. - 1e-6)
            {
                let aligned = if axis.dot(*group) < 0. { -axis } else { axis };
                *group += aligned * area_twice;
            } else if groups.len() < 127 {
                groups.push(axis * area_twice);
            }
        }
        // Summing cap cross products cancels the f32 noise amplified by the
        // short edges of a triangle fan, keeping a long cylinder's axis true.
        let mut candidates: Vec<_> = groups
            .into_iter()
            .map(|group| oriented(group.normalize()))
            .collect();
        // Do not deduplicate the seed against a measured cap normal: even a
        // small rotation can move a long cylinder outside radial tolerance.
        candidates.push(preferred);
        candidates.sort_by(|a, b| {
            b.dot(preferred)
                .total_cmp(&a.dot(preferred))
                .then_with(|| a.x.total_cmp(&b.x))
                .then_with(|| a.y.total_cmp(&b.y))
                .then_with(|| a.z.total_cmp(&b.z))
        });
        let coordinate = points
            .iter()
            .map(|p| p.abs().max_element())
            .fold(0., f64::max);
        for axis in candidates {
            let hint = if axis.x.abs() < 0.8 {
                DVec3::X
            } else {
                DVec3::Y
            };
            let u = axis.cross(hint).normalize();
            let v = axis.cross(u);
            let mut min = DVec3::splat(f64::INFINITY);
            let mut max = DVec3::splat(f64::NEG_INFINITY);
            for p in &points {
                let projected = DVec3::new(p.dot(u), p.dot(v), p.dot(axis));
                min = min.min(projected);
                max = max.max(projected);
            }
            let center = (min + max) * 0.5;
            let origin = u * center.x + v * center.y + axis * min.z;
            let length = max.z - min.z;
            let radius = points
                .iter()
                .map(|p| {
                    let relative = *p - origin;
                    (relative - axis * relative.dot(axis)).length()
                })
                .fold(0., f64::max);
            if !radius.is_finite() || !length.is_finite() || radius <= 1e-12 || length <= 1e-12 {
                continue;
            }
            let tolerance = 1e-4_f64
                .max(radius * 1e-5)
                .max(coordinate * f64::from(f32::EPSILON) * 2.)
                .min(radius * 1e-3);
            let mut side_area = 0.;
            let mut caps = [0., 0.];
            let mut valid = true;
            for tri in mesh.indices.chunks_exact(3) {
                let p = [tri[0], tri[1], tri[2]].map(|i| points[i as usize]);
                let area = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
                if area <= 1e-20 {
                    continue;
                }
                let axial = p.map(|p| (p - origin).dot(axis));
                let radial = p.map(|p| {
                    let relative = p - origin;
                    (relative - axis * relative.dot(axis)).length()
                });
                if axial
                    .iter()
                    .any(|&z| z < -tolerance || z > length + tolerance)
                    || radial.iter().any(|&r| r > radius + tolerance)
                {
                    valid = false;
                    break;
                }
                if axial.iter().all(|z| z.abs() <= tolerance) {
                    caps[0] += area;
                } else if axial.iter().all(|z| (z - length).abs() <= tolerance) {
                    caps[1] += area;
                } else if radial.iter().all(|r| (r - radius).abs() <= tolerance) {
                    side_area += area;
                } else {
                    valid = false;
                    break;
                }
            }
            let side_expected = std::f64::consts::TAU * radius * length;
            let cap_expected = std::f64::consts::PI * radius * radius;
            if valid
                && side_expected.is_finite()
                && cap_expected.is_finite()
                && (side_area / side_expected - 1.).abs() < 0.02
                && caps
                    .iter()
                    .all(|area| (area / cap_expected - 1.).abs() < 0.02)
            {
                let target = Self {
                    origin: origin.to_array(),
                    axis: axis.to_array(),
                    radius,
                    length,
                    ..self.clone()
                };
                if target.matches_full_wall(mesh) {
                    return Some(target);
                }
            }
        }
        None
    }
}

/// Recover a copy's actual affine placement from corresponding mesh vertices.
/// This deliberately requires identical topology and verifies every vertex:
/// a saved Copy recipe alone must never certify geometry edited afterwards.
pub fn copied_mesh_transform(source: &MeshData, destination: &MeshData) -> Option<DMat4> {
    if source.positions.len() < 4
        || source.positions.len() != destination.positions.len()
        || source.indices.is_empty()
        || source.indices.len() % 3 != 0
        || source.indices != destination.indices
        || source
            .indices
            .iter()
            .any(|&i| i as usize >= source.positions.len())
    {
        return None;
    }
    let source: Vec<_> = source
        .positions
        .iter()
        .map(|p| DVec3::from_array(p.map(f64::from)))
        .collect();
    let destination: Vec<_> = destination
        .positions
        .iter()
        .map(|p| DVec3::from_array(p.map(f64::from)))
        .collect();
    if source.iter().chain(&destination).any(|p| !p.is_finite()) {
        return None;
    }
    let farthest = |from: DVec3| {
        source
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.distance_squared(from)
                    .total_cmp(&b.distance_squared(from))
            })
            .map(|(index, _)| index)
    };
    // Two farthest-point sweeps give a well-separated baseline in linear time.
    let a = farthest(source[0])?;
    let b = farthest(source[a])?;
    let baseline = source[b] - source[a];
    let span = baseline.length();
    if !span.is_finite() || span <= 1e-12 {
        return None;
    }
    let baseline = baseline / span;
    let c = source
        .iter()
        .enumerate()
        .max_by(|(_, p), (_, q)| {
            (**p - source[a])
                .cross(baseline)
                .length_squared()
                .total_cmp(&(**q - source[a]).cross(baseline).length_squared())
        })
        .map(|(index, _)| index)?;
    let normal = (source[c] - source[a]).cross(baseline);
    if normal.length() <= span * 1e-6 {
        return None;
    }
    let normal = normal.normalize();
    let d = source
        .iter()
        .enumerate()
        .max_by(|(_, p), (_, q)| {
            (**p - source[a])
                .dot(normal)
                .abs()
                .total_cmp(&(**q - source[a]).dot(normal).abs())
        })
        .map(|(index, _)| index)?;
    if (source[d] - source[a]).dot(normal).abs() <= span * 1e-6 {
        return None;
    }
    let basis = |points: &[DVec3]| {
        DMat3::from_cols(
            points[b] - points[a],
            points[c] - points[a],
            points[d] - points[a],
        )
    };
    let source_basis = basis(&source);
    let linear = basis(&destination) * source_basis.inverse();
    let determinant = linear.determinant();
    let linear_scale = linear
        .x_axis
        .length()
        .max(linear.y_axis.length())
        .max(linear.z_axis.length());
    let volume_scale = linear.x_axis.length() * linear.y_axis.length() * linear.z_axis.length();
    if !determinant.is_finite()
        || !linear_scale.is_finite()
        || volume_scale <= 1e-30
        || determinant.abs() <= volume_scale * 1e-8
    {
        return None;
    }
    let translation = destination[a] - linear * source[a];
    let matrix = DMat4::from_cols(
        linear.x_axis.extend(0.),
        linear.y_axis.extend(0.),
        linear.z_axis.extend(0.),
        translation.extend(1.),
    );
    if matrix.to_cols_array().iter().any(|v| !v.is_finite()) {
        return None;
    }
    let max_coordinate = |points: &[DVec3]| {
        points
            .iter()
            .map(|p| p.abs().max_element())
            .fold(0., f64::max)
    };
    let rounding = 4.
        * f64::from(f32::EPSILON)
        * (max_coordinate(&destination) + linear_scale * max_coordinate(&source));
    let tolerance = 1e-5_f64.max(rounding);
    if source
        .iter()
        .zip(&destination)
        .any(|(p, q)| matrix.transform_point3(*p).distance(*q) > tolerance)
    {
        return None;
    }
    Some(matrix)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    #[default]
    TruncatedV,
    Rounded55,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    pub diameter: f64,
    pub pitch: f64,
    pub length: f64,
    pub offset: f64,
    pub depth: f64,
    pub angle_deg: f64,
    pub clearance: f64,
    /// Shift the whole profile inward for screws, outward for nuts. Each
    /// mating part contributes this radial allowance; legacy files use zero.
    pub radial_fit: f64,
    pub left_hand: bool,
    /// Diameter increase per axial millimeter, zero for parallel threads.
    pub taper: f64,
    pub profile: Profile,
    /// Root flat width as a fraction of pitch for the custom V cutter.
    pub flat_fraction: f64,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            diameter: 6.,
            pitch: 1.,
            length: 8.,
            offset: 0.,
            depth: 0.61343,
            angle_deg: 60.,
            clearance: 0.,
            radial_fit: 0.,
            left_hand: false,
            taper: 0.,
            profile: Profile::TruncatedV,
            flat_fraction: 1. / 6.,
        }
    }
}
pub fn presets() -> Vec<(&'static str, f64, f64, f64)> {
    vec![
        ("M3", 3., 0.5, 60.),
        ("M4", 4., 0.7, 60.),
        ("M5", 5., 0.8, 60.),
        ("M6", 6., 1., 60.),
        ("M8", 8., 1.25, 60.),
        ("M10", 10., 1.5, 60.),
        ("M12", 12., 1.75, 60.),
        ("UNC #6-32", 3.5052, 25.4 / 32., 60.),
        ("UNC 1/4-20", 6.35, 25.4 / 20., 60.),
        ("UNC 3/8-16", 9.525, 25.4 / 16., 60.),
        ("UNF 1/4-28", 6.35, 25.4 / 28., 60.),
        ("UNF 3/8-24", 9.525, 25.4 / 24., 60.),
        ("BSPP G1/8", 9.728, 25.4 / 28., 55.),
        ("BSPP G1/4", 13.157, 25.4 / 19., 55.),
        ("BSPP G1/2", 20.955, 25.4 / 14., 55.),
        ("BSPT R1/8", 9.728, 25.4 / 28., 55.),
        ("BSPT R1/4", 13.157, 25.4 / 19., 55.),
        ("BSPT R1/2", 20.955, 25.4 / 14., 55.),
    ]
}
pub fn preset_params(name: &str, internal: bool) -> Option<Params> {
    let (_, diameter, pitch, angle_deg) = presets().into_iter().find(|(n, _, _, _)| *n == name)?;
    Some(Params {
        diameter,
        pitch,
        angle_deg,
        depth: pitch
            * if angle_deg == 55. {
                0.640327
            } else if internal {
                0.541266
            } else {
                0.61343
            },
        profile: if angle_deg == 55. {
            Profile::Rounded55
        } else {
            Profile::TruncatedV
        },
        taper: if name.starts_with("BSPT") {
            1. / 16.
        } else {
            0.
        },
        // The external root and internal root have different truncations;
        // sharing the internal 1/8 P root makes the male flanks interfere.
        flat_fraction: if internal { 1. / 8. } else { 1. / 6. },
        ..Default::default()
    })
}
pub fn preset_values() -> serde_json::Value {
    serde_json::json!(presets().into_iter().map(|(name,_,_,_)|serde_json::json!({"name":name,"external":preset_params(name,false),"internal":preset_params(name,true)})).collect::<Vec<_>>())
}
pub fn required_blank_radius(internal: bool, p: &Params) -> f64 {
    if internal {
        p.diameter * 0.5 - p.depth
    } else {
        p.diameter * 0.5 + p.length * p.taper * 0.5
    }
}

/// Resize the selected blank to the requested thread, retaining all parameters.
/// Printing allowance is already part of the cutter profile, not the blank.
pub fn corrected_blank_target(t: &Target, p: &Params) -> Result<Target, String> {
    let corrected = Target {
        radius: required_blank_radius(t.internal, p),
        ..t.clone()
    };
    validate(&corrected, p)?;
    Ok(corrected)
}

/// Cheap validation shared by the editor, preset filter and mesh generator.
pub fn validate(t: &Target, p: &Params) -> Result<(), String> {
    let vals = [
        p.diameter,
        p.pitch,
        p.length,
        p.offset,
        p.depth,
        p.angle_deg,
        p.clearance,
        p.radial_fit,
        p.taper,
        p.flat_fraction,
        t.radius,
        t.length,
    ];
    if vals.iter().any(|v| !v.is_finite())
        || p.diameter <= 0.
        || t.radius <= 0.
        || p.pitch < 0.2
        || p.length <= p.pitch
        || !(0. ..=0.1).contains(&p.taper)
        || p.offset < 0.
        || p.length + p.offset > t.length + 1e-5
        || p.depth <= 0.
        || p.depth >= t.radius * 0.5
        || !(30. ..=90.).contains(&p.angle_deg)
        || p.clearance < 0.
        || p.clearance > p.pitch * 0.25
        || !(0. ..=p.pitch * 0.25).contains(&p.radial_fit)
        || p.diameter * 0.5 - p.depth - p.clearance - p.radial_fit <= 0.
        || !(0.01..=0.4).contains(&p.flat_fraction)
        || (p.profile == Profile::Rounded55
            && ((p.angle_deg - 55.).abs() > 1e-5 || (p.depth / p.pitch - 0.640327).abs() > 0.001))
    {
        let reason = if vals.iter().any(|v| !v.is_finite()) {
            "All thread dimensions must be finite"
        } else if p.pitch < 0.2 {
            "Pitch must be at least 0.2 mm"
        } else if p.diameter <= 0. || t.radius <= 0. {
            "Nominal diameter and wall radius must be positive"
        } else if p.length <= p.pitch || p.offset < 0. || p.length + p.offset > t.length + 1e-5 {
            "Thread length must exceed one pitch and fit the selected axial range"
        } else if p.depth <= 0. || p.depth >= t.radius * 0.5 {
            "Tooth depth must be positive and smaller than half the selected wall radius"
        } else if !(0. ..=p.pitch * 0.25).contains(&p.radial_fit) {
            "Mating allowance must be between zero and one quarter of the pitch"
        } else if p.diameter * 0.5 - p.depth - p.clearance - p.radial_fit <= 0. {
            "Tooth depth and allowances leave no positive screw core radius"
        } else if !(0. ..=0.1).contains(&p.taper) {
            "Diameter taper must be between 0 and 0.1 mm/mm"
        } else if !(30. ..=90.).contains(&p.angle_deg) {
            "Included angle must be between 30 and 90 degrees"
        } else if p.clearance < 0. || p.clearance > p.pitch * 0.25 {
            "Groove clearance must be between zero and one quarter of the pitch"
        } else if !(0.01..=0.4).contains(&p.flat_fraction) {
            "Root flat / pitch must be between 0.01 and 0.4"
        } else {
            "Whitworth requires a 55 degree angle and depth of 0.640327 times pitch"
        };
        return Err(reason.into());
    }
    let axis = DVec3::from_array(t.axis);
    let origin = DVec3::from_array(t.origin);
    if !axis.is_finite() || !origin.is_finite() || (axis.length() - 1.).abs() > 1e-5 {
        return Err("Invalid thread axis".into());
    }
    // External blank is the major diameter; internal blank is the minor bore.
    let expected = required_blank_radius(t.internal, p);
    if (expected - t.radius).abs() > 0.05_f64.max(p.pitch * 0.08) {
        return Err(format!("Blank radius {:.3} mm does not match required {:.3} mm; resize blank or choose matching parameters",t.radius,expected));
    }
    let overlap = 0.002_f64.max(p.pitch * 0.02);
    // Continue the flanks at their specified angle through the radial
    // Boolean overlap. Otherwise that overlap steepens the mating flanks.
    let half = (p.depth + p.clearance + overlap) * (p.angle_deg.to_radians() * 0.5).tan();
    if p.profile == Profile::TruncatedV && half * 2. + p.pitch * p.flat_fraction >= p.pitch * 0.98 {
        return Err("Profile overlaps adjacent turns; reduce depth, clearance or root flat".into());
    }
    if p.length / p.pitch > 125. {
        return Err("Thread request exceeds maximum 125 turns".into());
    }
    Ok(())
}

pub fn cutter(t: &Target, p: &Params) -> Result<MeshData, String> {
    validate(t, p)?;
    let axis = DVec3::from_array(t.axis);
    let origin = DVec3::from_array(t.origin);
    let overlap = 0.002_f64.max(p.pitch * 0.02);
    let half = (p.depth + p.clearance + overlap) * (p.angle_deg.to_radians() * 0.5).tan();
    let frame = PlaneDef::from_normal(t.axis);
    let x = frame.x_axis();
    let y = axis.cross(x);
    let sign = if t.internal { 1. } else { -1. };
    let base = (if t.internal {
        p.diameter * 0.5 - p.depth
    } else {
        p.diameter * 0.5
    }) + sign * p.radial_fit;
    let root = base + sign * (p.depth + p.clearance);
    let outer = base - sign * overlap;
    let flat = p.pitch * p.flat_fraction * 0.5;
    let profile = if p.profile == Profile::Rounded55 {
        // Whitworth: tangent 55-degree flanks, crest/root arcs of radius
        // 0.137329 P and radial height 0.640327 P. Mirror the half-profile.
        let rho = 0.137329 * p.pitch;
        let limit = 62.5_f64.to_radians();
        let mut half_curve = Vec::new();
        for i in 0..=8 {
            let a = limit * i as f64 / 8.;
            half_curve.push([rho * a.sin(), p.depth - rho + rho * a.cos()]);
        }
        let crest_limit = (0.01 * p.pitch / rho).asin();
        for i in 0..=8 {
            let a = limit + (crest_limit - limit) * i as f64 / 8.;
            half_curve.push([p.pitch * 0.5 - rho * a.sin(), rho - rho * a.cos()]);
        }
        let mut points = vec![[outer, -p.pitch * 0.49]];
        for [z, d] in half_curve.iter().rev().filter(|[z, _]| *z > 1e-12) {
            points.push([base + sign * (d + p.clearance), -*z]);
        }
        for [z, d] in half_curve {
            points.push([base + sign * (d + p.clearance), z]);
        }
        points.push([outer, p.pitch * 0.49]);
        points
    } else {
        vec![
            [outer, -half - flat],
            [root, -flat],
            [root, flat],
            [outer, half + flat],
        ]
    };
    let width = profile.len() as u32;
    let requested_turns = p.length / p.pitch;
    if requested_turns > 125. {
        return Err("Thread too long: maximum 125 turns".into());
    }
    // An inset helix leaves a blocking major-diameter collar at the entry.
    // Sweep a complete extra turn at either end, with phase tied to the
    // requested offset, then cap/trim to that interval. Overrun also covers
    // every angular phase for fractional-pitch lengths and either hand.
    let sweep_start = p.offset - p.pitch;
    let sweep_length = p.length + 2. * p.pitch;
    let turns = requested_turns + 2.;
    let steps = (turns * 96.).ceil().max(1.) as usize;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=steps {
        let f = i as f64 / steps as f64;
        let z = sweep_start + f * sweep_length;
        let a =
            (z - p.offset) / p.pitch * std::f64::consts::TAU * if p.left_hand { -1. } else { 1. };
        let radial = x * a.cos() + y * a.sin();
        for &[r, dz] in &profile {
            positions.push(
                (origin + axis * (z + dz) + radial * (r + (z + dz - p.offset) * p.taper * 0.5))
                    .to_array()
                    .map(|v| v as f32),
            );
        }
    }
    for i in 0..steps as u32 {
        for k in 0..width {
            let a = i * width + k;
            let b = i * width + (k + 1) % width;
            indices.extend([a, b, b + width, a, b + width, a + width]);
        }
    }
    let area: f64 = profile
        .iter()
        .zip(profile.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    let mut cap_ring = profile.clone();
    if area < 0. {
        cap_ring.reverse();
    }
    let caps = s33d_model::triangulate_region(&cap_ring, &[])
        .map_err(|error| format!("Thread profile triangulation failed: {error}"))?
        .triangles;
    if caps.len() != profile.len() - 2 {
        return Err("Thread profile triangulation failed".into());
    }
    for triangle in caps {
        let mut tri = triangle.map(|p| profile.iter().position(|q| *q == p).unwrap() as u32);
        if area < 0. {
            tri.swap(1, 2);
        }
        let e = steps as u32 * width;
        indices.extend([tri[2], tri[1], tri[0], e + tri[0], e + tri[1], e + tri[2]]);
    }
    let volume: f64 = indices
        .chunks_exact(3)
        .map(|v| {
            let a = DVec3::from_array(positions[v[0] as usize].map(f64::from));
            let b = DVec3::from_array(positions[v[1] as usize].map(f64::from));
            let c = DVec3::from_array(positions[v[2] as usize].map(f64::from));
            a.dot(b.cross(c)) / 6.
        })
        .sum();
    if volume < 0. {
        for tri in indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
    }
    // Slightly overrun the selected wall's physical ends to avoid coincident
    // cutter/blank end faces; partial-thread limits remain exactly requested.
    let end = p.offset + p.length;
    let coordinate = origin
        .abs()
        .max_element()
        .max((origin + axis * end).abs().max_element());
    let end_slack = 0.001_f64.max(coordinate * f64::from(f32::EPSILON) * 2.);
    let lo = if p.offset <= 1e-5 {
        -end_slack
    } else {
        p.offset
    };
    let hi = if (end - t.length).abs() <= 1e-5 {
        end + end_slack
    } else {
        end
    };
    let mesh = MeshData::new(positions, indices);
    let mesh = mesh_bool::split_by_plane(&mesh, (origin + axis * lo).to_array(), t.axis)
        .map_err(|e| format!("Thread start trim failed: {e}"))?
        .0;
    let mesh = mesh_bool::split_by_plane(&mesh, (origin + axis * hi).to_array(), t.axis)
        .map_err(|e| format!("Thread end trim failed: {e}"))?
        .1;
    Ok(mesh)
}
pub fn cut(source: &MeshData, t: &Target, p: &Params) -> Result<MeshData, String> {
    let tool = cutter(t, p)?;
    let mut blank = source.clone();
    if p.taper > 0. || p.radial_fit > 0. {
        let end = p.offset + p.length;
        let origin = DVec3::from_array(t.origin);
        let axis = DVec3::from_array(t.axis);
        let coordinate = origin
            .abs()
            .max_element()
            .max((origin + axis * end).abs().max_element());
        let slack = 0.001_f64.max(coordinate * f64::from(f32::EPSILON) * 2.);
        let lo = if p.offset <= 1e-5 { -slack } else { p.offset };
        let hi = if (end - t.length).abs() <= 1e-5 {
            end + slack
        } else {
            end
        };
        let major = p.diameter * 0.5;
        let a = if t.internal {
            0.001
        } else {
            major - p.radial_fit
        };
        let b = if t.internal {
            major - p.depth + p.radial_fit
        } else {
            t.radius + 0.05
        };
        let az = if t.internal {
            a
        } else {
            a + p.length * p.taper * 0.5
        };
        let bz = if t.internal {
            b + p.length * p.taper * 0.5
        } else {
            b
        };
        let profile = [[a, lo], [b, lo], [bz, hi], [az, hi]];
        let frame = PlaneDef::from_normal(t.axis);
        let envelope =
            mesh_bool::revolve_profile(&profile, t.origin, t.axis, frame.x_axis().to_array(), 128)
                .map_err(|e| e.to_string())?;
        blank = mesh_bool::boolean(&blank, &envelope, mesh_bool::BoolOp::Subtract)
            .map_err(|e| e.to_string())?;
    }
    mesh_bool::boolean(&blank, &tool, mesh_bool::BoolOp::Subtract).map_err(|e| e.to_string())
}

/// Persistent remaining analytic surfaces are valid only on this exact mesh.
/// A fixed FNV hash avoids depending on a randomized or version-specific hasher.
pub fn mesh_signature(mesh: &MeshData) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for bits in mesh
        .positions
        .iter()
        .flat_map(|p| p.iter().map(|v| v.to_bits()))
        .chain(mesh.indices.iter().copied())
    {
        for byte in bits.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ThreadAnnotation {
    pub target: Target,
    pub params: Params,
}
#[derive(Serialize, Deserialize)]
pub struct Saved {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pocket_sources: Vec<u64>,
    /// Drawing provenance for generated threads; absent in older projects.
    #[serde(default)]
    pub annotations: Vec<ThreadAnnotation>,
    pub remaining: Vec<Target>,
    pub params: Params,
    pub mesh_signature: String,
}

impl Saved {
    /// Thread form and pitch remain representable under rigid motions and
    /// uniform scaling (including reflections). Other affine changes must not
    /// retain an inaccurate thread annotation.
    pub fn transformed(&self, body_id: u64, mat: &DMat4, mesh: &MeshData) -> Option<Self> {
        let columns = [
            mat.x_axis.truncate(),
            mat.y_axis.truncate(),
            mat.z_axis.truncate(),
        ];
        let scale = columns[0].length();
        if !scale.is_finite()
            || scale <= 1e-12
            || columns
                .iter()
                .any(|v| (v.length() - scale).abs() > scale * 1e-5)
            || columns[0].dot(columns[1]).abs() > scale * scale * 1e-5
            || columns[0].dot(columns[2]).abs() > scale * scale * 1e-5
            || columns[1].dot(columns[2]).abs() > scale * scale * 1e-5
        {
            return None;
        }
        let params = |p: &Params| {
            let mut p = p.clone();
            p.diameter *= scale;
            p.pitch *= scale;
            p.length *= scale;
            p.offset *= scale;
            p.depth *= scale;
            p.clearance *= scale;
            p.radial_fit *= scale;
            if mat.determinant() < 0. {
                p.left_hand = !p.left_hand;
            }
            p
        };
        let remaining = self
            .remaining
            .iter()
            .filter_map(|t| t.transformed(body_id, mat))
            .filter(|t| t.matches_full_wall(mesh))
            .collect();
        let annotations = self
            .annotations
            .iter()
            .map(|a| {
                Some(ThreadAnnotation {
                    target: a.target.transformed(body_id, mat)?,
                    params: params(&a.params),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            remaining,
            annotations,
            pocket_sources: self.pocket_sources.clone(),
            params: params(&self.params),
            mesh_signature: mesh_signature(mesh),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wall_merge_preserves_distinct_bores_and_accepts_opposite_axis_duplicates() {
        let t = target(false);
        let reversed = Target {
            origin: [0., 10., 0.],
            axis: [0., -1., 0.],
            face_id: 0x80000000,
            ..t.clone()
        };
        assert!(t.same_wall(&reversed));
        assert!(!t.same_wall(&Target {
            internal: true,
            ..t.clone()
        }));
        assert!(!t.same_wall(&Target {
            origin: [0., 10., 0.],
            ..t.clone()
        }));
        assert!(!t.same_wall(&Target {
            radius: f64::NAN,
            ..t.clone()
        }));
    }

    #[test]
    fn complete_wall_recertification_rejects_a_small_missing_facet() {
        let t = target(false);
        let mut mesh = mesh_bool::revolve_profile(
            &[[1., 0.], [3., 0.], [3., 10.], [1., 10.]],
            [0.; 3],
            [0., 1., 0.],
            [1., 0., 0.],
            96,
        )
        .unwrap();
        assert!(t.matches_full_wall(&mesh));
        let slot = mesh
            .indices
            .chunks_exact(3)
            .position(|tri| t.contains_triangle(&mesh, tri))
            .unwrap()
            * 3;
        mesh.indices.drain(slot..slot + 3);
        assert!(
            !t.matches_full_wall(&mesh),
            "Area loss below 2% is still an incomplete wall"
        );
    }

    #[test]
    fn trimmed_wall_counts_only_the_axial_intersection_of_long_facets() {
        let mesh = mesh_bool::revolve_profile(
            &[[1., 0.], [3., 0.], [3., 10.], [1., 10.]],
            [0.; 3],
            [0., 1., 0.],
            [1., 0., 0.],
            96,
        )
        .unwrap();
        let band = Target {
            origin: [0., 2., 0.],
            length: 4.,
            ..target(false)
        };
        assert!(band.matches_full_wall(&mesh));
        assert!(!Target { length: 9., ..band }.matches_full_wall(&mesh));
    }

    fn target(internal: bool) -> Target {
        Target {
            body_id: 1,
            face_id: 0,
            origin: [0., 0., 0.],
            axis: [0., 1., 0.],
            radius: if internal { 3. - 0.61343 } else { 3. },
            length: 10.,
            internal,
        }
    }
    fn blank(t: &Target) -> MeshData {
        let profile = if t.internal {
            vec![[t.radius, 0.], [5., 0.], [5., 10.], [t.radius, 10.]]
        } else {
            vec![[0.001, 0.], [t.radius, 0.], [t.radius, 10.], [0.001, 10.]]
        };
        mesh_bool::revolve_profile(&profile, t.origin, t.axis, [1., 0., 0.], 96).unwrap()
    }

    fn copy_test_mesh() -> MeshData {
        MeshData {
            positions: vec![
                [-3., 0., -3.],
                [3., 0., -3.],
                [3., 30., -3.],
                [-3., 30., -3.],
                [-3., 0., 3.],
                [3., 0., 3.],
                [3., 30., 3.],
                [-3., 30., 3.],
                [0.5, 12., -1.],
            ],
            indices: vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6],
            ..Default::default()
        }
    }

    fn copied_mesh(mesh: &MeshData, matrix: DMat4) -> MeshData {
        let mut copy = mesh.clone();
        for p in &mut copy.positions {
            *p = matrix
                .transform_point3(DVec3::from_array(p.map(f64::from)))
                .as_vec3()
                .to_array();
        }
        copy
    }

    fn primitive_cylinder_mesh() -> MeshData {
        let mut mesh = MeshData::default();
        for height in [0., 30.] {
            for i in 0..96 {
                let angle = std::f64::consts::TAU * i as f64 / 96.;
                mesh.positions
                    .push([(3. * angle.cos()) as f32, height, (3. * angle.sin()) as f32]);
            }
        }
        mesh.positions.extend([[0., 0., 0.], [0., 30., 0.]]);
        for i in 0..96u32 {
            let next = (i + 1) % 96;
            mesh.indices
                .extend([i, next, i + 96, next, next + 96, i + 96]);
            mesh.indices.extend([192, next, i, 193, i + 96, next + 96]);
        }
        mesh
    }

    #[test]
    fn primitive_cylinder_recovers_rotated_reordered_tessellation() {
        let seed = Target {
            length: 30.,
            ..target(false)
        };
        for angle in [0.73, 0.0003] {
            let matrix = DMat4::from_translation(DVec3::new(37., -8., 20.))
                * DMat4::from_rotation_z(angle)
                * DMat4::from_rotation_x(angle);
            let mut mesh = copied_mesh(&primitive_cylinder_mesh(), matrix);
            let count = mesh.positions.len() as u32;
            mesh.positions.reverse();
            mesh.indices = mesh
                .indices
                .chunks_exact(3)
                .rev()
                .flat_map(|tri| [count - 1 - tri[1], count - 1 - tri[2], count - 1 - tri[0]])
                .collect();
            let recovered = seed.recover_primitive_cylinder(&mesh).unwrap();
            assert!((recovered.radius - 3.).abs() < 1e-4);
            assert!((recovered.length - 30.).abs() < 1e-4);
            assert!(
                DVec3::from_array(recovered.axis)
                    .dot(matrix.transform_vector3(DVec3::Y).normalize())
                    > 1. - 1e-6
            );
            assert!(recovered.matches_full_wall(&mesh));
        }
    }

    #[test]
    fn primitive_cylinder_recovery_rejects_ellipse_and_missing_strip() {
        let seed = Target {
            length: 30.,
            ..target(false)
        };
        let mesh = primitive_cylinder_mesh();
        let ellipse = copied_mesh(&mesh, DMat4::from_scale(DVec3::new(1.3, 1., 1.)));
        assert!(seed.recover_primitive_cylinder(&ellipse).is_none());
        let mut broken = mesh;
        // One missing strip is below the 2% area threshold, but not a closed
        // primitive; the geometric edge certificate must reject it.
        broken.indices.drain(..6);
        assert!(seed.recover_primitive_cylinder(&broken).is_none());
        let mut invalid = primitive_cylinder_mesh();
        invalid.positions[0][0] = f32::NAN;
        assert!(seed.recover_primitive_cylinder(&invalid).is_none());
    }

    #[test]
    fn copied_mesh_transform_recovers_translation_rotation_and_scaling() {
        let source = copy_test_mesh();
        for matrix in [
            DMat4::from_translation(DVec3::new(20., 0., 20.)),
            DMat4::from_translation(DVec3::new(7., -13., 5.))
                * DMat4::from_rotation_z(0.7)
                * DMat4::from_rotation_x(-0.4),
            DMat4::from_translation(DVec3::new(100., 25., -33.))
                * DMat4::from_rotation_y(0.3)
                * DMat4::from_scale(DVec3::new(-2., 3., 2.)),
        ] {
            let destination = copied_mesh(&source, matrix);
            let recovered = copied_mesh_transform(&source, &destination).unwrap();
            for (p, q) in source.positions.iter().zip(&destination.positions) {
                assert!(
                    recovered
                        .transform_point3(DVec3::from_array(p.map(f64::from)))
                        .distance(DVec3::from_array(q.map(f64::from)))
                        < 1e-4
                );
            }
        }
    }

    #[test]
    fn copied_mesh_transform_rejects_changed_geometry_topology_and_degeneracy() {
        let source = copy_test_mesh();
        let destination = copied_mesh(&source, DMat4::from_translation(DVec3::new(20., 0., 20.)));
        let mut changed_source = source.clone();
        changed_source.positions[8][0] += 0.5;
        assert!(copied_mesh_transform(&changed_source, &destination).is_none());
        let mut changed_destination = destination.clone();
        changed_destination.positions[8][1] += 0.5;
        assert!(copied_mesh_transform(&source, &changed_destination).is_none());
        changed_destination = destination.clone();
        changed_destination.indices.swap(0, 1);
        assert!(copied_mesh_transform(&source, &changed_destination).is_none());
        changed_destination = destination.clone();
        changed_destination.positions[0][0] = f32::NAN;
        assert!(copied_mesh_transform(&source, &changed_destination).is_none());
        let flat = copied_mesh(&source, DMat4::from_scale(DVec3::new(1., 1., 0.)));
        assert!(copied_mesh_transform(&source, &flat).is_none());
        assert!(copied_mesh_transform(&flat, &flat).is_none());
    }

    #[test]
    fn cylinder_target_transform_preserves_circular_walls_and_reflections() {
        for internal in [false, true] {
            let original = target(internal);
            let matrix = DMat4::from_translation(DVec3::new(20., 4., -8.))
                * DMat4::from_rotation_x(0.7)
                * DMat4::from_scale(DVec3::new(-2., 3., 2.));
            let transformed = original.transformed(9, &matrix).unwrap();
            assert_eq!(transformed.body_id, 9);
            assert_eq!(transformed.face_id, original.face_id);
            assert_eq!(transformed.internal, internal);
            assert!((transformed.radius - original.radius * 2.).abs() < 1e-10);
            assert!((transformed.length - original.length * 3.).abs() < 1e-10);
            assert!(
                DVec3::from_array(transformed.origin)
                    .distance(matrix.transform_point3(DVec3::from_array(original.origin)))
                    < 1e-10
            );
            assert!(
                DVec3::from_array(transformed.axis)
                    .distance(matrix.transform_vector3(DVec3::Y).normalize())
                    < 1e-10
            );
        }
    }

    #[test]
    fn cylinder_target_transform_rejects_ellipses_shear_and_invalid_axes() {
        let original = target(false);
        assert!(original
            .transformed(2, &DMat4::from_scale(DVec3::new(2., 3., 1.)))
            .is_none());
        assert!(original
            .transformed(
                2,
                &DMat4::from_cols(
                    glam::DVec4::X,
                    glam::DVec4::new(0.2, 1., 0., 0.),
                    glam::DVec4::Z,
                    glam::DVec4::W,
                )
            )
            .is_none());
        assert!(original
            .transformed(2, &DMat4::from_scale(DVec3::new(1., 0., 1.)))
            .is_none());
        let mut invalid = original.clone();
        invalid.axis = [0., 0., 0.];
        assert!(invalid.transformed(2, &DMat4::IDENTITY).is_none());
        let mut non_affine = DMat4::IDENTITY;
        non_affine.x_axis.w = 0.1;
        assert!(original.transformed(2, &non_affine).is_none());
    }

    #[test]
    fn thread_wall_matches_tilted_chord_triangle_but_not_end_cap() {
        let t = target(false);
        let point = |angle: f64, y: f64| {
            [
                (t.radius * angle.cos()) as f32,
                y as f32,
                (t.radius * angle.sin()) as f32,
            ]
        };
        let mesh = MeshData {
            positions: vec![point(0., 0.), point(0.1, 0.), point(0.2, 0.3)],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert!(t.contains_triangle(&mesh, &mesh.indices));
        let mut cap = mesh.clone();
        cap.positions[2][1] = 0.;
        assert!(!t.contains_triangle(&cap, &cap.indices));
    }

    #[test]
    fn cylinder_full_wall_matches_far_f32_copy_without_merging_adjacent_bore() {
        let outer = Target {
            origin: [10_000., 0., 0.],
            ..target(false)
        };
        let inner = Target {
            radius: 2.99,
            internal: true,
            ..outer.clone()
        };
        let profile = [
            [inner.radius, 0.],
            [outer.radius, 0.],
            [outer.radius, 10.],
            [inner.radius, 10.],
        ];
        let mesh = mesh_bool::revolve_profile(&profile, outer.origin, outer.axis, [1., 0., 0.], 96)
            .unwrap();
        assert!(outer.matches_full_wall(&mesh));
        assert!(inner.matches_full_wall(&mesh));
        assert!(mesh.indices.chunks_exact(3).all(|triangle| {
            !(outer.contains_triangle(&mesh, triangle) && inner.contains_triangle(&mesh, triangle))
        }));

        let mut strip = mesh.clone();
        strip.indices = mesh
            .indices
            .chunks_exact(3)
            .filter(|triangle| outer.contains_triangle(&mesh, triangle))
            .take(2)
            .flatten()
            .copied()
            .collect();
        assert!(!outer.matches_full_wall(&strip));
        assert!(!Target {
            length: 0.,
            ..outer.clone()
        }
        .matches_full_wall(&mesh));
        assert!(!Target {
            radius: f64::NAN,
            ..outer
        }
        .matches_full_wall(&mesh));
    }

    #[test]
    fn threads_cut_external_internal_both_hands_and_taper() {
        for internal in [false, true] {
            for left in [false, true] {
                for taper in [0., 1. / 16.] {
                    let mut t = target(internal);
                    let p = Params {
                        left_hand: left,
                        taper,
                        ..Default::default()
                    };
                    if !internal {
                        t.radius += p.length * p.taper * 0.5;
                    }
                    let source = blank(&t);
                    let result = cut(&source, &t, &p).unwrap();
                    assert!(!result.is_empty());
                    // A union of a closed result with itself validates the output topology.
                    mesh_bool::boolean(&result, &result, mesh_bool::BoolOp::Union).unwrap();
                    assert!(result.positions.len() > source.positions.len());
                }
            }
        }
    }
    #[test]
    fn bsp_rounded_profile_parallel_and_tapered_are_closed() {
        for name in ["BSPP G1/8", "BSPT R1/8"] {
            for internal in [false, true] {
                let mut p = preset_params(name, internal).unwrap();
                p.length = 4.;
                let mut t = target(internal);
                t.radius = if internal {
                    p.diameter * 0.5 - p.depth
                } else {
                    p.diameter * 0.5 + p.length * p.taper * 0.5
                };
                let profile = if internal {
                    vec![[t.radius, 0.], [8., 0.], [8., 10.], [t.radius, 10.]]
                } else {
                    vec![[0.001, 0.], [t.radius, 0.], [t.radius, 10.], [0.001, 10.]]
                };
                let source =
                    mesh_bool::revolve_profile(&profile, t.origin, t.axis, [1., 0., 0.], 96)
                        .unwrap();
                let result = cut(&source, &t, &p).unwrap();
                mesh_bool::boolean(&result, &result, mesh_bool::BoolOp::Union).unwrap();
                assert!(!result.is_empty());
            }
        }
    }
    #[test]
    fn turn_limit_counts_requested_thread_not_entry_overrun() {
        let t = Target {
            length: 130.,
            ..target(false)
        };
        let mut p = Params {
            length: 125.,
            ..Default::default()
        };
        assert!(!cutter(&t, &p)
            .expect("125 requested turns plus end overrun")
            .is_empty());
        p.length = 125.01;
        assert!(cutter(&t, &p).unwrap_err().contains("maximum 125 turns"));
    }

    #[test]
    fn threads_reject_mismatched_blank_and_degenerate_span() {
        let t = target(false);
        assert!(cutter(
            &t,
            &Params {
                diameter: 8.,
                ..Default::default()
            }
        )
        .is_err());
        assert!(cutter(
            &t,
            &Params {
                length: 1.,
                ..Default::default()
            }
        )
        .is_err());
        assert!(cutter(
            &t,
            &Params {
                length: f64::NAN,
                ..Default::default()
            }
        )
        .is_err());
    }
}
