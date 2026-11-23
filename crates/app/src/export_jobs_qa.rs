use super::*;

fn app() -> (S33DApp, BodyId) {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    let id = app
        .execute_agent_request(AgentRequest::new(
            json!(1),
            "primitive.create",
            json!({"kind":"box","width":20.,"depth":10.,"height":12.}),
        ))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    app.background_modeling = true;
    (app, id)
}
fn target(label: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "s33d-export-qa-{label}-{}-{}.step",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}
fn finish(app: &mut S33DApp) {
    let limit = Instant::now() + Duration::from_secs(30);
    while app.export_job.is_some() {
        assert!(Instant::now() < limit, "Export worker timeout");
        app.poll_export_job();
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn background_export_writes_valid_step_without_changing_document() {
    let (mut app, id) = app();
    let path = target("success");
    let bodies = app.doc.bodies.clone();
    let history = app.history.clone();
    std::fs::write(&path, b"previous").unwrap();
    app.start_export_job(id, "step", path.clone()).unwrap();
    assert!(app.progress_busy());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous");
    finish(&mut app);
    assert!(
        app.toasts.iter().any(|(s, _)| s.contains("Exported")),
        "{:?}",
        app.toasts
    );
    let step = std::fs::read_to_string(&path).unwrap();
    assert!(step.contains("MANIFOLD_SOLID_BREP"));
    assert_eq!(app.doc.bodies, bodies);
    assert_eq!(app.history, history);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn cancelled_failed_and_stale_exports_preserve_destination_and_remove_staging() {
    for disposition in ["cancel", "stale", "fail"] {
        let (mut app, id) = app();
        let path = target(disposition);
        std::fs::write(&path, b"previous").unwrap();
        let (release, wait) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let failed = disposition == "fail";
        app.start_export_work(id, "step".into(), path.clone(), move |_, _, temporary| {
            ready.send(temporary.to_owned()).unwrap();
            wait.recv().unwrap();
            std::fs::write(temporary, b"new export").unwrap();
            done.send(()).unwrap();
            if failed {
                Err("injected write failure".into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        let temporary = started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert!(app
            .execute_agent_request(AgentRequest::new(json!(2), "history.undo", json!({})))
            .is_err());
        if disposition == "cancel" {
            let ctx = egui::Context::default();
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ctx| app.export_progress_modal(ctx),
            );
            assert!(app.export_job.is_none());
        } else if disposition == "stale" {
            app.doc.body_mut(id).unwrap().name = "changed".into();
        }
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        finish(&mut app);
        let limit = Instant::now() + Duration::from_secs(5);
        while temporary.exists() {
            assert!(Instant::now() < limit);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn desktop_agent_export_uses_background_path_and_reset_discards_result() {
    let (mut app, id) = app();
    let path = target("agent");
    let request = AgentRequest::new(
        json!(1),
        "body.export",
        json!({"body_id":id,"path":path,"format":"STEP"}),
    );
    assert!(app.queue_agent_export_job(&request));
    finish(&mut app);
    assert!(std::fs::metadata(&path).unwrap().len() > 100);
    std::fs::remove_file(path).unwrap();
    let path = target("reset");
    let (release, wait) = mpsc::channel();
    let (ready, started) = mpsc::channel();
    app.start_export_work(id, "step".into(), path.clone(), move |_, _, temporary| {
        ready.send(temporary.to_owned()).unwrap();
        wait.recv().unwrap();
        std::fs::write(temporary, b"late").map_err(|e| e.to_string())
    })
    .unwrap();
    let temporary = started.recv_timeout(Duration::from_secs(5)).unwrap();
    app.reset_document_tools();
    assert!(!app.progress_busy());
    release.send(()).unwrap();
    let limit = Instant::now() + Duration::from_secs(5);
    while temporary.exists() {
        assert!(Instant::now() < limit);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!path.exists());
}

#[test]
fn desktop_agent_export_replies_after_publication_and_replays_without_rewriting() {
    for cancel in [false, true] {
        let (mut app, id) = app();
        let path = target("protocol");
        std::fs::write(&path, b"previous").unwrap();
        app.agent_server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = app.agent_server.address().unwrap().to_string();
        let mut request = AgentRequest::new(
            json!(1),
            "body.export",
            json!({"body_id":id,"path":path,"format":"step"}),
        );
        request.idempotency_key = Some("export-publication-test".into());
        let first = request.clone();
        let endpoint = address.clone();
        let client = std::thread::spawn(move || {
            crate::agent_protocol::send_request(&endpoint, &first).unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.export_job.is_none() {
            assert!(Instant::now() < deadline);
            app.process_agent_requests();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.export_job.as_ref().unwrap().pending_agent.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        if cancel {
            app.cancel_export_job();
        } else {
            finish(&mut app);
        }
        while !client.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let response = client.join().unwrap();
        assert_eq!(response.ok, !cancel);
        if cancel {
            assert_eq!(response.error.as_ref().unwrap().code, "cancelled");
        } else {
            assert_eq!(response.result.as_ref().unwrap()["exported"], true);
        }
        // If the replay were dispatched again it would overwrite this marker.
        std::fs::write(&path, b"published marker").unwrap();
        request.id = json!(2);
        let replay = crate::agent_protocol::send_request(&address, &request).unwrap();
        assert_eq!(replay.result, response.result);
        assert_eq!(replay.error, response.error);
        assert_eq!(std::fs::read(&path).unwrap(), b"published marker");
        std::fs::remove_file(path).unwrap();
    }
}
