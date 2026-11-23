//! Production scene/command coverage for the two-hole review counterexample.
use super::*;

fn counterexample(scale: f64, shift: f64, world_origin: f64) -> (S33DApp, BodyId, usize) {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    let point = |x, y| [shift + x * scale, shift + y * scale];
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: "Two-hole review counterexample".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane: PlaneDef {
                origin: [world_origin, 0., 0.],
                ..DatumPlane::Top.def()
            },
            entities: vec![
                SketchEntity::Rect {
                    c1: point(0., 0.),
                    c2: point(100., 100.),
                },
                SketchEntity::Circle {
                    c: point(65., 95.),
                    r: scale,
                },
                SketchEntity::Circle {
                    c: point(85., 52.),
                    r: 5. * scale,
                },
            ],
            world_xyz_coordinates: false,
            on_body: None,
        }),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    let index = app
        .regions_of(id)
        .iter()
        .position(|region| region.holes.len() == 2)
        .unwrap();
    app.select_from_objects(id, false);
    assert!(app.set_region_selection(id, vec![index]));
    (app, id, index)
}

#[test]
fn checked_two_hole_region_is_identical_for_scene_and_modeling_at_all_review_scales() {
    for (scale, shift, world_origin) in [
        (0.001, 0., 0.),
        (1., 0., 0.),
        (1000., 0., 0.),
        (1., 1e6, 0.),
        (1., 1e9, 0.),
        (0.001, 0., 1e6),
    ] {
        let (mut app, id, index) = counterexample(scale, shift, world_origin);
        let checked = app.validated_region_mesh(id, index).unwrap();
        let reference = checked.expected_area;
        app.refresh_scene();
        {
            let scene = app.scene.lock().unwrap();
            assert_eq!(scene.region_surfaces.len(), 1);
            let fill = &scene.region_surfaces[0];
            let plane = &app.body(id).unwrap().sketch().unwrap().plane;
            let mut actual = 0.;
            for indices in fill.indices.chunks_exact(3) {
                let [a, b, c] = [indices[0], indices[1], indices[2]]
                    .map(|index| glam::DVec3::from_array(fill.positions[index as usize]));
                actual += (b - a).cross(c - a).length() * 0.5;
                for position in [a, b, c] {
                    let distance =
                        (position - glam::DVec3::from_array(plane.origin)).dot(plane.normal());
                    assert!(
                        distance.abs() < 1e-12,
                        "no physical normal offset: {distance}"
                    );
                }
            }
            assert!(
                (actual - reference).abs() <= reference * 1e-7,
                "scale={scale}, shift={shift}: scene={actual}, expected={reference}"
            );
            assert!(!scene.region_lines.positions.is_empty());
        }
        let (_, _, regions, indices) = app.selected_profile_source("Extrude").unwrap();
        assert_eq!(indices, vec![index]);
        assert_eq!(regions[0].holes.len(), 2);
        let again = app.validated_region_mesh(id, index).unwrap();
        assert!(
            Arc::ptr_eq(&checked, &again),
            "render and modeling reuse checked geometry"
        );
    }
}

#[test]
fn geometry_edit_cannot_leave_an_old_filled_region_in_the_scene() {
    let (mut app, id, index) = counterexample(1., 0., 0.);
    let original = app.validated_region_mesh(id, index).unwrap();
    app.refresh_scene();
    let Feature::Sketch(sketch) = &mut app.doc.body_mut(id).unwrap().feature else {
        unreachable!()
    };
    sketch.entities[1] = SketchEntity::Circle {
        c: [65., 95.],
        r: 2.,
    };
    app.bump_scene();
    app.refresh_scene();
    assert!(app.scene.lock().unwrap().region_surfaces.is_empty());
    let index = app
        .regions_of(id)
        .iter()
        .position(|region| region.holes.len() == 2)
        .unwrap();
    assert!(app.set_region_selection(id, vec![index]));
    let revised = app.validated_region_mesh(id, index).unwrap();
    assert!(!Arc::ptr_eq(&original, &revised));
    assert!(revised.expected_area < original.expected_area);
}

#[test]
fn invalid_cap_aborts_all_regions_before_allocating_a_solid() {
    let mut kernel = TruckKernel::new();
    let valid = Region2D::new(vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]]);
    let invalid = Region2D {
        outer: valid.outer.clone(),
        holes: vec![vec![[9., 4.], [11., 4.], [11., 6.], [9., 6.]]],
    };
    let sketch = SketchFeature {
        vertex_locks: Default::default(),
        plane: DatumPlane::Top.def(),
        entities: vec![],
        world_xyz_coordinates: false,
        on_body: None,
    };
    let count = kernel.shape_count();
    let error = build_extrude_shape(
        &mut kernel,
        &sketch,
        &[valid, invalid],
        &ExtrudeParams {
            depth: 2.,
            symmetric: false,
            bevel: 0.,
        },
    )
    .unwrap_err();
    assert!(error.contains("region 1"), "{error}");
    assert_eq!(kernel.shape_count(), count);
}

#[test]
fn text_region_failure_is_an_error_without_partial_group_selection() {
    let (mut app, id, index) = counterexample(1., 0., 0.);
    let text = crate::sketch_text::outline_text("B8", crate::sketch_text::FONTS[0], 10., [0., 0.])
        .unwrap();
    let Feature::Sketch(sketch) = &mut app.doc.body_mut(id).unwrap().feature else {
        unreachable!()
    };
    sketch.entities = vec![text];
    let revision = s33d_model::entities_fingerprint(&sketch.entities);
    // Inject a rejected region to exercise the consumer's failure branch,
    // independently of the polygonizer and the backend's own tests.
    let invalid = Region2D::new(vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]]);
    app.regions_cache.insert(id, (revision, vec![invalid]));
    let before = (app.selected.clone(), app.selected_regions.clone());
    assert!(app.text_region_group(id, index).is_err());
    assert_eq!((app.selected.clone(), app.selected_regions.clone()), before);
}

#[test]
fn opening_a_document_resets_cached_failures_and_their_notification_state() {
    let (mut app, id, index) = counterexample(1., 0., 0.);
    let revision =
        s33d_model::entities_fingerprint(&app.body(id).unwrap().sketch().unwrap().entities);
    let invalid = Region2D::new(vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]]);
    assert!(app
        .region_mesh_cache
        .resolve(id, revision, index, &invalid)
        .0
        .is_err());
    assert!(
        !app.region_mesh_cache
            .resolve(id, revision, index, &invalid)
            .1
    );
    app.reset_document_tools();
    let (result, fresh) = app.region_mesh_cache.resolve(id, revision, index, &invalid);
    assert!(result.is_err());
    assert!(
        fresh,
        "the newly opened document must receive its own warning"
    );
}

/// Reproducible saved input for a real desktop double-click acceptance pass.
#[test]
#[ignore = "writes the explicit S33D_REGION_FIXTURE path for desktop acceptance"]
fn write_region_desktop_fixture() {
    let path =
        PathBuf::from(std::env::var_os("S33D_REGION_FIXTURE").expect("explicit output path"));
    let (mut app, _, _) = counterexample(1., 0., 0.);
    app.clear_region_selection();
    app.selected.clear();
    app.doc.rename("Two-hole region acceptance");
    app.camera.target = glam::Vec3::new(50., 0., -50.);
    app.camera.eye = glam::Vec3::new(50., 180., -50.);
    app.camera.up = glam::Vec3::NEG_Z;
    app.camera.ortho = true;
    app.camera.ortho_half = 70.;
    app.save_project_to_path(&path).unwrap();
    assert!(path.is_file());
}
