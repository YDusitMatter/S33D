//! File activation shared with the File menu's existing open/import paths.

use crate::{app::S33DApp, ui};
use std::{ffi::OsString, path::PathBuf};

const MIN_SPLASH_SECONDS: f64 = 2.0;

/// The first frame never waits for font I/O or a command-line model import.
/// The supplied engineering screenshot is embedded, with no temporary path.
pub(crate) struct LaunchScreen {
    prepared: std::sync::mpsc::Receiver<Result<Prepared, String>>,
    ready: Option<Prepared>,
    texture: Option<egui::TextureHandle>,
    first_painted_at: Option<f64>,
    app: Option<S33DApp>,
    arguments: Option<Vec<OsString>>,
    opening: bool,
    placement: crate::display::Placement,
    error: Option<String>,
}

struct Prepared {
    preferences: crate::app::UiStateFile,
    fonts: egui::FontDefinitions,
    image: egui::ColorImage,
}

impl LaunchScreen {
    pub fn new(cc: &eframe::CreationContext<'_>, arguments: Vec<OsString>) -> Self {
        let mut placement = crate::display::Placement::default();
        placement.update(&cc.egui_ctx, cc, false);
        let (sender, prepared) = std::sync::mpsc::channel();
        let context = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                let preferences = crate::app::load_ui_state();
                let fonts = crate::app::load_ui_fonts();
                let image = image::load_from_memory(include_bytes!(
                    "../../../assets/startup/engineering-preview.png"
                ))
                .map_err(|e| e.to_string())?
                .to_rgba8();
                Ok(Prepared {
                    preferences,
                    fonts,
                    image: egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                })
            })
            .unwrap_or_else(|_| Err("Startup preparation failed".into()));
            let _ = sender.send(result);
            context.request_repaint();
        });
        Self {
            prepared,
            ready: None,
            texture: None,
            first_painted_at: None,
            app: None,
            arguments: Some(arguments),
            opening: true,
            placement,
            error: None,
        }
    }

    fn show(&self, ctx: &egui::Context, opening: bool) {
        let language = self
            .app
            .as_ref()
            .map(|a| a.settings.language)
            .or_else(|| self.ready.as_ref().map(|r| r.preferences.settings.language))
            .unwrap_or_default();
        let bg = egui::Color32::from_rgb(31, 36, 43);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg))
            .show(ctx, |ui| {
                let available = ui.available_size();
                let scale = (available.x / 768.).min(available.y / 1024.);
                let size = egui::vec2(768., 1024.) * scale;
                let card = egui::Rect::from_center_size(ui.max_rect().center(), size);
                let image_area = egui::Rect::from_min_max(
                    card.min,
                    egui::pos2(card.right(), card.bottom() - 205. * scale),
                );
                if let Some(texture) = &self.texture {
                    // Preserve the entire reference image; never stretch or crop it.
                    let fit = texture.size_vec2()
                        * (image_area.width() / texture.size_vec2().x)
                            .min(image_area.height() / texture.size_vec2().y);
                    let rect = egui::Rect::from_center_size(image_area.center(), fit);
                    ui.painter().image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        egui::Color32::WHITE,
                    );
                }
                let right = card.right() - 34. * scale;
                let baseline = card.bottom() - 112. * scale;
                ui.painter().text(
                    egui::pos2(right, baseline),
                    egui::Align2::RIGHT_BOTTOM,
                    "S33D",
                    egui::FontId::proportional(64. * scale),
                    egui::Color32::WHITE,
                );
                ui.painter().text(
                    egui::pos2(right, baseline + 35. * scale),
                    egui::Align2::RIGHT_BOTTOM,
                    language.text("Sketch ideas. Shape objects.", "从草图构想到三维成形"),
                    egui::FontId::proportional(18. * scale),
                    egui::Color32::from_rgb(178, 199, 212),
                );
                let row = egui::Rect::from_min_max(
                    egui::pos2(card.left() + 34. * scale, card.bottom() - 54. * scale),
                    egui::pos2(right, card.bottom() - 18. * scale),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                    ui.visuals_mut().override_text_color = Some(egui::Color32::LIGHT_GRAY);
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(14. * scale));
                    ui.spacing_mut().interact_size.y = 18. * scale;
                    ui.spacing_mut().item_spacing *= scale;
                    ui.horizontal(|ui| {
                        if self.error.is_none() {
                            ui.add(egui::Spinner::new().size(16. * scale));
                        }
                        ui.label(self.error.as_deref().unwrap_or(if opening {
                            language.text("Loading model…", "正在加载模型…")
                        } else if self.app.is_some() {
                            language.text("Ready", "加载完成")
                        } else if self.ready.is_some() {
                            language.text("Preparing workspace…", "正在准备工作区…")
                        } else {
                            language.text("Loading fonts and resources…", "正在读取字体和资源…")
                        }));
                    });
                });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(32));
    }
}

impl eframe::App for LaunchScreen {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.placement.update(ctx, frame, !self.opening);
        if let Some(app) = self.app.as_mut() {
            if self.opening {
                app.poll_model_job();
                app.poll_step_import();
                let loading = app.progress_busy();
                let minimum_visible = self.first_painted_at.is_some_and(|started| {
                    ctx.input(|input| input.time) - started >= MIN_SPLASH_SECONDS
                });
                if loading || !minimum_visible {
                    self.show(ctx, loading);
                    let app = self.app.as_mut().unwrap();
                    app.model_progress_modal(ctx);
                    app.step_import_modal(ctx);
                    return;
                }
                self.opening = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                self.placement.update(ctx, frame, true);
            }
            eframe::App::update(self.app.as_mut().unwrap(), ctx, frame);
            return;
        }
        if self.ready.is_none() && self.error.is_none() {
            match self.prepared.try_recv() {
                Ok(Ok(mut ready)) => {
                    ctx.set_fonts(std::mem::take(&mut ready.fonts));
                    self.texture = Some(ctx.load_texture(
                        "startup-engineering",
                        std::mem::replace(
                            &mut ready.image,
                            egui::ColorImage::new([1, 1], egui::Color32::TRANSPARENT),
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                    self.ready = Some(ready);
                }
                Ok(Err(error)) => self.error = Some(error),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Startup preparation disconnected".into())
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.first_painted_at.is_some() {
            if let Some(ready) = self.ready.take() {
                let mut app = S33DApp::from_ui_state(ready.preferences, true);
                app.background_modeling = true;
                open_arguments(&mut app, self.arguments.take().unwrap_or_default());
                self.app = Some(app);
                self.show(ctx, self.app.as_ref().unwrap().progress_busy());
                ctx.request_repaint();
                return;
            }
        }
        self.show(ctx, false);
        if self.texture.is_some() {
            // Start when the complete splash is first drawn. Background
            // preparation and model loading continue during the minimum hold.
            self.first_painted_at
                .get_or_insert_with(|| ctx.input(|input| input.time));
        }
    }

    fn on_exit(&mut self) {
        if let Some(app) = self.app.as_mut() {
            eframe::App::on_exit(app);
        }
    }
}

pub(crate) fn native_cad_conversion_hint(language: crate::app::UiLanguage) -> &'static str {
    language.text(
        "Native SolidWorks / Parasolid is not supported by this build. In SolidWorks, export parts or the complete assembly as STEP (.step/.stp, preferred) or IGES (.igs/.iges), then import that file. Drawings must be exported as a 3D part/assembly first.",
        "当前版本不能直接读取原生 SolidWorks / Parasolid。请在 SolidWorks 中将零件或完整装配导出为 STEP（.step/.stp，推荐）或 IGES（.igs/.iges）再导入；工程图需要先取得对应的三维零件或装配。",
    )
}

pub(crate) fn open_arguments(app: &mut S33DApp, arguments: Vec<OsString>) {
    let mut arguments = arguments.into_iter().peekable();
    if arguments.peek().is_some_and(|argument| argument == "--") {
        arguments.next();
    }
    let Some(path) = arguments.next().map(PathBuf::from) else {
        return;
    };
    app.first_use_help_open = false;
    if arguments.next().is_some() {
        app.toast(app.settings.language.text(
            "Open one file at a time. Quote paths that contain spaces.",
            "请每次打开一个文件；命令行中的含空格路径需要加引号。",
        ));
        return;
    }
    if !path.is_file() {
        app.toast(format!(
            "{}: {}",
            app.settings
                .language
                .text("Cannot open file", "无法打开文件"),
            path.display(),
        ));
        return;
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    match extension.to_ascii_lowercase().as_str() {
        "s33d" => ui::open_project_path(app, &path),
        "stp" | "step" | "igs" | "iges" | "stl" | "obj" | "ply" | "glb" | "gltf" | "3mf"
        | "sldprt" | "sldasm" | "slddrw" | "x_t" | "x_b" => {
            // STEP retains the existing cancellable background import workflow.
            app.remember_directory(&path, false);
            ui::import_model_path(app, &path);
        }
        _ => app.toast(format!(
            "{}: {}",
            app.settings
                .language
                .text("Unsupported file format", "不支持的文件格式"),
            path.display(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "s33d-open-with-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn app() -> S33DApp {
        S33DApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ))
    }

    #[test]
    fn splash_stays_two_seconds_and_longer_until_startup_loading_finishes() {
        fn draw(screen: &mut LaunchScreen, ctx: &egui::Context, time: f64) -> egui::FullOutput {
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(768., 1024.),
                    )),
                    ..Default::default()
                },
                |ctx| eframe::App::update(screen, ctx, &mut eframe::Frame::_new_kittest()),
            )
        }
        fn has_logo(output: &egui::FullOutput) -> bool {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "S33D")
            })
        }
        fn restores_window(output: &egui::FullOutput) -> bool {
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Decorations(true)))
                && commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Resizable(true)))
        }

        for slow_file in [false, true] {
            let ctx = egui::Context::default();
            let (sender, prepared) = std::sync::mpsc::channel();
            let mut preferences = crate::app::UiStateFile::default();
            preferences.settings.show_first_use_tips = false;
            sender
                .send(Ok(Prepared {
                    preferences,
                    fonts: egui::FontDefinitions::default(),
                    image: egui::ColorImage::new([1, 1], egui::Color32::WHITE),
                }))
                .unwrap();
            let mut screen = LaunchScreen {
                prepared,
                ready: None,
                texture: None,
                first_painted_at: None,
                app: None,
                arguments: Some(vec![]),
                opening: true,
                placement: Default::default(),
                error: None,
            };
            // Resource preparation before the first picture does not count
            // toward the two seconds of visible branding.
            assert!(has_logo(&draw(&mut screen, &ctx, 20.)));
            assert_eq!(screen.first_painted_at, Some(20.));
            draw(&mut screen, &ctx, 20.1);
            assert!(screen.app.is_some());

            let (release, wait) = std::sync::mpsc::channel();
            if slow_file {
                screen
                    .app
                    .as_mut()
                    .unwrap()
                    .start_model_job(
                        ("Loading project", "正在加载工程"),
                        false,
                        None,
                        move |worker| {
                            wait.recv().unwrap();
                            worker.doc = s33d_model::Document::new("Loaded startup project");
                            Ok(serde_json::Value::Null)
                        },
                    )
                    .unwrap();
            }
            let held = draw(&mut screen, &ctx, 21.999);
            assert!(has_logo(&held));
            assert!(!restores_window(&held));
            assert!(screen.opening);
            let output = draw(&mut screen, &ctx, 22.);
            assert_eq!(screen.opening, slow_file);
            assert_eq!(restores_window(&output), !slow_file);
            if slow_file {
                assert!(has_logo(&output));
                assert!(has_logo(&draw(&mut screen, &ctx, 50.)));
                assert!(screen.opening);
                release.send(()).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while screen.opening {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "startup worker timed out"
                    );
                    let output = draw(&mut screen, &ctx, 50.);
                    assert_eq!(restores_window(&output), !screen.opening);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                assert_eq!(
                    screen.app.as_ref().unwrap().doc.name(),
                    "Loaded startup project"
                );
                // No second delay is introduced after a long load completes.
                assert_eq!(ctx.input(|input| input.time), 50.);
            }
        }
    }

    #[test]
    fn open_with_loads_project_with_spaces_unicode_and_uppercase_extension() {
        let fixture = Fixture::new();
        let path = fixture.0.join("测试 工程.S33D");
        let mut source = app();
        source.doc = s33d_model::Document::new("Open With project");
        std::fs::write(&path, source.project_to_s33d().unwrap()).unwrap();
        let mut opened = app();
        open_arguments(&mut opened, vec!["--".into(), path.into_os_string()]);
        assert_eq!(opened.doc.name(), "Open With project");
        assert!(!opened.first_use_help_open);
        assert!(opened
            .project_session
            .path
            .as_ref()
            .unwrap()
            .ends_with("测试 工程.S33D"));
        assert!(!opened.project_has_changes());
    }

    #[test]
    fn open_with_imports_model_and_selects_it() {
        let fixture = Fixture::new();
        let path = fixture.0.join("模型 with spaces.OBJ");
        std::fs::write(&path, "v 0 0 0\nv 10 0 0\nv 0 10 0\nf 1 2 3\n").unwrap();
        let mut opened = app();
        open_arguments(&mut opened, vec![path.into_os_string()]);
        assert_eq!(opened.doc.bodies.len(), 1);
        assert_eq!(opened.selected, vec![opened.doc.bodies[0].id]);
    }

    #[test]
    fn invalid_startup_file_does_not_replace_document_or_panic() {
        let fixture = Fixture::new();
        let mut opened = app();
        opened.doc = s33d_model::Document::new("Keep this project");
        open_arguments(
            &mut opened,
            vec![fixture.0.join("missing.s33d").into_os_string()],
        );
        let path = fixture.0.join("broken.s33d");
        std::fs::write(&path, "not a project").unwrap();
        open_arguments(&mut opened, vec![path.clone().into_os_string()]);
        open_arguments(
            &mut opened,
            vec![path.into_os_string(), "second.s33d".into()],
        );
        assert_eq!(opened.doc.name(), "Keep this project");
        assert_eq!(opened.toasts.len(), 3);
    }

    #[test]
    fn native_solidworks_files_get_conversion_guidance_without_changing_document() {
        let fixture = Fixture::new();
        let mut opened = app();
        opened.doc = s33d_model::Document::new("Keep this project");
        for ext in ["SLDPRT", "SLDASM", "SLDDRW", "X_T", "X_B"] {
            let path = fixture.0.join(format!("原文件.{ext}"));
            std::fs::write(&path, [0xD0, 0xCF, 0x11, 0xE0]).unwrap();
            open_arguments(&mut opened, vec![path.into_os_string()]);
            assert!(opened.toasts.last().unwrap().0.contains("STEP"));
            assert!(opened.step_import_job.is_none());
            assert_eq!(opened.doc.name(), "Keep this project");
        }
    }
}
