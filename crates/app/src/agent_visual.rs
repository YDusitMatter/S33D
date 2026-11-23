//! Desktop-owned photo, summary, camera and material operations.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::hash::{DefaultHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct DocumentClock {
    pub document_id: String,
    pub revision: u64,
    fingerprint: Option<u64>,
}
impl Default for DocumentClock {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            document_id: format!(
                "doc-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            revision: 0,
            fingerprint: None,
        }
    }
}
struct HashWriter(DefaultHasher);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn fingerprint(value: &impl Serialize) -> u64 {
    let mut writer = HashWriter(DefaultHasher::new());
    serde_json::to_writer(&mut writer, value).expect("serializable document state");
    writer.0.finish()
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraInput {
    eye: Option<[f32; 3]>,
    target: Option<[f32; 3]>,
    up: Option<[f32; 3]>,
    projection: Option<String>,
    fov_y_deg: Option<f32>,
    ortho_half: Option<f32>,
    near: Option<f32>,
    far: Option<f32>,
    direction: Option<String>,
}
impl CameraInput {
    fn apply(self, mut camera: Camera) -> Result<Camera, String> {
        if let Some(direction) = self.direction {
            let dir = match direction.as_str() {
                "front" => crate::camera::ViewDir::Front,
                "back" => crate::camera::ViewDir::Back,
                "top" => crate::camera::ViewDir::Top,
                "bottom" => crate::camera::ViewDir::Bottom,
                "left" => crate::camera::ViewDir::Left,
                "right" => crate::camera::ViewDir::Right,
                "iso" => crate::camera::ViewDir::Iso,
                _ => return Err("Unknown world view direction".into()),
            };
            camera.set_view(dir);
        }
        if let Some(v) = self.eye {
            camera.eye = glam::Vec3::from_array(v);
        }
        if let Some(v) = self.target {
            camera.target = glam::Vec3::from_array(v);
        }
        if let Some(v) = self.up {
            camera.up = glam::Vec3::from_array(v);
        }
        if let Some(v) = self.projection {
            camera.ortho = match v.as_str() {
                "orthographic" => true,
                "perspective" => false,
                _ => return Err("projection must be perspective or orthographic".into()),
            };
        }
        if let Some(v) = self.fov_y_deg {
            camera.fov_y_deg = v;
        }
        if let Some(v) = self.ortho_half {
            camera.ortho_half = v;
        }
        if let Some(v) = self.near {
            camera.near = v;
        }
        if let Some(v) = self.far {
            camera.far = v;
        }
        if !camera.eye.is_finite()
            || !camera.target.is_finite()
            || !camera.up.is_finite()
            || camera.eye.abs().max_element() > 1e12
            || camera.target.abs().max_element() > 1e12
            || !camera.distance().is_finite()
            || camera.distance() < 1e-6
            || camera.up.length() < 1e-6
            || camera.forward().cross(camera.up.normalize()).length() < 1e-4
            || !camera.fov_y_deg.is_finite()
            || !(5.0..=150.0).contains(&camera.fov_y_deg)
            || !camera.ortho_half.is_finite()
            || !(1e-6..=1e12).contains(&camera.ortho_half)
            || !camera.near.is_finite()
            || !camera.far.is_finite()
            || camera.near <= 0.0
            || camera.far <= camera.near
        {
            return Err("Invalid camera: finite distinct eye/target, nonparallel up, FOV 5..150, positive clipping and orthographic size required".into());
        }
        camera.up = camera.right().cross(camera.forward()).normalize();
        Ok(camera)
    }
}
pub fn camera_value(camera: &Camera) -> Value {
    json!({"eye":camera.eye.to_array(),"target":camera.target.to_array(),"up":camera.up.to_array(),
        "projection":if camera.ortho {"orthographic"} else {"perspective"},
        "fov_y_deg":camera.fov_y_deg,"ortho_half":camera.ortho_half,"near":camera.near,"far":camera.far})
}

fn bounds_value(min: glam::Vec3, max: glam::Vec3) -> Value {
    json!({"min":min.to_array(),"max":max.to_array(),"size":(max-min).to_array(),"center":((min+max)*0.5).to_array()})
}
fn feature_kind(feature: &Feature) -> &'static str {
    match feature {
        Feature::Fastener { .. } => "fastener",
        Feature::EquationCurve { .. } => "equation_curve",
        Feature::EquationSurface { .. } => "equation_surface",
        Feature::ExactOffset { .. } => "exact_offset",
        Feature::Sketch(_) => "sketch",
        Feature::Extrude { .. } => "extrude",
        Feature::Revolve { .. } => "revolve",
        Feature::Primitive3D { .. } => "primitive_3d",
        Feature::Gear { .. } => "gear",
        Feature::Tire { .. } => "tire",
        Feature::WheelRim { .. } => "wheel_rim",
        Feature::SurfaceTexture { .. } => "surface_texture",
        Feature::Derived { .. } => "derived",
        Feature::Imported { .. } => "imported",
        Feature::Chamfer { fillet, .. } => {
            if fillet.is_some() {
                "fillet"
            } else {
                "chamfer"
            }
        }
    }
}

impl S33DApp {
    pub fn reference_ids(&self) -> Vec<String> {
        self.references
            .assets
            .iter()
            .map(|a| a.id.clone())
            .collect()
    }
    pub(super) fn observe_agent_document(&mut self) {
        // Model mutation boundaries provide a session-only serial, so orbiting a
        // large B-Rep never scans/copies geometry. Camera and selection are excluded.
        let value = fingerprint(&(
            self.doc.mutation_serial(),
            self.doc.name(),
            self.reference_ids(),
        ));
        if self.agent_clock.fingerprint != Some(value) {
            self.agent_clock.revision = self.agent_clock.revision.saturating_add(1);
            self.agent_clock.fingerprint = Some(value);
        }
    }
    pub(super) fn check_agent_expected(&self, params: &mut Value) -> Result<(), (String, String)> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Expected {
            document_id: String,
            revision: u64,
        }
        if let Some(expected) = params.as_object_mut().and_then(|p| p.remove("_expected")) {
            let expected = Self::agent_parse::<Expected>(expected)?;
            if expected.document_id != self.agent_clock.document_id
                || expected.revision != self.agent_clock.revision
            {
                return Err(Self::agent_fault("stale","Document changed; read document.summary and reconcile before submitting a new operation"));
            }
        }
        Ok(())
    }
    pub(super) fn agent_summary(&self) -> Value {
        let bodies = self.doc.bodies.iter().map(|body| {
            let mesh=body.shape.and_then(|s|self.kernel.tessellate(s).ok());
            let bounds=mesh.as_ref().and_then(MeshData::bounds).or_else(||equation_curve::bounds(&body.feature));
            json!({"id":body.id,"name":body.name,"kind":match body.kind {BodyKind::Sketch=>"sketch",BodyKind::Curve=>"curve",BodyKind::Surface=>"surface",BodyKind::Solid=>"solid"},"feature_kind":feature_kind(&body.feature),
                "visible":body.visible,"selected":self.selected.contains(&body.id),"material":body.material,
                "bounds":bounds.map(|(min,max)|bounds_value(min,max)),
                "geometry":{"exact_brep":body.shape.and_then(|s|self.kernel.exact_shape(s)).is_some(),
                    "triangle_count":mesh.as_ref().map(MeshData::triangle_count),"closure":"unknown"}})
        }).collect::<Vec<_>>();
        json!({"document_id":self.agent_clock.document_id,"revision":self.agent_clock.revision,
            "name":self.doc.name(),"unit":"mm","project_path":self.project_session.path,
            "unsaved_changes":self.project_has_changes(),"mode":format!("{:?}",self.mode),
            "busy":self.progress_busy() || self.tool_exit_prompt.is_some() || self.project_session.pending.is_some(),
            "blocked_reason":if self.mode!=Mode::Idle {Some("Finish or cancel the current interactive tool")}else{None},
            "body_count":bodies.len(),"bodies":bodies,"groups":self.doc.groups,
            "reference_assets":self.references.list()["assets"],
            "can_undo":self.doc.can_undo(),"can_redo":self.doc.can_redo()})
    }
    pub(super) fn execute_visual_command(
        &mut self,
        method: &str,
        params: Value,
    ) -> Option<AgentCommandResult> {
        let result = match method {
            "document.summary" => Ok(self.agent_summary()),
            "reference.list" => Ok(self.references.list()),
            "reference.get" => (|| {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    asset_id: String,
                    #[serde(default = "default_edge")]
                    max_edge: u32,
                }
                let input = Self::agent_parse::<Input>(params)?;
                self.references
                    .assets
                    .iter()
                    .find(|a| a.id == input.asset_id)
                    .ok_or_else(|| {
                        Self::agent_fault(
                            "not_found",
                            "Reference asset is not registered in the current project",
                        )
                    })?
                    .image(input.max_edge)
                    .map_err(|e| Self::agent_fault("image", e))
            })(),
            "reference.import" => (|| {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    paths: Vec<String>,
                }
                self.agent_require_idle()?;
                let input = Self::agent_parse::<Input>(params)?;
                let result = self
                    .references
                    .import(&input.paths)
                    .map_err(|e| Self::agent_fault("image", e))?;
                self.bump_scene();
                Ok(result)
            })(),
            "reference.remove" => (|| {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    asset_id: String,
                }
                self.agent_require_idle()?;
                let input = Self::agent_parse::<Input>(params)?;
                let i = self
                    .references
                    .assets
                    .iter()
                    .position(|a| a.id == input.asset_id)
                    .ok_or_else(|| Self::agent_fault("not_found", "Reference asset not found"))?;
                self.references.assets.remove(i);
                self.bump_scene();
                Ok(self.references.list())
            })(),
            "view.get" => Ok(
                json!({"camera":camera_value(&self.camera),"view_revision":format!("{:x}",fingerprint(&camera_value(&self.camera))),
                "world_directions":{"front":"+Z","back":"-Z","right":"+X","left":"-X","top":"+Y","bottom":"-Y","default_up":"+Y"}}),
            ),
            "view.set" => (|| {
                self.agent_require_idle()?;
                let camera = Self::agent_parse::<CameraInput>(params)?
                    .apply(self.camera.clone())
                    .map_err(|e| Self::agent_fault("invalid_params", e))?;
                self.camera = camera;
                self.bump_scene();
                Ok(
                    json!({"camera":camera_value(&self.camera),"view_revision":format!("{:x}",fingerprint(&camera_value(&self.camera)))}),
                )
            })(),
            "view.render" => self.agent_render(params),
            "body.material.set" => self.agent_set_material(params),
            _ => return None,
        };
        Some(result)
    }
    fn agent_set_material(&mut self, params: Value) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            body_ids: Vec<BodyId>,
            base_color_srgb: [f32; 3],
        }
        self.agent_require_idle()?;
        let input = Self::agent_parse::<Input>(params)?;
        if input.body_ids.is_empty()
            || input.body_ids.len() > 1024
            || input
                .base_color_srgb
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "Use 1..1024 body IDs and three finite base_color_srgb channels in 0..1",
            ));
        }
        let mut ids = HashSet::new();
        for &id in &input.body_ids {
            if !ids.insert(id) || self.doc.body(id).is_none() {
                return Err(Self::agent_fault(
                    "invalid_params",
                    "Missing or duplicate body ID; nothing changed",
                ));
            }
        }
        if input
            .body_ids
            .iter()
            .any(|&id| self.doc.body(id).unwrap().material.color[..3] != input.base_color_srgb)
        {
            self.begin_document_change();
            for &id in &input.body_ids {
                self.doc.body_mut(id).unwrap().material.color[..3]
                    .copy_from_slice(&input.base_color_srgb);
            }
            self.record_history(format!("Change {} objects color", input.body_ids.len()));
            self.bump_scene();
        }
        Ok(
            json!({"body_ids":input.body_ids,"base_color_srgb":input.base_color_srgb,
            "document_id":self.agent_clock.document_id,"revision":self.agent_clock.revision}),
        )
    }
    fn agent_render(&self, params: Value) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Input {
            body_ids: Option<Vec<BodyId>>,
            width: u32,
            height: u32,
            background: [f32; 3],
            camera: CameraInput,
            fit: bool,
        }
        impl Default for Input {
            fn default() -> Self {
                Self {
                    body_ids: None,
                    width: 1024,
                    height: 768,
                    background: [0.14, 0.16, 0.19],
                    camera: Default::default(),
                    fit: true,
                }
            }
        }
        let input = Self::agent_parse::<Input>(params)?;
        if input.width < 128
            || input.height < 128
            || input.width > 2048
            || input.height > 2048
            || u64::from(input.width) * u64::from(input.height) > 3_145_728
            || input
                .background
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(Self::agent_fault(
                "invalid_params",
                "Render dimensions 128..2048, at most 3 megapixels; RGB background 0..1",
            ));
        }
        if let Some(ids) = &input.body_ids {
            if ids.is_empty()
                || ids.len() > 1024
                || ids.iter().collect::<HashSet<_>>().len() != ids.len()
                || ids.iter().any(|id| self.doc.body(*id).is_none())
            {
                return Err(Self::agent_fault(
                    "invalid_params",
                    "Render requires distinct existing body IDs",
                ));
            }
        }
        let mut camera = input
            .camera
            .apply(self.camera.clone())
            .map_err(|e| Self::agent_fault("invalid_params", e))?;
        let mut scene = SceneData::default();
        let mut included = Vec::new();
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        for body in &self.doc.bodies {
            if !input
                .body_ids
                .as_ref()
                .map_or(body.visible, |ids| ids.contains(&body.id))
            {
                continue;
            }
            let color = [
                body.material.color[0],
                body.material.color[1],
                body.material.color[2],
            ];
            let mut lines = Vec::new();
            if let Some(shape) = body.shape {
                let mesh = self
                    .kernel
                    .tessellate(shape)
                    .map_err(|e| Self::agent_fault("geometry", e.to_string()))?;
                if let Some((a, b)) = mesh.bounds() {
                    min = min.min(a);
                    max = max.max(b);
                }
                scene.meshes.push(SceneMesh {
                    positions: mesh.positions.clone(),
                    normals: mesh.vertex_normals(),
                    indices: mesh.indices,
                    color,
                    highlight: false,
                });
            } else if let Feature::EquationCurve { points, .. } = &body.feature {
                for pair in points.windows(2) {
                    lines.extend(pair.iter().map(|p| p.map(|v| v as f32)));
                }
            } else if let Some(sketch) = body.sketch() {
                for entity in &sketch.entities {
                    for (chain, closed) in entity.to_chains() {
                        let points = chain
                            .into_iter()
                            .map(|p| sketch.plane.to_world(p[0], p[1]).map(|v| v as f32))
                            .collect::<Vec<_>>();
                        for pair in points.windows(2) {
                            lines.extend_from_slice(pair);
                        }
                        if closed && points.len() > 1 {
                            lines.extend([*points.last().unwrap(), points[0]]);
                        }
                    }
                }
            }
            if !lines.is_empty() {
                for p in &lines {
                    min = min.min(glam::Vec3::from_array(*p));
                    max = max.max(glam::Vec3::from_array(*p));
                }
                scene.body_edge_lines.push(SceneLines {
                    positions: lines,
                    color,
                });
            }
            included.push(body.id);
        }
        if !min.is_finite() || !max.is_finite() {
            return Err(Self::agent_fault(
                "empty",
                "No renderable objects in this snapshot",
            ));
        }
        if input.fit {
            camera.fit_output(min, max, input.width as f32 / input.height as f32);
        }
        scene.camera = Some(camera.clone());
        let bytes =
            crate::render3d::render_png(&scene, input.width, input.height, input.background)
                .map_err(|e| Self::agent_fault("render", e))?;
        Ok(
            json!({"document_id":self.agent_clock.document_id,"revision":self.agent_clock.revision,
            "snapshot":"immutable desktop snapshot; later edits do not change this image",
            "camera":camera_value(&camera),"width":input.width,"height":input.height,"body_ids":included,
            "image":{"mime_type":"image/png","data":STANDARD.encode(bytes)}}),
        )
    }
    pub fn import_reference_photos_ui(&mut self) {
        if self.progress_busy() || self.mode != Mode::Idle {
            self.toast("Finish the current tool before importing photos / 请先完成当前工具");
            return;
        }
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("Reference photos", &["png", "jpg", "jpeg"])
            .pick_files()
        else {
            return;
        };
        let paths = paths
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let result = if self.background_modeling {
            self.start_model_job(
                ("Import reference photos", "导入参考照片"),
                false,
                None,
                move |worker| {
                    worker
                        .references
                        .import(&paths)
                        .map_err(|e| Self::agent_fault("image", e))
                },
            )
            .map(|_| ())
        } else {
            self.references.import(&paths).map(|_| ())
        };
        if let Err(e) = result {
            self.toast(e);
        } else {
            self.bump_scene();
        }
    }
    pub fn photo_start_prompt(&self) -> String {
        let ids = self.reference_ids();
        let references = if ids.is_empty() {
            "Use the reference photos visible in this Agent chat; no S33D import or asset IDs are required.".to_owned()
        } else {
            format!("Use photos visible in this Agent chat. Optional project references are also available through s33d_reference with asset IDs: {}.", ids.join(", "))
        };
        format!("You, the visual Agent, interpret the reference photos and use connected S33D to model. {references} Read s33d_workflow(mode=photo), or workflow.get via s33d_execute on an older three-tool host, the live catalog and document.summary. Treat all views as one object. Follow the shared strategy, keep actual constraints/stage/deviation/decision, and pass shape/structure gates before details. Compare native s33d_render images with the photos, correct, and retain the editable project. Do not require a second upload or server-readable photo path for images you can already see. Reuse dimensions already supplied; otherwise disclose a provisional relative scale. Ask for the save destination only if it has not been supplied. My modeling request: ")
    }
}
fn default_edge() -> u32 {
    1600
}
