//! Export interoperability and file publication regressions.
use super::*;
use serde_json::json;

fn cube(size: f64, offset: [f64; 3]) -> MeshData {
    use manifold_rust::{linalg::Vec3, manifold::Manifold};
    let gl = Manifold::cube(Vec3::new(size, size, size), false).get_mesh_gl(-1);
    MeshData::new(
        gl.vert_properties
            .chunks_exact(gl.num_prop as usize)
            .map(|p| std::array::from_fn(|i| p[i] + offset[i] as f32))
            .collect(),
        gl.tri_verts,
    )
}
fn join(a: &mut MeshData, b: &MeshData, invert: bool) {
    let base = a.positions.len() as u32;
    a.positions.extend_from_slice(&b.positions);
    for t in b.indices.chunks_exact(3) {
        a.indices.extend(if invert {
            [base + t[2], base + t[1], base + t[0]]
        } else {
            [base + t[0], base + t[1], base + t[2]]
        });
    }
}
fn volume(m: &MeshData) -> f64 {
    m.indices
        .chunks_exact(3)
        .map(|t| {
            let p: Vec<_> = t
                .iter()
                .map(|&i| glam::DVec3::from_array(m.positions[i as usize].map(f64::from)))
                .collect();
            p[0].dot(p[1].cross(p[2])) / 6.
        })
        .sum::<f64>()
}
fn temporary(label: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "s33d-export-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}
fn export(k: &TruckKernel, id: ShapeId, path: &std::path::Path, format: &str) -> Result<()> {
    let p = path.to_str().unwrap();
    match format {
        "stl" => k.export_stl(id, p),
        "obj" => k.export_obj(id, p),
        "ply" => k.export_ply(id, p),
        "glb" => k.export_glb(id, p),
        _ => k.export_step(id, p),
    }
}
fn read(path: &std::path::Path, format: &str) -> Vec<MeshData> {
    let p = path.to_str().unwrap();
    match format {
        "stl" => mesh_io::read_stl(p).unwrap(),
        "obj" => mesh_io::read_obj(p).unwrap(),
        "ply" => mesh_io::read_ply(p).unwrap(),
        "glb" => mesh_io::read_glb(p).unwrap(),
        _ => occt::ExactShape::import_step(&std::fs::read_to_string(path).unwrap())
            .unwrap()
            .into_iter()
            .map(|s| {
                assert_eq!(s.solid_count, 1);
                s.mesh
            })
            .collect(),
    }
}
fn bounds_match(original: &MeshData, restored: &MeshData, step: bool, tolerance: f32) {
    let mut expected = original.clone();
    if step {
        for p in &mut expected.positions {
            *p = [p[0], -p[2], p[1]];
        }
    }
    let (a, b) = expected.bounds().unwrap();
    let (c, d) = restored.bounds().unwrap();
    assert!(
        (a - c).abs().max_element() < tolerance,
        "lower bounds {a:?} {c:?}"
    );
    assert!(
        (b - d).abs().max_element() < tolerance,
        "upper bounds {b:?} {d:?}"
    );
}

#[test]
fn all_formats_roundtrip_closed_mesh_and_overwrite_atomically() {
    let mut k = TruckKernel::new();
    let mesh = cube(10., [1., 2., 3.]);
    let id = k.insert_mesh(mesh.clone());
    for format in ["step", "stl", "obj", "ply", "glb"] {
        let path = temporary(format);
        std::fs::write(&path, b"previous export").unwrap();
        export(&k, id, &path, format).unwrap();
        let imported = read(&path, format);
        assert_eq!(imported.len(), 1);
        assert!((volume(&imported[0]) - 1000.).abs() < 0.01);
        bounds_match(&mesh, &imported[0], format == "step", 1e-4);
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn closed_voids_and_disconnected_solids_are_preserved_in_step() {
    let mut k = TruckKernel::new();
    let mut mesh = cube(10., [0.; 3]);
    join(&mut mesh, &cube(6., [2.; 3]), true);
    join(&mut mesh, &cube(2., [20., 0., 0.]), false);
    let path = temporary("cavity");
    let id = k.insert_mesh(mesh);
    export(&k, id, &path, "step").unwrap();
    let mut parts = read(&path, "step");
    assert_eq!(parts.len(), 2);
    parts.sort_by(|a, b| volume(a).total_cmp(&volume(b)));
    assert!((volume(&parts[0]) - 8.).abs() < 1e-4);
    assert!(
        (volume(&parts[1]) - 784.).abs() < 1e-3,
        "Inner cavity must not become solid material"
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
fn collapsed_short_edge_is_conditioned_without_opening_the_shell() {
    let mut mesh = cube(10., [0.; 3]);
    let a = mesh.indices[0];
    let b = mesh.indices[1];
    let pa = glam::Vec3::from_array(mesh.positions[a as usize]);
    let pb = glam::Vec3::from_array(mesh.positions[b as usize]);
    let v = mesh.positions.len() as u32;
    mesh.positions.push((pa + (pb - pa) * 1e-9).to_array());
    let old = std::mem::take(&mut mesh.indices);
    for t in old.chunks_exact(3) {
        let split = (0..3)
            .find(|&j| (t[j] == a && t[(j + 1) % 3] == b) || (t[j] == b && t[(j + 1) % 3] == a));
        if let Some(j) = split {
            let x = t[j];
            let y = t[(j + 1) % 3];
            let z = t[(j + 2) % 3];
            mesh.indices.extend([x, v, z, v, y, z]);
        } else {
            mesh.indices.extend_from_slice(t);
        }
    }
    let mut k = TruckKernel::new();
    let id = k.insert_mesh(mesh);
    let path = temporary("sliver");
    export(&k, id, &path, "step").unwrap();
    assert!((volume(&read(&path, "step")[0]) - 1000.).abs() < 1e-3);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn invalid_geometry_never_truncates_existing_exports() {
    for bad in [
        MeshData::default(),
        MeshData::new(vec![[0.; 3]], vec![0, 1, 2]),
        MeshData::new(vec![[f32::NAN; 3]], vec![0, 0, 0]),
    ] {
        let mut k = TruckKernel::new();
        let id = k.insert_mesh(bad);
        for format in ["step", "stl", "obj", "ply", "glb"] {
            let path = temporary("invalid");
            std::fs::write(&path, b"previous export").unwrap();
            assert!(export(&k, id, &path, format).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"previous export");
            std::fs::remove_file(path).unwrap();
        }
    }
}
#[test]
fn open_and_substantially_intersecting_solids_are_rejected_without_creating_files() {
    let open = MeshData::new(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2],
    );
    let mut overlapping = cube(10., [0.; 3]);
    join(&mut overlapping, &cube(10., [5., 0., 0.]), false);
    for mesh in [open, overlapping] {
        let mut k = TruckKernel::new();
        let id = k.insert_mesh(mesh);
        let path = temporary("rejected");
        assert!(export(&k, id, &path, "step").is_err());
        assert!(!path.exists());
    }
}
#[test]
fn staged_files_are_discarded_on_failed_publication_or_cancel() {
    let path = temporary("staging");
    std::fs::write(&path, b"previous").unwrap();
    let stage = export_file::StagedExport::new(&path).unwrap();
    let temporary = stage.path().to_owned();
    std::fs::write(&temporary, b"cancelled").unwrap();
    drop(stage);
    assert!(!temporary.exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    let stage = export_file::StagedExport::new(&path).unwrap();
    let temporary = stage.path().to_owned();
    assert!(stage.commit().is_err());
    assert!(!temporary.exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    std::fs::remove_file(path).unwrap();
    let path = temporary_directory();
    let stage = export_file::StagedExport::new(&path).unwrap();
    let temporary = stage.path().to_owned();
    std::fs::write(&temporary, b"new").unwrap();
    assert!(stage.commit().is_err());
    assert!(!temporary.exists());
    assert!(path.is_dir());
    std::fs::remove_dir(path).unwrap();
    fn temporary_directory() -> std::path::PathBuf {
        let p = super::export_qa::temporary("directory");
        std::fs::create_dir(&p).unwrap();
        p
    }
}

#[test]
#[ignore = "requires S33D_EXPORT_FIXTURE and S33D_EXPORT_OUTPUT; never edits the fixture"]
fn user_project_export_roundtrip() {
    let project: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("S33D_EXPORT_FIXTURE").unwrap()).unwrap(),
    )
    .unwrap();
    let output = std::path::PathBuf::from(std::env::var_os("S33D_EXPORT_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let mut k = TruckKernel::new();
    let mut report = Vec::new();
    let mut count = 0;
    for body in project["document"]["current"]["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["visible"] == true && b["kind"] == "Solid")
    {
        count += 1;
        let stored = project["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["old_id"] == body["shape"])
            .unwrap();
        let mesh: MeshData = serde_json::from_value(stored["mesh"].clone()).unwrap();
        let id = k.insert_mesh(mesh.clone());
        for format in ["stl", "obj", "ply", "glb", "step"] {
            let path = output.join(format!("body-{}.{}", body["id"], format));
            let start = std::time::Instant::now();
            export(&k, id, &path, format)
                .unwrap_or_else(|e| panic!("Body {} {format}: {e}", body["id"]));
            let imported = read(&path, format);
            assert_eq!(imported.len(), 1);
            let restored = &imported[0];
            let before = volume(&mesh);
            let after = volume(restored);
            assert!(before > 0. && after > 0.);
            assert!(
                (before - after).abs() / before < 2e-6,
                "Body {} volume {before} -> {after}",
                body["id"]
            );
            bounds_match(&mesh, restored, format == "step", 5e-5);
            // The nut's axis must remain a through hole after STEP readback.
            if body["id"] == 11 && format == "step" {
                assert!(restored
                    .positions
                    .iter()
                    .all(|p| (f64::from(p[0]) + 17.).hypot(f64::from(p[1]) - 28.) > 7.9));
            }
            let row = json!({"body":body["id"],"format":format,"bytes":std::fs::metadata(&path).unwrap().len(),"source_volume":before,"restored_volume":after,"solids":imported.len(),"seconds":start.elapsed().as_secs_f64()});
            eprintln!("{row}");
            report.push(row);
            std::fs::write(
                output.join("roundtrip.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
        }
    }
    assert_eq!(count, 2);
    assert_eq!(report.len(), 10);
}
