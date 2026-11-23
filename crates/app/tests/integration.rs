//! Headless modeling and format-preservation tests; all lengths in mm.
use s33d_kernel::{GeometryKernel, MeshData, PlaneDef, ShapeId, TruckKernel};
use s33d_model::{compute_regions, SketchEntity};
use std::collections::HashMap;
use std::path::PathBuf;

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "s33d-integration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        // Only this test's exclusively created directory and output files.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn box_fixture(kernel: &mut TruckKernel) -> ShapeId {
    let regions = compute_regions(&[SketchEntity::Rect {
        c1: [0., 0.],
        c2: [10., 5.],
    }]);
    assert_eq!(regions.len(), 1);
    assert!((regions[0].signed_area() - 50.).abs() < 1e-6);
    kernel
        .extrude_region(&regions[0], &PlaneDef::from_normal([0., 0., 1.]), 20.)
        .unwrap()
}
/// Check actual mesh topology without repairing holes. Welding only joins
/// coincident format seam vertices; triangle count is not a closure surrogate.
fn closed_oriented_volume(mesh: &MeshData) -> Result<f64, String> {
    if mesh.indices.is_empty() || mesh.indices.len() % 3 != 0 {
        return Err("empty or incomplete triangle list".into());
    }
    if mesh.positions.iter().flatten().any(|x| !x.is_finite())
        || mesh
            .indices
            .iter()
            .any(|&i| i as usize >= mesh.positions.len())
    {
        return Err("nonfinite vertex or invalid index".into());
    }
    let mut mesh = mesh.clone();
    mesh.weld(1e-5);
    let mut edges: HashMap<(u32, u32), (usize, i32)> = HashMap::new();
    let mut volume = 0.;
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]];
        let pa = glam::DVec3::from_array(mesh.positions[a as usize].map(f64::from));
        let pb = glam::DVec3::from_array(mesh.positions[b as usize].map(f64::from));
        let pc = glam::DVec3::from_array(mesh.positions[c as usize].map(f64::from));
        if (pb - pa).cross(pc - pa).length_squared() < 1e-18 {
            return Err("degenerate triangle".into());
        }
        volume += pa.dot(pb.cross(pc)) / 6.;
        for (from, to) in [(a, b), (b, c), (c, a)] {
            let entry = edges.entry((from.min(to), from.max(to))).or_default();
            entry.0 += 1;
            entry.1 += if from < to { 1 } else { -1 };
        }
    }
    if edges
        .values()
        .any(|&(count, direction)| count != 2 || direction != 0)
    {
        return Err("mesh is open, nonmanifold, or inconsistently oriented".into());
    }
    if !volume.is_finite() || volume <= 0. {
        return Err("nonpositive oriented volume".into());
    }
    Ok(volume)
}
fn assert_solid(
    kernel: &TruckKernel,
    shape: ShapeId,
    label: &str,
    expected_min: [f32; 3],
    expected_max: [f32; 3],
    expected_volume: f64,
    relative_volume_error: f64,
) -> f64 {
    let mesh = kernel.tessellate(shape).unwrap();
    let (min, max) = mesh.bounds().unwrap();
    for axis in 0..3 {
        assert!(
            (min[axis] - expected_min[axis]).abs() < 1e-3,
            "{label}: min axis {axis}: {min:?}, expected {expected_min:?}"
        );
        assert!(
            (max[axis] - expected_max[axis]).abs() < 1e-3,
            "{label}: max axis {axis}: {max:?}, expected {expected_max:?}"
        );
    }
    let volume = closed_oriented_volume(&mesh).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert!(
        (volume - expected_volume).abs() <= expected_volume * relative_volume_error,
        "{label}: volume {volume} mm³, expected {expected_volume} mm³"
    );
    // Independent native validity check: the mesh conversion requires an
    // oriented closed shell, and runs OCCT BRepCheck_Analyzer before returning.
    let valid = s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh)
        .unwrap_or_else(|e| panic!("{label}: invalid B-Rep: {e}"));
    assert_eq!(valid.solid_count, 1, "{label}: expected one valid solid");
    if let Some(exact) = kernel.exact_shape(shape) {
        assert_eq!(exact.solid_count, 1, "{label}: exact solid count");
    }
    volume
}
#[test]
fn sketch_rect_to_extrude_full_chain() {
    let mut kernel = TruckKernel::new();
    let shape = box_fixture(&mut kernel);
    assert_solid(
        &kernel,
        shape,
        "extrude",
        [0., 0., 0.],
        [10., 5., 20.],
        1000.,
        1e-4,
    );
    let moved = kernel
        .transform(
            shape,
            &glam::Mat4::from_translation(glam::Vec3::new(5., 0., 0.)),
        )
        .unwrap();
    let union = kernel.boolean_union(shape, moved).unwrap();
    assert_solid(
        &kernel,
        union,
        "union",
        [0., 0., 0.],
        [15., 5., 20.],
        1500.,
        1e-4,
    );
    let dir = TestDirectory::new();
    let path = dir.0.join("union.step");
    kernel.export_step(union, path.to_str().unwrap()).unwrap();
    let imported = kernel.import_step(path.to_str().unwrap()).unwrap();
    assert_eq!(imported.len(), 1);
    assert_solid(
        &kernel,
        imported[0],
        "union STEP",
        [-7.5, 0., -10.],
        [7.5, 5., 10.],
        1500.,
        1e-4,
    );
}
#[test]
fn revolve_chain_produces_solid() {
    let regions = compute_regions(&[SketchEntity::Rect {
        c1: [0., 0.],
        c2: [10., 5.],
    }]);
    let mut kernel = TruckKernel::new();
    let shape = kernel
        .revolve_region(
            &regions[0],
            &PlaneDef::from_normal([0., 0., 1.]),
            &s33d_kernel::Axis2D {
                point: [0., 0.],
                dir: [1., 0.],
            },
            90.,
        )
        .unwrap();
    // Quarter cylinder, radius 5 mm and length 10 mm. Curved tessellation has
    // a 1% analytic-volume budget; linear fixtures below use a tighter budget.
    assert_solid(
        &kernel,
        shape,
        "revolve",
        [0., 0., 0.],
        [10., 5., 5.],
        std::f64::consts::PI * 25. * 10. / 4.,
        0.01,
    );
}
#[test]
fn all_formats_roundtrip() {
    let mut kernel = TruckKernel::new();
    let shape = box_fixture(&mut kernel);
    let dir = TestDirectory::new();
    // Unequal dimensions expose unit/axis errors. Imports intentionally center
    // X/Z and place the floor at Y=0, consistently across formats.
    for format in ["step", "stl", "obj", "ply", "glb"] {
        let path = dir.0.join(format!("box.{format}"));
        let path = path.to_str().unwrap();
        let imported = match format {
            "step" => {
                kernel.export_step(shape, path).unwrap();
                kernel.import_step(path)
            }
            "stl" => {
                kernel.export_stl(shape, path).unwrap();
                kernel.import_stl(path)
            }
            "obj" => {
                kernel.export_obj(shape, path).unwrap();
                kernel.import_obj(path)
            }
            "ply" => {
                kernel.export_ply(shape, path).unwrap();
                kernel.import_ply(path)
            }
            "glb" => {
                kernel.export_glb(shape, path).unwrap();
                kernel.import_glb(path)
            }
            _ => unreachable!(),
        }
        .unwrap_or_else(|e| panic!("{format} import: {e}"));
        assert_eq!(imported.len(), 1, "{format}: object count changed");
        assert_solid(
            &kernel,
            imported[0],
            format,
            [-5., 0., -10.],
            [5., 5., 10.],
            1000.,
            1e-4,
        );
    }
}
#[test]
fn split_chain_produces_two_closed_parts() {
    let mut kernel = TruckKernel::new();
    let shape = box_fixture(&mut kernel);
    let mut cut = PlaneDef::from_normal([1., 0., 0.]);
    cut.origin = [5., 0., 0.];
    let mut parts = kernel.split_by_plane(shape, &cut).unwrap();
    assert_eq!(parts.len(), 2);
    parts.sort_by(|a, b| {
        kernel
            .tessellate(*a)
            .unwrap()
            .bounds()
            .unwrap()
            .0
            .x
            .total_cmp(&kernel.tessellate(*b).unwrap().bounds().unwrap().0.x)
    });
    let first = assert_solid(
        &kernel,
        parts[0],
        "split left",
        [0., 0., 0.],
        [5., 5., 20.],
        500.,
        1e-4,
    );
    let second = assert_solid(
        &kernel,
        parts[1],
        "split right",
        [5., 0., 0.],
        [10., 5., 20.],
        500.,
        1e-4,
    );
    assert!((first + second - 1000.).abs() < 0.1, "split lost material");
    cut.origin = [30., 0., 0.];
    let missed = kernel.split_by_plane(shape, &cut).unwrap();
    assert_eq!(missed.len(), 1);
    assert_solid(
        &kernel,
        missed[0],
        "missed split",
        [0., 0., 0.],
        [10., 5., 20.],
        1000.,
        1e-4,
    );
}
#[test]
fn solid_validation_rejects_open_and_reversed_faces() {
    let mut kernel = TruckKernel::new();
    let shape = box_fixture(&mut kernel);
    let mesh = kernel.tessellate(shape).unwrap();
    assert!((closed_oriented_volume(&mesh).unwrap() - 1000.).abs() < 0.1);
    let mut open = mesh.clone();
    open.indices.truncate(open.indices.len() - 3);
    assert!(closed_oriented_volume(&open).is_err());
    let mut flipped = mesh.clone();
    flipped.indices.swap(0, 1);
    assert!(closed_oriented_volume(&flipped).is_err());
    let mut invalid = mesh.clone();
    invalid.positions[0][0] = f32::NAN;
    assert!(closed_oriented_volume(&invalid).is_err());
    let mut scaled = mesh;
    for p in &mut scaled.positions {
        for x in p {
            *x *= 25.4;
        }
    }
    assert!((closed_oriented_volume(&scaled).unwrap() - 1000.).abs() > 1000.);
}
