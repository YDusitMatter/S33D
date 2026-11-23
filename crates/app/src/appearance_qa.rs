//! Headless integration checks for optional appearances. No renderer, window,
//! preferences file, or native geometry operation is needed by these tests.

use super::*;

#[derive(Debug)]
struct PaintedText {
    text: String,
    rect: egui::Rect,
    colors: Vec<egui::Color32>,
}

fn collect_text(shape: &egui::Shape, out: &mut Vec<PaintedText>) {
    match shape {
        egui::Shape::Text(text) => out.push(PaintedText {
            text: text.galley.job.text.clone(),
            rect: egui::Rect::from_min_size(text.pos, text.galley.size()),
            colors: text
                .galley
                .job
                .sections
                .iter()
                .map(|s| {
                    if s.format.color == egui::Color32::PLACEHOLDER {
                        text.fallback_color
                    } else {
                        s.format.color
                    }
                })
                .collect(),
        }),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, out);
            }
        }
        _ => {}
    }
}

fn input(size: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    }
}

fn new_app(ctx: &egui::Context) -> S33DApp {
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    // App construction installs the same CJK fallback used by the real UI.
    let mut app = S33DApp::new(&creation);
    app.settings = AppSettings::default();
    app.mode = Mode::Idle;
    app
}

fn workflow_frame(
    ctx: &egui::Context,
    app: &mut S33DApp,
    events: Vec<egui::Event>,
    width: f32,
    draw: fn(&mut S33DApp, &mut egui::Ui),
) -> Vec<PaintedText> {
    let mut raw = input(egui::vec2(1000., 2600.));
    raw.events = events;
    let output = ctx.run(raw, |ctx| {
        apply_design_system(ctx, &app.settings);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.set_max_width(width);
            ui.set_style(morning_toolbar_style(&app.settings));
            draw(app, ui);
        });
    });
    let mut text = Vec::new();
    for item in output.shapes {
        collect_text(&item.shape, &mut text);
    }
    text
}

fn workflow_click(
    ctx: &egui::Context,
    app: &mut S33DApp,
    text: &[PaintedText],
    label: &str,
    width: f32,
    draw: fn(&mut S33DApp, &mut egui::Ui),
) -> Vec<PaintedText> {
    let pos = text
        .iter()
        .find(|t| t.text == label)
        .unwrap_or_else(|| panic!("Missing {label}: {text:?}"))
        .rect
        .center();
    for pressed in [true, false] {
        workflow_frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
            width,
            draw,
        );
    }
    workflow_frame(ctx, app, vec![], width, draw)
}

#[test]
fn workflow_toolbar_group_and_ungroup_buttons_change_real_selection_with_undo() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.settings.language = UiLanguage::English;
    for _ in 0..2 {
        let id = app.doc.alloc_id();
        app.doc.add_body(s33d_model::Body {
            id,
            name: format!("Object {id}"),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "group-qa.stl".into(),
            },
            shape: None,
            material: Default::default(),
            visible: true,
        });
    }
    let bodies = app.doc.bodies.clone();
    let text = workflow_frame(&ctx, &mut app, vec![], 134., toolbar);
    workflow_click(&ctx, &mut app, &text, "Group", 134., toolbar);
    assert!(
        app.doc.groups.is_empty(),
        "Group is disabled without a selection"
    );
    app.selected = app.doc.bodies.iter().map(|b| b.id).collect();
    let text = workflow_frame(&ctx, &mut app, vec![], 134., toolbar);
    let text = workflow_click(&ctx, &mut app, &text, "Group", 134., toolbar);
    assert_eq!(app.doc.groups.len(), 1);
    assert_eq!(app.doc.groups[0].members, app.selected);
    workflow_click(&ctx, &mut app, &text, "Ungroup", 134., toolbar);
    assert!(app.doc.groups.is_empty());
    assert_eq!(app.doc.bodies, bodies, "Grouping changes no geometry");
    assert!(app.undo());
    assert_eq!(app.doc.groups.len(), 1);
    assert!(app.redo());
    assert!(app.doc.groups.is_empty());
}

#[test]
fn workflow_graphite_extrude_done_is_readable_and_commits_on_click() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings.appearance = UiAppearance::GraphiteStudio;
        app.settings.language = language;
        let id = app.doc.alloc_id();
        app.doc.add_body(s33d_model::Body {
            id,
            name: "Sketch".into(),
            kind: BodyKind::Sketch,
            feature: Feature::Sketch(s33d_model::SketchFeature {
                vertex_locks: Default::default(),
                plane: s33d_kernel::PlaneDef::from_normal([0., 1., 0.]),
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
        assert!(app.set_region_selection(app.selected[0], vec![0]));
        app.start_extrude();
        assert_eq!(app.mode, Mode::Extrude);
        let text = workflow_frame(&ctx, &mut app, vec![], 316., extrude_properties);
        let palette = app.settings.appearance.palette();
        let title = text
            .iter()
            .find(|t| t.text == language.text("Extrude", "拉伸"))
            .unwrap();
        assert!(title.colors.iter().all(|c| *c == palette.text));
        let done = text
            .iter()
            .find(|t| t.text == language.text("Done", "完成"))
            .unwrap();
        assert!(
            done.colors.iter().all(|c| *c == palette.on_accent),
            "Done: {done:?}"
        );
        assert_ne!(palette.on_accent, palette.accent);
        workflow_click(
            &ctx,
            &mut app,
            &text,
            language.text("Done", "完成"),
            316.,
            extrude_properties,
        );
        assert_eq!(app.mode, Mode::Idle);
        assert_eq!(
            app.doc
                .bodies
                .iter()
                .filter(|b| b.kind == BodyKind::Solid && b.visible)
                .count(),
            1
        );
    }
}

#[test]
fn workflow_exit_edge_treatment_button_is_available_without_an_edge() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.settings.language = UiLanguage::English;
    app.chamfer_active = true;
    let before = app.doc.bodies.clone();
    let text = workflow_frame(&ctx, &mut app, vec![], 316., properties);
    workflow_click(
        &ctx,
        &mut app,
        &text,
        "Exit Chamfer/Fillet",
        316.,
        properties,
    );
    assert!(!app.chamfer_active);
    assert_eq!(app.doc.bodies, before);
}

#[test]
fn workflow_settings_reenables_all_hidden_tips_without_resetting_other_preferences() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings.language = language;
        app.settings.unit = UiUnit::Inch;
        app.settings.show_first_use_tips = false;
        app.settings.show_tool_exit_confirmation = false;
        app.show_grid = false;
        let mut expected = app.settings.clone();
        expected.show_first_use_tips = true;
        expected.show_tool_exit_confirmation = true;
        let before = app.doc.bodies.clone();
        let text = workflow_frame(&ctx, &mut app, vec![], 316., hidden_tips_settings);
        workflow_click(
            &ctx,
            &mut app,
            &text,
            language.text("Re-enable all hidden tips", "重新启用所有隐藏提示"),
            316.,
            hidden_tips_settings,
        );
        assert_eq!(app.settings, expected);
        assert!(
            app.show_grid,
            "Restoring hidden tips also restores the grid and RGB axes"
        );
        assert_eq!(app.doc.bodies, before);
    }
}

#[test]
fn multiclick_getting_started_explains_draw3d_single_click_and_keeps_close_visible() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings.language = language;
        app.settings.font_size = UiFontSize::Large;
        app.first_use_help_open = true;
        let mut text = Vec::new();
        for _ in 0..3 {
            let output = ctx.run(input(egui::vec2(1280., 900.)), |ctx| {
                apply_design_system(ctx, &app.settings);
                first_use_window(&mut app, ctx);
            });
            text.clear();
            for item in output.shapes {
                collect_text(&item.shape, &mut text);
            }
        }
        assert!(text.iter().any(|t| t.text
            == language.text(
                "In Draw 3D, single-click a body to select and edit it.",
                "在 Draw 3D（绘制3D）工具内，单击实体即可选择并编辑。"
            )));
        let close = text
            .iter()
            .find(|t| t.text == language.text("Got it", "知道了"))
            .unwrap();
        assert!(
            close.rect.top() > 0. && close.rect.bottom() < 900.,
            "{language:?}: {close:?}"
        );
    }
}

#[test]
fn drawing_qa_has_localized_controls_in_all_appearances_without_3d_callbacks() {
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.settings.appearance = appearance;
            app.settings.language = language;
            app.first_use_help_open = false;
            app.objects_panel_open = false;
            app.toggle_drawing_menu();
            assert!(!app.drawing.show_sheet());
            let camera = app.camera.view_matrix();
            let output = ctx.run(input(egui::vec2(1100., 900.)), |ctx| draw(&mut app, ctx));
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|s| matches!(s.shape, egui::Shape::Callback(_))),
                "before Generate: retain the 3D workspace"
            );
            assert_eq!(app.camera.view_matrix(), camera);
            app.drawing.views = vec![
                crate::drawing2d::Projection {
                    min: glam::Vec2::ZERO,
                    max: glam::Vec2::splat(10.),
                    ..Default::default()
                };
                3
            ];
            let before = app.doc.bodies.clone();
            for size in [egui::vec2(1100., 900.), egui::vec2(900., 600.)] {
                let mut text = Vec::new();
                for _ in 0..3 {
                    let output = ctx.run(input(size), |ctx| draw(&mut app, ctx));
                    text.clear();
                    for item in output.shapes {
                        assert!(
                            !matches!(item.shape, egui::Shape::Callback(_)),
                            "drawing must not render the model/grid"
                        );
                        collect_text(&item.shape, &mut text);
                    }
                }
                for expected in [
                    language.text("3-view Drawing", "三视图工程图"),
                    language.text("Generate / Refresh", "生成 / 刷新"),
                    language.text("Automatic dimensions", "自动标注"),
                    language.text("Manual dimensions", "手动标注"),
                    language.text("Export…", "导出…"),
                    language.text("Selection · 3D preview", "当前选择 · 三维预览"),
                ] {
                    assert!(
                        text.iter().any(|t| t.text.contains(expected)),
                        "{appearance:?}/{language:?}/{size:?}: missing {expected}"
                    );
                }
                assert!(app.drawing.open);
                assert_eq!(app.doc.bodies, before);
            }
        }
    }
}

#[test]
fn drawing_qa_keyboard_routes_to_annotations_and_escape_cancels_before_exit() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.toggle_drawing_menu();
    app.drawing.views = vec![
        crate::drawing2d::Projection {
            min: glam::Vec2::ZERO,
            max: glam::Vec2::splat(10.),
            ..Default::default()
        };
        3
    ];
    crate::drawing2d::auto_dimensions(&mut app.drawing);
    let before = app.doc.bodies.clone();
    let mut key = |app: &mut S33DApp, key: egui::Key, modifiers: egui::Modifiers| {
        let mut raw = input(egui::vec2(1100., 800.));
        raw.modifiers = modifiers;
        raw.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| viewport(app, ui));
        });
        let mut raw = input(egui::vec2(1100., 800.));
        raw.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        });
        let _ = ctx.run(raw, |_| {});
    };
    key(&mut app, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(app.drawing.dimensions.is_empty());
    key(&mut app, egui::Key::R, egui::Modifiers::CTRL);
    assert_eq!(app.drawing.dimensions.len(), 6);
    app.drawing.pending.push((0, glam::Vec2::ZERO));
    key(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(app.drawing.open && app.drawing.pending.is_empty());
    key(&mut app, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!app.drawing.open);
    assert_eq!(app.doc.bodies, before);
}

#[test]
fn appearance_qa_start_dialog_and_enabled_toolbars_are_localized_and_compact() {
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.settings.appearance = appearance;
            app.settings.language = language;
            app.first_use_help_open = true;
            app.plugins.set_enabled("screw-generator", true);
            app.plugins.set_enabled("nut-generator", true);
            let mut text = Vec::new();
            for _ in 0..3 {
                let output = ctx.run(input(egui::vec2(1280., 900.)), |ctx| {
                    apply_design_system(ctx, &app.settings);
                    plugin_toolbars(&mut app, ctx);
                    first_use_window(&mut app, ctx);
                });
                text.clear();
                for shape in output.shapes {
                    collect_text(&shape.shape, &mut text);
                }
            }
            for expected in [
                language.text("Getting Started", "首次使用指引"),
                language.text("Simple", "简洁"),
                language.text("Appearance", "界面外观"),
                language.text("Screw Generator", "螺钉生成器"),
                language.text("Nut Generator", "螺帽生成器"),
            ] {
                assert!(
                    text.iter().any(|t| t.text.contains(expected)),
                    "{appearance:?}/{language:?}: {expected}"
                );
            }
            for t in text.iter().filter(|t| t.text.starts_with('▶')) {
                assert!(
                    t.rect.top() >= 0. && t.rect.bottom() < 45.,
                    "toolbar too tall: {t:?}"
                );
            }
            if appearance.is_dark() {
                for expected in [
                    language.text("Getting Started", "首次使用指引"),
                    language.text("Appearance", "界面外观"),
                    language.text("Language", "语言"),
                    language.text("Unit", "单位"),
                    language.text("Tips", "使用提示"),
                    language.text("Select", "选择"),
                    language.text("Use a tool", "使用工具"),
                ] {
                    let caption = text.iter().find(|t| t.text == expected).unwrap();
                    assert!(
                        caption
                            .colors
                            .iter()
                            .all(|c| *c == appearance.palette().text),
                        "emphasis must remain readable on the dark panel: {caption:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn appearance_qa_localized_toolbar_fits_compact_width_at_both_font_sizes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for font_size in [UiFontSize::Compact, UiFontSize::Large] {
                app.settings.appearance = appearance;
                app.settings.language = language;
                app.settings.font_size = font_size;
                let width = if appearance.is_original() {
                    94.0
                } else {
                    134.0
                };
                let mut painted = Vec::new();
                let mut panel = egui::Rect::NOTHING;
                // New windows/fonts need one layout pass before inspection.
                for _ in 0..2 {
                    let output = ctx.run(input(egui::vec2(900.0, 2600.0)), |ctx| {
                        apply_design_system(ctx, &app.settings);
                        let style = morning_toolbar_style(&app.settings);
                        panel = egui::SidePanel::left("appearance_qa_toolbar")
                            .resizable(false)
                            .exact_width(width)
                            .frame(egui::Frame::side_top_panel(&style))
                            .show(ctx, |ui| {
                                ui.set_style(style);
                                toolbar(&mut app, ui);
                            })
                            .response
                            .rect;
                    });
                    painted.clear();
                    for shape in output.shapes {
                        collect_text(&shape.shape, &mut painted);
                    }
                }
                for (english, chinese) in [
                    ("Move", "移动"),
                    ("Rotate", "旋转"),
                    ("Scale", "缩放"),
                    ("Align", "对齐"),
                    ("Sketch", "草图"),
                    ("Draw 3D", "绘制3D"),
                    ("Union", "并集"),
                    ("Subtract", "差集"),
                    ("Intersect", "交集"),
                    ("Mirror", "镜像"),
                    ("Copy", "复制"),
                    ("Delete", "删除"),
                    ("Array", "阵列"),
                    ("Group", "编组"),
                    ("Ungroup", "解组"),
                    ("Objects", "对象"),
                ] {
                    let expected = language.text(english, chinese);
                    assert!(
                        painted.iter().any(|text| text.text == expected),
                        "{appearance:?}/{language:?}/{font_size:?}: missing {expected:?}"
                    );
                }
                for text in &painted {
                    assert!(
                        text.rect.left() >= panel.left() - 0.5
                            && text.rect.right() <= panel.right() + 0.5,
                        "{appearance:?}/{language:?}/{font_size:?}: {:?} escapes toolbar: {:?} / {panel:?}",
                        text.text,
                        text.rect
                    );
                    assert!(
                        !text.text.chars().any(|c| {
                            matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
                        }),
                        "toolbar icons must not depend on private-use font characters"
                    );
                }
                assert!(panel.width() <= width + 0.5);
            }
        }
    }
}

#[test]
fn appearance_qa_labeled_icon_buttons_and_sidebar_keep_text_in_bounds() {
    let ctx = egui::Context::default();
    let app = new_app(&ctx);
    let mut settings = app.settings.clone();
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            settings.appearance = appearance;
            settings.language = language;
            settings.font_size = UiFontSize::Large;
            let labels = [
                (
                    language.text("Section\nView", "剖切\n视图"),
                    ToolIcon::Section,
                ),
                (
                    language.text("Offset/\nShell", "偏移/\n抽壳"),
                    ToolIcon::OffsetShell,
                ),
                (
                    language.text("Chamfer/\nFillet", "倒角/\n圆角"),
                    ToolIcon::Fillet,
                ),
            ];
            let mut buttons = Vec::new();
            let mut painted = Vec::new();
            for _ in 0..2 {
                buttons.clear();
                let output = ctx.run(input(egui::vec2(800.0, 800.0)), |ctx| {
                    apply_design_system(ctx, &settings);
                    let style = morning_toolbar_style(&settings);
                    egui::SidePanel::left("appearance_qa_buttons")
                        .resizable(false)
                        .exact_width(if appearance.is_original() {
                            94.0
                        } else {
                            134.0
                        })
                        .frame(egui::Frame::side_top_panel(&style))
                        .show(ctx, |ui| {
                            ui.set_style(style);
                            for (index, &(label, icon)) in labels.iter().enumerate() {
                                let response = ui.add(tool_tile(label, icon, index == 1));
                                buttons.push((label.to_owned(), response.rect));
                            }
                        });
                    egui::SidePanel::right("appearance_qa_sidebar")
                        .resizable(false)
                        .exact_width(316.0)
                        .frame(sidebar_frame(appearance))
                        .show(ctx, |ui| {
                            section_title(ui, language.text("Properties", "属性"), None);
                            let apply = language.text("Apply", "应用");
                            let done = language.text("Done", "完成");
                            let response = ui.add(primary_button(apply, &settings));
                            buttons.push((apply.to_owned(), response.rect));
                            let response = ui.button(done);
                            buttons.push((done.to_owned(), response.rect));
                        });
                });
                painted.clear();
                for shape in output.shapes {
                    collect_text(&shape.shape, &mut painted);
                }
            }
            for (label, rect) in &buttons {
                let text = painted
                    .iter()
                    .find(|text| &text.text == label)
                    .unwrap_or_else(|| {
                        panic!("{appearance:?}/{language:?}: missing button text {label:?}")
                    });
                assert!(
                    rect.expand(0.5).contains_rect(text.rect),
                    "{appearance:?}/{language:?}: {label:?} exceeds its button: {:?} / {rect:?}",
                    text.rect
                );
            }
            assert!(painted
                .iter()
                .any(|text| { text.text == language.text("Properties", "属性") }));
            let style = ctx.style();
            assert_eq!(style.visuals.dark_mode, appearance.is_dark());
            assert_eq!(style.visuals.panel_fill, appearance.palette().panel);
            assert!(style.visuals.override_text_color.is_none());
            assert_eq!(
                style.text_styles[&egui::TextStyle::Button].size,
                13.0 * UiFontSize::Large.scale()
            );
        }
    }
}

#[test]
fn appearance_qa_icons_render_without_any_font_glyphs() {
    let ctx = egui::Context::default();
    let output = ctx.run(input(egui::vec2(800.0, 800.0)), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            for (index, &icon) in ToolIcon::ALL.iter().enumerate() {
                let origin = egui::pos2(
                    16.0 + (index % 8) as f32 * 48.0,
                    16.0 + (index / 8) as f32 * 48.0,
                );
                crate::ui_icons::paint(
                    ui.painter(),
                    &egui::Rect::from_min_size(origin, egui::vec2(18.0, 18.0)),
                    icon,
                    egui::Color32::WHITE,
                );
            }
        });
    });
    let mut text = Vec::new();
    for shape in &output.shapes {
        collect_text(&shape.shape, &mut text);
    }
    assert!(text.is_empty(), "CAD icons must use paths, not font glyphs");
    assert!(output.shapes.len() > ToolIcon::ALL.len());
}

#[test]
fn appearance_qa_settings_migrate_and_roundtrip_without_resetting_preferences() {
    let old = AppSettings {
        language: UiLanguage::ChineseSimplified,
        font_size: UiFontSize::Large,
        color_theme: UiColorTheme::Teal,
        unit: UiUnit::Inch,
        ..AppSettings::default()
    };
    let mut legacy = serde_json::to_value(&old).unwrap();
    legacy.as_object_mut().unwrap().remove("appearance");
    let migrated: AppSettings = serde_json::from_value(legacy).unwrap();
    assert_eq!(migrated.appearance, UiAppearance::GraphiteStudio);
    assert_eq!(migrated, old);
    for appearance in UiAppearance::ALL {
        let settings = AppSettings {
            appearance,
            ..old.clone()
        };
        let restored: AppSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
    }
}

#[test]
fn appearance_qa_switching_does_not_change_model_selection_mode_or_camera() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mesh = s33d_kernel::MeshData::new(
        vec![[0., 0., 0.], [20., 0., 0.], [0., 30., 0.], [0., 0., 40.]],
        vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    );
    let shape = app.kernel.insert_mesh(mesh.clone());
    let id = app.doc.alloc_id();
    app.doc.add_body(s33d_model::Body {
        id,
        name: "Appearance must preserve this model".into(),
        kind: BodyKind::Solid,
        feature: Feature::Imported {
            path: "appearance-qa.stl".into(),
        },
        shape: Some(shape),
        material: s33d_model::Material {
            color: [0.12, 0.34, 0.56, 1.0],
            ..Default::default()
        },
        visible: true,
    });
    app.selected = vec![id];
    app.clear_region_selection();
    app.gizmo = Some(GizmoMode::Rotate);
    app.camera.eye = glam::Vec3::new(132.0, 154.0, 176.0);
    app.camera.target = glam::Vec3::new(10.0, 15.0, 20.0);
    app.camera.ortho = true;
    app.viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 900.));
    let document = serde_json::to_value(app.doc.archive()).unwrap();
    let camera = app.camera.clone();
    app.refresh_scene();
    // Rendering may split shared vertices to preserve hard facet normals.
    // Compare appearances to that display representation, while the separate
    // kernel assertion below continues to protect the original indexed mesh.
    let original_render_meshes = app.scene.lock().unwrap().meshes.clone();
    assert_eq!(original_render_meshes.len(), 1);
    for appearance in [
        UiAppearance::PrecisionClassic,
        UiAppearance::GraphiteStudio,
        UiAppearance::PorcelainWorkshop,
        UiAppearance::Original,
    ] {
        let previous_version = app.scene_version;
        app.set_ui_appearance(appearance);
        assert!(app.scene_version > previous_version);
        app.refresh_scene();
        assert_eq!(serde_json::to_value(app.doc.archive()).unwrap(), document);
        assert_eq!(app.kernel.tessellate(shape).unwrap(), mesh);
        assert_eq!(app.selected, vec![id]);
        assert!(app.selected_regions.is_empty());
        assert_eq!(app.mode, Mode::Idle);
        assert_eq!(app.gizmo, Some(GizmoMode::Rotate));
        assert_eq!(app.camera.eye, camera.eye);
        assert_eq!(app.camera.target, camera.target);
        assert_eq!(app.camera.up, camera.up);
        assert_eq!(app.camera.ortho, camera.ortho);
        assert_eq!(
            app.camera.proj_matrix(4.0 / 3.0),
            camera.proj_matrix(4.0 / 3.0)
        );
        let scene = app.scene.lock().unwrap();
        assert_eq!(scene.version, app.scene_version);
        assert_eq!(scene.meshes, original_render_meshes);
        drop(scene);
        let version = app.scene_version;
        app.set_ui_appearance(appearance);
        assert_eq!(
            app.scene_version, version,
            "reselecting an appearance is inert"
        );
    }
}

fn display_frame(
    ctx: &egui::Context,
    app: &mut S33DApp,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> Vec<PaintedText> {
    let mut raw = input(size);
    raw.events = events;
    let output = ctx.run(raw, |ctx| {
        apply_design_system(ctx, &app.settings);
        egui::CentralPanel::default().show(ctx, |ui| viewport(app, ui));
    });
    let mut text = Vec::new();
    for shape in output.shapes {
        collect_text(&shape.shape, &mut text);
    }
    text
}

#[test]
fn combined_viewport_switch_is_at_top_right_and_never_clicks_through_into_draw3d() {
    for (language, size) in [
        (UiLanguage::English, egui::vec2(1280., 800.)),
        (UiLanguage::ChineseSimplified, egui::vec2(520., 650.)),
    ] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings.language = language;
        app.start_insert_3d();
        let before = app.doc.bodies.clone();
        let mut text = display_frame(&ctx, &mut app, vec![], size);
        for _ in 0..2 {
            text = display_frame(&ctx, &mut app, vec![], size);
        }
        let label = language.text("Grid & RGB axes", "网格与 RGB 辅助轴");
        let area = text.iter().find(|t| t.text == label).unwrap().rect;
        assert!(
            area.center().x > app.viewport_rect.center().x
                && area.bottom() < app.viewport_rect.top() + 70.
        );
        let camera = app.camera.eye;
        for shown in [false, true] {
            let pos = text.iter().find(|t| t.text == label).unwrap().rect.center();
            for pressed in [true, false] {
                display_frame(
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
                    size,
                );
            }
            text = display_frame(&ctx, &mut app, vec![], size);
            assert_eq!(app.show_grid, shown);
            assert_eq!(!app.scene.lock().unwrap().grid_axis_lines.is_empty(), shown);
            assert_eq!(app.doc.bodies, before);
            assert_eq!(app.mode, Mode::Insert3D);
            assert!(!app.tool_exit_prompt_open());
            assert_eq!(app.camera.eye, camera);
        }
    }
}

#[test]
fn file_recent_entry_queues_exact_path_and_keeps_unsaved_project_until_confirmation() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.settings.language = language;
        let path = std::env::temp_dir().join("recent UI 工程.s33d");
        app.settings.recent_projects = vec![path.clone()];
        app.doc.rename("unsaved current project");
        let text = workflow_frame(&ctx, &mut app, vec![], 900., file_menu);
        let text = workflow_click(
            &ctx,
            &mut app,
            &text,
            language.text("File", "文件"),
            900.,
            file_menu,
        );
        let text = workflow_click(
            &ctx,
            &mut app,
            &text,
            language.text("Open Recent", "最近打开"),
            900.,
            file_menu,
        );
        workflow_click(&ctx, &mut app, &text, "recent UI 工程", 900., file_menu);
        assert!(
            matches!(&app.project_session.pending,Some(crate::app::project_session::ProjectAction::Open(p)) if p==&path)
        );
        assert_eq!(app.doc.name(), "unsaved current project");
        assert!(app.project_session.path.is_none());
    }
}
