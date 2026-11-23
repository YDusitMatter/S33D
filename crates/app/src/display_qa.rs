//! Whole-window layout and real egui pointer/scroll regression tests.
use super::*;
use std::collections::HashMap;

struct Screen {
    ctx: egui::Context,
    app: S33DApp,
    nodes: HashMap<egui::accesskit::NodeId, egui::accesskit::Node>,
    root: Option<egui::accesskit::NodeId>,
    size: egui::Vec2,
    time: f64,
    paint: Vec<(String, egui::Rect, egui::Rect)>,
}
impl Screen {
    fn new(language: UiLanguage, font: UiFontSize, appearance: UiAppearance) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.settings = AppSettings::default();
        app.settings.language = language;
        app.settings.font_size = font;
        app.settings.appearance = appearance;
        app.first_use_help_open = false;
        app.objects_panel_open = false;
        Self {
            ctx,
            app,
            nodes: HashMap::new(),
            root: None,
            size: egui::vec2(900., 600.),
            time: 0.,
            paint: vec![],
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 1. / 60.;
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| draw(&mut self.app, ctx),
        );
        if let Some(update) = &output.platform_output.accesskit_update {
            if let Some(tree) = &update.tree {
                self.root = Some(tree.root);
            }
            for (id, node) in &update.nodes {
                self.nodes.insert(*id, node.clone());
            }
        }
        fn collect(
            shape: &egui::Shape,
            clip: egui::Rect,
            out: &mut Vec<(String, egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::Shape::Text(t) => out.push((
                    t.galley.job.text.clone(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                    clip,
                )),
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect(s, clip, out)
                    }
                }
                _ => {}
            }
        }
        self.paint.clear();
        for s in &output.shapes {
            collect(&s.shape, s.clip_rect, &mut self.paint);
        }
        output
    }
    fn paint_visible(&self, label: &str) -> bool {
        self.paint.iter().any(|(text, rect, clip)| {
            text == label
                && clip.contains_rect(*rect)
                && clip.contains(
                    self.widgets(label)
                        .last()
                        .map_or(rect.center(), |r| r.center()),
                )
        })
    }
    fn settle(&mut self) {
        for _ in 0..24 {
            self.frame(vec![]);
        }
    }
    fn widgets(&self, label: &str) -> Vec<egui::Rect> {
        fn walk(s: &Screen, id: egui::accesskit::NodeId, label: &str, out: &mut Vec<egui::Rect>) {
            let Some(n) = s.nodes.get(&id) else { return };
            if n.label() == Some(label) {
                if let Some(r) = n.bounds() {
                    out.push(egui::Rect::from_min_max(
                        egui::pos2(r.x0 as f32, r.y0 as f32),
                        egui::pos2(r.x1 as f32, r.y1 as f32),
                    ));
                }
            }
            for child in n.children() {
                walk(s, *child, label, out);
            }
        }
        let mut out = vec![];
        if let Some(root) = self.root {
            walk(self, root, label, &mut out);
        }
        out
    }
    fn widget(&self, label: &str) -> egui::Rect {
        let widgets = self.widgets(label);
        *widgets.last().unwrap_or_else(|| {
            panic!(
                "missing {label}; labels={:?}",
                self.nodes
                    .values()
                    .filter_map(|n| n.label())
                    .collect::<Vec<_>>()
            )
        })
    }
    fn click(&mut self, r: egui::Rect) {
        let p = r.center();
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        self.settle();
    }
    fn visible(&self, label: &str) -> egui::Rect {
        let rect = self.widget(label);
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, self.size).contains_rect(rect),
            "{label}: {rect:?}, size {:?}",
            self.size
        );
        rect
    }
}

#[test]
fn display_camera_sidebar_does_not_grow_and_all_four_hit_rects_fit() {
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for font in [UiFontSize::Standard, UiFontSize::Large] {
                let mut s = Screen::new(language, font, appearance);
                for width in [286., 316., 420.] {
                    s.ctx.data_mut(|d| {
                        d.insert_persisted(
                            egui::Id::new("properties"),
                            egui::containers::panel::PanelState {
                                rect: egui::Rect::from_min_size(
                                    egui::pos2(s.size.x - width, 0.),
                                    egui::vec2(width, s.size.y),
                                ),
                            },
                        )
                    });
                    s.settle();
                    let before = s.app.viewport_rect.right();
                    for _ in 0..32 {
                        s.frame(vec![]);
                        assert!((s.app.viewport_rect.right() - before).abs() < 0.6);
                    }
                    for label in [
                        language.text("Iso", "等轴测"),
                        language.text("Top", "顶视"),
                        language.text("Front", "前视"),
                        language.text("Right", "右视"),
                    ] {
                        let r = s.visible(label);
                        assert!(r.left() >= before && r.right() <= s.size.x);
                    }
                }
                let before = s.app.camera.clone();
                let top = s.visible(language.text("Top", "顶视"));
                s.click(top);
                assert_ne!(s.app.camera.eye, before.eye);
            }
        }
    }
}

#[test]
fn display_getting_started_footer_stays_visible_and_got_it_click_closes() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        for font in [UiFontSize::Standard, UiFontSize::Large] {
            let mut s = Screen::new(language, font, UiAppearance::Original);
            s.app.first_use_help_open = true;
            for size in [
                (900., 600.),
                (1024., 640.),
                (1093., 614.),
                (1280., 720.),
                (1366., 768.),
                (1440., 900.),
                (1920., 1080.),
                (2560., 1440.),
            ]
            .map(|(w, h)| egui::vec2(w, h))
            {
                s.size = size;
                s.settle();
                s.visible(language.text("Don't show this again", "不再显示"));
                s.visible(language.text("Got it", "知道了"));
            }
            s.size = egui::vec2(900., 600.);
            s.settle();
            s.click(s.visible(language.text("Don't show this again", "不再显示")));
            assert!(!s.app.settings.show_first_use_tips);
            s.click(s.visible(language.text("Got it", "知道了")));
            assert!(!s.app.first_use_help_open);
        }
    }
}

#[test]
fn display_short_gear_window_expands_position_scrolls_and_generate_is_clickable() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        for font in [UiFontSize::Standard, UiFontSize::Large] {
            let mut s = Screen::new(language, font, UiAppearance::Original);
            assert!(s.app.plugins.open("gear-generator"));
            s.settle();
            let label = language.text("Position", "位置");
            for _ in 0..8 {
                let generate = s.visible(language.text("Generate gear", "生成齿轮"));
                let position = s.widget(label);
                if s.paint_visible(label)
                    && position.bottom() < generate.top() - 8.
                    && position.top() > 60.
                {
                    break;
                }
                let point = egui::pos2(generate.left() + 40., generate.top() - 80.);
                s.frame(vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -160.),
                        modifiers: Default::default(),
                    },
                ]);
                s.settle();
            }
            let p = s.widget(label);
            s.click(p);
            assert!(
                s.paint_visible(label),
                "Position header is clipped before scrolling"
            );
            let generate = s.visible(language.text("Generate gear", "生成齿轮"));
            for _ in 0..4 {
                s.frame(vec![
                    egui::Event::PointerMoved(egui::pos2(
                        generate.left() + 60.,
                        generate.top() - 120.,
                    )),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -180.),
                        modifiers: Default::default(),
                    },
                ]);
                s.settle();
            }

            assert!(
                s.paint_visible("Z"),
                "Position failed to expand or scroll into view"
            );
            s.visible(language.text("Generate gear", "生成齿轮"));
            s.click(s.visible(language.text("Generate gear", "生成齿轮")));
            assert!(
                s.app
                    .doc
                    .bodies
                    .iter()
                    .any(|b| matches!(b.feature, Feature::Gear { .. })),
                "generate pointer click did not reach its button"
            );
        }
    }
}

#[test]
fn display_full_window_resolution_matrix_and_resize_restore() {
    let sizes = [
        (900., 600.),
        (1024., 640.),
        (1093., 614.),
        (1280., 720.),
        (1366., 768.),
        (1440., 900.),
        (1920., 1080.),
        (2560., 1440.),
        (900., 600.),
        (1920., 1080.),
    ];
    let mut observations = 0;
    for appearance in UiAppearance::ALL {
        for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for font in [UiFontSize::Standard, UiFontSize::Large] {
                for (plugin, action) in [
                    ("", ""),
                    ("gear-generator", language.text("Generate gear", "生成齿轮")),
                    (
                        "surface-texture",
                        language.text("Apply texture", "应用纹理"),
                    ),
                    ("tire-generator", language.text("Generate tire", "生成轮胎")),
                    (
                        "wheel-rim-generator",
                        language.text("Generate wheel rim", "生成轮毂"),
                    ),
                    ("screw-generator", ""),
                    ("nut-generator", ""),
                ] {
                    let mut s = Screen::new(language, font, appearance);
                    for id in s
                        .app
                        .plugins
                        .descriptors()
                        .iter()
                        .map(|d| d.id.to_owned())
                        .collect::<Vec<_>>()
                    {
                        s.app.plugins.set_enabled(&id, true);
                    }
                    if !plugin.is_empty() {
                        assert!(s.app.plugins.open(plugin));
                    }
                    for (w, h) in sizes {
                        s.size = egui::vec2(w, h);
                        s.settle();
                        assert!(
                            s.app.viewport_rect.is_positive(),
                            "{plugin}/{appearance:?}/{language:?}/{font:?}/{w}/{h}"
                        );
                        if !action.is_empty() {
                            s.visible(action);
                        }
                        s.visible(language.text("Top", "顶视"));
                        let before = s.app.viewport_rect;
                        for _ in 0..4 {
                            s.frame(vec![]);
                            assert_eq!(before, s.app.viewport_rect);
                        }
                        observations += 1;
                    }
                }
            }
        }
    }
    println!("{observations} full-window layout observations, all plugins enabled; 4 appearances / 2 languages / 2 font sizes; full action rectangles checked including disabled texture Apply");
}

fn display_thread_fixture(s: &mut Screen) {
    use s33d_kernel::{Axis2D, DatumPlane, GeometryKernel, Region2D};
    let shape = s
        .app
        .kernel
        .revolve_region(
            &Region2D::new(vec![[0., 0.], [4., 0.], [4., 12.], [0., 12.]]),
            &DatumPlane::Front.def(),
            &Axis2D {
                point: [0., 0.],
                dir: [0., 1.],
            },
            360.,
        )
        .unwrap();
    let id = s.app.doc.alloc_id();
    s.app.doc.add_body(s33d_model::Body {
        id,
        name: "Long named cylinder / 长名称圆柱 / fixture with dimensions and details".into(),
        kind: s33d_model::BodyKind::Solid,
        feature: Feature::Derived {
            source: "display fixture".into(),
            operation: None,
        },
        shape: Some(shape),
        material: Default::default(),
        visible: true,
    });
    s.app.camera.eye = glam::Vec3::new(20., 5., 0.);
    s.app.camera.target = glam::Vec3::new(0., 5., 0.);
    s.app.camera.up = glam::Vec3::Y;
    let r = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    let face = s.app.pick_face(r.center(), r).unwrap().0;
    assert!(s.app.begin_thread_on_face(&face));
    s.settle();
    s.app.thread_params.diameter = 6.;
}

#[test]
fn display_thread_warning_save_and_print_windows_fit_short_screens_and_restore() {
    let mut observations = 0;
    for appearance in UiAppearance::ALL {
        for lang in [UiLanguage::English, UiLanguage::ChineseSimplified] {
            for font in [UiFontSize::Standard, UiFontSize::Large] {
                let mut s = Screen::new(lang, font, appearance);
                display_thread_fixture(&mut s);
                s.app.thread_ui.load_error=Some("Cannot read thread presets: access denied while opening the saved settings file; please choose a writable preset location.".into());
                s.settle();
                s.click(s.visible("save as preset"));
                assert!(
                    s.app.thread_ui.save_open,
                    "actual Save as preset click must open its dialog"
                );
                for (w, h) in [
                    (900., 600.),
                    (1024., 640.),
                    (1093., 614.),
                    (1280., 720.),
                    (1366., 768.),
                    (1440., 900.),
                    (1920., 1080.),
                    (2560., 1440.),
                    (900., 600.),
                    (1920., 1080.),
                ] {
                    s.size = egui::vec2(w, h);
                    for scenario in 0..3 {
                        s.app.thread_ui.save_open = scenario == 1;
                        s.app.thread_ui.print_open = scenario == 2;
                        s.settle();
                        for label in [
                            lang.text("Generate thread", "生成螺纹"),
                            lang.text("Correct according to nominal diameter", "按公称直径修正"),
                            "save as preset",
                        ] {
                            s.visible(label);
                            assert!(
                                s.paint_visible(label),
                                "clipped thread footer: {label}/{lang:?}/{font:?}/{w}/{h}"
                            );
                        }
                        if scenario == 1 {
                            s.visible(lang.text("Save thread preset", "保存螺纹预设"));
                            assert!(s.paint_visible(lang.text("Preset name", "预设名称")));
                            s.visible(lang.text("Save", "保存"));
                        }
                        if scenario == 2 {
                            s.visible(lang.text("Print settings", "打印设置"));
                        }
                        observations += 1;
                    }
                }
            }
        }
    }
    println!("{observations} thread warning/save/print full-window observations with long object names and load errors");
}

#[test]
fn display_generator_footer_survives_actual_window_resize() {
    for language in [UiLanguage::English, UiLanguage::ChineseSimplified] {
        for font in [UiFontSize::Standard, UiFontSize::Large] {
            for (plugin, id, action) in [
                (
                    "gear-generator",
                    "plugin_gear_generator",
                    language.text("Generate gear", "生成齿轮"),
                ),
                (
                    "surface-texture",
                    "plugin_surface_texture",
                    language.text("Apply texture", "应用纹理"),
                ),
                (
                    "tire-generator",
                    "plugin_tire_generator",
                    language.text("Generate tire", "生成轮胎"),
                ),
                (
                    "wheel-rim-generator",
                    "plugin_wheel_rim_generator",
                    language.text("Generate wheel rim", "生成轮毂"),
                ),
            ] {
                let mut s = Screen::new(language, font, UiAppearance::Original);
                s.size = egui::vec2(1280., 900.);
                assert!(s.app.plugins.open(plugin));
                s.settle();
                let window = egui::Id::new(id);
                let before = egui::containers::AreaState::load(&s.ctx, window)
                    .unwrap()
                    .rect();
                let start = before.max - egui::vec2(3., 3.);
                let end = start - egui::vec2(0., 180.);
                s.frame(vec![egui::Event::PointerMoved(start)]);
                s.frame(vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }]);
                s.frame(vec![egui::Event::PointerMoved(end)]);
                s.frame(vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }]);
                s.settle();
                let after = egui::containers::AreaState::load(&s.ctx, window)
                    .unwrap()
                    .rect();
                assert!(
                    after.height() < before.height() - 100.,
                    "resize did not occur: {plugin}: {before:?} -> {after:?}"
                );
                let action_rect = s.visible(action);
                // Texture Apply is disabled without a selected surface; egui
                // can omit its text shape. Its full response rectangle still
                // has to fit the resized window, just like enabled actions.
                assert!(
                    after.contains_rect(action_rect)
                        && (plugin == "surface-texture" || s.paint_visible(action)),
                    "clipped resized footer {plugin}: {action_rect:?}, {after:?}"
                );
            }
        }
    }
}
