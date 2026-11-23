//! Retain complete uncut bands after a partial thread, including old saves.
use super::{Target, ThreadAnnotation};
use glam::DVec3;
use s33d_kernel::MeshData;
use std::collections::{HashMap, HashSet};

/// Subdivide the display mesh at logical cylinder boundaries, without caps or
/// changing the stored solid. Otherwise a long original facet can highlight
/// both the uncut band and the neighboring thread crest.
pub(crate) fn split_band_boundaries(mut mesh: MeshData, bands: &[Target]) -> MeshData {
    let mut planes: Vec<(DVec3, f64)> = Vec::new();
    for band in bands {
        let axis = DVec3::from_array(band.axis);
        let origin = DVec3::from_array(band.origin);
        for offset in [origin.dot(axis), origin.dot(axis) + band.length] {
            if !planes.iter().any(|(n, d)| {
                let alignment = n.dot(axis);
                (alignment.abs() - 1.).abs() < 1e-10 && (offset - d * alignment).abs() < 1e-6
            }) {
                planes.push((axis, offset));
            }
        }
    }
    for (axis, offset) in planes {
        let distances: Vec<_> = mesh
            .positions
            .iter()
            .map(|p| {
                let distance = DVec3::from_array(p.map(f64::from)).dot(axis) - offset;
                if distance.abs() < 1e-6 {
                    0.
                } else {
                    distance
                }
            })
            .collect();
        if !distances.iter().any(|d| *d > 0.) || !distances.iter().any(|d| *d < 0.) {
            continue;
        }
        let mut positions = mesh.positions;
        let mut indices = Vec::with_capacity(mesh.indices.len());
        let mut edges = HashMap::new();
        let mut intersection = |a: u32, b: u32| {
            let key = (a.min(b), a.max(b));
            *edges.entry(key).or_insert_with(|| {
                let pa = DVec3::from_array(positions[a as usize].map(f64::from));
                let pb = DVec3::from_array(positions[b as usize].map(f64::from));
                let da = distances[a as usize];
                let db = distances[b as usize];
                let point = (pb * da - pa * db) / (da - db);
                let id = positions.len() as u32;
                positions.push(point.as_vec3().to_array());
                id
            })
        };
        for tri in mesh.indices.chunks_exact(3) {
            let d = [
                distances[tri[0] as usize],
                distances[tri[1] as usize],
                distances[tri[2] as usize],
            ];
            if !d.iter().any(|d| *d > 0.) || !d.iter().any(|d| *d < 0.) {
                indices.extend_from_slice(tri);
                continue;
            }
            for sign in [-1., 1.] {
                let mut polygon = Vec::with_capacity(4);
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    if d[a] * sign >= 0. {
                        polygon.push(tri[a]);
                    }
                    if d[a] * d[b] < 0. {
                        polygon.push(intersection(tri[a], tri[b]));
                    }
                }
                for i in 1..polygon.len() - 1 {
                    indices.extend([polygon[0], polygon[i], polygon[i + 1]]);
                }
            }
        }
        mesh = MeshData::new(positions, indices);
    }
    mesh
}

/// Seeds and annotations belong to one body. Provenance supplies candidate
/// intervals only; the current mesh must still certify every complete wall.
pub(crate) fn remaining_targets(
    body_id: u64,
    seeds: &[Target],
    annotations: &[ThreadAnnotation],
    mesh: &MeshData,
) -> Vec<Target> {
    let seeds: Vec<_> = seeds
        .iter()
        .chain(annotations.iter().map(|a| &a.target))
        .collect();
    let reserved: HashSet<_> = seeds.iter().map(|t| t.face_id).collect();
    let mut used = HashSet::new();
    // Mesh transforms may reverse winding. Encode it in the reserved band
    // identity so triangle matching can distinguish the tube's opposite wall
    // without recomputing whole-mesh orientation for each hover triangle.
    let volume: f64 = mesh
        .indices
        .chunks_exact(3)
        .map(|tri| {
            let p = |i: usize| DVec3::from_array(mesh.positions[tri[i] as usize].map(f64::from));
            p(0).dot(p(1).cross(p(2)))
        })
        .sum();
    let mut next_id = if volume < 0. {
        0xb000_0000
    } else {
        0xa000_0000
    };
    let mut result: Vec<Target> = Vec::new();
    for seed in seeds {
        let origin = DVec3::from_array(seed.origin);
        let axis = DVec3::from_array(seed.axis);
        if !origin.is_finite()
            || !axis.is_finite()
            || (axis.length() - 1.).abs() > 1e-5
            || !seed.length.is_finite()
            || seed.length <= 0.
            || !seed.radius.is_finite()
            || seed.radius <= 0.
        {
            continue;
        }
        let tolerance = 1e-4_f64.max(seed.radius * 1e-5);
        let mut cuts = Vec::new();
        for annotation in annotations {
            let t = &annotation.target;
            let p = &annotation.params;
            let direction = DVec3::from_array(t.axis);
            let delta = DVec3::from_array(t.origin) - origin;
            let alignment = axis.dot(direction);
            if seed.internal != t.internal
                || !delta.is_finite()
                || !direction.is_finite()
                || (direction.length() - 1.).abs() > 1e-5
                || !t.radius.is_finite()
                || !alignment.is_finite()
                || (alignment.abs() - 1.).abs() > 1e-5
                || (seed.radius - t.radius).abs() > tolerance
                || (delta - axis * delta.dot(axis)).length() > tolerance
                || !p.offset.is_finite()
                || !p.length.is_finite()
                || p.offset < 0.
                || p.length <= 0.
            {
                continue;
            }
            // Each annotation's offset is local to its own selected band;
            // later operations can use a different origin or reversed axis.
            let a = delta.dot(axis) + alignment * p.offset;
            let b = delta.dot(axis) + alignment * (p.offset + p.length);
            let lo = a.min(b).max(0.);
            let hi = a.max(b).min(seed.length);
            if hi > lo {
                cuts.push((lo, hi));
            }
        }
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut intervals = Vec::new();
        let mut cursor: f64 = 0.;
        for (lo, hi) in cuts {
            if lo > cursor {
                intervals.push((cursor, lo));
            }
            cursor = cursor.max(hi);
        }
        if cursor < seed.length {
            intervals.push((cursor, seed.length));
        }
        for (lo, hi) in intervals {
            let mut candidate = Target {
                body_id,
                origin: (origin + axis * lo).to_array(),
                length: hi - lo,
                ..seed.clone()
            };
            // Boolean intersections add vertices along polygon chords, not
            // exactly on the analytic circle. Trimmed walls use the thread-
            // band certificate, while intact native walls retain theirs.
            let needs_id = lo > 0. || hi < seed.length || used.contains(&candidate.face_id);
            if needs_id {
                while reserved.contains(&next_id) || used.contains(&next_id) {
                    next_id += 1;
                }
                candidate.face_id = next_id;
            }
            if result.iter().any(|t| t.same_wall(&candidate)) || !candidate.matches_full_wall(mesh)
            {
                continue;
            }
            // Two residual bands of one native face need distinct identities
            // for selection and batch generation. Preserve existing IDs first.
            if needs_id {
                next_id += 1;
            }
            used.insert(candidate.face_id);
            result.push(candidate);
        }
    }
    result
}
