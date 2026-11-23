//! Exercise desktop jobs, including cancellation while the worker is blocked.
use super::*;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn app() -> S33DApp {
    S33DApp::from_ui_state(UiStateFile::default(), false)
}

#[test]
fn launch_task_background_project_open_commits_drawing_and_checkpoint_atomically() {
    let (mut source, _) = box_app();
    source.background_modeling = false;
    call(
        &mut source,
        "primitive.create",
        json!({"kind":"cylinder","radius":3.,"height":6.}),
    );
    source.drawing.title = "Background drawing".into();
    source.drawing.revision = "B".into();
    let path = std::env::temp_dir().join(format!(
        "s33d-background-open-{}-{}.s33d",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, source.project_to_s33d().unwrap()).unwrap();
    let (mut dest, _) = box_app();
    dest.drawing.title = "Original drawing".into();
    let before = dest.doc.bodies.clone();
    dest.start_sketch_on(DatumPlane::Front);
    dest.open_project_responsive(&path).unwrap();
    assert_eq!(dest.doc.bodies, before);
    assert_eq!(dest.drawing.title, "Original drawing");
    assert!(dest.open_project_responsive(&path).is_err());
    finish(&mut dest);
    assert_eq!(dest.drawing.title, "Background drawing");
    assert_eq!(dest.drawing.revision, "B");
    assert_eq!(dest.doc.bodies.len(), 2);
    assert!(dest.sketch.is_none());
    assert_eq!(dest.project_session.path.as_ref(), Some(&path));
    assert!(!dest.project_has_changes());
    dest.drawing.title = "Keep on cancel".into();
    let loaded = dest.doc.bodies.clone();
    dest.open_project_responsive(&path).unwrap();
    dest.cancel_model_job();
    dest.poll_model_job();
    assert_eq!(dest.drawing.title, "Keep on cancel");
    assert_eq!(dest.doc.bodies, loaded);
    std::fs::write(&path, "invalid project").unwrap();
    dest.open_project_responsive(&path).unwrap();
    finish(&mut dest);
    assert_eq!(dest.drawing.title, "Keep on cancel");
    assert_eq!(dest.doc.bodies, loaded);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn launch_task_mesh_import_is_deferred_duplicate_safe_and_one_undo() {
    let path =
        std::env::temp_dir().join(format!("s33d-background-mesh-{}.obj", std::process::id()));
    std::fs::write(&path, "v 0 0 0\nv 10 0 0\nv 0 10 0\nf 1 2 3\n").unwrap();
    let (mut app, _) = box_app();
    let before = app.doc.bodies.clone();
    crate::ui::import_model_path(&mut app, &path);
    assert!(app.model_job.is_some());
    assert_eq!(app.doc.bodies, before);
    crate::ui::import_model_path(&mut app, &path);
    finish(&mut app);
    assert_eq!(app.doc.bodies.len(), before.len() + 1);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, before);
    crate::ui::import_model_path(&mut app, &path);
    app.cancel_model_job();
    app.poll_model_job();
    assert_eq!(app.doc.bodies, before);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn launch_task_diameter_edit_is_background_and_one_undo() {
    let mut app = app();
    let id = call(
        &mut app,
        "primitive.create",
        json!({"kind":"cylinder","radius":4.,"height":12.}),
    )["id"]
        .as_u64()
        .unwrap();
    let target = app.thread_targets(&[id]).unwrap().pop().unwrap();
    let before = app.doc.bodies.clone();
    app.background_modeling = true;
    app.edit_cylinder_diameter(target.clone(), 10.).unwrap();
    assert_eq!(app.doc.bodies, before);
    assert!(app.edit_cylinder_diameter(target, 12.).is_err());
    finish(&mut app);
    assert!(
        (app.thread_targets(&[id]).unwrap()[0].radius - 5.).abs() < 1e-6,
        "{:?}, {:?}",
        app.thread_targets(&[id]),
        app.toasts
    );
    assert!(app.undo());
    assert_eq!(app.doc.bodies, before);
}

fn call(app: &mut S33DApp, method: &str, params: Value) -> Value {
    app.execute_agent_request(AgentRequest::new(json!(1), method, params))
        .unwrap()
}

fn box_app() -> (S33DApp, BodyId) {
    let mut app = app();
    let id = call(
        &mut app,
        "primitive.create",
        json!({"kind":"box", "width":20., "depth":20., "height":20.}),
    )["id"]
        .as_u64()
        .unwrap();
    app.background_modeling = true;
    (app, id)
}

fn finish(app: &mut S33DApp) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while app.model_job.is_some() || app.face_extrude_job.is_some() {
        assert!(
            Instant::now() < deadline,
            "worker timeout: {:?}",
            app.toasts
        );
        app.poll_model_job();
        app.poll_exact_face_commit();
        std::thread::sleep(Duration::from_millis(2));
    }
    app.poll_tool_exit();
    assert!(
        app.model_job.is_none(),
        "Exit must not start a second calculation"
    );
}

fn edge(app: &mut S33DApp, id: BodyId) -> Vec<EdgeSeg> {
    let mesh = app
        .kernel
        .tessellate(app.doc.body(id).unwrap().shape.unwrap())
        .unwrap();
    mesh_sharp_edge_segments(&mesh)
        .into_iter()
        .find(|c| c.len() == 1)
        .unwrap()
}

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800., 600.),
        )),
        events,
        ..Default::default()
    }
}

fn frame(
    ctx: &egui::Context,
    app: &mut S33DApp,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn collect(s: &egui::Shape, text: &mut Vec<(String, egui::Rect)>) {
        match s {
            egui::Shape::Text(t) => text.push((
                t.galley.job.text.clone(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::Shape::Vec(shapes) => {
                for s in shapes {
                    collect(s, text);
                }
            }
            _ => {}
        }
    }
    let output = ctx.run(raw(events), |ctx| {
        app.model_progress_modal(ctx);
    });
    let mut text = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut text);
    }
    text
}

#[test]
fn progress_modal_is_responsive_cancels_and_ignores_late_result_in_both_languages() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        for escape in [false, true] {
            let ctx = egui::Context::default();
            let (mut app, id) = box_app();
            app.settings.language = language;
            let original = app.doc.bodies.clone();
            let original_history = app.history.clone();
            let (release, wait) = mpsc::channel();
            let (ready, started) = mpsc::channel();
            let (done, finished) = mpsc::channel();
            app.start_model_job(("Chamfer", "倒角"), false, None, move |worker| {
                ready.send(()).unwrap();
                wait.recv().unwrap();
                worker.begin_document_change();
                worker.doc.body_mut(id).unwrap().name = "late result".into();
                done.send(()).unwrap();
                Ok(Value::Null)
            })
            .unwrap();
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(app.doc.bodies, original);
            let mutation =
                app.execute_agent_request(AgentRequest::new(json!(2), "history.undo", json!({})));
            assert_eq!(mutation.unwrap_err().0, "busy");
            assert!(app
                .execute_agent_request(AgentRequest::new(json!(3), "system.ping", json!({})))
                .is_ok());
            frame(&ctx, &mut app, vec![]); // First egui pass measures the modal.
            let text = frame(&ctx, &mut app, vec![]);
            assert!(text
                .iter()
                .any(|(t, _)| t == language.text("Chamfer", "倒角")));
            assert!(text
                .iter()
                .any(|(t, _)| t.contains(language.text("Elapsed:", "已用时："))));
            if escape {
                frame(
                    &ctx,
                    &mut app,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                assert!(!ctx.input(|i| i.key_pressed(egui::Key::Escape)));
            } else {
                let pos = text
                    .iter()
                    .find(|(t, _)| t == language.text("Cancel", "取消"))
                    .unwrap()
                    .1
                    .center();
                for pressed in [true, false] {
                    frame(
                        &ctx,
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
            }
            assert!(app.model_job.is_none());
            release.send(()).unwrap();
            finished.recv_timeout(Duration::from_secs(5)).unwrap();
            app.poll_model_job();
            assert_eq!(app.doc.bodies, original);
            assert_eq!(app.history, original_history);
            assert!(app.undo()); // Exactly the original box creation remains.
            assert!(app.doc.bodies.is_empty());
        }
    }
}

#[test]
fn error_panic_stale_document_and_project_reset_cannot_commit_worker_changes() {
    for failure in ["error", "panic", "stale", "reset"] {
        let (mut app, id) = box_app();
        let original = app.doc.bodies.clone();
        let history = app.history.clone();
        let (release, wait) = mpsc::channel();
        app.start_model_job(("Test", "测试"), false, None, move |worker| {
            wait.recv().unwrap();
            worker.begin_document_change();
            worker.doc.body_mut(id).unwrap().name = "never commit".into();
            if failure == "panic" {
                panic!("injected geometry failure");
            }
            if failure == "error" {
                return Err(("geometry".into(), "injected failure".into()));
            }
            Ok(Value::Null)
        })
        .unwrap();
        if failure == "stale" {
            app.doc.body_mut(id).unwrap().name = "newer change".into();
        }
        if failure == "reset" {
            app.new_project();
        }
        release.send(()).unwrap();
        finish(&mut app);
        match failure {
            "reset" => assert!(app.doc.bodies.is_empty()),
            "stale" => assert_eq!(app.doc.body(id).unwrap().name, "newer change"),
            _ => {
                assert_eq!(app.doc.bodies, original);
                assert_eq!(app.history, history);
            }
        }
    }
}

#[test]
fn async_edge_apply_updates_chains_and_exit_applies_latest_parameters_once() {
    for fillet in [false, true] {
        let (mut app, root) = box_app();
        let first_edge = edge(&mut app, root);
        app.chamfer_active = true;
        app.chamfer_fillet = fillet;
        app.chamfer_param = 1.;
        app.selected_edge = Some(EdgeRef {
            body_id: root,
            segments: first_edge,
        });
        let before = app.doc.bodies.clone();
        app.apply_chamfer();
        assert_eq!(app.doc.bodies, before, "button returns before committing");
        assert!(app.model_job.is_some());
        finish(&mut app);
        assert_eq!(app.doc.bodies.len(), 2, "{:?}", app.toasts);
        let first = app.selected[0];
        let initial = app.doc.body(first).unwrap().shape;
        app.chamfer_param = 1.5;
        app.apply_chamfer();
        finish(&mut app);
        assert_eq!(app.selected, [first]);
        assert_eq!(app.doc.bodies.len(), 2);
        assert_ne!(app.doc.body(first).unwrap().shape, initial);
        let mesh = app
            .kernel
            .tessellate(app.doc.body(root).unwrap().shape.unwrap())
            .unwrap();
        let first_mid = (glam::Vec3::from(app.selected_edge.as_ref().unwrap().segments[0].a)
            + glam::Vec3::from(app.selected_edge.as_ref().unwrap().segments[0].b))
            * 0.5;
        let opposite = mesh_sharp_edge_segments(&mesh)
            .into_iter()
            .filter(|c| c.len() == 1)
            .max_by(|a, b| {
                let mid =
                    |e: &Vec<EdgeSeg>| (glam::Vec3::from(e[0].a) + glam::Vec3::from(e[0].b)) * 0.5;
                mid(a)
                    .distance_squared(first_mid)
                    .total_cmp(&mid(b).distance_squared(first_mid))
            })
            .unwrap();
        app.selected_edge = Some(EdgeRef {
            body_id: first,
            segments: opposite,
        });
        app.apply_chamfer();
        finish(&mut app);
        assert_eq!(app.doc.bodies.len(), 3);
        let last = app.selected[0];
        assert!(
            matches!(app.doc.body(last).unwrap().feature,Feature::Chamfer {base:Some(b),..} if b==first)
        );
        let before_exit = app.doc.bodies.clone();
        app.chamfer_param = 2.;
        app.draw3d_tools_open = true;
        app.exit_chamfer_tool();
        assert!(app.model_job.is_some());
        finish(&mut app);
        assert!(!app.chamfer_active);
        assert!(!app.draw3d_tools_open);
        assert!(app.selected_edge.is_none());
        assert!(app.undo());
        assert_eq!(
            app.doc.bodies, before_exit,
            "exit adds precisely one undo step"
        );
    }
}

#[test]
fn switching_tools_waits_for_old_geometry_and_retains_the_requested_tool() {
    let (mut app, id) = box_app();
    app.chamfer_active = true;
    app.selected_edge = Some(EdgeRef {
        body_id: id,
        segments: edge(&mut app, id),
    });
    app.toggle_gizmo_tool(GizmoMode::Translate);
    assert!(app.model_job.is_some());
    finish(&mut app);
    assert_eq!(app.doc.bodies.len(), 2);
    assert!(!app.chamfer_active);
    assert_eq!(app.gizmo, Some(GizmoMode::Translate));
}

#[test]
fn async_thread_generation_and_cancellation_preserve_draft_and_one_step_undo() {
    let mut app = app();
    let id = call(
        &mut app,
        "primitive.create",
        json!({"kind":"cylinder","radius":3.,"height":8.}),
    )["id"]
        .as_u64()
        .unwrap();
    let target = app.thread_targets(&[id]).unwrap().remove(0);
    let params = crate::threads::Params {
        diameter: 6.,
        pitch: 1.,
        depth: 0.3,
        length: 5.,
        ..Default::default()
    };
    app.thread_params = params.clone();
    app.thread_choices = vec![target.clone()];
    app.thread_ui.expanded = true;
    app.background_modeling = true;
    let original = app.doc.bodies.clone();
    app.apply_threads(vec![target.clone()], params.clone())
        .unwrap();
    assert_eq!(app.doc.bodies, original);
    app.cancel_model_job();
    assert!(app.thread_ui.expanded);
    assert_eq!(app.thread_choices.len(), 1);
    assert_eq!(app.thread_params.pitch, params.pitch);
    app.apply_threads(vec![target], params).unwrap();
    finish(&mut app);
    assert_ne!(app.doc.bodies, original, "{:?}", app.toasts);
    assert!(!app.thread_ui.expanded);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, original);
    assert!(app.redo());
    assert_ne!(app.doc.bodies, original);
}

#[test]
fn geometry_requests_use_workers_and_return_real_results_after_completion() {
    let (mut app, id) = box_app();
    let request = AgentRequest::new(
        json!(8),
        "body.transform",
        json!({"body_ids":[id],"translation":[2.,0.,0.]}),
    );
    // An invalid request still completes through the error path without a mutation.
    assert!(app.queue_agent_model_job(&request));
    finish(&mut app);
    assert_eq!(app.doc.bodies.len(), 1);
    let request = AgentRequest::new(
        json!(9),
        "primitive.create",
        json!({"kind":"cylinder","radius":3.,"height":8.}),
    );
    assert!(app.queue_agent_model_job(&request));
    finish(&mut app);
    assert_eq!(app.doc.bodies.len(), 2);
    assert!(app.undo());
    assert_eq!(app.doc.bodies.len(), 1);
}

fn pointer_frame(ctx: &egui::Context, app: &mut S33DApp, pos: egui::Pos2, pressed: Option<bool>) {
    let mut events = vec![egui::Event::PointerMoved(pos)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
    }
    let _ = ctx.run(raw(events), |ctx| {
        app.model_progress_modal(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(ui.max_rect(), egui::Sense::click_and_drag());
                app.viewport_rect = response.rect;
                if !app.progress_busy() {
                    app.viewport_pointer(&response);
                }
            });
    });
}

#[test]
fn asynchronous_multiclick_exit_replays_selection_and_never_applies_twice() {
    let ctx = egui::Context::default();
    let (mut app, id) = box_app();
    app.settings.show_tool_exit_confirmation = false;
    app.selected_edge = Some(EdgeRef {
        body_id: id,
        segments: edge(&mut app, id),
    });
    app.chamfer_active = true;
    app.chamfer_param = 1.;
    app.apply_chamfer();
    finish(&mut app);
    app.chamfer_param = 2.;
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    app.camera.up = glam::Vec3::Y;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(app.model_job.is_some());
    assert!(app.pending_tool_exit.is_some());
    finish(&mut app);
    assert!(!app.chamfer_active);
    let result = app.selected[0];
    assert_eq!(app.face_pick.as_ref().map(|f| f.body_id), Some(result));
    let after = app.doc.bodies.clone();
    pointer_frame(&ctx, &mut app, pos, Some(true));
    pointer_frame(&ctx, &mut app, pos, Some(false));
    assert_eq!(app.selected, [result]);
    assert!(app.face_pick.is_none());
    assert_eq!(app.doc.bodies, after);
    assert!(app.model_job.is_none());
}

#[test]
fn asynchronous_profile_pocket_recovers_internal_thread_and_undo() {
    let (mut app, id) = box_app();
    let profile = app.doc.alloc_id();
    let mut plane = DatumPlane::Top.def();
    plane.origin[1] = 20.;
    app.doc.add_body(Body {
        id: profile,
        name: "Circle".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Circle {
                c: [0., 0.],
                r: 2.5,
            }],
            on_body: Some(id),
            world_xyz_coordinates: false,
        }),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    app.selected = vec![profile];
    assert!(app.set_region_selection(app.selected[0], vec![0]));
    app.start_pocket();
    app.extrude_ctx.as_mut().unwrap().target_body = Some(id);
    app.extrude_ctx.as_mut().unwrap().through = true;
    app.set_extrude_depth(20.);
    let original = app.doc.bodies.clone();
    app.commit_extrude();
    assert!(app.model_job.is_some());
    assert_eq!(app.doc.bodies, original);
    finish(&mut app);
    assert_eq!(app.mode, Mode::Idle);
    let bore = app
        .thread_targets(&[id])
        .unwrap()
        .into_iter()
        .find(|t| t.internal)
        .unwrap();
    let pocketed = app.doc.bodies.clone();
    let params = crate::threads::Params {
        diameter: 5.6,
        depth: 0.3,
        pitch: 1.,
        length: 5.,
        ..Default::default()
    };
    app.apply_threads(vec![bore], params).unwrap();
    finish(&mut app);
    assert_ne!(app.doc.bodies, pocketed, "{:?}", app.toasts);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, pocketed);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, original);
}

#[test]
fn threaded_round_nut_dense_edge_treatment_runs_in_background_and_undoes() {
    let mut app = app();
    let mut params = s33d_model::FastenerParams::defaults(s33d_model::FastenerKind::RoundNut);
    params.diameter = 6.;
    params.pitch = 1.;
    params.length = 6.;
    params.head_size = 14.;
    let mesh = crate::fasteners::build_mesh(&params).unwrap();
    let chains = mesh_sharp_edge_segments(&mesh);
    let chain = chains
        .into_iter()
        .find(|c| {
            c.len() >= 80
                && c.iter().all(|s| {
                    let p = glam::Vec3::from(s.a);
                    p.y.abs() < 1e-4 && glam::Vec2::new(p.x, p.z).length() > 6.
                })
        })
        .expect("dense outer rim of threaded nut");
    let shape = app.kernel.insert_mesh(mesh);
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: "Threaded nut".into(),
        kind: BodyKind::Solid,
        feature: Feature::Fastener { params },
        shape: Some(shape),
        material: Material::default(),
        visible: true,
    });
    app.selected = vec![id];
    app.selected_edge = Some(EdgeRef {
        body_id: id,
        segments: chain,
    });
    app.chamfer_active = true;
    app.chamfer_param = 0.5;
    app.background_modeling = true;
    let before = app.doc.bodies.clone();
    app.apply_chamfer();
    assert!(app.model_job.is_some());
    assert_eq!(app.doc.bodies, before);
    finish(&mut app);
    assert_eq!(app.doc.bodies.len(), 2, "{:?}", app.toasts);
    assert!(app.undo());
    assert_eq!(app.doc.bodies, before);
}

#[test]
fn desktop_agent_dispatch_preserves_responses_and_idempotency_after_work_or_cancel() {
    for action in ["success", "cancel", "error"] {
        let mut app = app();
        app.background_modeling = true;
        app.agent_server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = app.agent_server.address().unwrap().to_string();
        let params = if action == "error" {
            json!({"kind":"invalid"})
        } else {
            json!({"kind":"box","width":10.,"depth":10.,"height":10.})
        };
        let mut request = AgentRequest::new(json!(1), "primitive.create", params);
        request.idempotency_key = Some("background-geometry-test".into());
        let first = request.clone();
        let endpoint = address.clone();
        let client = std::thread::spawn(move || {
            crate::agent_protocol::send_request(&endpoint, &first).unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.model_job.is_none() {
            assert!(Instant::now() < deadline);
            app.process_agent_requests();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.model_job.as_ref().unwrap().pending_agent.is_some());
        assert!(
            app.doc.bodies.is_empty(),
            "Reply and mutation wait for completion"
        );
        if action == "cancel" {
            app.cancel_model_job();
        } else {
            finish(&mut app);
        }
        while !client.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let response = client.join().unwrap();
        assert_eq!(response.ok, action == "success");
        if action == "cancel" {
            assert_eq!(response.error.as_ref().unwrap().code, "cancelled");
        }
        assert_eq!(app.doc.bodies.len(), usize::from(action == "success"));
        request.id = json!(2);
        let replay = crate::agent_protocol::send_request(&address, &request).unwrap();
        assert_eq!(replay.ok, response.ok);
        assert_eq!(replay.result, response.result);
        assert_eq!(replay.error, response.error);
        assert_eq!(app.doc.bodies.len(), usize::from(action == "success"));
    }
}
