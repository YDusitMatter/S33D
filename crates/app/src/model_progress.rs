//! Isolated modeling transactions with responsive, cancellable desktop progress.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Instant;

// Only owned model/editor data crosses the thread boundary. GPU resources,
// listeners, plugins, user preferences and files remain on the UI thread.
macro_rules! model_state {
    ($($field:ident: $ty:ty),* $(,)?) => {
        struct ModelState {
            $($field: $ty,)*
            drawing_open: bool,
            thread_menus: [bool; 3],
        }
        impl ModelState {
            fn capture(app: &S33DApp) -> Self { Self {
                $($field: app.$field.clone(),)*
                drawing_open: app.drawing.open,
                thread_menus: [app.thread_ui.expanded, app.thread_ui.print_open, app.thread_ui.save_open],
            } }
            fn take(app: S33DApp) -> Self { Self {
                $($field: app.$field,)*
                drawing_open: app.drawing.open,
                thread_menus: [app.thread_ui.expanded, app.thread_ui.print_open, app.thread_ui.save_open],
            } }
            fn install(self, app: &mut S33DApp) {
                $(app.$field = self.$field;)*
                app.drawing.open = self.drawing_open;
                [app.thread_ui.expanded, app.thread_ui.print_open, app.thread_ui.save_open] = self.thread_menus;
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct WorkProgress {
    stage: Arc<Mutex<((&'static str, &'static str), Option<(usize, usize)>)>>,
    cancelled: Arc<AtomicBool>,
}

struct Outcome {
    state: ModelState,
    result: AgentCommandResult,
    toasts: Vec<(String, Instant)>,
    hint: String,
}

pub(crate) struct ModelJob {
    title: (&'static str, &'static str),
    started: Instant,
    progress: WorkProgress,
    receiver: mpsc::Receiver<Outcome>,
    source_bodies: Vec<Body>,
    source_groups: Vec<s33d_model::ObjectGroup>,
    source_name: String,
    source_document_id: String,
    source_revision: u64,
    source_history: Vec<String>,
    thread_editor: bool,
    pub(super) pending_agent: Option<crate::agent_protocol::PendingAgentRequest>,
}

impl S33DApp {
    pub(crate) fn progress_busy(&self) -> bool {
        self.model_job.is_some()
            || self.export_job.is_some()
            || self.step_import_job.is_some()
            || self.face_extrude_job.is_some()
            || self.fastener_job.is_some()
            || self.offset_ctx.as_ref().is_some_and(|c| c.job.is_some())
            || self.surface_progress_open()
            || self.drawing.projection_pending()
    }

    pub(crate) fn report_model_progress(
        &self,
        stage: (&'static str, &'static str),
        count: Option<(usize, usize)>,
    ) -> Result<(), String> {
        if let Some(work) = &self.model_work {
            if work.cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            *work.stage.lock().unwrap() = (stage, count);
        }
        Ok(())
    }

    pub(crate) fn start_model_job(
        &mut self,
        title: (&'static str, &'static str),
        thread_editor: bool,
        pending_agent: Option<crate::agent_protocol::PendingAgentRequest>,
        command: impl FnOnce(&mut S33DApp) -> AgentCommandResult + Send + 'static,
    ) -> Result<(), String> {
        if self.progress_busy() || self.surface_preview_pending() {
            return Err(self
                .settings
                .language
                .text("Wait for the current operation", "请等待当前操作完成")
                .into());
        }
        self.observe_agent_document();
        let state = ModelState::capture(self);
        let settings = self.settings.clone();
        let project_path = self.project_session.path.clone();
        let progress = WorkProgress {
            stage: Arc::new(Mutex::new((("Computing geometry…", "正在计算几何…"), None))),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let work = progress.clone();
        let (sender, receiver) = mpsc::channel();
        // Install the job before spawning: even an immediate result cannot
        // commit before the UI has returned from the button/exit handler.
        self.model_job = Some(ModelJob {
            title,
            started: Instant::now(),
            progress,
            receiver,
            source_bodies: self.doc.bodies.clone(),
            source_groups: self.doc.groups.clone(),
            source_name: self.doc.name().to_owned(),
            source_document_id: self.agent_clock.document_id.clone(),
            source_revision: self.agent_clock.revision,
            source_history: self.history.clone(),
            thread_editor,
            pending_agent,
        });
        std::thread::spawn(move || {
            let mut worker = Self::from_ui_state(
                UiStateFile {
                    settings,
                    ..Default::default()
                },
                false,
            );
            state.install(&mut worker);
            worker.project_session.path = project_path;
            worker.model_work = Some(work);
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| command(&mut worker)))
                    .unwrap_or_else(|_| {
                        Err((
                            "geometry".into(),
                            "Geometry calculation failed / 几何计算失败".into(),
                        ))
                    });
            let toasts = std::mem::take(&mut worker.toasts);
            let hint = std::mem::take(&mut worker.hint);
            let _ = sender.send(Outcome {
                state: ModelState::take(worker),
                result,
                toasts,
                hint,
            });
        });
        Ok(())
    }

    pub(super) fn needs_background_finish(&self) -> bool {
        matches!(
            self.mode,
            Mode::Extrude | Mode::Revolve | Mode::Boolean | Mode::Split | Mode::Mirror
        ) || (self.mode == Mode::Idle
            && ((self.chamfer_active && !self.chamfer_state_applied())
                || (self.array_active
                    && self.array_exit_applied.as_ref() != Some(&self.array_exit_signature()))))
    }

    pub(super) fn queue_tool_switch(
        &mut self,
        command: impl FnOnce(&mut Self) + Send + 'static,
    ) -> bool {
        if !self.background_modeling || !self.needs_background_finish() {
            return false;
        }
        let title = self.exit_tool_name().unwrap_or(("Finish tool", "完成工具"));
        if let Err(error) = self.start_model_job(title, false, None, move |worker| {
            let before = worker.doc.bodies.clone();
            command(worker);
            if before == worker.doc.bodies && worker.needs_background_finish() {
                return Err((
                    "geometry".into(),
                    worker
                        .toasts
                        .last()
                        .map(|t| t.0.clone())
                        .unwrap_or_else(|| "Cannot finish tool / 无法完成当前工具".into()),
                ));
            }
            Ok(Value::Null)
        }) {
            self.toast(error);
        }
        true
    }

    pub(super) fn queue_model_command(
        &mut self,
        title: (&'static str, &'static str),
        command: fn(&mut Self),
    ) -> bool {
        if !self.background_modeling {
            return false;
        }
        if let Err(error) = self.start_model_job(title, false, None, move |worker| {
            let before = worker.doc.bodies.clone();
            command(worker);
            if before == worker.doc.bodies {
                return Err((
                    "geometry".into(),
                    worker
                        .toasts
                        .last()
                        .map(|t| t.0.clone())
                        .unwrap_or_else(|| {
                            worker
                                .settings
                                .language
                                .text(
                                    "No geometry was changed; check the selection and parameters",
                                    "几何未改变，请检查选择和参数",
                                )
                                .into()
                        }),
                ));
            }
            Ok(Value::Null)
        }) {
            self.toast(error);
        }
        true
    }

    pub(super) fn queue_agent_model_job(&mut self, request: &AgentRequest) -> bool {
        if !self.background_modeling
            || self.progress_busy()
            || self.tool_exit_prompt.is_some()
            || self.project_session.pending.is_some()
            || self.mode != Mode::Idle
        {
            return false;
        }
        let title = match request.method.as_str() {
            "view.render" => ("Render model image", "生成模型图"),
            "reference.import" => ("Import reference photos", "导入参考照片"),
            "thread.apply" => ("Generate thread", "生成螺纹"),
            "thread.targets" => ("Find cylindrical surfaces", "识别圆柱面"),
            "feature.extrude" => ("Extrude / Pocket", "拉伸 / 挖孔"),
            "feature.revolve" => ("Revolve", "旋转"),
            "boolean.apply" => ("Boolean", "布尔运算"),
            "primitive.create" => ("Create solid", "创建实体"),
            "body.transform" => ("Transform", "变换"),
            _ => return false,
        };
        let request = request.clone();
        self.start_model_job(title, false, None, move |worker| {
            worker.execute_agent_request(request)
        })
        .is_ok()
    }

    pub(crate) fn cancel_model_job(&mut self) {
        if let Some(job) = self.model_job.take() {
            job.progress.cancelled.store(true, Ordering::Relaxed);
            if job.thread_editor {
                self.thread_ui.expanded = true;
            }
            if let Some(pending) = job.pending_agent {
                let id = pending.request.id.clone();
                pending.respond(AgentResponse::failure(
                    id,
                    "cancelled",
                    "Operation cancelled; document unchanged",
                ));
            }
            self.pending_tool_exit = None;
            self.set_hint("Ready");
        }
    }

    pub(crate) fn poll_model_job(&mut self) {
        self.observe_agent_document();
        let Some(job) = &self.model_job else {
            return;
        };
        let outcome = match job.receiver.try_recv() {
            Ok(outcome) => Ok(outcome),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err((
                "geometry".into(),
                "Geometry worker disconnected / 几何计算进程中断".into(),
            )),
        };
        let job = self.model_job.take().unwrap();
        let current = job.source_document_id == self.agent_clock.document_id
            && job.source_revision == self.agent_clock.revision
            && job.source_bodies == self.doc.bodies
            && job.source_groups == self.doc.groups
            && job.source_name == self.doc.name()
            && job.source_history == self.history;
        let result = if !current {
            Err((
                "stale".into(),
                self.settings
                    .language
                    .text(
                        "Document changed; calculate again",
                        "项目已改变，请重新计算",
                    )
                    .into(),
            ))
        } else {
            outcome.and_then(|outcome| {
                outcome.result.and_then(|value| {
                    // File-open jobs carry their view/drawing/session alongside
                    // the model snapshot. Validate before installing anything.
                    let opened = value
                        .get("_opened_project")
                        .and_then(Value::as_str)
                        .map(|path| -> Result<_, (String, String)> {
                            let drawing: crate::drawing2d::Drawing =
                                serde_json::from_value(value["_opened_drawing"].clone())
                                    .map_err(|e| ("file".into(), e.to_string()))?;
                            Ok((PathBuf::from(path), drawing))
                        })
                        .transpose()?;
                    if job
                        .pending_agent
                        .as_ref()
                        .is_some_and(|p| p.request.method == "view.render")
                    {
                        return Ok(value);
                    }
                    if opened.is_some() {
                        self.reset_document_tools();
                    }
                    outcome.state.install(self);
                    if let Some((path, drawing)) = opened {
                        self.drawing = drawing;
                        self.mark_project_saved(Some(path));
                    }
                    self.toasts.extend(outcome.toasts);
                    self.hint = outcome.hint;
                    self.mesh_cache.clear();
                    self.section_cache.clear();
                    self.regions_cache.clear();
                    self.pick_geometry_cache.clear();
                    self.measurement_cache.clear();
                    self.cylinder_pick_cache.clear();
                    self.last_hover_query = None;
                    self.hover_body = None;
                    self.hover_face = None;
                    self.hover_edge = None;
                    self.bump_scene();
                    if job.thread_editor {
                        self.thread_choices.clear();
                        self.thread_checked.clear();
                        self.thread_ui.expanded = false;
                        self.toast(
                            self.settings
                                .language
                                .text("Thread generated", "螺纹已生成"),
                        );
                    }
                    Ok(value)
                })
            })
        };
        if let Some(pending) = job.pending_agent {
            let id = pending.request.id.clone();
            pending.respond(match result {
                Ok(value) => AgentResponse::success(id, value),
                Err((code, message)) => AgentResponse::failure(id, code, message),
            });
        } else if let Err((_, message)) = result {
            self.pending_tool_exit = None;
            if job.thread_editor {
                self.thread_ui.expanded = true;
                self.thread_ui.error = Some(message.clone());
            }
            self.toast(message);
        }
    }

    /// Actual phases/counts only; a spinner is used for indivisible kernel calls.
    pub(crate) fn model_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.model_job else {
            return;
        };
        let language = self.settings.language;
        let title = job.title;
        let (stage, count) = *job.progress.stage.lock().unwrap();
        let elapsed = job.started.elapsed().as_secs_f64();
        if progress_modal(ctx, language, title, stage, count, elapsed) {
            self.cancel_model_job();
            self.toast(language.text("Cancelled; document unchanged", "已取消，模型未改变"));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

pub(crate) fn progress_modal(
    ctx: &egui::Context,
    language: UiLanguage,
    title: (&'static str, &'static str),
    stage: (&'static str, &'static str),
    count: Option<(usize, usize)>,
    elapsed: f64,
) -> bool {
    let mut cancel = false;
    egui::Modal::new(egui::Id::new("modeling_progress")).show(ctx, |ui| {
        ui.set_width(340.);
        ui.heading(language.text(title.0, title.1));
        ui.add_space(8.);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(language.text(stage.0, stage.1));
        });
        if let Some((done, total)) = count {
            if total > 0 {
                ui.add(
                    egui::ProgressBar::new((done as f32 / total as f32).clamp(0., 1.))
                        .show_percentage(),
                );
                ui.small(format!("{done} / {total}"));
            }
        }
        ui.label(match language {
            UiLanguage::English => format!("Elapsed: {elapsed:.1} s"),
            UiLanguage::ChineseSimplified => format!("已用时：{elapsed:.1} 秒"),
        });
        ui.add_space(6.);
        ui.label(language.text(
            "Please wait. Cancel keeps the current model.",
            "请稍候。取消将保留当前模型。",
        ));
        ui.add_space(8.);
        cancel = ui.button(language.text("Cancel", "取消")).clicked();
    });
    cancel || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}
model_state! {
    doc: Document,
    references: crate::reference_assets::ReferenceLibrary,
    agent_clock: agent_visual::DocumentClock,
    kernel: TruckKernel,
    camera: Camera,
    mode: Mode,
    sketch_edit_body: Option<BodyId>,
    sketch_edit_original_doc: Option<Document>,
    sketch_edit_original_selection: Vec<BodyId>,
    selected: Vec<BodyId>,
    selected_regions: Vec<usize>,
    selected_region_source: Option<region_selection::RegionSelectionSource>,
    object_list_anchor: Option<BodyId>,
    extrude_ctx: Option<ExtrudeCtx>,
    revolve_ctx: Option<RevolveCtx>,
    boolean_ctx: Option<BooleanCtx>,
    split_ctx: Option<SplitCtx>,
    section_view_ctx: Option<SectionViewCtx>,
    mirror_ctx: Option<MirrorCtx>,
    insert_3d_ctx: Option<Insert3DCtx>,
    gizmo: Option<GizmoMode>,
    gizmo_drag: Option<GizmoDrag>,
    face_pick: Option<FaceHit>,
    align_active: bool,
    axis_align: axis_align::AxisAlign,
    align_axis: usize,
    align_mode: AlignMode,
    align_reference: Option<BodyId>,
    move_input: [f32; 3],
    move_to_input: [f32; 3],
    move_step: f32,
    rotate_input: (usize, f32),
    rotate_custom_axis: Option<RotationAxis>,
    rotate_axis_pick_active: bool,
    scale_input: [f32; 3],
    scale_uniform: bool,
    scale_size_input: [f32; 3],
    scale_size_uniform: bool,
    chamfer_active: bool,
    chamfer_fillet: bool,
    chamfer_param: f32,
    selected_edge: Option<EdgeRef>,
    chamfer_base_shapes: std::collections::HashMap<BodyId, ShapeId>,
    chamfer_committed: bool,
    array_active: bool,
    array_axes: [bool; 3],
    array_positive: [bool; 3],
    array_spacing: [f32; 3],
    array_copies: [u32; 3],
    array_exit_applied: Option<String>,
    exit_move_to_baseline: [f32; 3],
    exit_scale_size_baseline: [f32; 3],
    sketch_on_body: Option<BodyId>,
    snap_enabled: bool,
    snap_step: f64,
    show_grid: bool,
    saved_camera: Option<Camera>,
    viewport_rect: egui::Rect,
    history: Vec<String>,
    undo_selection_stack: Vec<Vec<BodyId>>,
    redo_selection_stack: Vec<Vec<BodyId>>,
    sketch_undo_stack: Vec<SketchSnapshot>,
    sketch_redo_stack: Vec<SketchSnapshot>,
    sketch_history_current: Option<SketchSnapshot>,
    sketch_tools_open: bool,
    draw3d_tools_open: bool,
    thread_choices: Vec<crate::threads::Target>,
    thread_checked: Vec<bool>,
    open_thread_panel: bool,
    thread_params: crate::threads::Params,
}
