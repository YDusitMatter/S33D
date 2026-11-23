//! Selection-bound thread editor. Recognition remains in the geometry layer.
use super::{config::*, Params, Profile, Target};
use crate::app::{S33DApp, UiLanguage};
use egui::{self, Color32};

#[cfg(test)]
mod launch_task_tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        app: &mut S33DApp,
        target: &Target,
        height: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect, egui::Rect)> {
        frame_with_size(ctx, app, target, egui::vec2(286., height), events)
    }

    fn frame_with_size(
        ctx: &egui::Context,
        app: &mut S33DApp,
        target: &Target,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect, egui::Rect)> {
        fn collect(
            shape: &egui::Shape,
            clip: egui::Rect,
            texts: &mut Vec<(String, egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Text(t) => texts.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                    clip,
                )),
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect(s, clip, texts);
                    }
                }
                _ => {}
            }
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    // Match the nested properties scroller and the controls
                    // above the editor; its budget must exclude both headers.
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.heading(app.settings.language.text("Properties", "属性"));
                            ui.add_space(4.);
                            egui::CollapsingHeader::new("Thread generator")
                                .default_open(true)
                                .open(Some(true))
                                .show(ui, |ui| editor(app, ui, target));
                        });
                });
            },
        );
        let mut texts = vec![];
        for s in output.shapes {
            collect(&s.shape, s.clip_rect, &mut texts);
        }
        texts
    }

    #[test]
    fn launch_task_thread_parameters_expand_and_footer_adapts_to_available_space() {
        for lang in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for font_scale in [1., 1.3] {
                let ctx = egui::Context::default();
                ctx.style_mut(|style| {
                    for font in style.text_styles.values_mut() {
                        font.size *= font_scale;
                    }
                });
                let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
                app.settings.language = lang;
                let target = Target {
                    body_id: 1,
                    face_id: 1,
                    origin: [0.; 3],
                    axis: [0., 1., 0.],
                    radius: 4.,
                    length: 12.,
                    internal: false,
                };
                // Resize the same panel and change its footer content. A tall
                // window must show the final parameter without scrolling; a
                // short one must retain the validation and action controls.
                for (width, height, invalid, save_open) in [
                    (286., 1000., false, false),
                    (286., 620., true, false),
                    (286., 320., true, false),
                    (420., 620., true, true),
                    (286., 1000., false, false),
                ] {
                    app.thread_params.diameter = if invalid { 6. } else { 8. };
                    app.thread_ui.save_open = save_open;
                    app.thread_ui.load_error =
                        save_open.then(|| "Cannot read thread presets".into());
                    let size = egui::vec2(width, height);
                    let texts = frame_with_size(&ctx, &mut app, &target, size, vec![]);
                    let mut expected = vec![lang.text("Generate thread", "生成螺纹")];
                    if invalid {
                        expected.extend([
                            "warning:",
                            lang.text("Correct according to nominal diameter", "按公称直径修正"),
                        ]);
                    } else {
                        expected.extend([
                            lang.text("Included angle", "牙型角"),
                            lang.text("Root flat / pitch", "牙底平段 / 螺距"),
                            lang.text("Left hand", "左旋"),
                            lang.text("Diameter taper (mm/mm)", "直径锥度（mm/mm）"),
                        ]);
                    }
                    if save_open {
                        expected.push(lang.text("Preset name", "预设名称"));
                        expected.push(lang.text("Save", "保存"));
                    }
                    for needle in expected {
                        let (_, rect, clip) = texts
                            .iter()
                            .find(|(text, _, _)| {
                                if needle == "warning:" {
                                    text.starts_with(needle)
                                } else {
                                    text == needle
                                }
                            })
                            .expect(needle);
                        assert!(
                            clip.contains_rect(*rect) && rect.bottom() <= height,
                            "{lang:?}, scale {font_scale}, size {size:?}, {needle}: {rect:?}, clip {clip:?}"
                        );
                    }
                    if invalid {
                        // If parameters still need scrolling, their viewport
                        // must use the space down to the footer, not leave an
                        // arbitrary height cap's blank area beneath it.
                        let generate = texts
                            .iter()
                            .find(|(t, _, _)| t == lang.text("Generate thread", "生成螺纹"))
                            .unwrap();
                        let last = texts
                            .iter()
                            .find(|(t, _, _)| t == "save as preset")
                            .unwrap();
                        assert!(
                            last.1.bottom() > height - 70.,
                            "unused panel space: {last:?}, height {height}"
                        );
                        assert!(generate.1.bottom() <= last.1.bottom());
                    }
                }
            }
        }
    }

    #[test]
    fn launch_task_warning_generate_visible_and_correct_preserves_other_parameters() {
        for lang in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for (height, kind) in [
                (320., 0),
                (620., 0),
                (320., 1),
                (620., 1),
                (320., 2),
                (620., 2),
            ] {
                let internal = kind != 0;
                let ctx = egui::Context::default();
                let mut app = S33DApp::from_ui_state(crate::app::UiStateFile::default(), false);
                app.settings.language = lang;
                use s33d_kernel::{Axis2D, DatumPlane, GeometryKernel, Region2D};
                let (lo, hi) = if internal { (4., 6.) } else { (0., 4.) };
                let mut shape = app
                    .kernel
                    .revolve_region(
                        &Region2D::new(vec![[lo, 0.], [hi, 0.], [hi, 12.], [lo, 12.]]),
                        &DatumPlane::Front.def(),
                        &Axis2D {
                            point: [0., 0.],
                            dir: [0., 1.],
                        },
                        360.,
                    )
                    .unwrap();
                let mut feature = s33d_model::Feature::Derived {
                    source: "test cylinder / bore".into(),
                    operation: None,
                };
                if kind == 2 {
                    let sketch = s33d_model::SketchFeature {
                        vertex_locks: Default::default(),
                        plane: DatumPlane::Front.def(),
                        on_body: None,
                        world_xyz_coordinates: false,
                        entities: vec![
                            s33d_model::SketchEntity::Rect {
                                c1: [-6., -6.],
                                c2: [6., 6.],
                            },
                            s33d_model::SketchEntity::Circle { c: [0., 0.], r: 4. },
                        ],
                    };
                    let regions = s33d_model::compute_regions(&sketch.entities);
                    let index = regions.iter().position(|r| r.holes.len() == 1).unwrap();
                    shape = app
                        .kernel
                        .extrude_region(&regions[index], &sketch.plane, 12.)
                        .unwrap();
                    feature = s33d_model::Feature::Extrude {
                        sketch,
                        params: s33d_model::ExtrudeParams {
                            depth: 12.,
                            symmetric: false,
                            bevel: 0.,
                        },
                        source_sketch: None,
                        pocket_source_sketches: vec![],
                        region_indices: vec![index],
                    };
                }
                let id = app.doc.alloc_id();
                app.doc.add_body(s33d_model::Body {
                    id,
                    name: "Correction fixture".into(),
                    kind: s33d_model::BodyKind::Solid,
                    feature,
                    shape: Some(shape),
                    material: Default::default(),
                    visible: true,
                });
                let target = app
                    .thread_targets(&[id])
                    .unwrap()
                    .into_iter()
                    .find(|t| t.internal == internal)
                    .unwrap();
                let before = app.thread_params.clone();
                frame(&ctx, &mut app, &target, height, vec![]);
                let texts = frame(&ctx, &mut app, &target, height, vec![]);
                for needle in [
                    lang.text("Generate thread", "生成螺纹"),
                    "warning:",
                    lang.text("Correct according to nominal diameter", "按公称直径修正"),
                ] {
                    let (_, rect, clip) = texts
                        .iter()
                        .find(|(text, _, _)| text.contains(needle))
                        .expect(needle);
                    assert!(
                        clip.contains_rect(*rect) && rect.bottom() <= height,
                        "{needle}: {rect:?}, clip {clip:?}, height {height}"
                    );
                }
                let click = texts
                    .iter()
                    .find(|(t, _, _)| {
                        t == lang.text("Correct according to nominal diameter", "按公称直径修正")
                    })
                    .unwrap()
                    .1
                    .center();
                frame(
                    &ctx,
                    &mut app,
                    &target,
                    height,
                    vec![
                        egui::Event::PointerMoved(click),
                        egui::Event::PointerButton {
                            pos: click,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                frame(
                    &ctx,
                    &mut app,
                    &target,
                    height,
                    vec![egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                assert_eq!(
                    app.thread_params, before,
                    "Correct must retain nominal diameter too"
                );
                let corrected = app
                    .thread_targets(&[id])
                    .unwrap()
                    .into_iter()
                    .find(|t| t.internal == internal)
                    .unwrap();
                let radius = 3. - if internal { before.depth } else { 0. };
                assert!((corrected.radius - radius).abs() < 1e-6);
                assert!(super::super::validate(&corrected, &app.thread_params).is_ok());
                // Exercise the real panel after the shape/face identity changes.
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| panel(&mut app, ui));
                });
                assert_eq!(app.thread_params, before);
                assert!(app.thread_ui.expanded);
            }
        }
    }

    #[test]
    fn launch_task_blank_correction_retains_nominal_for_bore_and_taper() {
        let p = Params {
            length: 8.,
            taper: 0.0625,
            ..Default::default()
        };
        for internal in [false, true] {
            let t = Target {
                body_id: 1,
                face_id: 0,
                origin: [0.; 3],
                axis: [0., 1., 0.],
                radius: 4.,
                length: 12.,
                internal,
            };
            let corrected = super::super::corrected_blank_target(&t, &p).unwrap();
            let expected = if internal {
                p.diameter * 0.5 - p.depth
            } else {
                p.diameter * 0.5 + p.length * p.taper * 0.5
            };
            assert!((corrected.radius - expected).abs() < 1e-12);
            assert_eq!(p.diameter, 6.);
        }
    }
}

pub fn panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let Some(face) = app
        .face_pick
        .clone()
        .filter(|f| app.selected.contains(&f.body_id))
    else {
        app.thread_ui.target_key = None;
        app.thread_ui.expanded = false;
        app.thread_ui.print_open = false;
        app.open_thread_panel = false;
        return;
    };
    let Some(target) = app.thread_target_for_face(&face) else {
        app.thread_ui.target_key = None;
        app.thread_ui.expanded = false;
        app.thread_ui.print_open = false;
        app.open_thread_panel = false;
        return;
    };
    let key = format!(
        "{:?}:{:?}",
        app.doc.body(face.body_id).and_then(|b| b.shape),
        serde_json::to_string(&target)
    );
    let force_open = std::mem::take(&mut app.open_thread_panel);
    if app.thread_ui.target_key.as_ref() != Some(&key) {
        // Explicit opening already prepared a draft, or rebound a corrected
        // wall to the existing draft. Do not fit parameters back to geometry.
        if !force_open {
            app.prepare_thread_draft(&target);
        }
        app.thread_ui.target_key = Some(key);
        app.thread_ui.expanded = force_open;
        app.thread_ui.selected_preset = None;
        app.thread_ui.baseline = None;
        app.thread_ui.save_open = false;
        app.thread_ui.print_open = false;
        app.thread_ui.error = None;
    } else if force_open {
        app.thread_ui.expanded = true;
    }
    let lang = app.settings.language;
    let shown = egui::CollapsingHeader::new(lang.text("Thread generator", "螺纹生成"))
        .id_salt("selected_cylinder_thread_generator")
        .open(Some(app.thread_ui.expanded))
        .show(ui, |ui| editor(app, ui, &target));
    if shown.header_response.clicked() {
        app.thread_ui.expanded = !app.thread_ui.expanded;
    }
    print_settings_window(app, ui.ctx());
}

fn editor(app: &mut S33DApp, ui: &mut egui::Ui, target: &Target) {
    let lang = app.settings.language;
    let unit = app.settings.unit;
    let chinese = lang == UiLanguage::ChineseSimplified;
    // The parent properties scroller may offer more room than is visible.
    // Use the remaining viewport, after the properties/header controls, and
    // reserve only the footer's measured height (including wrapped warnings).
    let footer_height_id = ui.make_persistent_id("thread_footer_height");
    let footer_height = ui.ctx().data(|data| data.get_temp::<f32>(footer_height_id));
    let available_height = (ui
        .available_rect_before_wrap()
        .bottom()
        .min(ui.clip_rect().bottom())
        - ui.cursor().top())
    .max(0.);
    let parameter_height =
        (available_height - footer_height.unwrap_or(0.) - ui.spacing().item_spacing.y).max(0.);
    // Only parameters scroll. Validation and Generate remain below this area.
    egui::ScrollArea::vertical().id_salt("thread_parameters_scroll")
        .min_scrolled_height(0.)
        .max_height(parameter_height).show(ui, |ui| {
    ui.label(
        egui::RichText::new(lang.text("Selected cylinder geometry", "选中柱面的几何特征"))
            .strong()
            .color(ui.visuals().text_color()),
    );
    let name = app
        .doc
        .body(target.body_id)
        .map(|b| b.name.as_str())
        .unwrap_or("?");
    ui.label(format!(
        "{} · {}",
        name,
        if target.internal {
            lang.text("Internal / nut", "内孔 / 螺帽")
        } else {
            lang.text("External / screw", "外柱 / 螺钉")
        }
    ));
    ui.label(format!(
        "Ø{} · {} {}",
        unit.format_mm(target.radius * 2.),
        lang.text("Axial length", "轴向长度"),
        unit.format_mm(target.length)
    ));
    egui::CollapsingHeader::new(lang.text("Axis details", "轴线明细")).show(ui, |ui| {
    ui.label(format!(
        "{}: ({:.3}, {:.3}, {:.3})",
        lang.text("Axis", "轴向"),
        target.axis[0],
        target.axis[1],
        target.axis[2]
    ));
    ui.label(format!(
        "{}: ({}, {}, {})",
        lang.text("Axis origin", "轴线起点"),
        unit.format_mm(target.origin[0]),
        unit.format_mm(target.origin[1]),
        unit.format_mm(target.origin[2])
    ));
    });
    ui.separator();

    app.thread_ui.mark_edited(&app.thread_params);
    // Build and sort only candidates that can actually be cut on this wall.
    let mut choices: Vec<(String, Params, Option<PrintSettings>)> = super::presets()
        .into_iter()
        .filter_map(|(name, _, _, _)| {
            let mut p = super::preset_params(name, target.internal)?;
            p.length = app.thread_params.length.min(target.length);
            p.offset = app.thread_params.offset;
            compatible(target, &p).then_some((name.into(), p, None))
        })
        .collect();
    choices.extend(
        app.thread_ui
            .saved
            .iter()
            .filter(|p| p.internal == target.internal && compatible(target, &p.params))
            .map(|p| (p.name.clone(), p.params.clone(), Some(p.printing.clone()))),
    );
    choices.sort_by(|a, b| {
        let error =
            |p: &Params| (super::required_blank_radius(target.internal, p) - target.radius).abs();
        error(&a.1).total_cmp(&error(&b.1)).then(a.0.cmp(&b.0))
    });
    ui.label(lang.text("Preset", "预设"));
    egui::ComboBox::from_id_salt("thread_presets")
        .selected_text(app.thread_ui.selected_preset.as_deref().unwrap_or("Custom"))
        .width(ui.available_width().min(250.))
        .show_ui(ui, |ui| {
            for (name, params, printing) in &choices {
                if ui
                    .selectable_label(app.thread_ui.selected_preset.as_deref() == Some(name), name)
                    .clicked()
                {
                    app.thread_params = params.clone();
                    app.thread_ui.selected_preset = Some(name.clone());
                    app.thread_ui.baseline = Some(params.clone());
                    if let Some(printing) = printing {
                        app.thread_ui.printing = printing.clone();
                    }
                }
            }
            if choices.is_empty() {
                ui.small(lang.text("No compatible presets", "没有符合当前柱面的预设"));
            }
            ui.separator();
            if ui
                .selectable_label(app.thread_ui.selected_preset.is_none(), "Custom")
                .clicked()
            {
                app.thread_ui.selected_preset = None;
                app.thread_ui.baseline = None;
            }
        });
    ui.separator();
    ui.label(
        egui::RichText::new(lang.text("Thread parameters", "螺纹特征"))
            .strong()
            .color(ui.visuals().text_color()),
    );
    let p = &mut app.thread_params;
    ui.horizontal(|ui| {
        ui.label(lang.text("Nominal diameter", "公称直径"));
        ui.add(crate::ui::length_drag_value(&mut p.diameter, unit).speed(0.01));
    });
    let suggest = ui
        .horizontal(|ui| {
            let suggest = ui.button("suggest for 3D printing").clicked();
            if ui
                .button("...")
                .on_hover_text(lang.text("Print settings", "打印设置"))
                .clicked()
            {
                app.thread_ui.print_open = true;
            }
            suggest
        })
        .inner;
    if suggest {
        match app.thread_ui.printing.suggest(p, target.internal) {
            Ok(suggested) => {
                *p = suggested;
                app.thread_ui.error = None;
            }
            Err(e) => app.thread_ui.error = Some(e),
        }
        app.thread_ui.selected_preset = None;
        app.thread_ui.baseline = None;
    }
    let cell_width = ((ui.available_width() - 12.) * 0.5).max(60.);
    egui::Grid::new("thread_parameter_pairs").num_columns(2).spacing([8., 5.])
        .show(ui, |ui| {
    for (index, (label, value)) in [
        (lang.text("Pitch", "螺距"), &mut p.pitch),
        (lang.text("Length", "长度"), &mut p.length),
        (lang.text("Start offset", "起始偏移"), &mut p.offset),
        (lang.text("Radial depth", "径向牙深"), &mut p.depth),
        (
            lang.text("Groove clearance", "牙槽附加切深"),
            &mut p.clearance,
        ),
        (
            lang.text("Mating allowance / part", "每件径向配合余量"),
            &mut p.radial_fit,
        ),
    ].into_iter().enumerate() {
            ui.vertical(|ui| {
                ui.set_width(cell_width);
                let label_response = ui.add(egui::Label::new(label).wrap());
                if index == 2 {
                    label_response.on_hover_text(lang.text(
                        "Measured from the origin of the selected cylindrical band, including an uncut band left by an earlier thread operation.",
                        "从当前选中柱面段的起点计算；再次加工时以未加工剩余段的起点为准。",
                    ));
                }
                ui.add(crate::ui::length_drag_value(value, unit).speed(0.01).max_decimals(3));
            });
            if index % 2 == 1 { ui.end_row(); }
    }
    });
    egui::ComboBox::from_id_salt("thread_profile")
        .selected_text(match p.profile {
            Profile::TruncatedV => lang.text("Truncated V profile", "截平 V 形牙"),
            Profile::Rounded55 => lang.text("Rounded Whitworth 55°", "惠氏圆弧 55° 牙"),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut p.profile,
                Profile::TruncatedV,
                lang.text("Truncated V profile", "截平 V 形牙"),
            );
            ui.selectable_value(
                &mut p.profile,
                Profile::Rounded55,
                lang.text("Rounded Whitworth 55°", "惠氏圆弧 55° 牙"),
            );
        });
    ui.add_enabled_ui(p.profile == Profile::TruncatedV, |ui| {
        ui.horizontal(|ui| {
            ui.label(lang.text("Included angle", "牙型角"));
            ui.add(egui::DragValue::new(&mut p.angle_deg).suffix("°"));
        });
        ui.horizontal(|ui| {
            ui.label(lang.text("Root flat / pitch", "牙底平段 / 螺距"));
            ui.add(egui::DragValue::new(&mut p.flat_fraction).speed(0.005));
        });
    });
    if p.profile == Profile::Rounded55 {
        p.angle_deg = 55.;
        p.depth = p.pitch * 0.640327;
    }
    ui.checkbox(&mut p.left_hand, lang.text("Left hand", "左旋"));
    ui.horizontal(|ui| {
        ui.label(lang.text("Diameter taper (mm/mm)", "直径锥度（mm/mm）"));
        ui.add(egui::DragValue::new(&mut p.taper).speed(0.001));
    });
    if app.thread_ui.print_open || p.radial_fit > 0. {
        egui::CollapsingHeader::new(lang.text("Print fit details", "打印配合明细")).show(ui, |ui| {
        ui.small(format!(
            "{}: {} · {}: {}",
            lang.text("Screw major Ø", "螺钉牙顶直径"),
            unit.format_mm(p.diameter - 2. * p.radial_fit),
            lang.text("Nut major Ø", "螺帽牙底直径"),
            unit.format_mm(p.diameter + 2. * p.radial_fit)
        ));
        ui.small(format!("{} {}", lang.text("Same nominal Ø, pitch, hand and allowance on both parts. Combined radial allowance:", "两件须使用相同公称直径、螺距、旋向和余量。合计径向余量："), unit.format_mm(p.radial_fit * 2.)));
        for warning in app.thread_ui.printing.warnings(p, chinese) {
            ui.colored_label(Color32::RED, format!("warning: {warning}"));
        }
        });
    }
    });
    let footer_top = ui.cursor().top();
    let p = &mut app.thread_params;
    app.thread_ui.mark_edited(p);
    ui.label(format!(
        "{}: Ø{}",
        lang.text("Required blank / bore", "所需毛坯 / 底孔"),
        unit.format_mm(super::required_blank_radius(target.internal, p) * 2.)
    ));
    let valid = super::validate(target, p);
    if let Err(e) = &valid {
        let warning = if (super::required_blank_radius(target.internal, p) - target.radius).abs()
            > 0.05_f64.max(p.pitch * 0.08)
        {
            format!(
                "{} Ø{}; {} Ø{}",
                lang.text("Selected blank / bore", "当前毛坯 / 底孔"),
                unit.format_mm(target.radius * 2.),
                lang.text("required", "所需"),
                unit.format_mm(super::required_blank_radius(target.internal, p) * 2.)
            )
        } else if p.length <= p.pitch || p.offset < 0. || p.offset + p.length > target.length + 1e-5
        {
            lang.text(
                "Axial range must fit the wall and length must exceed one pitch",
                "螺纹范围必须在柱面内，且长度必须大于一个螺距",
            )
            .into()
        } else {
            format!(
                "{}: {}",
                lang.text("Invalid thread parameters", "螺纹参数存在问题"),
                localized_error(e, chinese)
            )
        };
        ui.colored_label(Color32::RED, format!("warning: {warning}"));
        if (super::required_blank_radius(target.internal, p) - target.radius).abs()
            > 0.05_f64.max(p.pitch * 0.08)
        {
            let correction = super::corrected_blank_target(target, p);
            let editable = app.cylinder_diameter_editable(target);
            let enabled = correction.is_ok() && editable;
            if ui
                    .add_enabled(enabled, egui::Button::new(lang.text(
                        "Correct according to nominal diameter", "按公称直径修正")))
                    .on_disabled_hover_text(if !editable {
                        lang.text("Requires a parametric cylinder, sketch extrusion or exact solid; raw imported mesh walls are read-only.",
                            "需要参数圆柱、草图拉伸实体或精确实体；独立导入的原始网格柱面只读。")
                    } else {
                        correction.as_ref().err()
                            .map(|error| localized_error(error, chinese)).unwrap_or_default()
                    })
                    .on_hover_text(lang.text(
                        "Resize the bore / blank to the specified nominal diameter; retain all thread and print parameters. One Undo step.",
                        "按指定公称直径调整底孔 / 毛坯；保留全部螺纹及打印参数。可一步撤销。",
                    ))
                    .clicked()
                {
                    if let Err(error) = app.correct_thread_blank(target.clone()) {
                        app.thread_ui.error = Some(error);
                    }
                }
        }
    }
    if let Some(error) = app
        .thread_ui
        .error
        .as_ref()
        .or(app.thread_ui.load_error.as_ref())
    {
        ui.colored_label(
            Color32::RED,
            format!("warning: {}", localized_error(error, chinese)),
        );
    }
    ui.small(lang.text(
        "Sampled mesh result; one Undo step.",
        "生成采样网格实体；一步撤销。",
    ));
    if ui
        .add_enabled(
            valid.is_ok(),
            egui::Button::new(lang.text("Generate thread", "生成螺纹")),
        )
        .clicked()
    {
        match app.apply_threads(vec![target.clone()], app.thread_params.clone()) {
            Ok(_) if app.model_job.is_some() => {
                app.thread_ui.error = None;
            }
            Ok(_) => {
                app.thread_choices.clear();
                app.thread_checked.clear();
                app.thread_ui.expanded = false;
                app.toast(lang.text("Thread generated", "螺纹已生成"));
            }
            Err(e) => app.thread_ui.error = Some(e),
        }
    }
    ui.separator();
    if ui
        .button("save as preset")
        .on_hover_text(lang.text(
            "Open a separate window to name and save these thread settings.",
            "打开独立窗口，为当前螺纹设置命名并保存。",
        ))
        .clicked()
    {
        app.thread_ui.save_open = !app.thread_ui.save_open;
    }
    let measured_footer_height = ui.cursor().top() - footer_top;
    ui.ctx()
        .data_mut(|data| data.insert_temp(footer_height_id, measured_footer_height));
    if footer_height.is_none_or(|height| (height - measured_footer_height).abs() > 0.5) {
        // Settle before displaying a frame when validation, font size, sidebar
        // width or the preset form changes the required footer space.
        ui.ctx().request_discard("thread footer height changed");
    }
    save_preset_window(app, ui.ctx(), target);
}

fn save_preset_window(app: &mut S33DApp, ctx: &egui::Context, target: &Target) {
    if !app.thread_ui.save_open {
        return;
    }
    let lang = app.settings.language;
    let mut open = true;
    let mut saved = false;
    egui::Window::new(lang.text("Save thread preset", "保存螺纹预设"))
        .id(egui::Id::new("thread_save_preset_window"))
        .default_width(320.0)
        .collapsible(false)
        .open(&mut open)
        .max_height((ctx.screen_rect().height() - 64.0).max(0.0))
        .show(ctx, |ui| {
            ui.label(lang.text("Preset name", "预设名称"));
            ui.text_edit_singleline(&mut app.thread_ui.save_name);
            if ui
                .add_enabled(
                    super::validate(target, &app.thread_params).is_ok(),
                    egui::Button::new(lang.text("Save", "保存")),
                )
                .clicked()
            {
                match app
                    .thread_ui
                    .remember(&preset_path(), target.internal, &app.thread_params)
                {
                    Ok(()) => {
                        app.thread_ui.error = None;
                        app.toast(
                            lang.text("Preset saved for next time", "预设已保存，下次可调用"),
                        );
                        saved = true;
                    }
                    Err(e) => app.thread_ui.error = Some(e),
                }
            }
        });
    app.thread_ui.save_open = open && !saved;
}

fn print_settings_window(app: &mut S33DApp, ctx: &egui::Context) {
    let lang = app.settings.language;
    let mut open = app.thread_ui.print_open;
    let mut done = false;
    egui::Window::new(
        egui::RichText::new(lang.text("Print settings", "打印设置"))
            .color(ctx.style().visuals.text_color()),
    )
    .id(egui::Id::new("thread_print_settings_window"))
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .show(ctx, |ui| {
        let settings = &mut app.thread_ui.printing;
        egui::ComboBox::from_id_salt("thread_print_process")
            .selected_text(match settings.process {
                PrintProcess::Fdm => "FDM",
                PrintProcess::Resin => lang.text("Resin", "树脂"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut settings.process, PrintProcess::Fdm, "FDM");
                ui.selectable_value(
                    &mut settings.process,
                    PrintProcess::Resin,
                    lang.text("Resin", "树脂"),
                );
            });
        if settings.process == PrintProcess::Fdm {
            ui.horizontal(|ui| {
                ui.label(lang.text("Nozzle", "喷嘴"));
                ui.add(
                    egui::DragValue::new(&mut settings.nozzle)
                        .speed(0.05)
                        .range(0.1..=2.)
                        .suffix(" mm"),
                );
            });
            ui.small(lang.text(
                "FDM suggestions assume the thread axis is printed upright.",
                "FDM 建议按螺纹轴竖直方向打印。",
            ));
        }
        ui.horizontal(|ui| {
            ui.label(lang.text("Layer height", "层高"));
            let (layer, range) = if settings.process == PrintProcess::Fdm {
                (&mut settings.fdm_layer, 0.02..=1.)
            } else {
                (&mut settings.resin_layer, 0.01..=0.3)
            };
            ui.add(
                egui::DragValue::new(layer)
                    .speed(0.01)
                    .range(range)
                    .suffix(" mm"),
            );
        });
        ui.label(lang.text(
            "Close settings, then click suggest for 3D printing to update the thread.",
            "关闭设置后，点击 suggest for 3D printing 更新螺纹参数。",
        ));
        ui.small(lang.text(
            "Calibrate a short mating pair for your printer and material.",
            "请用当前打印机和材料试打一对短螺纹校准。",
        ));
        done = ui.button(lang.text("Close", "关闭")).clicked();
    });
    app.thread_ui.print_open = open && !done;
}

fn localized_error(error: &str, chinese: bool) -> &str {
    if !chinese {
        return error;
    }
    match error {
        "All thread dimensions must be finite" => "所有尺寸必须是有限数值",
        "Pitch must be at least 0.2 mm" => "螺距不能小于 0.2 mm",
        "Nominal diameter and wall radius must be positive" => "公称直径和柱面半径必须大于零",
        "Tooth depth must be positive and smaller than half the selected wall radius" => {
            "牙深必须大于零，并小于所选柱面半径的一半；请减小螺距或牙深"
        }
        "Mating allowance must be between zero and one quarter of the pitch" => {
            "每件径向配合余量必须在零到四分之一螺距之间"
        }
        "Tooth depth and allowances leave no positive screw core radius" => {
            "牙深与余量过大，螺钉芯部半径已不足"
        }
        "Diameter taper must be between 0 and 0.1 mm/mm" => "直径锥度必须在 0 至 0.1 mm/mm 之间",
        "Included angle must be between 30 and 90 degrees" => "牙型角必须在 30° 至 90° 之间",
        "Groove clearance must be between zero and one quarter of the pitch" => {
            "牙槽附加切深必须在零到四分之一螺距之间"
        }
        "Root flat / pitch must be between 0.01 and 0.4" => {
            "牙底平段与螺距的比例必须在 0.01 至 0.4 之间"
        }
        "Whitworth requires a 55 degree angle and depth of 0.640327 times pitch" => {
            "惠氏牙必须为 55°，牙深必须为螺距的 0.640327 倍"
        }
        "Profile overlaps adjacent turns; reduce depth, clearance or root flat" => {
            "相邻牙型发生重叠，请减小牙深、附加切深或牙底平段"
        }
        "Thread request exceeds maximum 125 turns" => "螺纹超过 125 圈，请缩短长度或增大螺距",
        "Invalid diameter or print settings" => "公称直径或打印参数无效",
        "Invalid print settings" => "打印参数无效",
        "Existing preset file could not be read; it was preserved" => {
            "已有预设文件无法读取，原文件已保留"
        }
        "Choose a unique preset name (1–80 characters; not Custom or a built-in name)" => {
            "名称须为 1 至 80 字，且不能使用 Custom 或内置预设名"
        }
        "A preset with this name already exists" => "已存在同名预设，请使用其他名称",
        _ => error,
    }
}
