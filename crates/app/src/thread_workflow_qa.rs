//! Regressions for the six cylindrical-wall workflow failures found in review.
use super::*;
use crate::threads::{self, Params, Target};

fn app() -> (egui::Context, S33DApp) {
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.settings.show_tool_exit_confirmation = false;
    (ctx, app)
}

fn call(app: &mut S33DApp, method: &str, params: Value) -> Value {
    app.execute_agent_request(AgentRequest::new(json!(1), method, params))
        .unwrap()
}

fn add(app: &mut S33DApp, shape: ShapeId) -> BodyId {
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: "Thread QA".into(),
        kind: BodyKind::Solid,
        feature: Feature::Imported { path: "QA".into() },
        shape: Some(shape),
        material: Material::default(),
        visible: true,
    });
    id
}

fn profile(app: &mut S33DApp, radius: f64, y: f64, owner: Option<BodyId>) -> BodyId {
    let id = app.doc.alloc_id();
    let mut plane = DatumPlane::Top.def();
    plane.origin[1] = y;
    app.doc.add_body(Body {
        id,
        name: "Profile".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Circle {
                c: [0., 0.],
                r: radius,
            }],
            on_body: owner,
            world_xyz_coordinates: false,
        }),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    id
}

fn exact_tube(app: &mut S33DApp, inner: f64, outer: f64, length: f64) -> BodyId {
    let shape = app
        .kernel
        .revolve_region(
            &Region2D::new(vec![
                [inner, 0.],
                [outer, 0.],
                [outer, length],
                [inner, length],
            ]),
            &DatumPlane::Front.def(),
            &Axis2D {
                point: [0., 0.],
                dir: [0., 1.],
            },
            360.,
        )
        .unwrap();
    add(app, shape)
}

fn pocket(app: &mut S33DApp, body: BodyId, through: bool) -> BodyId {
    let sketch = profile(app, 2.5, 10., None);
    app.selected = vec![sketch];
    assert!(app.set_region_selection(app.selected[0], vec![0]));
    app.start_pocket();
    let ctx = app.extrude_ctx.as_mut().unwrap();
    ctx.target_body = Some(body);
    ctx.through = through;
    app.set_extrude_depth(6.);
    app.commit_extrude();
    while app.face_extrude_job.is_some() {
        app.poll_exact_face_commit();
        std::thread::yield_now();
    }
    assert_eq!(app.mode, Mode::Idle);
    sketch
}

fn cut_and_undo(app: &mut S33DApp, target: Target) {
    let depth = 0.25;
    let params = Params {
        diameter: 2. * (target.radius + if target.internal { depth } else { 0. }),
        depth,
        pitch: 1.,
        length: 4.,
        flat_fraction: 0.125,
        ..Default::default()
    };
    let id = target.body_id;
    let before = app.doc.body(id).unwrap().clone();
    app.apply_threads(vec![target], params).unwrap();
    assert_ne!(app.doc.body(id).unwrap().shape, before.shape);
    let result = app.doc.body(id).unwrap().clone();
    assert!(app.undo());
    assert_eq!(app.doc.body(id).unwrap(), &before);
    assert!(app.redo());
    assert_eq!(app.doc.body(id).unwrap(), &result);
    assert!(app.undo());
}

#[test]
fn mixed_exact_pockets_and_bosses_recover_all_walls_without_duplicates() {
    for mode in ["through", "blind", "mixed", "boss"] {
        let (_, mut app) = app();
        let id = if mode == "mixed" {
            exact_tube(&mut app, 0., 6., 10.)
        } else {
            let id = call(
                &mut app,
                "primitive.create",
                json!({"kind":"box", "width":20., "depth":20., "height":10.}),
            )["id"]
                .as_u64()
                .unwrap();
            let mesh = app
                .kernel
                .tessellate(app.doc.body(id).unwrap().shape.unwrap())
                .unwrap();
            let shape = app
                .kernel
                .insert_exact(s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh).unwrap());
            app.doc.body_mut(id).unwrap().shape = Some(shape);
            id
        };
        let native_ids: Vec<_> = app
            .thread_targets(&[id])
            .unwrap()
            .iter()
            .map(|t| t.face_id)
            .collect();
        if mode == "boss" {
            let sketch = profile(&mut app, 3., 10., Some(id));
            app.selected = vec![sketch];
            assert!(app.set_region_selection(app.selected[0], vec![0]));
            app.start_extrude();
            app.extrude_ctx.as_mut().unwrap().target_body = Some(id);
            app.set_extrude_depth(6.);
            app.commit_extrude();
        } else {
            pocket(&mut app, id, mode != "blind");
        }
        let targets = app.thread_targets(&[id]).unwrap();
        assert_eq!(
            targets.len(),
            if mode == "mixed" { 2 } else { 1 },
            "{mode}: {targets:?}"
        );
        if mode == "mixed" {
            assert_eq!(targets.iter().filter(|t| t.internal).count(), 1);
            assert!(native_ids.contains(&targets.iter().find(|t| !t.internal).unwrap().face_id));
        }
        for target in targets {
            cut_and_undo(&mut app, target);
        }
    }
}

#[test]
fn symmetric_extrude_preserves_total_depth_and_center_on_agent_and_rebuild() {
    for (agent, depth) in [(true, 10.), (false, 10.), (true, 6.), (false, 6.)] {
        let (_, mut app) = app();
        let sketch = profile(&mut app, 3., 0., None);
        let id = if agent {
            call(
                &mut app,
                "feature.extrude",
                json!({"sketch_id":sketch,"depth":depth,"symmetric":true}),
            )["id"]
                .as_u64()
                .unwrap()
        } else {
            app.selected = vec![sketch];
            assert!(app.set_region_selection(app.selected[0], vec![0]));
            app.start_extrude();
            app.extrude_ctx.as_mut().unwrap().symmetric = true;
            app.set_extrude_depth(depth);
            app.commit_extrude();
            app.selected[0]
        };
        for radius in [3., 3.2] {
            if radius != 3. {
                app.edit_sketch(sketch);
                app.sketch.as_mut().unwrap().entities[0] = SketchEntity::Circle {
                    c: [0., 0.],
                    r: radius,
                };
                app.finish_sketch();
            }
            let mesh = app
                .kernel
                .tessellate(app.doc.body(id).unwrap().shape.unwrap())
                .unwrap();
            let (lo, hi) = mesh.bounds().unwrap();
            assert!(
                (f64::from(lo.y) + depth / 2.).abs() < 1e-4
                    && (f64::from(hi.y) - depth / 2.).abs() < 1e-4,
                "{lo:?}..{hi:?}"
            );
            assert!((app.thread_targets(&[id]).unwrap()[0].length - depth).abs() < 1e-4);
        }
    }
}

#[test]
fn native_and_faceted_step_roundtrips_keep_one_cuttable_cylinder() {
    for native in [false, true] {
        let (_, mut app) = app();
        let id = if native {
            exact_tube(&mut app, 0., 3., 10.)
        } else {
            let sketch = profile(&mut app, 3., 0., None);
            call(
                &mut app,
                "feature.extrude",
                json!({"sketch_id":sketch,"depth":10.}),
            )["id"]
                .as_u64()
                .unwrap()
        };
        let path = std::env::temp_dir().join(format!(
            "s33d-thread-step-{}-{native}.step",
            std::process::id()
        ));
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let file = Fixture(path);
        app.kernel
            .export_step(
                app.doc.body(id).unwrap().shape.unwrap(),
                file.0.to_str().unwrap(),
            )
            .unwrap();
        let shape = app.kernel.import_step(file.0.to_str().unwrap()).unwrap()[0];
        let imported = add(&mut app, shape);
        let targets = app.thread_targets(&[imported]).unwrap();
        assert_eq!(targets.len(), 1, "native={native}: {targets:?}");
        cut_and_undo(&mut app, targets[0].clone());
    }
}

#[test]
fn batch_threading_rejects_a_wall_destroyed_by_an_earlier_cut_atomically() {
    let (_, mut app) = app();
    let mut params = threads::preset_params("M6", false).unwrap();
    params.radial_fit = 0.15;
    let id = exact_tube(&mut app, 3. - params.depth, 3., 8.);
    let mut targets = app.thread_targets(&[id]).unwrap();
    targets.sort_by_key(|t| t.internal);
    assert_eq!(targets.len(), 2);
    let before = app.doc.bodies.clone();
    let error = app.apply_threads(targets, params).unwrap_err();
    assert!(error.contains("previous thread cut"), "{error}");
    assert_eq!(app.doc.bodies, before);
}

#[test]
fn transformed_threaded_tube_preserves_bore_annotations_and_second_cut() {
    for operation in ["move", "rotate", "agent", "scale"] {
        let (_, mut app) = app();
        let id = exact_tube(&mut app, 2.5, 6., 10.);
        let outer = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| !t.internal)
            .unwrap();
        app.apply_threads(
            vec![outer],
            Params {
                diameter: 12.,
                pitch: 1.,
                depth: 0.25,
                length: 4.,
                ..Default::default()
            },
        )
        .unwrap();
        let before = app.doc.body(id).unwrap().clone();
        let matrix = match operation {
            "move" | "agent" => glam::Mat4::from_translation(glam::vec3(20., 4., -8.)),
            "scale" => glam::Mat4::from_scale(glam::Vec3::splat(2.)),
            _ => glam::Mat4::from_rotation_z(0.5),
        };
        app.selected = vec![id];
        if operation == "agent" {
            call(
                &mut app,
                "body.transform",
                json!({"body_ids":[id],"translation":[20.,4.,-8.]}),
            );
        } else {
            app.apply_transform(&matrix, operation);
        }
        let targets = app.thread_targets(&[id]).unwrap();
        assert_eq!(targets.len(), 2, "{operation}: {targets:?}");
        let bore = targets.iter().find(|t| t.internal).unwrap().clone();
        let uncut_outer = targets.iter().find(|t| !t.internal).unwrap();
        let scale = if operation == "scale" { 2. } else { 1. };
        assert!((uncut_outer.length - 6. * scale).abs() < 1e-4);
        let Feature::Derived { source, .. } = &app.doc.body(id).unwrap().feature else {
            unreachable!()
        };
        let saved: threads::Saved =
            serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
        assert_eq!(saved.annotations.len(), 1);
        assert!((saved.annotations[0].params.diameter - 12. * scale).abs() < 1e-4);
        assert!((saved.annotations[0].params.pitch - scale).abs() < 1e-4);
        assert!(
            glam::DVec3::from_array(saved.annotations[0].target.origin)
                .distance(matrix.as_dmat4().transform_point3(glam::DVec3::ZERO))
                < 1e-4
        );
        cut_and_undo(&mut app, bore);
        assert!(app.undo());
        assert_eq!(app.doc.body(id).unwrap(), &before);
    }
}

#[test]
fn destroyed_remaining_bore_is_removed_and_legacy_records_are_rechecked() {
    let (_, mut app) = app();
    let id = exact_tube(&mut app, 2.7, 3., 8.);
    let original = app.thread_targets(&[id]).unwrap();
    let outer = original.iter().find(|t| !t.internal).unwrap().clone();
    let inner = original.iter().find(|t| t.internal).unwrap().clone();
    app.apply_threads(vec![outer], threads::preset_params("M6", false).unwrap())
        .unwrap();
    assert!(app.thread_targets(&[id]).unwrap().is_empty());
    let Feature::Derived { source, .. } = &mut app.doc.body_mut(id).unwrap().feature else {
        unreachable!()
    };
    let mut saved: threads::Saved =
        serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
    saved.remaining = vec![inner];
    *source = format!("Thread v1: {}", serde_json::to_string(&saved).unwrap());
    assert!(
        app.thread_targets(&[id]).unwrap().is_empty(),
        "Old same-signature records must also recertify"
    );
}

#[test]
fn legacy_moved_thread_record_recovers_real_bore_and_deformed_circles_are_rejected() {
    let (_, mut app) = app();
    let id = exact_tube(&mut app, 2.5, 6., 10.);
    let target = app
        .thread_targets(&[id])
        .unwrap()
        .into_iter()
        .find(|t| !t.internal)
        .unwrap();
    app.apply_threads(
        vec![target],
        Params {
            diameter: 12.,
            pitch: 1.,
            depth: 0.25,
            length: 4.,
            ..Default::default()
        },
    )
    .unwrap();
    let original = app.doc.body(id).unwrap().shape.unwrap();
    let moved = app
        .kernel
        .transform(
            original,
            &glam::Mat4::from_translation(glam::vec3(16., -5., 3.)),
        )
        .unwrap();
    app.doc.body_mut(id).unwrap().shape = Some(moved); // Old projects did not update saved metadata.
    let targets = app.thread_targets(&[id]).unwrap();
    assert_eq!(targets.len(), 1);
    assert!(targets[0].internal);
    cut_and_undo(&mut app, targets[0].clone());
    app.selected = vec![id];
    app.apply_transform(
        &glam::Mat4::from_scale(glam::vec3(1.2, 1., 1.)),
        "elliptical deformation",
    );
    assert!(app.thread_targets(&[id]).unwrap().is_empty());
}

#[test]
fn end_fillet_keeps_a_complete_straight_threadable_band() {
    for (fillet, end) in [(true, 0), (true, 1), (false, 0), (false, 1)] {
        let (_, mut app) = app();
        let id = call(
            &mut app,
            "primitive.create",
            json!({"kind":"cylinder","radius":3.,"height":10.}),
        )["id"]
            .as_u64()
            .unwrap();
        let mesh = app
            .kernel
            .tessellate(app.doc.body(id).unwrap().shape.unwrap())
            .unwrap();
        // Both rims must work; HashMap iteration previously chose one at random.
        let mut rims: Vec<_> = mesh_sharp_edge_segments(&mesh)
            .into_iter()
            .filter(|chain| chain.len() > 8)
            .collect();
        let height =
            |rim: &Vec<EdgeSeg>| rim.iter().map(|s| s.a[1] as f64).sum::<f64>() / rim.len() as f64;
        rims.sort_by(|a, b| height(a).total_cmp(&height(b)));
        let edge = rims.remove(end);
        app.selected = vec![id];
        app.selected_edge = Some(EdgeRef {
            body_id: id,
            segments: edge,
        });
        app.chamfer_active = true;
        app.chamfer_fillet = fillet;
        app.chamfer_param = 0.5;
        app.apply_chamfer();
        app.exit_chamfer_tool();
        let id = app.selected[0];
        let targets = app.thread_targets(&[id]).unwrap();
        assert_eq!(targets.len(), 1, "fillet={fillet}: {targets:?}");
        assert!(targets[0].length > 9.4 && targets[0].length < 9.6);
        cut_and_undo(&mut app, targets[0].clone());
        app.selected = vec![id];
        app.apply_transform(
            &(glam::Mat4::from_translation(glam::vec3(12., -3., 7.))
                * glam::Mat4::from_rotation_z(0.7)),
            "move rounded cylinder",
        );
        let moved = app.thread_targets(&[id]).unwrap();
        assert_eq!(moved.len(), 1, "Moved fillet={fillet}: {moved:?}");
        cut_and_undo(&mut app, moved[0].clone());
    }
}
