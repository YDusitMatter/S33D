//! Exercise the real Sketch -> Extrude/Pocket path, not an analytic tube proxy.
use super::*;
use std::time::Instant;

fn fixture(oblique: bool, depth: f64, symmetric: bool) -> (S33DApp, BodyId, BodyId, Target) {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    let mut plane = if oblique {
        PlaneDef::from_normal(DVec3::new(1., 2., 3.).normalize().to_array())
    } else {
        DatumPlane::Front.def()
    };
    plane.origin = [11., -4., 20.];
    let sketch = SketchFeature {
        vertex_locks: Default::default(),
        plane,
        on_body: None,
        world_xyz_coordinates: false,
        entities: vec![
            SketchEntity::Rect {
                c1: [-12., -10.],
                c2: [12., 10.],
            },
            SketchEntity::Circle {
                c: [-4., 0.],
                r: 3.,
            },
            SketchEntity::Circle { c: [5., 0.], r: 2. },
        ],
    };
    let region = s33d_model::compute_regions(&sketch.entities)
        .iter()
        .position(|r| r.holes.len() == 2)
        .unwrap();
    let sketch_id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id: sketch_id,
        name: "Two-hole sketch".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(sketch),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    app.selected = vec![sketch_id];
    assert!(app.set_region_selection(sketch_id, vec![region]));
    app.start_extrude();
    app.extrude_ctx
        .as_mut()
        .expect("Actual extrusion tool must open")
        .symmetric = symmetric;
    app.set_extrude_depth(depth);
    app.commit_extrude();
    assert_eq!(app.mode, Mode::Idle);
    let id = app.selected[0];
    assert_ne!(id, sketch_id);
    assert!(matches!(
        app.doc.body(id).unwrap().feature,
        Feature::Extrude { .. }
    ));
    assert!(
        app.kernel
            .exact_shape(app.doc.body(id).unwrap().shape.unwrap())
            .is_none(),
        "Reproducer must begin with the formerly rejected faceted sketch extrusion"
    );
    let target = app
        .thread_targets(&[id])
        .unwrap()
        .into_iter()
        .find(|t| t.internal && (t.radius - 3.).abs() < 0.01)
        .expect("Recognized original bore");
    (app, id, sketch_id, target)
}

fn selected(app: &mut S33DApp) -> Target {
    let face = app.face_pick.clone().expect("Keep corrected wall selected");
    app.thread_target_for_face(&face)
        .expect("Selection uses new display mesh")
}

fn finish(app: &mut S33DApp) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while app.model_job.is_some() {
        assert!(Instant::now() < deadline, "Correction timeout");
        app.poll_model_job();
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn sketch_extrude_correct_changes_real_bore_then_threads_and_roundtrips() {
    for (preset, oblique, depth, symmetric) in [
        ("M6", false, 12., false),
        ("M8", false, 12., false),
        ("M8", true, 12., true),
        ("M6", true, -12., false),
    ] {
        let (mut app, id, sketch_id, target) = fixture(oblique, depth, symmetric);
        let promoted = s33d_kernel::occt::ExactShape::from_faceted_mesh(
            &app.kernel
                .tessellate(app.doc.body(id).unwrap().shape.unwrap())
                .unwrap(),
        )
        .unwrap();
        s33d_kernel::occt::ExactShape::from_brep(&promoted.brep)
            .unwrap_or_else(|error| panic!("promoted {preset}, oblique={oblique}: {error}"));
        let old_body = app.doc.body(id).unwrap().clone();
        let old_sketch = app.doc.body(sketch_id).unwrap().clone();
        let untouched = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal && (t.radius - 2.).abs() < 0.01)
            .unwrap();
        let mut params = crate::threads::preset_params(preset, true).unwrap();
        params.length = 5.;
        params.offset = 1.;
        params.radial_fit = 0.05;
        app.thread_params = params.clone();
        assert!(app.cylinder_diameter_editable(&target));
        app.correct_thread_blank(target).unwrap_or_else(|error| {
            panic!("{preset}, oblique={oblique}, depth={depth}, symmetric={symmetric}: {error}")
        });
        let corrected = selected(&mut app);
        assert!((corrected.radius * 2. - (params.diameter - 2. * params.depth)).abs() < 1e-5);
        assert_eq!(app.thread_params, params);
        assert_eq!(
            app.doc.body(sketch_id).unwrap(),
            &old_sketch,
            "Do not change a shared source sketch"
        );
        assert_eq!(app.doc.body(id).unwrap().name, old_body.name);
        assert_eq!(app.doc.body(id).unwrap().material, old_body.material);
        let walls = app.thread_targets(&[id]).unwrap();
        assert_eq!(
            walls.iter().filter(|t| t.internal).count(),
            2,
            "No residual chordal wall: {walls:?}"
        );
        assert!(
            walls.iter().any(|t| t.same_wall(&untouched)),
            "Other hole changed"
        );
        let shape = app.doc.body(id).unwrap().shape.unwrap();
        assert_eq!(
            app.kernel.exact_shape(shape).unwrap().solid_count,
            1,
            "No detached thin sliver"
        );
        let mesh = app.kernel.tessellate(shape).unwrap();
        let origin = DVec3::from_array(corrected.origin);
        let axis = DVec3::from_array(corrected.axis);
        for i in &mesh.indices {
            let d = DVec3::from_array(mesh.positions[*i as usize].map(f64::from)) - origin;
            assert!(
                (d - axis * d.dot(axis)).length() >= corrected.radius - 1e-4,
                "No referenced vertex may remain inside the corrected bore"
            );
        }
        let resized = app.doc.body(id).unwrap().clone();
        let project = app.project_to_s33d().unwrap();
        let mut reopened = S33DApp::from_ui_state(UiStateFile::default(), false);
        reopened.load_s33d_project(&project).unwrap_or_else(|error| {
            panic!("reopen {preset}, oblique={oblique}, depth={depth}, symmetric={symmetric}: {error}")
        });
        assert!(reopened
            .thread_targets(&[id])
            .unwrap()
            .iter()
            .any(|t| t.same_wall(&corrected)));
        app.apply_threads(vec![corrected], params.clone()).unwrap();
        let Feature::Derived { source, .. } = &app.doc.body(id).unwrap().feature else {
            panic!()
        };
        let saved: crate::threads::Saved =
            serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
        assert_eq!(saved.params, params);
        assert!(app.undo());
        assert_eq!(app.doc.body(id).unwrap(), &resized);
        assert!(app.undo());
        assert_eq!(app.doc.body(id).unwrap(), &old_body);
        assert!(app.redo());
        assert_eq!(app.doc.body(id).unwrap(), &resized);
    }
}

#[test]
fn sketch_extrude_correct_is_atomic_on_cancel_interference_and_stale_selection() {
    let (mut app, id, _, target) = fixture(false, 12., false);
    let before = app.doc.bodies.clone();
    let history = app.history.clone();
    app.thread_params = crate::threads::preset_params("M8", true).unwrap();
    app.background_modeling = true;
    app.thread_ui.expanded = true;
    app.correct_thread_blank(target.clone()).unwrap();
    assert_eq!(app.doc.bodies, before);
    app.cancel_model_job();
    app.poll_model_job();
    assert_eq!(app.doc.bodies, before);
    assert_eq!(app.history, history);
    app.correct_thread_blank(target).unwrap();
    finish(&mut app);
    let corrected = selected(&mut app);
    assert!(app.thread_ui.expanded && app.open_thread_panel);
    assert_eq!(app.thread_params.diameter, 8.);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, before);
    app.background_modeling = false;
    let mut stale = corrected;
    stale.face_id += 1000;
    assert!(app.correct_thread_blank(stale).is_err());
    let target = app
        .thread_targets(&[id])
        .unwrap()
        .into_iter()
        .find(|t| (t.radius - 3.).abs() < 0.01)
        .unwrap();
    let before_history = app.history.clone();
    app.thread_params.diameter = 18.; // Enlarging this hole would merge with its neighbor.
    assert!(app.correct_thread_blank(target).is_err());
    assert_eq!(app.doc.bodies, before);
    assert_eq!(app.history, before_history);
}

#[test]
fn sketch_pocket_faceted_bore_can_be_corrected_without_changing_other_holes() {
    for through in [false, true] {
        let (mut app, id, _, _) = fixture(false, 12., false);
        let mut plane = DatumPlane::Front.def();
        plane.origin = [11., -4., 32.];
        let sketch_id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: sketch_id,
            name: "Pocket profile".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(SketchFeature {
                vertex_locks: Default::default(),
                plane,
                entities: vec![SketchEntity::Circle {
                    c: [0., 6.],
                    r: 1.5,
                }],
                on_body: Some(id),
                world_xyz_coordinates: false,
            }),
            shape: None,
            material: Material::default(),
            visible: true,
        });
        app.selected = vec![sketch_id];
        assert!(app.set_region_selection(sketch_id, vec![0]));
        app.start_pocket();
        let ctx = app.extrude_ctx.as_mut().unwrap();
        ctx.target_body = Some(id);
        ctx.through = through;
        app.set_extrude_depth(6.);
        app.commit_extrude();
        let target = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal && (t.radius - 1.5).abs() < 0.01)
            .unwrap();
        let remaining: Vec<_> = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .filter(|t| !t.same_wall(&target))
            .collect();
        app.thread_params = crate::threads::preset_params("M4", true).unwrap();
        app.thread_params.length = 4.;
        app.correct_thread_blank(target).unwrap();
        assert!(app.pocket_sources(id).contains(&sketch_id));
        let corrected = selected(&mut app);
        crate::threads::validate(&corrected, &app.thread_params).unwrap();
        let walls = app.thread_targets(&[id]).unwrap();
        assert!(remaining
            .iter()
            .all(|t| walls.iter().any(|u| u.same_wall(t))));
        // A second correction must retain consumed-profile provenance too.
        app.thread_params.diameter = 4.2;
        app.correct_thread_blank(corrected).unwrap();
        let corrected = selected(&mut app);
        assert!(app.pocket_sources(id).contains(&sketch_id));
        let project = app.project_to_s33d().unwrap();
        let mut reopened = S33DApp::from_ui_state(UiStateFile::default(), false);
        reopened.load_s33d_project(&project).unwrap();
        assert!(reopened.pocket_sources(id).contains(&sketch_id));
        let reopened_target = reopened
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.same_wall(&corrected))
            .unwrap();
        reopened
            .apply_threads(vec![reopened_target], app.thread_params.clone())
            .unwrap();
        assert!(reopened.pocket_sources(id).contains(&sketch_id));
        app.apply_threads(vec![corrected], app.thread_params.clone())
            .unwrap();
        assert!(app.pocket_sources(id).contains(&sketch_id));
    }
}
