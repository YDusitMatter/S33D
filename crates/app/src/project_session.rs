//! Project lifetime and save checkpoints; selection/view changes are not edits.
use super::*;

#[derive(Clone, Debug)]
pub enum ProjectAction {
    New,
    Close,
    Open(PathBuf),
    Exit,
}

#[derive(Default)]
pub struct ProjectSession {
    pub path: Option<PathBuf>,
    saved_bodies: Vec<Body>,
    saved_references: Vec<String>,
    saved_groups: Vec<s33d_model::ObjectGroup>,
    saved_name: Option<String>,
    saved_drawing: Option<Value>,
    pub pending: Option<ProjectAction>,
    pub exit_approved: bool,
    pub save_error: Option<String>,
}

impl S33DApp {
    pub fn toggle_sketch_menu(&mut self) {
        self.sketch_tools_open = !self.sketch_tools_open;
        if self.sketch_tools_open {
            self.draw3d_tools_open = false;
            self.drawing.open = false;
            self.drawing.pending.clear();
        }
        if self.sketch_tools_open && !matches!(self.mode, Mode::Sketch | Mode::PickPlane) {
            self.begin_plane_pick();
        }
    }

    pub fn toggle_draw3d_menu(&mut self) {
        self.draw3d_tools_open = !self.draw3d_tools_open;
        if self.draw3d_tools_open {
            self.sketch_tools_open = false;
            self.drawing.open = false;
            self.drawing.pending.clear();
        }
        if self.draw3d_tools_open && self.mode != Mode::Insert3D {
            self.start_insert_3d();
        }
    }

    pub fn project_has_changes(&self) -> bool {
        if self.reference_ids() != self.project_session.saved_references {
            return true;
        }
        self.drawing.saved_content()
            != self
                .project_session
                .saved_drawing
                .clone()
                .unwrap_or_else(|| crate::drawing2d::Drawing::default().saved_content())
            || self.doc.bodies != self.project_session.saved_bodies
            || self.doc.groups != self.project_session.saved_groups
            || self
                .project_session
                .saved_name
                .as_deref()
                .unwrap_or("Untitled Project")
                != self.doc.name()
            || self.model_job.is_some()
            || self.step_import_job.is_some()
            || self.fastener_job.is_some()
            || match self.mode {
                Mode::Sketch => self.sketch.as_ref().is_some_and(|s| {
                    !s.entities.is_empty() || !s.pending.is_empty() || s.edit_preview.is_some()
                }),
                Mode::Insert3D => self
                    .insert_3d_ctx
                    .as_ref()
                    .is_some_and(|c| c.params.is_some()),
                Mode::Extrude
                | Mode::Revolve
                | Mode::RevolveAxis
                | Mode::EquationCurve
                | Mode::OffsetShell => true,
                _ => false,
            }
    }

    pub fn mark_project_saved(&mut self, path: Option<PathBuf>) {
        self.project_session.saved_drawing = Some(self.drawing.saved_content());
        // The live desktop records successful opens/saves. A background worker
        // must not overwrite user preferences with its cloned settings.
        if self.model_work.is_none() {
            if let Some(path) = &path {
                crate::recent_projects::remember(&mut self.settings.recent_projects, path);
                self.save_ui_preferences();
            }
        }
        self.project_session.path = path;
        self.project_session.saved_bodies = self.doc.bodies.clone();
        self.project_session.saved_references = self.reference_ids();
        self.project_session.saved_groups = self.doc.groups.clone();
        self.project_session.saved_name = Some(self.doc.name().to_owned());
        self.project_session.save_error = None;
        self.project_name = self
            .project_session
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.doc.name().to_owned());
    }

    pub fn save_project_to_path(&mut self, path: &Path) -> Result<(), String> {
        // Never report a successful save while omitting an interactive draft.
        if self.progress_busy() {
            return Err(self
                .settings
                .language
                .text(
                    "Wait for the operation to finish, or cancel it before saving.",
                    "请等待操作完成，或取消操作后再保存。",
                )
                .into());
        }
        if self.sketch.as_ref().is_some_and(|s| {
            s.edit_preview.is_some() || (s.entities.is_empty() && !s.pending.is_empty())
        }) {
            return Err(self
                .settings
                .language
                .text(
                    "Finish the current sketch stroke or edit, then save.",
                    "请先完成当前草图笔画或编辑，再保存。",
                )
                .into());
        }
        if matches!(self.mode, Mode::Sketch | Mode::Insert3D | Mode::PickPlane) {
            if !self.finish_creation_for_tool_switch() {
                return Err(self
                    .settings
                    .language
                    .text(
                        "Finish or cancel the current drawing before saving.",
                        "请完成或取消当前绘制后再保存。",
                    )
                    .into());
            }
        }
        if !matches!(self.mode, Mode::Idle | Mode::SectionView) {
            return Err(self
                .settings
                .language
                .text(
                    "Finish or cancel the current operation before saving.",
                    "请先完成或取消当前操作，再保存项目。",
                )
                .into());
        }
        let text = self.project_to_s33d()?;
        // Write beside the destination, then rename: a failed write must not
        // truncate the last good project.
        atomic_project_write(path, text.as_bytes()).map_err(|e| e.to_string())?;
        self.mark_project_saved(Some(path.to_path_buf()));
        Ok(())
    }

    pub fn open_project_from_path(&mut self, path: &Path) -> Result<(), String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let total = file.metadata().map_err(|e| e.to_string())?.len() as usize;
        let mut bytes = Vec::new();
        let mut chunk = vec![0u8; 1024 * 1024];
        loop {
            self.report_model_progress(
                ("Reading project…", "正在读取工程…"),
                Some((bytes.len(), total)),
            )?;
            let count = file.read(&mut chunk).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        self.report_model_progress(
            ("Restoring geometry and drawings…", "正在恢复几何和图纸…"),
            None,
        )?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        // Decoder constructs a replacement first; failure preserves the session.
        self.load_s33d_project(&text)?;
        self.mark_project_saved(Some(path.to_path_buf()));
        Ok(())
    }

    pub(crate) fn open_project_responsive(&mut self, path: &Path) -> Result<(), String> {
        if !self.background_modeling {
            return self.open_project_from_path(path);
        }
        let path = path.to_path_buf();
        self.start_model_job(
            ("Open project", "打开工程"),
            false,
            None,
            move |worker| {
                worker
                    .open_project_from_path(&path)
                    .map_err(|e| ("file".into(), e))?;
                Ok(serde_json::json!({
                    "_opened_project": path,
                    "_opened_drawing": serde_json::to_value(&worker.drawing)
                        .map_err(|e| ("file".to_owned(), e.to_string()))?
                }))
            },
        )
    }

    pub fn request_project_action(&mut self, action: ProjectAction, ctx: &egui::Context) {
        if self.progress_busy() && self.project_has_changes() {
            self.toast(self.settings.language.text(
                "Wait for the operation, or cancel it first",
                "请等待当前操作完成，或先取消操作",
            ));
            return;
        }
        if self.project_session.pending.is_some() {
            return;
        }
        if self.project_has_changes() {
            self.project_session.pending = Some(action);
        } else {
            self.perform_project_action(action, ctx);
        }
    }

    fn perform_project_action(&mut self, action: ProjectAction, ctx: &egui::Context) {
        match action {
            ProjectAction::New | ProjectAction::Close => self.new_project(),
            ProjectAction::Open(path) => {
                if let Err(e) = self.open_project_responsive(&path) {
                    self.toast(format!(
                        "{}: {e}",
                        self.settings.language.text("Open failed", "打开失败")
                    ));
                }
            }
            ProjectAction::Exit => {
                self.project_session.exit_approved = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub fn project_lifetime_ui(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.project_session.exit_approved {
            if self.project_session.pending.is_some() || self.project_has_changes() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_project_action(ProjectAction::Exit, ctx);
            } else {
                // Let eframe finish the native close. Canceling it and
                // requesting a second close in the same frame can stall exit.
                self.project_session.exit_approved = true;
            }
        }
        if self.project_session.pending.is_none() {
            return;
        }
        let l = self.settings.language;
        let mut choice = 0;
        egui::Modal::new(egui::Id::new("unsaved_project")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(l.text("Save changes?", "是否保存更改？"));
            ui.label(format!(
                "{} {}",
                self.project_name,
                l.text("has unsaved changes.", "有尚未保存的更改。")
            ));
            ui.label(l.text(
                "Save before continuing, discard changes, or cancel to keep working.",
                "保存后继续、放弃更改，或取消并返回当前项目。",
            ));
            if let Some(error) = &self.project_session.save_error {
                ui.colored_label(egui::Color32::from_rgb(200, 65, 50), error);
            }
            ui.horizontal(|ui| {
                if ui.button(l.text("Save", "保存")).clicked() {
                    choice = 1;
                }
                if ui.button(l.text("Discard", "不保存")).clicked() {
                    choice = 2;
                }
                if ui.button(l.text("Cancel", "取消")).clicked() {
                    choice = 3;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = 3;
        }
        match choice {
            1 => {
                if crate::ui::save_project(self, false) {
                    if let Some(action) = self.project_session.pending.take() {
                        self.perform_project_action(action, ctx);
                    }
                }
            }
            2 => {
                if let Some(action) = self.project_session.pending.take() {
                    self.perform_project_action(action, ctx);
                }
            }
            3 => {
                self.project_session.pending = None;
                self.project_session.save_error = None;
            }
            _ => {}
        }
    }
}

fn atomic_project_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let (temp, mut file) = loop {
        let temp = parent.join(format!(
            ".s33d-save-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_project_file(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(windows)]
fn replace_project_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace_project_file(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> S33DApp {
        S33DApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ))
    }
    fn add_body(app: &mut S33DApp) -> BodyId {
        app.begin_document_change();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: format!("Object {id}"),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "test.step".into(),
            },
            shape: None,
            material: Material::default(),
            visible: true,
        });
        id
    }
    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("s33d-session-{}-{label}.s33d", std::process::id()))
    }
    #[test]
    fn selection_view_and_undo_to_saved_checkpoint_are_clean() {
        let mut app = app();
        assert!(!app.project_has_changes());
        let id = add_body(&mut app);
        app.mark_project_saved(None);
        app.selected = vec![id];
        app.camera.ortho = true;
        app.record_history("Select object");
        assert!(!app.project_has_changes());
        app.begin_document_change();
        app.doc.body_mut(id).unwrap().visible = false;
        assert!(app.project_has_changes());
        app.undo();
        assert!(!app.project_has_changes());
        app.redo();
        assert!(app.project_has_changes());
    }
    #[test]
    fn drawing_project_roundtrip_and_legacy() {
        let mut app = app();
        app.drawing.title = "工程图".into();
        app.drawing.views = vec![crate::drawing2d::Projection::default(); 3];
        assert!(app.project_has_changes());
        let encoded = app.project_to_s33d().unwrap();
        app.new_project();
        app.load_s33d_project(&encoded).unwrap();
        assert_eq!(app.drawing.title, "工程图");
        assert_eq!(app.drawing.views.len(), 3);
        assert!(!app.project_has_changes());
        let mut legacy: Value = serde_json::from_str(&encoded).unwrap();
        legacy.as_object_mut().unwrap().remove("drawing");
        legacy["version"] = json!(1);
        app.load_s33d_project(&legacy.to_string()).unwrap();
        assert!(app.drawing.views.is_empty());
        assert!(app.drawing.title.is_empty());
    }

    #[test]
    fn save_as_remembers_path_and_failed_save_or_open_keeps_session() {
        let mut app = app();
        add_body(&mut app);
        let first = path("保存 A");
        let second = path("保存 B");
        app.save_project_to_path(&first).unwrap();
        assert!(!app.project_has_changes());
        add_body(&mut app);
        app.save_project_to_path(&second).unwrap();
        assert_eq!(app.project_session.path.as_ref(), Some(&second));
        add_body(&mut app);
        assert!(app
            .save_project_to_path(&path("missing").join("project.s33d"))
            .is_err());
        assert_eq!(app.project_session.path.as_ref(), Some(&second));
        assert!(app.project_has_changes());
        let invalid = path("invalid");
        std::fs::write(&invalid, "broken project").unwrap();
        assert!(app.open_project_from_path(&invalid).is_err());
        assert_eq!(app.doc.bodies.len(), 3);
        assert_eq!(app.project_session.path.as_ref(), Some(&second));
        app.open_project_from_path(&first).unwrap();
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.project_session.path.as_ref(), Some(&first));
        assert!(!app.project_has_changes());
        // Saving to the same destination replaces it atomically, too.
        add_body(&mut app);
        app.save_project_to_path(&first).unwrap();
        app.open_project_from_path(&first).unwrap();
        assert_eq!(app.doc.bodies.len(), 2);
        for p in [first, second, invalid] {
            std::fs::remove_file(p).unwrap();
        }
    }
    #[test]
    fn clean_native_close_is_allowed_in_the_original_frame() {
        let mut app = app();
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run(input, |ctx| app.project_lifetime_ui(ctx));
        assert!(app.project_session.exit_approved);
        assert!(app.project_session.pending.is_none());
        assert!(!output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(
                c,
                egui::ViewportCommand::Close | egui::ViewportCommand::CancelClose
            )));
    }

    #[test]
    fn dirty_project_actions_wait_and_escape_cancels_native_close() {
        let mut app = app();
        add_body(&mut app);
        let ctx = egui::Context::default();
        for action in [
            ProjectAction::New,
            ProjectAction::Close,
            ProjectAction::Open(path("not-opened")),
            ProjectAction::Exit,
        ] {
            app.request_project_action(action, &ctx);
            assert!(app.project_session.pending.is_some());
            assert_eq!(app.doc.bodies.len(), 1);
            app.project_session.pending = None;
        }
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run(input, |ctx| app.project_lifetime_ui(ctx));
        assert!(matches!(
            app.project_session.pending,
            Some(ProjectAction::Exit)
        ));
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(input, |ctx| app.project_lifetime_ui(ctx));
        assert!(app.project_session.pending.is_none());
        assert_eq!(app.doc.bodies.len(), 1);
        app.perform_project_action(ProjectAction::Close, &ctx);
        assert!(app.doc.bodies.is_empty());
        assert!(app.project_session.path.is_none());
        assert!(!app.project_has_changes());
    }
    #[test]
    fn collapsed_sketch_and_draw3d_keep_drafts_and_save_commits_sketch() {
        let mut app = app();
        app.toggle_sketch_menu();
        assert!(app.sketch_tools_open);
        app.start_sketch_on(DatumPlane::Front);
        app.sketch
            .as_mut()
            .unwrap()
            .entities
            .push(SketchEntity::Line {
                a: [0., 0.],
                b: [10., 0.],
            });
        app.toggle_sketch_menu();
        assert!(!app.sketch_tools_open);
        assert_eq!(app.mode, Mode::Sketch);
        app.toggle_sketch_menu();
        assert_eq!(app.sketch.as_ref().unwrap().entities.len(), 1);
        let p = path("draft");
        app.save_project_to_path(&p).unwrap();
        assert_eq!(app.mode, Mode::Idle);
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(!app.project_has_changes());
        std::fs::remove_file(p).unwrap();
        app.toggle_draw3d_menu();
        assert!(app.draw3d_tools_open);
        app.choose_insert_3d_primitive(Primitive3DKind::Box);
        let params = app.insert_3d_ctx.as_ref().unwrap().params;
        app.toggle_draw3d_menu();
        assert!(!app.draw3d_tools_open);
        assert_eq!(app.mode, Mode::Insert3D);
        app.toggle_draw3d_menu();
        assert_eq!(app.insert_3d_ctx.as_ref().unwrap().params, params);
    }

    #[test]
    fn menu_categories_are_exclusive_and_group_only_changes_are_dirty() {
        let mut app = app();
        app.toggle_sketch_menu();
        assert!(app.sketch_tools_open && !app.draw3d_tools_open);
        app.toggle_draw3d_menu();
        assert!(app.draw3d_tools_open && !app.sketch_tools_open);
        app.toggle_sketch_menu();
        assert!(app.sketch_tools_open && !app.draw3d_tools_open);
        app.reset_document_tools();
        let a = add_body(&mut app);
        let b = add_body(&mut app);
        app.mark_project_saved(None);
        app.selected = vec![a, b];
        app.group_selected();
        assert!(app.project_has_changes());
        app.mark_project_saved(None);
        assert!(!app.project_has_changes());
        app.ungroup_selected();
        assert!(app.project_has_changes());
        assert!(app.doc.undo());
        assert!(!app.project_has_changes());
    }
}
