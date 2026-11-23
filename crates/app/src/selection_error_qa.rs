//! Real intersecting/branched sketch supplied for the November selection bug.
use super::*;

fn fixture_app() -> (S33DApp, BodyId) {
    let entities: Vec<SketchEntity> = serde_json::from_str(include_str!(
        "../../model/tests/fixtures/selection-error-entities.json"
    ))
    .unwrap();
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: "Selection error regression".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane: DatumPlane::Top.def(),
            entities,
            world_xyz_coordinates: false,
            on_body: None,
        }),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    app.selected = vec![id];
    (app, id)
}

fn mesh_volume(mesh: &MeshData) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                .map(|i| glam::DVec3::from_array(mesh.positions[i as usize].map(f64::from)));
            a.dot(b.cross(c)) / 6.
        })
        .sum::<f64>()
        .abs()
}

fn verify_all_regions(app: &mut S33DApp, id: BodyId) {
    let regions = app.regions_of(id);
    assert_eq!(
        regions.len(),
        17,
        "branches must not add or delete material faces"
    );
    for index in 0..regions.len() {
        app.selected = vec![id];
        assert!(app.set_region_selection(id, vec![index]));
        let mesh = app
            .validated_region_mesh(id, index)
            .unwrap_or_else(|error| panic!("region {index}: {error}"));
        app.bump_scene();
        app.refresh_scene();
        {
            let scene = app.scene.lock().unwrap();
            assert_eq!(
                scene.region_surfaces.len(),
                1,
                "region {index} missing fill"
            );
            assert!(!scene.region_lines.positions.is_empty());
            assert_eq!(
                scene.region_surfaces[0].indices.len(),
                mesh.indices.len() * 3
            );
        }
        assert!(app.selected_region_validation_errors().is_empty());
        let (_, sketch, picked, indices) = app.selected_profile_source("Extrude").unwrap();
        assert_eq!(indices, vec![index]);
        assert_eq!(picked.len(), 1);
        let before = app.kernel.shape_count();
        let shape = build_extrude_shape(
            &mut app.kernel,
            &sketch,
            &picked,
            &ExtrudeParams {
                depth: 3.,
                symmetric: false,
                bevel: 0.,
            },
        )
        .unwrap();
        let solid = app.kernel.tessellate(shape).unwrap();
        assert!(solid
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        let expected_volume = mesh.expected_area * 3.;
        let actual_volume = mesh_volume(&solid);
        assert!(
            (actual_volume - expected_volume).abs() < expected_volume * 2e-5 + 1e-7,
            "region {index}: volume {actual_volume}, expected {expected_volume}"
        );
        app.kernel.delete(shape);
        assert_eq!(app.kernel.shape_count(), before);
    }
}

#[test]
fn selection_error_intersections_branches_and_holes_all_render_and_extrude() {
    let (mut app, id) = fixture_app();
    verify_all_regions(&mut app, id);
    let saved = app.project_to_s33d().unwrap();
    app.load_s33d_project(&saved).unwrap();
    assert!(app.selected_regions.is_empty());
    verify_all_regions(&mut app, id);
}

#[test]
fn selection_error_complex_material_region_can_be_picked_in_the_viewport() {
    let (mut app, id) = fixture_app();
    let regions = app.regions_of(id);
    let index = regions
        .iter()
        .position(|region| {
            let expected = s33d_kernel::region::signed_area(&region.outer).abs()
                - region
                    .holes
                    .iter()
                    .map(|ring| s33d_kernel::region::signed_area(ring).abs())
                    .sum::<f64>();
            (expected - 2217.879327149846).abs() < 1e-7
        })
        .expect("the formerly rejected material region");
    let mesh = app.validated_region_mesh(id, index).unwrap();
    let point = mesh
        .representable_interior_point()
        .expect("a stable interior witness");
    let plane = app.body(id).unwrap().sketch().unwrap().plane.clone();
    let world = glam::DVec3::from_array(plane.to_world(point[0], point[1])).as_vec3();
    app.camera.eye = glam::Vec3::new(0., 200., 0.);
    app.camera.target = glam::Vec3::ZERO;
    app.camera.up = glam::Vec3::NEG_Z;
    app.camera.ortho = true;
    app.camera.ortho_half = 80.;
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 900.));
    let ndc = (app.camera.proj_matrix(viewport.width() / viewport.height())
        * app.camera.view_matrix())
    .project_point3(world);
    let screen = egui::pos2(
        viewport.left() + (ndc.x + 1.) * viewport.width() * 0.5,
        viewport.top() + (1. - ndc.y) * viewport.height() * 0.5,
    );
    let (owner, hit, _) = app
        .pick_sketch_region(screen, viewport)
        .expect("actual viewport pick");
    assert_eq!((owner, hit), (id, index));
}

#[test]
#[ignore = "read-only user project acceptance; set S33D_SELECTION_ERROR_PROJECT"]
fn selection_error_saved_user_project_all_regions_acceptance() {
    let path =
        PathBuf::from(std::env::var_os("S33D_SELECTION_ERROR_PROJECT").expect("project path"));
    let original = std::fs::read(&path).unwrap();
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    app.open_project_from_path(&path).unwrap();
    let id = app
        .doc
        .bodies
        .iter()
        .find(|body| body.is_sketch())
        .unwrap()
        .id;
    verify_all_regions(&mut app, id);
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
