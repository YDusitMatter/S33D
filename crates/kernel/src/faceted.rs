//! A precision-preserving boundary between display meshes and solid B-Rep.
use crate::{KernelError, MeshData, Result};
use manifold_rust::{
    manifold::Manifold,
    types::{Error, MeshGL, MeshGL64},
};

pub(crate) struct Facets {
    pub positions: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
}

pub(crate) fn validate_mesh(mesh: &MeshData) -> Result<()> {
    if mesh.positions.is_empty()
        || mesh.indices.is_empty()
        || mesh.indices.len() % 3 != 0
        || mesh.positions.iter().flatten().any(|p| !p.is_finite())
        || mesh
            .indices
            .iter()
            .any(|&i| i as usize >= mesh.positions.len())
    {
        return Err(KernelError::Geometry(
            "Cannot export an empty or invalid triangle mesh".into(),
        ));
    }
    Ok(())
}

pub(crate) fn prepare(mesh: &MeshData) -> Result<Facets> {
    validate_mesh(mesh)?;
    let fail = |s: &str| KernelError::Geometry(format!("Cannot convert mesh to a STEP solid: {s}"));
    if mesh.indices.len() > 300_000 {
        return Err(fail("maximum 100000 triangles"));
    }
    // Merge exact position duplicates only (including independent face seams).
    // A distance weld can erase the small clearances between mating threads.
    let mut vertices = std::collections::HashMap::new();
    let mut gl = MeshGL64 {
        num_prop: 3,
        ..Default::default()
    };
    let mut remap = Vec::with_capacity(mesh.positions.len());
    for p in &mesh.positions {
        let key = p.map(|x| if x == 0. { 0 } else { x.to_bits() });
        let index = *vertices.entry(key).or_insert_with(|| {
            let index = (gl.vert_properties.len() / 3) as u64;
            gl.vert_properties.extend(p.map(f64::from));
            index
        });
        remap.push(index);
    }
    gl.tri_verts = mesh.indices.iter().map(|&i| remap[i as usize]).collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &index in &mesh.indices {
        let p = mesh.positions[index as usize].map(f64::from);
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    let extent = (0..3).map(|i| hi[i] - lo[i]).fold(0., f64::max);
    let tolerance = (extent * 5e-7).clamp(1e-7, 1e-5);
    let mut source = Manifold::from_mesh_gl64(&gl);
    if source.status() != Error::NoError {
        // Independent tessellation seams can differ by the last float bits
        // (e.g. sin(2*pi)). Merge only vertices on open boundaries; never weld
        // the already closed fine topology of a thread.
        let mut seams = MeshGL {
            num_prop: 3,
            vert_properties: gl.vert_properties.iter().map(|&x| x as f32).collect(),
            tri_verts: gl.tri_verts.iter().map(|&i| i as u32).collect(),
            tolerance: tolerance as f32,
            ..Default::default()
        };
        seams.merge();
        gl.merge_from_vert = seams.merge_from_vert.into_iter().map(u64::from).collect();
        gl.merge_to_vert = seams.merge_to_vert.into_iter().map(u64::from).collect();
        source = Manifold::from_mesh_gl64(&gl);
    }
    if source.status() != Error::NoError || source.is_empty() {
        return Err(fail(
            "mesh has open boundaries, inconsistent winding or non-manifold edges",
        ));
    }
    // Collapse numerical slivers with the manifold's topology-aware operation.
    // Keep f64 all the way into OCCT: narrowing back to display f32 creates
    // zero-area triangles again. Edge-collapse tolerance is at most 0.00001 mm;
    // bound cumulative surface sampling error separately below.
    // MeshGL's tolerance also sets the edge-collapse epsilon on ingestion;
    // Simplify alone leaves short original edges below OCCT precision.
    gl.tolerance = tolerance;
    let mut clean = Manifold::from_mesh_gl64(&gl).simplify(tolerance);
    if clean.status() == Error::NoError && clean.has_self_intersections() {
        // The default manifold engine is Exact: NoError certifies paired
        // topology, not a non-intersecting boundary. Use the rational robust
        // arrangement to resolve numerical contacts, then certify the result.
        clean = clean.union_with_engine(
            &clean.as_original(),
            manifold_rust::types::BooleanEngine::Robust,
        );
    }
    if clean.status() != Error::NoError || clean.is_empty() || clean.has_self_intersections() {
        return Err(fail(
            "mesh contains self-intersections or cannot be conditioned safely",
        ));
    }
    let volume = source.volume();
    if !volume.is_finite()
        || volume <= 0.
        || (clean.volume() - volume).abs() > (volume * 1e-6).max(tolerance.powi(3) * 10.)
    {
        return Err(fail("conditioning would change the solid volume"));
    }
    let gl = clean.get_mesh_gl64(-1);
    let positions: Vec<[f64; 3]> = gl
        .vert_properties
        .chunks_exact(gl.num_prop as usize)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    for i in 0..3 {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for p in &positions {
            if !p[i].is_finite() {
                return Err(fail("conditioning produced a non-finite vertex"));
            }
            min = min.min(p[i]);
            max = max.max(p[i]);
        }
        if (min - lo[i]).abs() > tolerance * 2. || (max - hi[i]).abs() > tolerance * 2. {
            return Err(fail("conditioning would change the solid bounds"));
        }
    }
    let indices = gl
        .tri_verts
        .into_iter()
        .map(|i| u32::try_from(i).map_err(|_| fail("too many vertices")))
        .collect::<Result<_>>()?;
    let facets = Facets { positions, indices };
    let original = Facets {
        positions: mesh.positions.iter().map(|p| p.map(f64::from)).collect(),
        indices: mesh.indices.clone(),
    };
    // Consecutive collinear collapses accumulate several epsilon-sized moves.
    // Reject material changes beyond 0.00008 mm at bidirectional face samples.
    // A legacy mesh can fold back on itself along an overlapping Boolean
    // seam. Permit the disappearance of such samples only when their two
    // opposite sides cancel within the smaller welding tolerance.
    if !crate::facet_distance::within(&original, &facets, tolerance * 8., None)
        && !(source.has_self_intersections()
            && crate::facet_distance::within(&original, &facets, tolerance * 8., Some(tolerance)))
    {
        return Err(fail(
            "conditioning would move the boundary beyond the numerical tolerance",
        ));
    }
    Ok(facets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires a copy of the local display holder project"]
    fn display_holder_step_export() {
        let project = std::env::var("S33D_DISPLAY_HOLDER_PROJECT").unwrap();
        let contents = std::fs::read_to_string(project).unwrap();
        let archive: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let body = archive["document"]["current"]["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "Chamfer 14")
            .unwrap();
        let shape = archive["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["old_id"] == body["shape"])
            .unwrap();
        let mesh: MeshData = serde_json::from_value(shape["mesh"].clone()).unwrap();
        let step = crate::occt::ExactShape::export_mesh_step(&mesh).unwrap();
        assert!(step.contains("MANIFOLD_SOLID_BREP"));
        let imported = crate::occt::ExactShape::import_step(&step).unwrap();
        assert_eq!(imported.len(), 1);
    }
}
