//! Exercise real Objects selection and modeling consumers, not only Vec helpers.
use super::*;

fn app() -> S33DApp {
    S33DApp::from_ui_state(UiStateFile::default(), false)
}

fn add_sketch(app: &mut S33DApp, name: &str, entities: Vec<SketchEntity>) -> BodyId {
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: name.into(),
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
    id
}

fn annulus_and_rectangles() -> (S33DApp, BodyId, BodyId, usize) {
    let mut app = app();
    let a = add_sketch(
        &mut app,
        "A annulus",
        vec![
            SketchEntity::Rect {
                c1: [0., 0.],
                c2: [10., 10.],
            },
            SketchEntity::Circle { c: [5., 5.], r: 1. },
        ],
    );
    let b = add_sketch(
        &mut app,
        "B two rectangles",
        vec![
            SketchEntity::Rect {
                c1: [20., 0.],
                c2: [22., 2.],
            },
            SketchEntity::Rect {
                c1: [25., 0.],
                c2: [28., 3.],
            },
        ],
    );
    let region = app
        .regions_of(a)
        .iter()
        .position(|r| !r.holes.is_empty())
        .unwrap();
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    (app, a, b, region)
}

#[test]
fn region_selection_objects_additive_owner_transition_never_reuses_a_region_on_b() {
    let (mut app, a, b, region) = annulus_and_rectangles();
    assert!(
        region < app.regions_of(b).len(),
        "old index fits B: bounds alone cannot fix this"
    );
    app.selected_edge = Some(EdgeRef {
        body_id: a,
        segments: Vec::new(),
    });
    app.face_pick = Some(FaceHit {
        body_id: a,
        normal: [0., 1., 0.],
        center: [0.; 3],
        triangles: Vec::new(),
        area_mm2: 100.,
    });

    // This is the production Objects-list API used by Ctrl-click rows.
    app.select_from_objects(b, true);
    assert_eq!(app.selected, vec![a, b]);
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
    assert!(app.face_pick.is_none());
    assert!(app.selected_edge.is_none());
    app.select_from_objects(a, true);
    assert_eq!(app.selected, vec![b]);
    assert!(app.selected_regions_for(b).is_empty());
    assert!(app.selected_profile_source("Extrude").is_err());
    app.start_extrude();
    assert!(
        app.extrude_ctx.is_none(),
        "B's unrelated rectangle must not be extruded"
    );
    app.refresh_scene();
    assert!(app
        .scene
        .lock()
        .unwrap()
        .highlight_lines
        .positions
        .is_empty());
}

#[test]
fn region_selection_same_owner_retains_pick_but_deselect_reselect_does_not() {
    let (mut app, a, _, region) = annulus_and_rectangles();
    app.select_from_objects(a, false);
    assert_eq!(app.selected_regions_for(a), &[region]);
    app.select_from_objects(a, true);
    assert!(app.selected.is_empty());
    assert!(app.selected_region_source.is_none());
    app.select_from_objects(a, true);
    assert_eq!(app.selected, vec![a]);
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_profile_source("Pocket").is_err());
}

#[test]
fn region_selection_geometry_revision_rejects_in_range_old_indices_before_consumption() {
    let (mut app, a, b, region) = annulus_and_rectangles();
    let new_feature = app.body(b).unwrap().feature.clone();
    app.doc.body_mut(a).unwrap().feature = new_feature;
    assert!(region < app.regions_of(a).len());
    assert!(!app.region_selection_is_current(a));
    assert!(
        app.selected_regions_for(a).is_empty(),
        "render is safe even without bump_scene"
    );
    assert!(app.selected_profile_source("Revolve").is_err());
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_plane_revision_invalidates_and_rename_preserves() {
    let (mut app, a, _, region) = annulus_and_rectangles();
    app.doc.body_mut(a).unwrap().name = "Only a label".into();
    app.bump_scene();
    assert_eq!(app.selected_regions_for(a), &[region]);
    let Feature::Sketch(sketch) = &mut app.doc.body_mut(a).unwrap().feature else {
        unreachable!()
    };
    sketch.plane.origin[0] += 20.;
    app.bump_scene();
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_history_load_and_new_project_clear_transient_identity() {
    let (mut app, a, _, region) = annulus_and_rectangles();
    app.begin_document_change();
    let Feature::Sketch(sketch) = &mut app.doc.body_mut(a).unwrap().feature else {
        unreachable!()
    };
    let SketchEntity::Rect { c2, .. } = &mut sketch.entities[0] else {
        unreachable!()
    };
    c2[0] = 12.;
    app.bump_scene();
    assert!(app.selected_region_source.is_none());
    assert!(app.set_region_selection(a, vec![region]));
    assert!(app.undo());
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
    assert!(app.set_region_selection(a, vec![region]));
    assert!(app.redo());
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
    assert!(app.set_region_selection(a, vec![region]));
    let project = app.project_to_s33d().unwrap();
    app.load_s33d_project(&project).unwrap();
    assert_eq!(app.selected, vec![a]);
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
    assert!(app.set_region_selection(a, vec![region]));
    app.new_project();
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_cross_tool_clear_and_single_region_default_are_explicit() {
    let (mut app, a, _, region) = annulus_and_rectangles();
    app.start_extrude();
    assert_eq!(
        app.extrude_ctx.as_ref().unwrap().region_indices,
        vec![region]
    );
    app.cancel_extrude();
    assert_eq!(app.selected_regions_for(a), &[region]);
    app.start_revolve();
    assert_eq!(app.revolve_ctx.as_ref().unwrap().region_index, region);
    app.cancel_revolve();
    app.clear_selection();
    assert!(app.selected_region_source.is_none());
    app.select_from_objects(a, false);
    app.start_pocket();
    assert!(
        app.extrude_ctx.is_none(),
        "cleared multi-region selection needs a new pick"
    );

    let single = add_sketch(
        &mut app,
        "One rectangle",
        vec![SketchEntity::Rect {
            c1: [40., 0.],
            c2: [44., 3.],
        }],
    );
    app.select_from_objects(single, false);
    app.start_extrude();
    assert_eq!(
        app.extrude_ctx.as_ref().unwrap().region_indices,
        vec![0],
        "existing default for an intentionally selected one-region profile is preserved"
    );
}

#[test]
fn region_selection_click_snapshot_restores_identity_and_rejects_changed_geometry() {
    let (mut app, a, b, region) = annulus_and_rectangles();
    app.click_selection_base = Some(app.capture_click_selection());
    app.select_from_objects(b, false);
    app.restore_click_selection();
    assert_eq!(app.selected_regions_for(a), &[region]);
    // Finishing Extrude can hide its source before exit-click replay restores
    // the first-click selection. Its old highlight must stay hidden as well.
    app.doc.body_mut(a).unwrap().visible = false;
    app.restore_click_selection();
    assert!(app.selected_regions.is_empty());
    app.doc.body_mut(a).unwrap().visible = true;
    app.restore_click_selection();
    assert_eq!(app.selected_regions_for(a), &[region]);
    app.doc.body_mut(a).unwrap().feature = app.body(b).unwrap().feature.clone();
    app.restore_click_selection();
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_background_transaction_preserves_source_and_checks_worker_revision() {
    use std::time::{Duration, Instant};
    let (mut app, a, _, region) = annulus_and_rectangles();
    app.start_model_job(
        ("Region identity QA", "区域身份测试"),
        false,
        None,
        move |worker| {
            assert_eq!(worker.selected_regions_for(a), &[region]);
            worker.bump_scene();
            worker.hint = "Region selection source transferred".into();
            Ok(serde_json::Value::Null)
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.model_job.is_some() {
        assert!(Instant::now() < deadline);
        app.poll_model_job();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.selected_regions_for(a), &[region]);
    assert_eq!(app.hint, "Region selection source transferred");
    app.start_model_job(
        ("Region revision QA", "区域变更测试"),
        false,
        None,
        move |worker| {
            let Feature::Sketch(sketch) = &mut worker.doc.body_mut(a).unwrap().feature else {
                unreachable!()
            };
            sketch.entities.push(SketchEntity::Circle {
                c: [2., 2.],
                r: 0.5,
            });
            worker.bump_scene();
            assert!(worker.selected_region_source.is_none());
            Ok(serde_json::Value::Null)
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.model_job.is_some() {
        assert!(Instant::now() < deadline);
        app.poll_model_job();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_unbound_and_out_of_range_indices_are_never_adopted() {
    let (mut app, a, _, _) = annulus_and_rectangles();
    app.clear_region_selection();
    app.selected_regions = vec![0];
    assert!(app.selected_profile_source("Extrude").is_err());
    assert!(app.selected_regions.is_empty());
    assert!(!app.set_region_selection(a, vec![usize::MAX]));
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_agent_creation_selection_and_paste_do_not_inherit_region_indices() {
    let (mut app, a, b, region) = annulus_and_rectangles();
    let call = |app: &mut S33DApp, method: &str, params| {
        app.execute_agent_request(AgentRequest::new(serde_json::json!(1), method, params))
            .unwrap()
    };
    call(
        &mut app,
        "selection.set",
        serde_json::json!({"body_ids":[b]}),
    );
    assert!(app.selected_region_source.is_none());
    assert!(app.selected_profile_source("Extrude").is_err());
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    call(
        &mut app,
        "sketch.create",
        serde_json::json!({
            "plane":"xy", "entities":[
                {"type":"rectangle","corner1":[40.,0.],"corner2":[42.,2.]},
                {"type":"rectangle","corner1":[45.,0.],"corner2":[48.,3.]}
            ]
        }),
    );
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
    assert!(app.selected_profile_source("Pocket").is_err());
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    call(
        &mut app,
        "primitive.create",
        serde_json::json!({
            "kind":"box","width":5.,"depth":5.,"height":5.
        }),
    );
    assert!(app.selected_region_source.is_none());
    app.copy_selected();
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    app.paste_clipboard();
    assert_ne!(app.selected, vec![a]);
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

#[test]
fn region_selection_range_group_and_visibility_changes_clear_identity() {
    let (mut app, a, b, region) = annulus_and_rectangles();
    app.select_object_range(b, true);
    assert_eq!(app.selected, vec![a, b]);
    assert!(app.selected_region_source.is_none());
    let group = app.doc.group_objects(&[a, b]).unwrap();
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    app.select_group(group, false);
    assert_eq!(app.selected, vec![a, b]);
    assert!(app.selected_region_source.is_none());
    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![region]));
    app.set_body_visible(a, false);
    assert!(app.selected_regions.is_empty());
    assert!(app.selected_region_source.is_none());
}

fn inject_selected_region_failure(app: &mut S33DApp, owner: BodyId, index: usize) -> String {
    let revision =
        s33d_model::entities_fingerprint(&app.body(owner).unwrap().sketch().unwrap().entities);
    let mut regions = app.regions_of(owner);
    // Isolate the selection/notification contract from polygonization. Keep the
    // region count and geometry fingerprint unchanged so its identity is valid.
    regions[index] = Region2D::new(vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]]);
    app.regions_cache.insert(owner, (revision, regions));
    app.validated_region_mesh(owner, index).unwrap_err()
}

#[test]
fn region_selection_cached_error_remains_queryable_after_toast_expiry_and_tool_exit() {
    let (mut app, a, _, index) = annulus_and_rectangles();
    let error = inject_selected_region_failure(&mut app, a, index);
    app.refresh_scene();
    assert!(app.region_selection_is_current(a));
    assert_eq!(app.selected_region_validation_errors(), vec![error.clone()]);
    {
        let scene = app.scene.lock().unwrap();
        assert!(scene.region_surfaces.is_empty());
        assert!(!scene.region_lines.positions.is_empty());
    }

    // The UI removes expired toasts, and the shared double-click exit path
    // writes Ready. Neither may erase the current region's persistent error.
    app.toasts.clear();
    assert!(app.exit_tool_for_workspace_selection());
    assert_eq!(app.hint, "Ready");
    app.refresh_scene();
    assert!(
        app.toasts.is_empty(),
        "cached errors must not spam new toasts"
    );
    assert_eq!(app.selected_region_validation_errors(), vec![error.clone()]);
    let document = serde_json::to_value(app.doc.archive()).unwrap();
    let shapes = app.kernel.shape_count();
    let selection = (app.selected.clone(), app.selected_regions.clone());
    let history = app.history.clone();
    for start in [
        S33DApp::start_extrude as fn(&mut S33DApp),
        S33DApp::start_pocket,
        S33DApp::start_revolve,
    ] {
        start(&mut app);
        assert_eq!(
            app.mode,
            Mode::Idle,
            "failed modeling stays in the Properties view"
        );
        assert!(app.extrude_ctx.is_none());
        assert!(app.revolve_ctx.is_none());
        assert_eq!(serde_json::to_value(app.doc.archive()).unwrap(), document);
        assert_eq!(app.kernel.shape_count(), shapes);
        assert_eq!(
            (app.selected.clone(), app.selected_regions.clone()),
            selection
        );
        assert_eq!(app.history, history);
        assert_eq!(app.selected_region_validation_errors(), vec![error.clone()]);
    }
}

#[test]
fn region_selection_cached_error_follows_current_owner_region_and_revision() {
    let (mut app, a, b, index) = annulus_and_rectangles();
    let error = inject_selected_region_failure(&mut app, a, index);
    assert_eq!(app.selected_region_validation_errors(), vec![error.clone()]);
    app.clear_selection();
    assert!(app.selected_region_validation_errors().is_empty());
    app.select_from_objects(a, false);
    assert!(app.selected_region_validation_errors().is_empty());
    assert!(app.set_region_selection(a, vec![index]));
    assert_eq!(app.selected_region_validation_errors(), vec![error.clone()]);
    app.select_from_objects(b, true);
    assert!(app.selected_region_validation_errors().is_empty());
    app.select_from_objects(a, true);
    assert_eq!(app.selected, vec![b]);
    assert!(app.selected_region_validation_errors().is_empty());

    app.select_from_objects(a, false);
    assert!(app.set_region_selection(a, vec![index]));
    let Feature::Sketch(sketch) = &mut app.doc.body_mut(a).unwrap().feature else {
        unreachable!()
    };
    let SketchEntity::Circle { r, .. } = &mut sketch.entities[1] else {
        unreachable!()
    };
    *r = 2.;
    // A read must reject old diagnostics even before the scene is invalidated.
    assert!(app.selected_region_validation_errors().is_empty());
    app.bump_scene();
    let revised_index = app
        .regions_of(a)
        .iter()
        .position(|r| !r.holes.is_empty())
        .unwrap();
    assert!(app.set_region_selection(a, vec![revised_index]));
    assert!(app.selected_region_validation_errors().is_empty());
    app.validated_region_mesh(a, revised_index).unwrap();
    assert!(app.selected_region_validation_errors().is_empty());
}
