use super::*;

fn profile(app: &mut S33DApp, pocket: bool) {
    let id = app.doc.alloc_id();
    app.doc.add_body(s33d_model::Body {
        id,
        name: "Profile".into(),
        kind: BodyKind::Sketch,
        feature: s33d_model::Feature::Sketch(s33d_model::SketchFeature {
            vertex_locks: Default::default(),
            plane: s33d_kernel::DatumPlane::Top.def(),
            entities: vec![s33d_model::SketchEntity::Rect {
                c1: [-5., -5.],
                c2: [5., 5.],
            }],
            world_xyz_coordinates: false,
            on_body: None,
        }),
        shape: None,
        material: Default::default(),
        visible: true,
    });
    app.selected = vec![id];
    if pocket {
        app.start_pocket();
    } else {
        app.start_extrude();
    }
    assert_eq!(app.mode, Mode::Extrude);
}
fn frame(ctx: &egui::Context, app: &mut S33DApp, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 800.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::SidePanel::right("properties")
                .default_width(300.)
                .show(ctx, |ui| properties(app, ui));
            egui::CentralPanel::default().show(ctx, |ui| viewport(app, ui));
        },
    )
}
fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn viewport_click(
    ctx: &egui::Context,
    app: &mut S33DApp,
    pos: egui::Pos2,
    button: egui::PointerButton,
) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn rendered_label(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().rev().find_map(|shape| find(shape, label))
            }
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|shape| find(&shape.shape, label))
}

fn select_prism_face(ctx: &egui::Context, app: &mut S33DApp, cylinder: bool) -> u64 {
    let points = if cylinder {
        (0..96)
            .map(|index| {
                let angle = std::f64::consts::TAU * f64::from(index) / 96.;
                [3. * angle.cos(), 3. * angle.sin()]
            })
            .collect()
    } else {
        vec![[-5., -5.], [5., -5.], [5., 5.], [-5., 5.]]
    };
    let shape = app
        .kernel
        .extrude_region(
            &s33d_kernel::Region2D::new(points),
            &DatumPlane::Top.def(),
            10.,
        )
        .unwrap();
    let id = app.doc.alloc_id();
    app.doc.add_body(s33d_model::Body {
        id,
        name: "Test prism".into(),
        kind: BodyKind::Solid,
        feature: Feature::Derived {
            source: "UI test".into(),
            operation: None,
        },
        shape: Some(shape),
        material: Default::default(),
        visible: true,
    });
    app.camera.target = glam::Vec3::new(0., 5., 0.);
    app.camera.eye = glam::Vec3::new(20., 5., 0.);
    app.camera.up = glam::Vec3::Y;
    frame(ctx, app, vec![]);
    let hit = app
        .pick_face(app.viewport_rect.center(), app.viewport_rect)
        .unwrap()
        .0;
    assert_eq!(hit.body_id, id);
    app.selected = vec![id];
    app.face_pick = Some(hit);
    id
}

#[test]
fn viewport_surface_menu_activates_the_selected_planar_profile_tool() {
    for sketch_region in [false, true] {
        for (action, mode) in [
            ("Sketch on this surface", Mode::Sketch),
            ("Extrude (E)", Mode::Extrude),
            ("Pocket (P)", Mode::Extrude),
            ("Revolve", Mode::RevolveAxis),
            ("Deselect surface (Esc)", Mode::Idle),
        ] {
            let ctx = egui::Context::default();
            let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
            app.settings.language = UiLanguage::English;
            let owner = if sketch_region {
                profile(&mut app, false);
                app.cancel_extrude();
                let id = app.selected[0];
                assert!(app.set_region_selection(id, vec![0]));
                frame(&ctx, &mut app, vec![]);
                id
            } else {
                select_prism_face(&ctx, &mut app, false)
            };
            let center = app.viewport_rect.center();
            viewport_click(&ctx, &mut app, center, egui::PointerButton::Secondary);
            let menu = frame(&ctx, &mut app, vec![]);
            assert!(rendered_label(&menu, "Generate thread").is_none());
            let item = rendered_label(&menu, action)
                .unwrap_or_else(|| panic!("Missing {action}, sketch={sketch_region}"));
            viewport_click(&ctx, &mut app, item, egui::PointerButton::Primary);
            assert_eq!(app.mode, mode, "{action}, sketch={sketch_region}");
            if mode == Mode::Extrude {
                let cut = app.extrude_ctx.as_ref().unwrap();
                assert_eq!(cut.body_id, owner);
                assert_eq!(cut.pocket, action == "Pocket (P)");
            } else if mode == Mode::Idle {
                assert!(app.face_pick.is_none());
                assert!(app.selected_regions.is_empty());
                assert_eq!(app.selected, vec![owner]);
            }
        }
    }
}

#[test]
fn viewport_surface_menu_opens_thread_only_for_a_supported_cylinder() {
    let ctx = egui::Context::default();
    let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
    app.settings.language = UiLanguage::English;
    let owner = select_prism_face(&ctx, &mut app, true);
    let center = app.viewport_rect.center();
    viewport_click(&ctx, &mut app, center, egui::PointerButton::Secondary);
    let menu = frame(&ctx, &mut app, vec![]);
    assert!(rendered_label(&menu, "Extrude (E)").is_none());
    let item = rendered_label(&menu, "Generate thread").expect("cylinder thread option");
    viewport_click(&ctx, &mut app, item, egui::PointerButton::Primary);
    assert!(app.open_thread_panel);
    assert_eq!(app.thread_choices[0].body_id, owner);
}

#[test]
fn export_suggests_the_project_name_and_prefers_the_step_suffix() {
    assert_eq!(
        export_project_name("display holder V2"),
        "display holder V2"
    );
    assert_eq!(export_project_name("支架.v2"), "支架.v2");
    assert_eq!(export_project_name("test/fixture"), "test_fixture");
    assert_eq!(export_project_name("  .. "), "Untitled Project");
    assert_eq!(MODEL_EXPORT_FILTERS[0], ("STEP", &["step", "stp"][..]));
}

#[test]
fn pocket_shortcut_uses_selected_closed_region() {
    let ctx = egui::Context::default();
    let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
    profile(&mut app, false);
    app.cancel_extrude();
    let owner = app.selected[0];
    assert!(app.set_region_selection(owner, vec![0]));
    frame(&ctx, &mut app, vec![key(egui::Key::P, true)]);
    let cut = app.extrude_ctx.as_ref().unwrap();
    assert!(cut.pocket);
    assert_eq!(cut.body_id, owner);
    assert_eq!(cut.region_indices, vec![0]);
}

#[test]
fn escape_keeps_completed_sketch_entities_and_remembered_vertex() {
    let ctx = egui::Context::default();
    let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
    app.start_sketch(s33d_kernel::DatumPlane::Top.def());
    let sketch = app.sketch.as_mut().unwrap();
    let line = s33d_model::SketchEntity::Line {
        a: [0., 0.],
        b: [3., 0.],
    };
    sketch.entities.push(line.clone());
    app.sync_sketch_history();
    app.sketch.as_mut().unwrap().set_locked_vertex(0, Some(1));
    app.sync_sketch_history();
    assert!(app.undo());
    assert_eq!(app.sketch.as_ref().unwrap().locked_vertex(0), Some(0));
    assert!(app.redo());
    let sketch = app.sketch.as_mut().unwrap();
    sketch.pending.push([8., 2.]);
    frame(&ctx, &mut app, vec![key(egui::Key::Escape, true)]);
    assert_eq!(app.mode, Mode::Sketch);
    assert!(app.sketch.as_ref().unwrap().pending.is_empty());
    frame(&ctx, &mut app, vec![key(egui::Key::Escape, false)]);
    frame(&ctx, &mut app, vec![key(egui::Key::Escape, true)]);
    assert_eq!(app.mode, Mode::Idle);
    let id = app.selected[0];
    let saved = app.body(id).unwrap().sketch().unwrap();
    assert_eq!(saved.entities, vec![line.clone()]);
    assert_eq!(saved.vertex_locks.get(&0), Some(&Some(1)));
    assert!(!app.sketch_tools_open);

    app.edit_sketch(id);
    let edited = app.sketch.as_mut().unwrap();
    assert_eq!(edited.locked_vertex(0), Some(1));
    edited.update_line_entity(0, [0., 2.], [3., 0.]);
    app.escape_sketch();
    assert_eq!(app.mode, Mode::Idle);
    let updated = app.body(id).unwrap().sketch().unwrap();
    assert_ne!(updated.entities, vec![line]);
    assert_eq!(updated.vertex_locks.get(&0), Some(&Some(1)));
}

#[test]
fn tool_input_first_number_accepts_typing_once_and_viewport_drag_updates_same_value() {
    for pocket in [false, true] {
        let ctx = egui::Context::default();
        let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
        profile(&mut app, pocket);
        app.camera.set_view(crate::camera::ViewDir::Top);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        assert!(ctx.memory(|m| m.focused().is_some()));
        frame(&ctx, &mut app, vec![egui::Event::Text("12".into())]);
        assert!((app.extrude_ctx.as_ref().unwrap().depth - 12.).abs() < 1e-6);
        let before = app.doc.bodies.clone();
        let p = app.viewport_rect.center();
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(p), pointer(p, true)],
        );
        let end = p + egui::vec2(0., -80.);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(end)]);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(end), pointer(end, false)],
        );
        let depth = app.extrude_ctx.as_ref().unwrap().depth;
        assert!(depth > 12., "head-on dragging must change depth: {depth}");
        frame(&ctx, &mut app, vec![]);
        assert_eq!(
            app.extrude_ctx.as_ref().unwrap().depth,
            depth,
            "text editor must not restore stale typed depth"
        );
        assert_eq!(app.doc.bodies, before, "drag edits only the draft");
        assert!(
            ctx.memory(|m| m.focused().is_none()),
            "viewport retains focus after drag"
        );
        app.cancel_extrude();
        if pocket {
            app.start_pocket();
        } else {
            app.start_extrude();
        }
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        assert!(
            ctx.memory(|m| m.focused().is_some()),
            "reopening the same tool also focuses its first number"
        );
        frame(&ctx, &mut app, vec![egui::Event::Text("7".into())]);
        assert_eq!(app.extrude_ctx.as_ref().unwrap().depth, 7.);
    }
}

#[test]
fn tool_input_section_handle_drag_keeps_normal_and_updates_equation_without_picking_face() {
    let ctx = egui::Context::default();
    let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
    let region =
        s33d_kernel::Region2D::new(vec![[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]]);
    let shape = app
        .kernel
        .extrude_region(&region, &s33d_kernel::DatumPlane::Top.def(), 20.)
        .unwrap();
    let id = app.doc.alloc_id();
    app.doc.add_body(s33d_model::Body {
        id,
        name: "Section box".into(),
        kind: BodyKind::Solid,
        feature: Feature::Derived {
            source: "fixture".into(),
            operation: None,
        },
        shape: Some(shape),
        material: Default::default(),
        visible: true,
    });
    app.fit_view();
    app.start_section_view();
    app.camera.set_view(crate::camera::ViewDir::Front);
    frame(&ctx, &mut app, vec![]);
    let p = app.viewport_rect.center();
    let original = app.section_view_ctx.as_ref().unwrap().plane_def.unwrap();
    let equation = app.section_view_ctx.as_ref().unwrap().equation;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(p), pointer(p, true)],
    );
    let end = p + egui::vec2(0., -65.);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(end)]);
    frame(&ctx, &mut app, vec![pointer(end, false)]);
    let section = app.section_view_ctx.as_ref().unwrap();
    assert_eq!(section.plane_def.unwrap().normal(), original.normal());
    assert_ne!(section.equation[3], equation[3]);
    assert_eq!(
        section.equation,
        crate::app::section_interaction::equation(&section.plane_def.unwrap())
    );
    assert!(section.drag_start.is_none());
    assert!(app.face_pick.is_none());
}
