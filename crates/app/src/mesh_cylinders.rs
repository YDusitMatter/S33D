//! Conservative recovery of complete cylindrical bands in faceted imports.
//! Geometric edge welding handles STL's duplicated vertices. Never modifies mesh.
use crate::threads::Target;
use glam::DVec3;
use s33d_kernel::MeshData;
use std::collections::{HashMap, HashSet};

pub fn targets(mesh: &MeshData, body_id: u64) -> Vec<Target> {
    recover(mesh, body_id, true)
}

fn recover(mesh: &MeshData, body_id: u64, allow_trim: bool) -> Vec<Target> {
    if mesh
        .indices
        .iter()
        .any(|&i| i as usize >= mesh.positions.len())
    {
        return vec![];
    }
    let points: Vec<_> = mesh
        .positions
        .iter()
        .map(|p| DVec3::from_array(p.map(f64::from)))
        .collect();
    if points.iter().any(|p| !p.is_finite()) {
        return vec![];
    }
    let mut welded = HashMap::new();
    let ids: Vec<_> = mesh
        .positions
        .iter()
        .map(|p| {
            let key = p.map(|x| (f64::from(x) / 1e-5).round() as i64);
            let next = welded.len();
            *welded.entry(key).or_insert(next)
        })
        .collect();
    let tris: Vec<_> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]])
        .collect();
    let normals: Vec<_> = tris
        .iter()
        .map(|t| {
            (points[t[1] as usize] - points[t[0] as usize])
                .cross(points[t[2] as usize] - points[t[0] as usize])
                .normalize_or_zero()
        })
        .collect();
    let mut edges = HashMap::<_, Vec<usize>>::new();
    for (i, t) in tris.iter().enumerate() {
        let v = t.map(|x| ids[x as usize]);
        if v[0] == v[1] || v[1] == v[2] || v[2] == v[0] {
            continue;
        }
        for (a, b) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
            edges.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let mut neighbors = vec![vec![]; tris.len()];
    for ts in edges.values().filter(|ts| ts.len() == 2) {
        neighbors[ts[0]].push(ts[1]);
        neighbors[ts[1]].push(ts[0]);
    }
    for ns in &mut neighbors {
        ns.sort_unstable();
    }
    let mut accepted = vec![false; tris.len()];
    let mut attempted = vec![false; tris.len()];
    let mut result = vec![];
    let mut trim_attempts = Vec::<Target>::new();
    // Bound pathological non-cylindrical smooth meshes; failed fits don't flood
    // indefinitely on every hover. Results are cached by immutable ShapeId.
    let mut budget = tris.len().saturating_mul(128).max(10000);
    for seed in 0..tris.len() {
        if accepted[seed] || attempted[seed] {
            continue;
        }
        let n = normals[seed];

        let candidate = neighbors[seed]
            .iter()
            .chain(neighbors[seed].iter().flat_map(|&i| neighbors[i].iter()))
            .map(|&i| (n.dot(normals[i]), n.cross(normals[i])))
            .filter(|(dot, cross)| *dot > 0.8 && cross.length_squared() > 1e-6)
            .max_by(|a, b| a.1.length_squared().total_cmp(&b.1.length_squared()));
        let Some((_, cross)) = candidate else {
            continue;
        };
        let mut axis = cross.normalize();
        // A flat neighboring end/shoulder supplies an unbiased axis even when
        // the two polygon rings are staggered. Use it only when it agrees with
        // the normal-cross estimate; full cylindrical certification follows.
        if let Some(&end) = neighbors[seed]
            .iter()
            .chain(neighbors[seed].iter().flat_map(|&i| neighbors[i].iter()))
            .filter(|&&i| normals[i].dot(n).abs() < 0.2 && normals[i].dot(axis).abs() > 0.9)
            .max_by(|&&a, &&b| {
                let area = |i: usize| {
                    let t = tris[i];
                    (points[t[1] as usize] - points[t[0] as usize])
                        .cross(points[t[2] as usize] - points[t[0] as usize])
                        .length_squared()
                };
                area(a).total_cmp(&area(b))
            })
        {
            axis = normals[end] * normals[end].dot(axis).signum();
        }
        let dominant = axis.abs().max_element();
        if [axis.x, axis.y, axis.z]
            .into_iter()
            .find(|v| v.abs() == dominant)
            .unwrap_or(1.)
            < 0.
        {
            axis = -axis;
        }
        let mut visited = HashSet::new();
        let mut queue = vec![seed];
        let mut patch = vec![];
        while let Some(i) = queue.pop() {
            if !visited.insert(i) {
                continue;
            }
            if budget == 0 {
                return result;
            }
            budget -= 1;
            // Coarse STL rings can be angularly staggered. Their connecting
            // triangles then have a small axial chord-normal component even
            // though every vertex lies on the cylinder. This is only candidate
            // growth; radial fit, both closed rings and full area still certify.
            if normals[i].length_squared() < 0.5 || normals[i].dot(axis).abs() > 0.05 {
                continue;
            }
            patch.push(i);
            queue.extend(
                neighbors[i]
                    .iter()
                    .copied()
                    .filter(|&j| normals[i].dot(normals[j]) > 0.8),
            );
        }
        // Avoid refitting the same large planar cap once per triangle. A
        // different cylinder strip still gets its own candidate axis.
        for &i in &patch {
            if normals[i].dot(n) > 1. - 1e-5 {
                attempted[i] = true;
            }
        }
        if patch.len() < 24 {
            continue;
        }
        // Area-weighted normals recover the common axis without allowing a
        // skinny STL sliver's noisy normal to tilt the whole bore.
        let mut covariance = glam::DMat3::ZERO;
        for &i in &patch {
            let t = tris[i];
            let normal = normals[i];
            let area = (points[t[1] as usize] - points[t[0] as usize])
                .cross(points[t[2] as usize] - points[t[0] as usize])
                .length();
            covariance +=
                glam::DMat3::from_cols(normal * normal.x, normal * normal.y, normal * normal.z)
                    * area;
        }
        let regularizer = (covariance.x_axis.x + covariance.y_axis.y + covariance.z_axis.z) * 1e-8;
        let inverse = (covariance + glam::DMat3::IDENTITY * regularizer).inverse();
        for _ in 0..6 {
            let next = (inverse * axis).normalize_or_zero();
            if next.is_finite() && next.length_squared() > 0.5 {
                axis = next;
            }
        }
        let mut vertices: Vec<_> = patch.iter().flat_map(|&i| tris[i]).collect();
        vertices.sort_unstable();
        vertices.dedup();
        // Prefer the plane of an actual end ring over chord normals. Staggered
        // polygon rings bias those normals even with area weighting.
        let low = vertices
            .iter()
            .map(|&i| points[i as usize].dot(axis))
            .fold(f64::INFINITY, f64::min);
        let high = vertices
            .iter()
            .map(|&i| points[i as usize].dot(axis))
            .fold(f64::NEG_INFINITY, f64::max);
        let ring: Vec<_> = vertices
            .iter()
            .map(|&i| points[i as usize])
            .filter(|p| p.dot(axis) < low + (high - low) * 0.08)
            .collect();
        if ring.len() >= 8 {
            let center = ring.iter().copied().sum::<DVec3>() / ring.len() as f64;
            let mut covariance = glam::DMat3::ZERO;
            for &p in &ring {
                let d = p - center;
                covariance += glam::DMat3::from_cols(d * d.x, d * d.y, d * d.z);
            }
            let trace = covariance.x_axis.x + covariance.y_axis.y + covariance.z_axis.z;
            let inverse = (covariance + glam::DMat3::IDENTITY * (trace * 1e-9)).inverse();
            let mut normal = axis;
            for _ in 0..8 {
                normal = (inverse * normal).normalize_or_zero();
            }
            let variance = covariance * normal;
            if normal.is_finite()
                && normal.dot(axis) > 0.98
                && variance.length() < trace * 1e-6
                && ring.iter().all(|p| (*p - center).dot(normal).abs() < 0.001)
            {
                axis = normal;
            }
        }
        let base = points[tris[seed][0] as usize];
        let u = axis
            .cross(if axis.x.abs() < 0.8 {
                DVec3::X
            } else {
                DVec3::Y
            })
            .normalize();
        let v = axis.cross(u);
        // Least-squares circle in a centered frame; rejects cones/ellipses below.
        let mut matrix = glam::DMat3::ZERO;
        let mut rhs = DVec3::ZERO;
        for &i in &vertices {
            let d = points[i as usize] - base;
            let row = DVec3::new(d.dot(u), d.dot(v), 1.);
            matrix += glam::DMat3::from_cols(row * row.x, row * row.y, row);
            rhs += row * (row.x * row.x + row.y * row.y);
        }
        if matrix.determinant().abs() < 1e-12 {
            continue;
        }
        let fit = matrix.inverse() * rhs;
        let center = base + u * fit.x * 0.5 + v * fit.y * 0.5;
        let r2 = fit.z + (fit.x * fit.x + fit.y * fit.y) * 0.25;
        if !r2.is_finite() || r2 <= 1e-8 {
            continue;
        }
        let radius = r2.sqrt();
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        // STL exporters insert vertices on polygon chords as well as on the
        // source circle. Allow bounded tessellation error, not exact-CAD error.
        let tolerance = fit_tolerance(radius);
        let mut valid = true;
        for &i in &vertices {
            let d = points[i as usize] - center;
            let z = d.dot(axis);
            lo = lo.min(z);
            hi = hi.max(z);
            if ((d - axis * z).length() - radius).abs() > tolerance {
                valid = false;
                break;
            }
        }
        if !valid || hi - lo < tolerance * 4. {
            continue;
        }
        // Complete the band by geometry after fitting. Exporter slivers may
        // have unreliable chord normals although their vertices lie on it.
        let seed_target = Target {
            body_id,
            face_id: 0x8000_0000 | seed as u32,
            origin: (center + axis * lo).to_array(),
            axis: axis.to_array(),
            radius,
            length: hi - lo,
            internal: n.dot(base - center) < 0.,
        };
        let mut band = HashSet::new();
        let mut queue = vec![seed];
        while let Some(i) = queue.pop() {
            if band.contains(&i) || !seed_target.contains_triangle(mesh, &tris[i]) {
                continue;
            }
            band.insert(i);
            queue.extend(neighbors[i].iter().copied());
        }
        patch = band.iter().copied().collect();
        patch.sort_unstable();
        let patch_set: HashSet<_> = patch.iter().copied().collect();
        // Only two closed end rings are allowed. A partial arc, missing facet,
        // intersecting hole, or broken seam has axial boundary edges and fails.
        let mut boundary = HashMap::<usize, usize>::new();
        for &i in &patch {
            let t = tris[i];
            let w = t.map(|x| ids[x as usize]);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let edge = (w[a].min(w[b]), w[a].max(w[b]));
                let incident = &edges[&edge];
                if incident.iter().filter(|j| patch_set.contains(j)).count() == 2 {
                    continue;
                }
                let za = (points[t[a] as usize] - center).dot(axis);
                let zb = (points[t[b] as usize] - center).dot(axis);
                if !((za - lo).abs() < tolerance && (zb - lo).abs() < tolerance
                    || (za - hi).abs() < tolerance && (zb - hi).abs() < tolerance)
                {
                    valid = false;
                    break;
                }
                *boundary.entry(w[a]).or_default() += 1;
                *boundary.entry(w[b]).or_default() += 1;
            }
            if !valid {
                break;
            }
        }
        if !valid
            || boundary.len() < 24
            || boundary.values().any(|&n| n != 2)
            || !seed_target.matches_full_wall(mesh)
        {
            // A mesh fillet can meet the straight strip at slightly different
            // axial heights around the circumference. Crop to the common
            // interior interval, then run the full closed-ring classifier on
            // that temporary mesh. The actual document geometry is unchanged.
            if allow_trim
                && trim_attempts.len() < 8
                && !trim_attempts.iter().any(|t| t.same_wall(&seed_target))
            {
                trim_attempts.push(seed_target.clone());
                if let Some(target) = trimmed_band(mesh, &seed_target) {
                    for (i, tri) in tris.iter().enumerate() {
                        if target.contains_triangle(mesh, tri) {
                            accepted[i] = true;
                        }
                    }
                    result.push(target);
                }
            }
            continue;
        }
        let radial = base - center - axis * (base - center).dot(axis);
        let target = Target {
            body_id,
            face_id: 0x8000_0000 | seed as u32,
            origin: (center + axis * lo).to_array(),
            axis: axis.to_array(),
            radius,
            length: hi - lo,
            internal: n.dot(radial) < 0.,
        };
        if !target.matches_full_wall(mesh) {
            continue;
        }
        for i in patch {
            accepted[i] = true;
        }
        result.push(target);
    }
    result
}

fn trimmed_band(mesh: &MeshData, seed: &Target) -> Option<Target> {
    let axis = DVec3::from_array(seed.axis);
    let origin = DVec3::from_array(seed.origin);
    let tolerance = fit_tolerance(seed.radius);
    let mut ids = HashMap::new();
    let mut positions = Vec::new();
    let remap: Vec<_> = mesh
        .positions
        .iter()
        .map(|p| {
            let key = p.map(|x| (f64::from(x) / 1e-5).round() as i64);
            let next = ids.len();
            *ids.entry(key).or_insert_with(|| {
                positions.push(DVec3::from_array(p.map(f64::from)));
                next
            })
        })
        .collect();
    let mut edges = HashMap::<(usize, usize), usize>::new();
    for tri in mesh.indices.chunks_exact(3) {
        if !seed.contains_triangle(mesh, tri) {
            continue;
        }
        let p = tri
            .iter()
            .map(|&i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)))
            .collect::<Vec<_>>();
        let cross = (p[1] - p[0]).cross(p[2] - p[0]);
        let area_twice = cross.length();
        let normal = cross.normalize_or_zero();
        // This fallback deliberately excludes the curved transition. The
        // original growth still supports coarse staggered STL cylinder rings.
        // Boolean seams can contain very thin wall triangles. After a rigid
        // f32 transform, coordinate rounding amplifies their normal error;
        // a fixed angular cutoff would remove them and invent an axial gap.
        // Bound the allowance by conditioning, and still exclude the curved
        // transition. Closed rings and full-wall certification remain required.
        let coordinate = p.iter().map(|p| p.abs().max_element()).fold(1., f64::max);
        let perimeter = p[0].distance(p[1]) + p[1].distance(p[2]) + p[2].distance(p[0]);
        let uncertainty =
            coordinate * f64::from(f32::EPSILON) * 2. * perimeter / area_twice.max(1e-20);
        let angular_tolerance = 1e-4 + uncertainty.min(9e-4);
        if normal.length_squared() < 0.5 || normal.dot(axis).abs() > angular_tolerance {
            continue;
        }
        let w = [tri[0], tri[1], tri[2]].map(|i| remap[i as usize]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue;
        }
        for (a, b) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut lo: f64 = 0.;
    let mut hi = seed.length;
    let mut low = false;
    let mut high = false;
    for ((a, b), count) in edges {
        if count == 2 {
            continue;
        }
        if count != 1 {
            return None;
        }
        let za = (positions[a] - origin).dot(axis);
        let zb = (positions[b] - origin).dot(axis);
        if za.max(zb) < seed.length * 0.2 {
            lo = lo.max(za.max(zb));
            low = true;
        } else if za.min(zb) > seed.length * 0.8 {
            hi = hi.min(za.min(zb));
            high = true;
        } else {
            return None;
        }
    }
    if !low
        || !high
        || hi - lo < tolerance * 4.
        || lo <= tolerance * 0.1 && seed.length - hi <= tolerance * 0.1
    {
        return None;
    }
    let clipped =
        s33d_kernel::mesh_bool::split_by_plane(mesh, (origin + axis * lo).to_array(), seed.axis)
            .ok()?
            .0;
    let clipped = s33d_kernel::mesh_bool::split_by_plane(
        &clipped,
        (origin + axis * hi).to_array(),
        seed.axis,
    )
    .ok()?
    .1;
    let expected = Target {
        origin: (origin + axis * lo).to_array(),
        length: hi - lo,
        ..seed.clone()
    };
    let mut target = recover(&clipped, seed.body_id, false)
        .into_iter()
        .find(|t| t.same_wall(&expected))?;
    // Synthetic IDs must still refer to triangles of the original mesh.
    target.face_id = seed.face_id;
    target.matches_full_wall(mesh).then_some(target)
}

pub fn fit_tolerance(radius: f64) -> f64 {
    (radius * 0.01).min(0.02).max(2e-5)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staggered_coarse_rings_select_both_complete_walls() {
        let mut mesh = s33d_kernel::mesh_bool::revolve_profile(
            &[[2.25, 0.], [4., 0.], [4., 5.5], [2.25, 5.5]],
            [0.; 3],
            [0., 1., 0.],
            [1., 0., 0.],
            16,
        )
        .unwrap();
        let turn = glam::Mat3::from_rotation_y(std::f32::consts::TAU / 32.);
        for p in &mut mesh.positions {
            if p[1] > 2. {
                *p = (turn * glam::Vec3::from_array(*p)).to_array();
            }
        }
        let walls = targets(&mesh, 1);
        assert_eq!(walls.len(), 2, "Coarse staggered rings: {walls:?}");
        assert_eq!(walls.iter().filter(|t| t.internal).count(), 1);
        for t in walls {
            assert!((t.length - 5.5).abs() < 0.02);
            assert!((t.radius - if t.internal { 2.25 } else { 4. }).abs() < 0.01);
        }
    }
    #[test]
    fn complete_tube_walls_but_not_caps_cones_or_broken_arcs() {
        use s33d_kernel::mesh_bool;
        let tube = mesh_bool::revolve_profile(
            &[[3., 0.], [5., 0.], [5., 10.], [3., 10.]],
            [0.; 3],
            [0., 1., 0.],
            [1., 0., 0.],
            96,
        )
        .unwrap();
        let walls = targets(&tube, 7);
        assert_eq!(walls.len(), 2, "{walls:?}");
        assert_eq!(walls.iter().filter(|t| t.internal).count(), 1);
        for t in &walls {
            assert!((t.length - 10.).abs() < 1e-4);
        }
        let mut tilted = tube.clone();
        let transform = glam::Mat4::from_translation(glam::vec3(24., -12., 33.))
            * glam::Mat4::from_rotation_z(0.61)
            * glam::Mat4::from_rotation_x(0.37);
        for p in &mut tilted.positions {
            *p = transform
                .transform_point3(glam::Vec3::from_array(*p))
                .to_array();
        }
        let tilted_walls = targets(&tilted, 7);
        assert_eq!(tilted_walls.len(), 2, "Inclined tube: {tilted_walls:?}");
        assert_eq!(tilted_walls.iter().filter(|t| t.internal).count(), 1);
        let mut broken = tube.clone();
        let outer = walls.iter().find(|t| !t.internal).unwrap();
        let slot = broken
            .indices
            .chunks_exact(3)
            .position(|tri| outer.contains_triangle(&tube, tri))
            .unwrap()
            * 3;
        broken.indices.drain(slot..slot + 3);
        assert_eq!(
            targets(&broken, 7).len(),
            1,
            "Broken outer wall must not be accepted"
        );
        let cone = mesh_bool::revolve_profile(
            &[[1., 0.], [5., 0.], [3., 10.], [1., 10.]],
            [0.; 3],
            [0., 1., 0.],
            [1., 0., 0.],
            96,
        )
        .unwrap();
        assert!(targets(&cone, 1).iter().all(|t| t.internal));
        let mut ellipse = tube.clone();
        for p in &mut ellipse.positions {
            p[0] *= 1.2;
        }
        assert!(targets(&ellipse, 1).is_empty());
    }
    #[test]
    #[ignore = "local user STL; set S33D_MESH_BORE_FIXTURE"]
    fn mesh_bore_fixture() {
        let path = std::env::var("S33D_MESH_BORE_FIXTURE").unwrap();
        use s33d_kernel::GeometryKernel;
        let mut k = s33d_kernel::TruckKernel::new();
        let shape = k.import_stl(&path).unwrap()[0];
        let mesh = k.tessellate(shape).unwrap();
        let ts = targets(&mesh, 1);
        eprintln!("{}: {} walls: {:?}", path, ts.len(), ts);
        assert!(ts.iter().filter(|t| t.internal).count() >= 6);
    }
}
