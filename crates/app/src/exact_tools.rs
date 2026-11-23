//! Exact-operation UI state. Native work runs off the UI thread; cancellation
//! drops the receiver and never commits a late result into another document.
use super::*;
use s33d_kernel::{occt::ExactShape, OffsetMode};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Display-only prism of an already tessellated native face. It is never used
/// as a Boolean operand: Done reconstructs and validates the exact face prism.
pub(super) fn face_preview_mesh(
    input: &ExactShape,
    face: u32,
    normal: [f64; 3],
    depth: f64,
    start: f64,
) -> Result<MeshData, String> {
    let normal = glam::DVec3::from_array(normal);
    if !depth.is_finite()
        || !start.is_finite()
        || depth.abs() <= 1e-9
        || !normal.is_finite()
        || normal.length() < 1e-9
        || input.planar_faces.get(face as usize) != Some(&true)
    {
        return Err("Invalid planar face preview".into());
    }
    let normal = normal.normalize();
    let mut remap = HashMap::new();
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (t, id) in input
        .mesh
        .indices
        .chunks_exact(3)
        .zip(&input.triangle_faces)
    {
        if *id != face {
            continue;
        }
        for index in t {
            let mapped = *remap.entry(*index).or_insert_with(|| {
                let mapped = positions.len() as u32;
                positions.push(input.mesh.positions[*index as usize]);
                mapped
            });
            indices.push(mapped);
        }
    }
    let mut patch = MeshData::new(positions, indices);
    patch.weld(1e-5);
    if patch.is_empty() {
        return Err("Face has no preview triangles".into());
    }
    let delta = normal * depth;
    let n = patch.positions.len() as u32;
    let mut positions: Vec<_> = patch
        .positions
        .iter()
        .map(|p| {
            (glam::DVec3::from_array(p.map(f64::from)) + normal * start)
                .as_vec3()
                .to_array()
        })
        .collect();
    positions.extend(patch.positions.iter().map(|p| {
        (glam::DVec3::from_array(p.map(f64::from)) + normal * start + delta)
            .as_vec3()
            .to_array()
    }));
    let mut indices = Vec::new();
    let mut boundary: HashMap<(u32, u32), (u32, u32, usize)> = HashMap::new();
    for tri in patch.indices.chunks_exact(3) {
        let [a, mut b, mut c] = [tri[0], tri[1], tri[2]];
        let p = |i: u32| glam::DVec3::from_array(patch.positions[i as usize].map(f64::from));
        if (p(b) - p(a)).cross(p(c) - p(a)).dot(delta) < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        indices.extend([a, c, b, a + n, b + n, c + n]);
        for (a, b) in [(a, b), (b, c), (c, a)] {
            let key = if a < b { (a, b) } else { (b, a) };
            boundary
                .entry(key)
                .and_modify(|entry| entry.2 += 1)
                .or_insert((a, b, 1));
        }
    }
    for (_, (a, b, count)) in boundary {
        if count == 1 {
            indices.extend([a, b, b + n, a, b + n, a + n]);
        }
    }
    Ok(MeshData::new(positions, indices))
}

pub struct StepImportJob {
    path: String,
    started: std::time::Instant,
    receiver: Receiver<Result<Vec<ExactShape>, String>>,
    read_progress: Arc<Mutex<Option<(usize, usize)>>>,
}

pub struct FaceExtrudeJob {
    context: ExtrudeCtx,
    target: BodyId,
    source: ShapeId,
    started: std::time::Instant,
    receiver: Receiver<Result<ExactShape, String>>,
}

pub struct OffsetContext {
    pub equation_extrude: bool,
    pub agent_job: Option<String>,
    pub source: BodyId,
    pub input: ExactShape,
    pub distance: f64,
    pub tolerance: f64,
    pub faces: Vec<u32>,
    pub operation: OffsetMode,
    pub keep_original: bool,
    pub preview: Option<ExactShape>,
    pub error: Option<String>,
    pub job: Option<Receiver<Result<ExactShape, String>>>,
    started: Option<std::time::Instant>,
}

impl S33DApp {
    pub(crate) fn offset_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(work) = self.offset_ctx.as_ref().filter(|c| c.job.is_some()) else {
            return;
        };
        let elapsed = work
            .started
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.);
        if elapsed < 0.25 {
            return;
        }
        if super::model_progress::progress_modal(
            ctx,
            self.settings.language,
            ("Offset / Shell", "偏移 / 抽壳"),
            ("Computing preview…", "正在计算预览…"),
            None,
            elapsed,
        ) {
            let work = self.offset_ctx.as_mut().unwrap();
            work.job = None;
            work.error = Some(
                self.settings
                    .language
                    .text(
                        "Cancelled; adjust parameters or apply again",
                        "已取消，可修改参数或重新应用",
                    )
                    .into(),
            );
            self.pending_tool_exit = None;
            self.bump_scene();
        }
    }

    pub(super) fn start_exact_face_commit(&mut self) -> bool {
        let Some(context) = self.extrude_ctx.as_ref() else {
            return false;
        };
        let Some((source, face)) = context.exact_face else {
            return false;
        };
        let target = if context.pocket {
            context.target_body
        } else {
            context.sketch.on_body
        };
        let Some(target) = target else {
            return false;
        };
        // Only this immutable source may be replaced by a late result.
        if self.doc.body(target).and_then(|body| body.shape) != Some(source) {
            return false;
        }
        let Some(input) = self.kernel.exact_shape(source).cloned() else {
            return false;
        };
        let mut context = self.extrude_ctx.take().unwrap();
        for ghost in context.ghost_shapes.drain(..) {
            self.kernel.delete(ghost);
        }
        let work = context.clone();
        let (tx, receiver) = mpsc::channel();
        self.face_extrude_job = Some(FaceExtrudeJob {
            context,
            target,
            source,
            started: std::time::Instant::now(),
            receiver,
        });
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                if !work.depth.is_finite() || work.depth <= 0.0 {
                    return Err("Invalid extrusion depth".into());
                }
                let plane = &work.sketch.plane;
                let (depth, start) = if work.pocket && work.through {
                    let (cutter, depth) = through_pocket_cutter_spec(plane, &input.mesh)?;
                    (
                        depth,
                        (cutter.origin() - plane.origin()).dot(plane.normal()),
                    )
                } else if work.pocket {
                    (-(work.depth + 2.0), 2.0)
                } else {
                    (
                        work.depth,
                        if work.symmetric {
                            -work.depth / 2.0
                        } else {
                            0.0
                        },
                    )
                };
                input
                    .extrude_planar_face_boolean(
                        face,
                        plane.normal().to_array(),
                        depth,
                        start,
                        work.pocket,
                    )
                    .map_err(|e| e.to_string())
            })
            .unwrap_or_else(|_| Err("Exact extrusion worker failed".into()));
            let _ = tx.send(result);
        });
        self.bump_scene();
        true
    }

    pub fn poll_exact_face_commit(&mut self) {
        let result = self
            .face_extrude_job
            .as_ref()
            .map(|job| job.receiver.try_recv());
        let result = match result {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => {
                Err("Exact extrusion worker disconnected".into())
            }
            _ => return,
        };
        let mut job = self.face_extrude_job.take().unwrap();
        self.mode = Mode::Idle;
        if self.doc.body(job.target).and_then(|b| b.shape) != Some(job.source) {
            self.pending_tool_exit = None;
            self.toast("Source changed; exact extrusion result discarded");
            self.bump_scene();
            return;
        }
        match result {
            Ok(shape) => {
                self.begin_document_change();
                let shape = self.kernel.insert_exact(shape);
                let mut pocket_source_sketches = self.pocket_sources(job.target);
                if job.context.pocket {
                    if let Some(id) = self.profile_source_link(job.context.body_id) {
                        if !pocket_source_sketches.contains(&id) {
                            pocket_source_sketches.push(id);
                        }
                    }
                }
                let body = self.doc.body_mut(job.target).unwrap();
                body.shape = Some(shape);
                body.feature = Feature::Extrude {
                    sketch: job.context.sketch,
                    params: ExtrudeParams {
                        depth: job.context.depth,
                        symmetric: job.context.symmetric,
                        bevel: 0.0,
                    },
                    source_sketch: None,
                    pocket_source_sketches,
                    region_indices: Vec::new(),
                };
                self.selected = vec![job.target];
                self.clear_region_selection();
                self.face_pick = None;
                self.hover_face = None;
                self.selected_edge = None;
                self.hover_edge = None;
                self.mesh_cache.clear();
                self.record_history(format!(
                    "{} {}{}",
                    if job.context.pocket {
                        "Pocket"
                    } else {
                        "Surface Extrude"
                    },
                    self.settings.unit.format_mm(job.context.depth),
                    if job.context.through {
                        " (through)"
                    } else {
                        ""
                    }
                ));
                self.toast(
                    self.settings
                        .language
                        .text("Exact operation complete", "精确建模操作完成"),
                );
            }
            Err(error) => {
                self.pending_tool_exit = None;
                self.toast(format!(
                    "{} failed: {error}",
                    if job.context.pocket {
                        "Pocket"
                    } else {
                        "Surface Extrude"
                    }
                ));
                job.context.ghost_shapes.clear();
                self.extrude_ctx = Some(job.context);
                self.mode = Mode::Extrude;
                self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
            }
        }
        self.bump_scene();
    }

    pub fn exact_face_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = self.face_extrude_job.as_ref() else {
            return;
        };
        let language = self.settings.language;
        let title = if job.context.pocket {
            ("Pocket", "挖孔")
        } else {
            ("Extrude", "拉伸")
        };
        if super::model_progress::progress_modal(
            ctx,
            language,
            title,
            ("Computing geometry…", "正在计算几何…"),
            None,
            job.started.elapsed().as_secs_f64(),
        ) {
            let job = self.face_extrude_job.take().unwrap();
            self.extrude_ctx = Some(job.context);
            self.pending_tool_exit = None;
            self.mode = Mode::Extrude;
            self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
            self.toast(language.text("Cancelled; document unchanged", "已取消，模型未改变"));
            self.bump_scene();
        }
    }

    pub fn start_step_import(&mut self, path: String) {
        if self.progress_busy() {
            return;
        }
        if !self.finish_creation_for_tool_switch() {
            return;
        }
        self.exit_sidebar_tool();
        if self.progress_busy() {
            return;
        }
        let (tx, receiver) = mpsc::channel();
        let input = path.clone();
        let read_progress = Arc::new(Mutex::new(None));
        self.step_import_job = Some(StepImportJob {
            path,
            started: std::time::Instant::now(),
            receiver,
            read_progress: read_progress.clone(),
        });
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                let ext = Path::new(&input)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if ext.eq_ignore_ascii_case("igs") || ext.eq_ignore_ascii_case("iges") {
                    ExactShape::import_iges(&input, true).map_err(|e| e.to_string())
                } else {
                    use std::io::Read;
                    let mut file = std::fs::File::open(input).map_err(|e| e.to_string())?;
                    let total = file.metadata().map_err(|e| e.to_string())?.len() as usize;
                    let mut bytes = Vec::new();
                    let mut chunk = vec![0; 1024 * 1024];
                    loop {
                        *read_progress.lock().unwrap() = Some((bytes.len(), total));
                        let count = file.read(&mut chunk).map_err(|e| e.to_string())?;
                        if count == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&chunk[..count]);
                    }
                    *read_progress.lock().unwrap() = None;
                    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                    ExactShape::import_step_normalized(&text).map_err(|e| e.to_string())
                }
            })
            .unwrap_or_else(|_| Err("CAD import worker failed".into()));
            let _ = tx.send(result);
        });
    }

    pub fn poll_step_import(&mut self) {
        let result = self
            .step_import_job
            .as_ref()
            .map(|job| job.receiver.try_recv());
        let result = match result {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => Err("CAD import worker disconnected".into()),
            _ => return,
        };
        let job = self.step_import_job.take().unwrap();
        match result {
            Ok(shapes) if !shapes.is_empty() => {
                self.begin_document_change();
                let first = self.doc.bodies.len() + 1;
                let mut selected = Vec::new();
                for (index, shape) in shapes.into_iter().enumerate() {
                    let kind = if shape.solid_count == 0 {
                        BodyKind::Surface
                    } else {
                        BodyKind::Solid
                    };
                    let shape = self.kernel.insert_exact(shape);
                    let id = self.doc.alloc_id();
                    self.doc.add_body(Body {
                        id,
                        name: format!("Imported {}", first + index),
                        kind,
                        feature: Feature::Imported {
                            path: job.path.clone(),
                        },
                        shape: Some(shape),
                        material: Material {
                            color: palette_color(first + index),
                            ..Material::default()
                        },
                        visible: true,
                    });
                    selected.push(id);
                }
                self.selected = selected;
                self.clear_region_selection();
                self.face_pick = None;
                self.selected_edge = None;
                self.hover_face = None;
                self.hover_edge = None;
                self.hover_body = None;
                self.camera.set_view(crate::camera::ViewDir::Iso);
                self.fit_view();
                self.record_history(format!("Imported {} CAD object(s)", self.selected.len()));
                self.toast(format!(
                    "CAD import complete: {} object(s), {:.1}s",
                    self.selected.len(),
                    job.started.elapsed().as_secs_f64()
                ));
                self.bump_scene();
            }
            Ok(_) => self.toast("CAD file contains no surfaces"),
            Err(error) => self.toast(format!("CAD import failed: {error}")),
        }
    }

    pub fn step_import_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = self.step_import_job.as_ref() else {
            return;
        };
        let language = self.settings.language;
        let count = *job.read_progress.lock().unwrap();
        if super::model_progress::progress_modal(
            ctx,
            language,
            ("Import CAD model", "导入 CAD 模型"),
            if count.is_some() {
                ("Reading CAD file…", "正在读取 CAD 文件…")
            } else {
                (
                    "Reconstructing geometry and preparing display…",
                    "正在重建几何并准备显示…",
                )
            },
            count,
            job.started.elapsed().as_secs_f64(),
        ) {
            self.step_import_job = None;
            self.set_hint("Ready");
            self.toast(language.text(
                "CAD import cancelled; document unchanged",
                "已取消 CAD 导入，文档未改变",
            ));
        }
    }

    pub(super) fn agent_offset_start(&mut self, params: Value) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            body_id: BodyId,
            mode: OffsetMode,
            distance: f64,
            tolerance: f64,
            #[serde(default)]
            faces: Vec<u32>,
            #[serde(default)]
            hide_original: bool,
        }
        self.agent_require_idle()?;
        let input = Self::agent_parse::<Input>(params)?;
        let shape = self
            .body(input.body_id)
            .and_then(|body| body.shape)
            .and_then(|id| self.kernel.exact_shape(id))
            .cloned()
            .ok_or_else(|| {
                Self::agent_fault("not_exact", "Object has no original B-Rep surfaces")
            })?;
        if !input.distance.is_finite()
            || !input.tolerance.is_finite()
            || !(1e-9..=1.0).contains(&input.tolerance)
            || input.distance.abs() <= input.tolerance
            || input.faces.iter().any(|face| *face >= shape.face_count)
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "Invalid thickness, tolerance or topology face index",
            ));
        }
        self.deactivate_sidebar_tools();
        let job_id = format!(
            "offset-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.offset_ctx = Some(OffsetContext {
            equation_extrude: false,
            agent_job: Some(job_id.clone()),
            source: input.body_id,
            input: shape,
            distance: input.distance,
            tolerance: input.tolerance,
            faces: input.faces,
            operation: input.mode,
            keep_original: !input.hide_original,
            preview: None,
            error: None,
            job: None,
            started: None,
        });
        self.mode = Mode::OffsetShell;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.apply_offset_preview();
        Ok(json!({"job_id":job_id,"status":"computing","committed":false}))
    }

    pub(super) fn agent_offset_job(&mut self, method: &str, params: Value) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Job {
            job_id: String,
        }
        let input = Self::agent_parse::<Job>(params)?;
        self.poll_offset_preview();
        let ctx = self
            .offset_ctx
            .as_ref()
            .filter(|ctx| ctx.agent_job.as_ref() == Some(&input.job_id))
            .ok_or_else(|| Self::agent_fault("not_found", "No matching Agent-owned offset job"))?;
        if method == "feature.offset.status" {
            return Ok(
                json!({"job_id":input.job_id,"status":if ctx.job.is_some(){"computing"}else if ctx.preview.is_some(){"ready"}else{"failed"},"error":ctx.error,"committed":false}),
            );
        }
        if method == "feature.offset.cancel" {
            self.cancel_offset_shell();
            return Ok(json!({"cancelled":true}));
        }
        if ctx.preview.is_none() {
            return Err(Self::agent_fault(
                "not_ready",
                "Offset preview is not ready to commit",
            ));
        }
        self.commit_offset_shell();
        if self.mode != Mode::Idle {
            return Err(Self::agent_fault(
                "geometry_changed",
                "Source changed; preview not committed",
            ));
        }
        self.agent_body_value_by_id(self.selected[0])
    }

    pub fn start_offset_shell(&mut self) {
        if !self.finish_creation_for_tool_switch() || self.mode != Mode::Idle {
            return;
        }
        let Some(id) = self
            .selected
            .first()
            .copied()
            .filter(|_| self.selected.len() == 1)
        else {
            self.toast(self.settings.language.text(
                "Select one exact STEP object first",
                "请先选择一个保留精确曲面的 STEP 对象",
            ));
            return;
        };
        let Some(input) = self
            .body(id)
            .and_then(|b| b.shape)
            .and_then(|id| self.kernel.exact_shape(id))
            .cloned()
        else {
            self.toast(self.settings.language.text(
                "Mesh-only object: re-import the original STEP for exact offset",
                "对象只有网格；精确偏移需要重新导入原始 STEP",
            ));
            return;
        };
        self.deactivate_sidebar_tools();
        self.face_pick = None;
        let operation = if input.solid_count == 1 {
            OffsetMode::HollowSolid
        } else {
            OffsetMode::ThickenSurface
        };
        self.offset_ctx = Some(OffsetContext {
            equation_extrude: false,
            agent_job: None,
            source: id,
            input,
            distance: -1.0,
            tolerance: 1e-6,
            faces: Vec::new(),
            operation,
            keep_original: true,
            preview: None,
            error: None,
            job: None,
            started: None,
        });
        self.mode = Mode::OffsetShell;
        self.numeric_focus_generation = self.numeric_focus_generation.wrapping_add(1);
        self.bump_scene();
    }

    pub fn start_equation_surface_extrude(&mut self) {
        self.start_offset_shell();
        if let Some(ctx) = self.offset_ctx.as_mut() {
            ctx.equation_extrude = true;
            ctx.operation = OffsetMode::SymmetricThickenSurface;
            ctx.distance = 2.0;
            ctx.faces.clear();
        }
        self.apply_offset_preview();
    }

    pub fn apply_offset_preview(&mut self) {
        let Some(ctx) = self.offset_ctx.as_mut() else {
            return;
        };
        if ctx.job.is_some() {
            return;
        }
        let input = ctx.input.clone();
        let faces = ctx.faces.clone();
        let (distance, tolerance, operation) = (ctx.distance, ctx.tolerance, ctx.operation);
        let (tx, rx) = mpsc::channel();
        ctx.job = Some(rx);
        ctx.started = Some(std::time::Instant::now());
        ctx.preview = None;
        ctx.error = None;
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(|| input.offset(distance, tolerance, &faces, operation))
                    .map_err(|_| "Exact geometry worker failed".to_string())
                    .and_then(|value| value.map_err(|e| e.to_string()));
            let _ = tx.send(result);
        });
        self.bump_scene();
    }

    pub fn poll_offset_preview(&mut self) {
        let result = self
            .offset_ctx
            .as_ref()
            .and_then(|ctx| ctx.job.as_ref())
            .map(|rx| rx.try_recv());
        let result = match result {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => {
                Err("Exact geometry worker disconnected".into())
            }
            _ => return,
        };
        if let Some(ctx) = self.offset_ctx.as_mut() {
            ctx.job = None;
            match result {
                Ok(shape) => ctx.preview = Some(shape),
                Err(error) => ctx.error = Some(error),
            }
        }
        self.bump_scene();
    }

    pub fn commit_offset_shell(&mut self) {
        let Some(ctx) = self.offset_ctx.as_ref() else {
            return;
        };
        let Some(preview) = ctx.preview.as_ref() else {
            return;
        };
        let Some(source) = self.doc.body(ctx.source).cloned() else {
            return;
        };
        if self
            .kernel
            .exact_shape(source.shape.unwrap_or(ShapeId(u64::MAX)))
            .is_none_or(|s| s.brep != ctx.input.brep)
        {
            self.toast("Source geometry changed; restart the offset operation");
            return;
        }
        let feature = Feature::ExactOffset {
            input_brep: ctx.input.brep.clone(),
            distance: ctx.distance,
            tolerance: ctx.tolerance,
            faces: ctx.faces.clone(),
            mode: ctx.operation,
        };
        let keep = ctx.keep_original;
        let name = if ctx.equation_extrude {
            "Surface Extrude"
        } else {
            "Offset"
        };
        let source_id = ctx.source;
        let kind = if preview.solid_count == 0 {
            BodyKind::Surface
        } else {
            BodyKind::Solid
        };
        let shape = self.kernel.insert_exact(preview.clone());
        self.begin_document_change();
        let id = self.doc.alloc_id();
        self.doc.add_body(Body {
            id,
            name: format!("{name} {id}"),
            kind,
            feature,
            shape: Some(shape),
            material: source.material,
            visible: true,
        });
        if !keep {
            if let Some(body) = self.doc.body_mut(source_id) {
                body.visible = false;
            }
        }
        self.selected = vec![id];
        self.clear_region_selection();
        self.selected_edge = None;
        self.face_pick = None;
        self.offset_ctx = None;
        self.mode = Mode::Idle;
        self.record_history(name.to_string());
        self.bump_scene();
    }

    pub fn cancel_offset_shell(&mut self) {
        self.pending_tool_exit = None;
        self.offset_ctx = None;
        self.mode = Mode::Idle;
        self.bump_scene();
    }

    pub fn pick_offset_face(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let Some(ctx) = self.offset_ctx.as_ref() else {
            return;
        };
        if ctx.job.is_some() || ctx.equation_extrude {
            return;
        }
        let ndc = [
            (pos.x - rect.left()) / rect.width() * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / rect.height() * 2.0,
        ];
        let (origin, direction) = self.ray_from_ndc(ndc);
        let Some((triangle, _)) = ray_mesh_tri(origin, direction, &ctx.input.mesh) else {
            return;
        };
        let Some(&face) = ctx.input.triangle_faces.get(triangle) else {
            return;
        };
        let ctx = self.offset_ctx.as_mut().unwrap();
        if ctx.faces.contains(&face) {
            ctx.faces.retain(|f| *f != face);
        } else {
            ctx.faces.push(face);
        }
        ctx.preview = None;
        ctx.error = None;
        self.bump_scene();
    }

    pub fn offset_scene(&self, scene: &mut SceneData) {
        let Some(ctx) = self.offset_ctx.as_ref() else {
            return;
        };
        if let Some(preview) = &ctx.preview {
            scene.meshes.push(SceneMesh {
                positions: preview.mesh.positions.clone(),
                normals: preview.mesh.vertex_normals(),
                indices: preview.mesh.indices.clone(),
                color: [0.55, 0.78, 0.86],
                highlight: false,
            });
        } else {
            let indices = ctx
                .input
                .mesh
                .indices
                .chunks_exact(3)
                .zip(&ctx.input.triangle_faces)
                .filter(|(_, face)| ctx.faces.contains(face))
                .flat_map(|(tri, _)| tri.iter().copied())
                .collect();
            scene.planes.push(ScenePlane {
                positions: ctx.input.mesh.positions.clone(),
                indices,
                color: [1.0, 0.45, 0.10],
                opacity: 0.55,
            });
        }
    }
}

pub fn panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let language = app.settings.language;
    let unit = app.settings.unit;
    let Some(ctx) = app.offset_ctx.as_mut() else {
        return;
    };
    ui.heading(if ctx.equation_extrude {
        language.text("Extrude surface", "曲面拉伸")
    } else {
        language.text("Exact Offset / Shell", "精确偏移 / 抽壳")
    });
    ui.label(if ctx.equation_extrude {
        language.text(
            "Thickness follows local surface normals; symmetric gives half on each side.",
            "沿曲面各处法线加厚；对称时两侧各占一半厚度。",
        )
    } else {
        language.text(
            "Click faces to toggle removal. Negative thickness points inward.",
            "单击面切换移除选择；负厚度向内。",
        )
    });
    let mut changed = false;
    ui.add_enabled_ui(ctx.job.is_none(), |ui| {
        if ctx.equation_extrude {
            let mut symmetric = ctx.operation == OffsetMode::SymmetricThickenSurface;
            if ui
                .checkbox(&mut symmetric, language.text("Symmetric", "对称"))
                .changed()
            {
                ctx.operation = if symmetric {
                    OffsetMode::SymmetricThickenSurface
                } else {
                    OffsetMode::ThickenSurface
                };
                changed = true;
            }
        } else {
            for (mode, en, zh) in [
                (OffsetMode::SurfaceOffset, "Offset surface", "曲面偏移"),
                (OffsetMode::HollowSolid, "Hollow solid", "实体抽壳"),
                (
                    OffsetMode::ThickenSurface,
                    "Thicken open surface",
                    "开放曲面加厚",
                ),
            ] {
                changed |= ui
                    .selectable_value(&mut ctx.operation, mode, language.text(en, zh))
                    .changed();
            }
        }
        let factor = unit.millimeters_per_unit();
        let mut distance = ctx.distance / factor;
        ui.horizontal(|ui| {
            ui.label(language.text("Thickness", "厚度"));
            if crate::ui::tool_numeric(ui, egui::DragValue::new(&mut distance).speed(0.1)).changed()
            {
                ctx.distance = distance * factor;
                changed = true;
            }
            ui.label(unit.symbol());
        });
        ui.horizontal(|ui| {
            ui.label(language.text("Tolerance", "容差"));
            let mut tolerance = ctx.tolerance / factor;
            if crate::ui::tool_numeric(
                ui,
                egui::DragValue::new(&mut tolerance)
                    .range(1e-9 / factor..=1.0 / factor)
                    .max_decimals(9)
                    .speed(1e-6 / factor),
            )
            .changed()
            {
                ctx.tolerance = tolerance * factor;
                changed = true;
            }
            ui.label(unit.symbol());
        });
        ui.checkbox(
            &mut ctx.keep_original,
            language.text("Keep original", "保留原物体"),
        );
        if ctx.operation == OffsetMode::HollowSolid {
            ui.label(language.text("Faces to remove", "需要移除的面"));
            egui::ScrollArea::vertical()
                .id_salt("offset-faces")
                .max_height(160.0)
                .show(ui, |ui| {
                    for face in 0..ctx.input.face_count {
                        let mut selected = ctx.faces.contains(&face);
                        if ui
                            .checkbox(
                                &mut selected,
                                format!("{} {}", language.text("Face", "面"), face + 1),
                            )
                            .changed()
                        {
                            if selected {
                                ctx.faces.push(face);
                            } else {
                                ctx.faces.retain(|f| *f != face);
                            }
                            changed = true;
                        }
                    }
                });
        }
    });
    if changed {
        ctx.preview = None;
        ctx.error = None;
    }
    if let Some(error) = &ctx.error {
        ui.colored_label(egui::Color32::from_rgb(200, 70, 60), error);
    }
    let busy = ctx.job.is_some();
    let ready = ctx.preview.is_some();
    if busy {
        ui.spinner();
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    if ui
        .add_enabled(
            !busy,
            egui::Button::new(language.text("Apply preview", "应用预览")),
        )
        .clicked()
    {
        app.apply_offset_preview();
    }
    if ui
        .add_enabled(ready, egui::Button::new(language.text("Done", "完成")))
        .clicked()
    {
        app.commit_offset_shell();
    }
    if ui
        .button(language.text("Cancel (Esc)", "取消（Esc）"))
        .clicked()
    {
        app.cancel_offset_shell();
    }
    if changed {
        app.bump_scene();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_plane_picking_uses_native_face_and_pocket_commits_with_undo() {
        let (mut app, id) = app_with_exact_box();
        let source = app.body(id).unwrap().shape.unwrap();
        let display = app.mesh_for(source);
        let native = app.kernel.exact_shape(source).unwrap();
        assert_eq!(display.indices, native.mesh.indices);
        assert_eq!(display.vertex_normals, native.mesh.vertex_normals);
        app.camera.eye = glam::Vec3::new(10.0, 15.0, 100.0);
        app.camera.target = glam::Vec3::new(10.0, 15.0, 40.0);
        app.camera.up = glam::Vec3::Y;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let (hit, _) = app
            .pick_face(rect.center(), rect)
            .expect("planar face pick");
        let face = exact_face_for_hit(app.kernel.exact_shape(source).unwrap(), &display, &hit)
            .expect("complete native face");
        assert!(app.kernel.exact_shape(source).unwrap().planar_faces[face as usize]);
        app.selected = vec![id];
        app.face_pick = Some(hit);
        app.start_pocket();
        app.set_extrude_depth(3.0);
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.commit_extrude();
        assert!(app.face_extrude_job.is_some());
        assert_eq!(app.body(id).unwrap().shape, Some(source));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while app.face_extrude_job.is_some() {
            app.poll_exact_face_commit();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let result = app.body(id).unwrap().shape.unwrap();
        assert_ne!(source, result);
        let bounds = app.kernel.tessellate(result).unwrap().bounds().unwrap();
        assert!((bounds.1.z - 37.0).abs() < 1e-4);
        assert!(app.undo());
        assert_eq!(app.body(id).unwrap().shape, Some(source));
        assert!(app.redo());
        assert_eq!(app.body(id).unwrap().shape, Some(result));
    }

    #[test]
    fn step_import_commit_is_atomic_and_cancel_or_new_document_discards_late_results() {
        let (mut app, id) = app_with_exact_box();
        let input = app
            .kernel
            .exact_shape(app.body(id).unwrap().shape.unwrap())
            .unwrap()
            .clone();
        let (tx, receiver) = mpsc::channel();
        app.step_import_job = Some(StepImportJob {
            path: "fixture.step".into(),
            started: std::time::Instant::now(),
            receiver,
            read_progress: Default::default(),
        });
        assert!(app.agent_require_idle().is_err());
        app.poll_step_import();
        assert_eq!(app.doc.bodies.len(), 1);
        tx.send(Ok(vec![input.clone(), input.clone()])).unwrap();
        app.poll_step_import();
        assert_eq!(app.doc.bodies.len(), 3);
        assert_eq!(app.selected.len(), 2);
        assert!(app.undo());
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(app.redo());
        assert_eq!(app.doc.bodies.len(), 3);
        let (tx, receiver) = mpsc::channel();
        app.step_import_job = Some(StepImportJob {
            path: "late.step".into(),
            started: std::time::Instant::now(),
            receiver,
            read_progress: Default::default(),
        });
        app.new_project();
        assert!(tx.send(Ok(vec![input])).is_err());
        app.poll_step_import();
        assert!(app.doc.bodies.is_empty());
    }

    #[test]
    fn exact_face_job_failure_or_changed_source_never_commits() {
        let (mut app, id) = app_with_exact_box();
        let source = app.body(id).unwrap().shape.unwrap();
        let display = app.mesh_for(source);
        let triangles: Vec<_> = display
            .indices
            .chunks_exact(3)
            .filter(|t| {
                t.iter()
                    .all(|&i| (display.positions[i as usize][2] - 40.0).abs() < 1e-5)
            })
            .flatten()
            .copied()
            .collect();
        app.selected = vec![id];
        app.face_pick = Some(FaceHit {
            body_id: id,
            normal: [0.0, 0.0, 1.0],
            center: [10.0, 15.0, 40.0],
            area_mm2: 600.0,
            triangles,
        });
        app.start_pocket();
        let context = app.extrude_ctx.take().unwrap();
        assert!(context.exact_face.is_some());
        assert!(context.preview_error.is_none());
        for &ghost in &context.ghost_shapes {
            assert!(
                app.kernel.exact_shape(ghost).is_none(),
                "Preview must not run the native builder"
            );
            assert!(!app.kernel.tessellate(ghost).unwrap().is_empty());
        }
        let (tx, receiver) = mpsc::channel();
        app.face_extrude_job = Some(FaceExtrudeJob {
            context: context.clone(),
            source,
            target: id,
            started: std::time::Instant::now(),
            receiver,
        });
        tx.send(Err("deliberate failure".into())).unwrap();
        app.poll_exact_face_commit();
        assert_eq!(app.body(id).unwrap().shape, Some(source));
        let (tx, receiver) = mpsc::channel();
        let input = app.kernel.exact_shape(source).unwrap().clone();
        app.face_extrude_job = Some(FaceExtrudeJob {
            context,
            source,
            target: id,
            started: std::time::Instant::now(),
            receiver,
        });
        let replacement = app.kernel.insert_exact(input.clone());
        app.doc.body_mut(id).unwrap().shape = Some(replacement);
        tx.send(Ok(input)).unwrap();
        app.poll_exact_face_commit();
        assert_eq!(app.body(id).unwrap().shape, Some(replacement));
    }

    fn app_with_exact_box() -> (S33DApp, BodyId) {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let shape = app
            .kernel
            .extrude_region(
                &Region2D::new(vec![[0.0, 0.0], [20.0, 0.0], [20.0, 30.0], [0.0, 30.0]]),
                &PlaneDef::from_normal([0.0, 0.0, 1.0]),
                40.0,
            )
            .unwrap();
        let exact = ExactShape::from_faceted_mesh(&app.kernel.tessellate(shape).unwrap()).unwrap();
        let shape = app.kernel.insert_exact(exact);
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Test source".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "test".into(),
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        (app, id)
    }
    #[test]
    fn agent_offset_job_is_owned_cancelable_and_commits_only_when_ready() {
        let (mut app, id) = app_with_exact_box();
        let params =
            json!({"body_id":id,"mode":"HollowSolid","distance":-1.0,"tolerance":1e-6,"faces":[0]});
        let start = app.agent_offset_start(params.clone()).unwrap();
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(app
            .agent_offset_job("feature.offset.cancel", json!({"job_id":"wrong"}))
            .is_err());
        app.agent_offset_job("feature.offset.cancel", json!({"job_id":start["job_id"]}))
            .unwrap();
        assert_eq!(app.mode, Mode::Idle);
        assert_eq!(app.doc.bodies.len(), 1);
        let start = app.agent_offset_start(params).unwrap();
        let job = json!({"job_id":start["job_id"]});
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let state = app
                .agent_offset_job("feature.offset.status", job.clone())
                .unwrap();
            if state["status"] == "ready" {
                break;
            }
            assert_ne!(state["status"], "failed", "{state}");
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.doc.bodies.len(), 1);
        app.agent_offset_job("feature.offset.commit", job).unwrap();
        assert_eq!(app.doc.bodies.len(), 2);
        assert!(app.doc.undo());
        assert_eq!(app.doc.bodies.len(), 1);
    }
}
