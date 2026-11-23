//! Integration coverage for section rendering, clipboard and numeric tool entry.
use super::*;

fn app() -> S33DApp {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    app.settings.show_tool_exit_confirmation = false;
    app
}
fn box_body(app: &mut S33DApp, position: [f64; 3]) -> BodyId {
    app.execute_agent_request(AgentRequest::new(
        json!(1),
        "primitive.create",
        json!({"kind":"box","width":20.,"depth":20.,"height":20.,"position":position}),
    ))
    .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn section_misses_and_tangency_show_all_objects_for_both_flip_sides() {
    let mut app = app();
    box_body(&mut app, [0., 0., 0.]);
    box_body(&mut app, [60., 0., 0.]);
    app.selected.clear();
    app.bump_scene();
    app.refresh_scene();
    let original: Vec<_> = app
        .scene
        .lock()
        .unwrap()
        .meshes
        .iter()
        .map(|m| (m.positions.clone(), m.indices.clone()))
        .collect();
    let document = app.project_to_s33d().unwrap();
    app.start_section_view();
    for distance in [-100., -10., 10., 100.] {
        app.set_section_view_equation([1., 0., 0., distance]);
        // x=10 intersects neither box; the second remains visible even on the removed side.
        if distance == 10. || distance == -10. || distance.abs() == 100. {
            for flip in [false, true] {
                app.set_section_view_flip(flip);
                app.refresh_scene();
                let scene = app.scene.lock().unwrap();
                assert_eq!(
                    scene.meshes.len(),
                    original.len(),
                    "distance={distance}, flip={flip}"
                );
                for (mesh, expected) in scene.meshes.iter().zip(&original) {
                    assert_eq!((&mesh.positions, &mesh.indices), (&expected.0, &expected.1));
                }
            }
        }
    }
    app.set_section_view_equation([1., 0., 0., 0.]);
    app.refresh_scene();
    {
        let scene = app.scene.lock().unwrap();
        assert!(
            scene.meshes.iter().any(|m| m.positions == original[1].0),
            "uncut object remains whole"
        );
        assert!(
            scene
                .meshes
                .iter()
                .filter(|m| m.positions != original[1].0)
                .any(|m| !m.indices.is_empty()),
            "intersected object remains visible"
        );
    }
    app.cancel_section_view();
    assert_eq!(
        app.project_to_s33d().unwrap(),
        document,
        "view changes must not alter saved geometry or history"
    );
}

#[test]
fn cut_retains_geometry_for_paste_and_one_undo_restores_deleted_body() {
    let mut app = app();
    let id = box_body(&mut app, [0., 0., 0.]);
    let original = app.doc.body(id).unwrap().clone();
    app.selected = vec![id];
    app.cut_selected();
    assert!(app.doc.body(id).is_none());
    assert_eq!(app.clipboard.as_ref().unwrap().body, original);
    app.paste_clipboard();
    let pasted = app.selected[0];
    let shape = app.doc.body(pasted).unwrap().shape.unwrap();
    assert!(!app.kernel.tessellate(shape).unwrap().is_empty());
    assert!(app.undo());
    assert!(app.doc.body(pasted).is_none());
    assert!(app.undo());
    assert_eq!(app.doc.body(id).unwrap(), &original);
}

#[test]
fn input_and_output_directories_roundtrip_independently_without_changing_theme() {
    let mut app = app();
    let input = std::env::current_dir()
        .unwrap()
        .join("fixtures")
        .join("part.3mf");
    let output = std::env::current_dir()
        .unwrap()
        .join("output")
        .join("part.step");
    app.settings.appearance = crate::appearance::UiAppearance::Original;
    app.remember_directory(&input, false);
    app.remember_directory(&output, true);
    let restored: AppSettings =
        serde_json::from_str(&serde_json::to_string(&app.settings).unwrap()).unwrap();
    assert_eq!(restored.last_input_directory.as_deref(), input.parent());
    assert_eq!(restored.last_output_directory.as_deref(), output.parent());
    assert_eq!(
        restored.appearance,
        crate::appearance::UiAppearance::Original
    );
}

#[test]
fn three_mf_agent_and_ui_import_build_geometry_as_one_undoable_command() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kernel/tests/fixtures/core-build.3mf");
    for agent in [false, true] {
        let mut app = app();
        if agent {
            app.execute_agent_request(AgentRequest::new(
                json!(1),
                "file.import",
                json!({"path":path}),
            ))
            .unwrap();
        } else {
            crate::ui::import_model_path(&mut app, &path);
        }
        assert_eq!(app.doc.bodies.len(), 2, "only build items are imported");
        assert!(app.doc.bodies.iter().all(|b| b.shape.is_some()));
        assert!(app.undo());
        assert!(app.doc.bodies.is_empty());
        assert!(app.redo());
        assert_eq!(app.doc.bodies.len(), 2);
    }
}

#[test]
fn section_plane_between_disconnected_shells_does_not_remove_either_shell() {
    let mut app = app();
    let a = box_body(&mut app, [-40., 0., 0.]);
    let b = box_body(&mut app, [40., 0., 0.]);
    let mut mesh = app
        .kernel
        .tessellate(app.doc.body(a).unwrap().shape.unwrap())
        .unwrap();
    let second = app
        .kernel
        .tessellate(app.doc.body(b).unwrap().shape.unwrap())
        .unwrap();
    let offset = mesh.positions.len() as u32;
    mesh.positions.extend(second.positions);
    mesh.indices
        .extend(second.indices.into_iter().map(|i| i + offset));
    mesh.vertex_normals = None;
    let shape = app.kernel.insert_mesh(mesh.clone());
    app.doc.bodies.retain(|body| body.id == a);
    app.doc
        .bodies
        .iter_mut()
        .find(|body| body.id == a)
        .unwrap()
        .shape = Some(shape);
    app.start_section_view();
    app.set_section_view_equation([1., 0., 0., 0.]);
    for flip in [false, true] {
        app.set_section_view_flip(flip);
        app.refresh_scene();
        assert_eq!(app.scene.lock().unwrap().meshes.len(), 1);
        assert_eq!(
            app.scene.lock().unwrap().meshes[0].indices.len(),
            mesh.indices.len()
        );
    }
}
