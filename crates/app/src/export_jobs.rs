//! Background exports publish their staged file only on the owning UI thread.
use super::*;
use s33d_kernel::export_file::StagedExport;
use std::{sync::mpsc, time::Instant};

pub(crate) struct ExportJob {
    started: Instant,
    receiver: mpsc::Receiver<Result<StagedExport, String>>,
    source_bodies: Vec<Body>,
    source_history: Vec<String>,
    path: PathBuf,
    body_id: BodyId,
    format: String,
    pub(super) pending_agent: Option<crate::agent_protocol::PendingAgentRequest>,
}

fn export_shape(
    kernel: &TruckKernel,
    shape: ShapeId,
    format: &str,
    path: &Path,
) -> Result<(), String> {
    let path = path.to_str().ok_or("Invalid export path")?;
    let result = match format {
        "step" | "stp" => kernel.export_step(shape, path),
        "stl" => kernel.export_stl(shape, path),
        "obj" => kernel.export_obj(shape, path),
        "ply" => kernel.export_ply(shape, path),
        "glb" | "gltf" => kernel.export_glb(shape, path),
        _ => return Err(format!("Unsupported export format: {format}")),
    };
    result.map_err(|e| e.to_string())
}

impl S33DApp {
    pub(crate) fn start_export_job(
        &mut self,
        body: BodyId,
        format: &str,
        path: PathBuf,
    ) -> Result<(), String> {
        let format_owned = format.to_ascii_lowercase();
        let worker_format = format_owned.clone();
        self.start_export_work(body, format_owned, path, move |kernel, shape, path| {
            export_shape(kernel, shape, &worker_format, path)
        })
    }

    fn start_export_work(
        &mut self,
        body_id: BodyId,
        format: String,
        path: PathBuf,
        work: impl FnOnce(&TruckKernel, ShapeId, &Path) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        if self.progress_busy() || self.surface_preview_pending() {
            return Err("Wait for the current operation / 请等待当前操作完成".into());
        }
        let shape = self
            .doc
            .body(body_id)
            .and_then(|b| b.shape)
            .ok_or("Select a solid body / 请选择实体")?;
        let kernel = self.kernel.clone();
        let destination = path.clone();
        let (sender, receiver) = mpsc::channel();
        self.export_job = Some(ExportJob {
            started: Instant::now(),
            receiver,
            source_bodies: self.doc.bodies.clone(),
            source_history: self.history.clone(),
            path,
            body_id,
            format,
            pending_agent: None,
        });
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let stage = StagedExport::new(&destination).map_err(|e| e.to_string())?;
                work(&kernel, shape, stage.path())?;
                Ok(stage)
            }))
            .unwrap_or_else(|_| Err("Export calculation failed / 导出计算失败".into()));
            // Receiver drop (cancel/reset/close) drops the staged file too.
            let _ = sender.send(result);
        });
        Ok(())
    }

    pub(super) fn queue_agent_export_job(&mut self, request: &AgentRequest) -> bool {
        if request.method != "body.export"
            || !self.background_modeling
            || self.progress_busy()
            || self.tool_exit_prompt.is_some()
            || self.project_session.pending.is_some()
        {
            return false;
        }
        let Ok(input) = parse_params::<ExportParams>(request.params.clone()) else {
            return false;
        };
        let path = PathBuf::from(&input.path);
        let Some(format) = input
            .format
            .or_else(|| path.extension().and_then(|e| e.to_str()).map(str::to_owned))
        else {
            return false;
        };
        if !matches!(
            format.to_ascii_lowercase().as_str(),
            "step" | "stp" | "stl" | "obj" | "ply" | "glb" | "gltf"
        ) {
            return false;
        }
        self.start_export_job(input.body_id, &format, path).is_ok()
    }

    pub(crate) fn cancel_export_job(&mut self) {
        if let Some(job) = self.export_job.take() {
            if let Some(pending) = job.pending_agent {
                let id = pending.request.id.clone();
                pending.respond(AgentResponse::failure(
                    id,
                    "cancelled",
                    "Export cancelled; destination unchanged",
                ));
            }
        }
    }

    pub(crate) fn poll_export_job(&mut self) {
        let Some(job) = &self.export_job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Export worker disconnected / 导出进程中断".into()),
        };
        let job = self.export_job.take().unwrap();
        let current = job.source_bodies == self.doc.bodies && job.source_history == self.history;
        let result = result.and_then(|stage| {
            if !current {
                return Err("Document changed; export again / 项目已改变，请重新导出".into());
            }
            stage.commit().map_err(|e| e.to_string())
        });
        if let Some(pending) = job.pending_agent {
            let id = pending.request.id.clone();
            pending.respond(match result {
                Ok(())=>AgentResponse::success(id,json!({"exported":true,"body_id":job.body_id,"format":job.format,"path":job.path})),
                Err(message)=>AgentResponse::failure(id,"export_error",message),
            });
        } else {
            let language = self.settings.language;
            self.toast(match result {
                Ok(()) => format!(
                    "{} {}",
                    language.text("Exported", "已导出"),
                    job.path.display()
                ),
                Err(error) => format!("{}: {error}", language.text("Export failed", "导出失败")),
            });
        }
    }

    pub(crate) fn export_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.export_job else {
            return;
        };
        let language = self.settings.language;
        if model_progress::progress_modal(
            ctx,
            language,
            ("Export", "导出"),
            (
                "Validating geometry and writing file…",
                "正在检查几何并写出文件…",
            ),
            None,
            job.started.elapsed().as_secs_f64(),
        ) {
            self.cancel_export_job();
            self.toast(language.text(
                "Export cancelled; destination unchanged",
                "已取消导出，目标文件未改变",
            ));
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[cfg(test)]
#[path = "export_jobs_qa.rs"]
mod tests;
