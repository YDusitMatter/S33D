//! Conservative selection of a recessed floor through its consumed profile.
use super::*;

fn segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let point = glam::DVec2::from_array(point);
    let a = glam::DVec2::from_array(a);
    let delta = glam::DVec2::from_array(b) - a;
    let t = ((point - a).dot(delta) / delta.length_squared().max(1e-24)).clamp(0., 1.);
    point.distance(a + delta * t)
}

fn segments(region: &Region2D) -> Vec<([f64; 2], [f64; 2])> {
    std::iter::once(&region.outer)
        .chain(&region.holes)
        .flat_map(|ring| (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()])))
        .collect()
}

fn follows(from: &[([f64; 2], [f64; 2])], to: &[([f64; 2], [f64; 2])], tolerance: f64) -> bool {
    !from.is_empty()
        && from.iter().all(|&(a, b)| {
            [a, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]]
                .iter()
                .all(|&p| {
                    to.iter()
                        .any(|&(c, d)| segment_distance(p, c, d) <= tolerance)
                })
        })
}

impl S33DApp {
    /// Ownership alone is insufficient: require a matching recessed planar
    /// footprint AND an actual mouth rim at the sketch plane. This keeps moved,
    /// edited, unrelated and coplanar sketches selectable by the usual rule.
    pub(super) fn pocket_bottom_matches(
        &mut self,
        face: &FaceHit,
        sketch: &SketchFeature,
        region: &Region2D,
    ) -> bool {
        let normal = sketch.plane.normal();
        let face_normal = glam::DVec3::from_array(face.normal.map(f64::from));
        let center = glam::DVec3::from_array(face.center.map(f64::from));
        let offset = (center - sketch.plane.origin()).dot(normal);
        let area = s33d_kernel::region::signed_area(&region.outer).abs()
            - region
                .holes
                .iter()
                .map(|ring| s33d_kernel::region::signed_area(ring).abs())
                .sum::<f64>();
        if normal.dot(face_normal) < 1. - 1e-6 || !area.is_finite() || area <= 0. {
            return false;
        }
        let tolerance = (area.sqrt() * 0.001).clamp(1e-4, 0.02);
        if offset >= -tolerance || (face.area_mm2 - area).abs() > area * 0.01 {
            return false;
        }
        let Some(shape) = self.body(face.body_id).and_then(|body| body.shape) else {
            return false;
        };
        let mesh = self.mesh_for(shape);
        if face.triangles.iter().any(|&i| {
            mesh.positions.get(i as usize).is_none_or(|p| {
                let d =
                    (glam::DVec3::from_array(p.map(f64::from)) - sketch.plane.origin()).dot(normal);
                (d - offset).abs() > tolerance
            })
        }) {
            return false;
        }
        let Ok((floor_plane, mut floor)) = bounded_planar_face_profile(&mesh, face) else {
            return false;
        };
        if floor.holes.len() != region.holes.len() {
            return false;
        }
        for ring in std::iter::once(&mut floor.outer).chain(&mut floor.holes) {
            for point in ring {
                let (u, v) = sketch
                    .plane
                    .to_local(floor_plane.to_world(point[0], point[1]));
                *point = [u, v];
            }
        }
        let expected = segments(region);
        let actual = segments(&floor);
        if !follows(&expected, &actual, tolerance) || !follows(&actual, &expected, tolerance) {
            return false;
        }
        // Top edges of walls descending from the sketch plane certify the
        // opening. A floating profile with the same footprint is not enough.
        let mut mouth = Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            let points = [tri[0], tri[1], tri[2]]
                .map(|i| glam::DVec3::from_array(mesh.positions[i as usize].map(f64::from)));
            let distances = points.map(|point| (point - sketch.plane.origin()).dot(normal));
            for (a, b, c) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
                if distances[a].abs() <= tolerance
                    && distances[b].abs() <= tolerance
                    && distances[c] < -tolerance
                {
                    let local = |p: glam::DVec3| {
                        let (u, v) = sketch.plane.to_local(p.to_array());
                        [u, v]
                    };
                    mouth.push((local(points[a]), local(points[b])));
                }
            }
        }
        follows(&expected, &mouth, tolerance)
    }
}
