//! Integration coverage for display controls, saved views and guarded MRU opens.
use super::*;

fn app() -> S33DApp {
    S33DApp::from_ui_state(UiStateFile::default(), false)
}

#[test]
fn viewport_grid_and_axes_toggle_together_restore_and_persist_without_dirtying_model() {
    let mut app = app();
    app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
    let doc = app.doc.bodies.clone();
    for shown in [true, false, true] {
        app.show_grid = shown;
        app.bump_scene();
        app.refresh_scene();
        let scene = app.scene.lock().unwrap();
        assert_eq!(scene.show_grid, shown);
        assert_eq!(!scene.grid_axis_lines.is_empty(), shown);
        assert_eq!(!scene.grid_minor_lines.positions.is_empty(), shown);
    }
    app.show_grid = false;
    let encoded = app.project_to_s33d().unwrap();
    app.show_grid = true;
    app.load_s33d_project(&encoded).unwrap();
    assert!(!app.show_grid);
    app.reenable_all_hidden_tips();
    app.refresh_scene();
    assert!(app.show_grid && !app.scene.lock().unwrap().grid_axis_lines.is_empty());
    assert_eq!(app.doc.bodies, doc);
    assert!(
        !app.project_has_changes(),
        "Display changes must not dirty the model"
    );
}

#[test]
fn recent_projects_record_only_successful_sessions_and_open_uses_unsaved_guard() {
    let mut app = app();
    let root = std::env::temp_dir().join(format!(
        "s33d-recent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let first = root.join("同名.s33d");
    let second = root.join("second.s33d");
    app.save_project_to_path(&first).unwrap();
    app.save_project_to_path(&second).unwrap();
    assert_eq!(app.settings.recent_projects.len(), 2);
    assert_eq!(
        crate::recent_projects::name(&app.settings.recent_projects[0]),
        "second"
    );
    let old = app.settings.recent_projects.clone();
    assert!(app
        .open_project_from_path(&root.join("missing.s33d"))
        .is_err());
    let invalid = root.join("invalid.s33d");
    std::fs::write(&invalid, "broken").unwrap();
    assert!(app.open_project_from_path(&invalid).is_err());
    assert_eq!(app.settings.recent_projects, old);
    app.doc.rename("unsaved edit");
    let before = app.doc.bodies.clone();
    let ctx = egui::Context::default();
    app.request_project_action(project_session::ProjectAction::Open(first.clone()), &ctx);
    assert!(
        matches!(&app.project_session.pending,Some(project_session::ProjectAction::Open(p)) if p==&first)
    );
    assert_eq!(app.project_session.path.as_ref(), Some(&second));
    assert_eq!(app.settings.recent_projects, old);
    assert_eq!(app.doc.bodies, before);
    // Cancel is the same operation used by the existing unsaved-changes dialog.
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    });
    let _ = ctx.run(input, |ctx| app.project_lifetime_ui(ctx));
    assert!(app.project_session.pending.is_none());
    assert_eq!(app.doc.name(), "unsaved edit");
    app.open_project_from_path(&first).unwrap();
    assert_eq!(app.settings.recent_projects.len(), 2);
    assert_eq!(
        crate::recent_projects::name(&app.settings.recent_projects[0]),
        "同名"
    );
    for file in [first, second, invalid] {
        std::fs::remove_file(file).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}
