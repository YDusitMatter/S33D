//! Parametric surfaces: bounded equation sampling -> interpolated OCCT B-Rep.
use super::*;
use s33d_kernel::occt::ExactShape;
use s33d_model::document::EquationSurfaceParams;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

pub struct SurfaceContext {
    pub editing: Option<BodyId>,
    pub params: EquationSurfaceParams,
    pub preview: Option<ExactShape>,
    pub error: Option<String>,
    job: Option<(EquationSurfaceParams, Receiver<Result<ExactShape, String>>)>,
    changed_at: Option<Instant>,
    started: Option<Instant>,
}

fn evaluate(text: &str, u: f64, v: f64, degrees: bool) -> Result<f64, String> {
    if text.len() > 256 {
        return Err("Expression exceeds 256 bytes".into());
    }
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| -> Result<(), String> {
        match word.to_ascii_lowercase().as_str() {
            "u" => out.push_str(&format!("({u:.17})")),
            "v" => out.push_str(&format!("({v:.17})")),
            "t" | "ans" => return Err("Surface parameters are u and v, not t or Ans".into()),
            _ => out.push_str(word),
        };
        word.clear();
        Ok(())
    };
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            word.push(c);
        } else {
            flush(&mut word, &mut out)?;
            out.push(c);
        }
    }
    flush(&mut word, &mut out)?;
    // Replacement can exceed the input byte limit; the shared parser still
    // enforces its own bounded recursion. No code or identifiers are executed.
    crate::plugins::evaluate_surface_expression(&out, degrees)
}

pub fn build(params: &EquationSurfaceParams) -> Result<ExactShape, String> {
    if [params.u_domain, params.v_domain]
        .iter()
        .any(|d| !d[0].is_finite() || !d[1].is_finite() || d[0] >= d[1])
        || !(4..=64).contains(&params.resolution)
        || !params.unit_mm.is_finite()
        || params.unit_mm <= 0.0
        || params.transform.iter().any(|v| !v.is_finite())
    {
        return Err("Invalid surface domain, units, resolution or transform".into());
    }
    let matrix = glam::DMat4::from_cols_array(&params.transform);
    if matrix.determinant().abs() < 1e-12
        || matrix.x_axis.w != 0.0
        || matrix.y_axis.w != 0.0
        || matrix.z_axis.w != 0.0
        || matrix.w_axis.w != 1.0
    {
        return Err("Expected an invertible affine surface transform".into());
    }
    let n = params.resolution as usize;
    let mut points = Vec::with_capacity((n + 1) * (n + 1));
    for i in 0..=n {
        for j in 0..=n {
            let u = params.u_domain[0]
                + (params.u_domain[1] - params.u_domain[0]) * (i as f64 / n as f64);
            let v = params.v_domain[0]
                + (params.v_domain[1] - params.v_domain[0]) * (j as f64 / n as f64);
            let values = params
                .expressions
                .iter()
                .map(|s| evaluate(s, u, v, params.degrees))
                .collect::<Result<Vec<_>, _>>()?;
            let p = if params.spherical {
                if values[0] < 0.0 {
                    return Err("Negative surface radius".into());
                }
                let theta = if params.degrees {
                    values[1].to_radians()
                } else {
                    values[1]
                };
                let phi = if params.degrees {
                    values[2].to_radians()
                } else {
                    values[2]
                };
                glam::DVec3::new(
                    values[0] * phi.sin() * theta.cos(),
                    values[0] * phi.sin() * theta.sin(),
                    values[0] * phi.cos(),
                )
            } else {
                glam::DVec3::new(values[0], values[1], values[2])
            };
            let p = matrix.transform_point3(p * params.unit_mm);
            if !p.is_finite() || p.abs().max_element() > 1e7 {
                return Err("Surface coordinate outside supported range".into());
            }
            points.push(p.to_array());
        }
    }
    ExactShape::from_surface_grid(&points, n + 1, n + 1, params.fill).map_err(|e| e.to_string())
}

impl S33DApp {
    pub(super) fn surface_progress_open(&self) -> bool {
        self.surface_ctx.as_ref().is_some_and(|work| {
            work.job.is_some()
                && work
                    .started
                    .is_some_and(|t| t.elapsed() >= Duration::from_millis(350))
        })
    }

    pub(crate) fn surface_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(work) = self.surface_ctx.as_ref().filter(|c| c.job.is_some()) else {
            return;
        };
        let elapsed = work
            .started
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.);
        if elapsed < 0.35 {
            return;
        }
        if super::model_progress::progress_modal(
            ctx,
            self.settings.language,
            ("Equation surface", "方程曲面"),
            ("Computing preview…", "正在计算预览…"),
            None,
            elapsed,
        ) {
            let work = self.surface_ctx.as_mut().unwrap();
            work.job = None;
            work.changed_at = None;
            self.pending_tool_exit = None;
            self.bump_scene();
        }
    }

    pub(super) fn surface_preview_pending(&self) -> bool {
        self.surface_ctx
            .as_ref()
            .is_some_and(|c| c.job.is_some() || c.changed_at.is_some())
    }
    pub fn start_equation_surface(&mut self, editing: Option<BodyId>) {
        if self.mode == Mode::EquationCurve
            && (editing.is_some() || self.surface_ctx.is_some())
            && !self.finish_creation_for_tool_switch()
        {
            return;
        }
        if self.mode != Mode::EquationCurve
            && (!self.finish_creation_for_tool_switch() || self.mode != Mode::Idle)
        {
            return;
        }
        let params = if let Some(id) = editing {
            match self.body(id).map(|b| &b.feature) {
                Some(Feature::EquationSurface { params }) => params.clone(),
                _ => return,
            }
        } else {
            EquationSurfaceParams {
                unit_mm: self.settings.unit.millimeters_per_unit(),
                ..Default::default()
            }
        };
        self.deactivate_sidebar_tools();
        self.curve_ctx = None;
        self.surface_ctx = Some(SurfaceContext {
            editing,
            params,
            preview: None,
            error: None,
            job: None,
            started: None,
            changed_at: None,
        });
        self.mode = Mode::EquationCurve;
        self.request_surface_preview();
    }
    pub fn request_surface_preview(&mut self) {
        let Some(ctx) = self.surface_ctx.as_mut() else {
            return;
        };
        ctx.preview = None;
        ctx.error = None;
        ctx.changed_at = Some(Instant::now());
        self.bump_scene();
    }
    pub fn poll_surface_preview(&mut self) {
        let Some(ctx) = self.surface_ctx.as_mut() else {
            return;
        };
        let mut refreshed = false;
        if let Some((params, rx)) = &ctx.job {
            let result = match rx.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Disconnected) => Some(Err("Surface worker disconnected".into())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                refreshed = true;
                if *params == ctx.params {
                    match result {
                        Ok(shape) => {
                            ctx.preview = Some(shape);
                            ctx.error = None;
                        }
                        Err(e) => ctx.error = Some(e),
                    }
                }
                ctx.job = None;
            }
        }
        if ctx.job.is_none()
            && ctx
                .changed_at
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(180))
        {
            ctx.changed_at = None;
            let params = ctx.params.clone();
            let worker_params = params.clone();
            let (tx, rx) = mpsc::channel();
            ctx.job = Some((params, rx));
            ctx.started = Some(Instant::now());
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| build(&worker_params))
                    .unwrap_or_else(|_| Err("Surface worker failed".into()));
                let _ = tx.send(result);
            });
        }
        if refreshed {
            self.bump_scene();
        }
    }
    /// Never wait for native geometry on the UI thread when changing tools.
    pub fn finish_equation_surface(&mut self) -> bool {
        if self.surface_ctx.is_none() {
            return false;
        }
        self.poll_surface_preview();
        if self.surface_ctx.as_ref().unwrap().preview.is_none() {
            self.toast(self.settings.language.text(
                "Surface is not ready; finish its preview before switching tools",
                "曲面尚未就绪；请待预览成功后再切换工具",
            ));
            return false;
        }
        self.commit_equation_surface();
        self.mode == Mode::Idle
    }
    pub fn commit_equation_surface(&mut self) {
        let Some(ctx) = self.surface_ctx.as_ref() else {
            return;
        };
        let Some(preview) = ctx.preview.clone() else {
            return;
        };
        let editing = ctx.editing;
        let params = ctx.params.clone();
        if editing.is_some_and(|id| {
            !self
                .doc
                .body(id)
                .is_some_and(|b| matches!(b.feature, Feature::EquationSurface { .. }))
        }) {
            return;
        }
        let kind = if preview.solid_count > 0 {
            BodyKind::Solid
        } else {
            BodyKind::Surface
        };
        let shape = self.kernel.insert_exact(preview);
        self.begin_document_change();
        let id = if let Some(id) = editing {
            let body = self.doc.body_mut(id).unwrap();
            body.feature = Feature::EquationSurface { params };
            body.kind = kind;
            body.shape = Some(shape);
            id
        } else {
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("Equation surface {id}"),
                kind,
                feature: Feature::EquationSurface { params },
                shape: Some(shape),
                material: Material::default(),
                visible: true,
            });
            id
        };
        self.surface_ctx = None;
        self.mode = Mode::Idle;
        self.selected = vec![id];
        self.clear_region_selection();
        self.face_pick = None;
        self.selected_edge = None;
        self.mesh_cache.clear();
        self.bump_scene();
        self.record_history("Equation surface");
    }
    pub fn surface_scene(&self, scene: &mut SceneData) {
        if let Some(shape) = self.surface_ctx.as_ref().and_then(|c| c.preview.as_ref()) {
            scene.meshes.push(SceneMesh {
                positions: shape.mesh.positions.clone(),
                normals: shape.mesh.vertex_normals(),
                indices: shape.mesh.indices.clone(),
                color: [0.60, 0.79, 0.90],
                highlight: false,
            });
        }
    }
    pub fn agent_equation_surface(&mut self, method: &str, params: Value) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            params: EquationSurfaceParams,
            #[serde(default)]
            body_id: Option<BodyId>,
        }
        self.agent_require_idle()?;
        let input = Self::agent_parse::<Input>(params)?;
        if input.body_id.is_some_and(|id| {
            !self
                .body(id)
                .is_some_and(|b| matches!(b.feature, Feature::EquationSurface { .. }))
        }) {
            return Err(Self::agent_fault(
                "invalid_selection",
                "Expected an equation surface",
            ));
        }
        let shape = build(&input.params).map_err(|e| Self::agent_fault("invalid_surface", e))?;
        if method == "surface.equation.preview" {
            return Ok(
                json!({"committed":false,"solid_count":shape.solid_count,"triangle_count":shape.mesh.triangle_count(),"bounds":shape.mesh.bounds().map(|(min,max)|json!({"min":min.to_array(),"max":max.to_array()}))}),
            );
        }
        self.surface_ctx = Some(SurfaceContext {
            editing: input.body_id,
            params: input.params,
            preview: Some(shape),
            error: None,
            job: None,
            started: None,
            changed_at: None,
        });
        self.commit_equation_surface();
        self.agent_body_value_by_id(self.selected[0])
    }
}

pub fn panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    app.poll_surface_preview();
    let lang = app.settings.language;
    let Some(ctx) = app.surface_ctx.as_mut() else {
        return;
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .selectable_value(&mut ctx.params.spherical, false, "XYZ")
            .changed();
        changed |= ui
            .selectable_value(&mut ctx.params.spherical, true, "R, theta, phi")
            .changed();
    });
    ui.label(lang.text(
        "Parameters u, v; use explicit multiplication (*)",
        "参数 u、v；乘法显式输入 *",
    ));
    for i in 0..3 {
        ui.label(if ctx.params.spherical {
            ["R(u,v)", "theta(u,v)", "phi(u,v)"][i]
        } else {
            ["x(u,v)", "y(u,v)", "z(u,v)"][i]
        });
        changed |= ui
            .text_edit_singleline(&mut ctx.params.expressions[i])
            .changed();
    }
    for (label, domain) in [
        ("u", &mut ctx.params.u_domain),
        ("v", &mut ctx.params.v_domain),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            for p in domain {
                changed |= ui.add(egui::DragValue::new(p).speed(0.1)).changed();
            }
        });
    }
    changed |= ui
        .checkbox(
            &mut ctx.params.degrees,
            lang.text("Angles in degrees", "角度使用度"),
        )
        .changed();
    ui.horizontal(|ui| {
        ui.label(lang.text("Expression unit", "表达式单位"));
        changed |= ui
            .selectable_value(&mut ctx.params.unit_mm, 1.0, "mm")
            .changed();
        changed |= ui
            .selectable_value(&mut ctx.params.unit_mm, 25.4, "inch")
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(lang.text("Subdivisions per axis", "各参数细分数"));
        changed |= ui
            .add(egui::DragValue::new(&mut ctx.params.resolution).range(4..=64))
            .changed();
    });
    changed |= ui
        .checkbox(
            &mut ctx.params.fill,
            lang.text("Fill closed surface into solid", "填充闭合曲面为实体"),
        )
        .changed();
    ui.small(lang.text("Fill never caps an open surface. Extrude adds normal thickness, symmetric by default. B-spline interpolation approximates the equation between samples.","Fill 不封口开放曲面。Extrude 沿法线加厚，默认对称。采样点之间使用 B 样条插值近似方程。"));
    if let Some(error) = &ctx.error {
        ui.colored_label(egui::Color32::from_rgb(200, 65, 45), error);
    }
    let ready = ctx.preview.is_some() && !changed;
    if ctx.job.is_some() || ctx.changed_at.is_some() {
        ui.spinner();
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    let apply = ui.button(lang.text("Apply preview", "应用预览")).clicked();
    let done = ui
        .add_enabled(ready, egui::Button::new(lang.text("Done", "完成")))
        .clicked();
    let exit = ui
        .button(lang.text("Exit Draw Equation", "退出方程绘图"))
        .clicked();
    let cancel = ui
        .button(lang.text("Cancel (Esc)", "取消（Esc）"))
        .clicked();
    if changed || apply {
        app.request_surface_preview();
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }
    if done {
        app.commit_equation_surface();
    } else if exit {
        app.finish_equation_surface();
    } else if cancel {
        app.cancel_equation_curve();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_preview_keeps_input_enabled_until_the_progress_dialog_then_cancel_retains_parameters(
    ) {
        let ctx = egui::Context::default();
        let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
        app.start_equation_surface(None);
        assert!(app.surface_preview_pending());
        assert!(!app.progress_busy(), "Debouncing must not interrupt typing");
        let work = app.surface_ctx.as_mut().unwrap();
        let params = work.params.clone();
        let (_sender, receiver) = mpsc::channel();
        work.job = Some((params.clone(), receiver));
        work.changed_at = None;
        work.started = Some(Instant::now());
        assert!(!app.progress_busy(), "Short previews stay editable");
        app.surface_ctx.as_mut().unwrap().started = Some(Instant::now() - Duration::from_secs(1));
        assert!(app.progress_busy());
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| app.surface_progress_modal(ctx),
        );
        assert!(!app.progress_busy());
        assert!(!app.surface_preview_pending());
        assert_eq!(app.surface_ctx.as_ref().unwrap().params, params);
        assert!(app.doc.bodies.is_empty());
    }
    use s33d_kernel::OffsetMode;
    #[test]
    fn equation_surface_edit_roundtrip_cancel_and_undo() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let mut p = EquationSurfaceParams {
            resolution: 6,
            ..Default::default()
        };
        app.agent_equation_surface("surface.equation.apply", json!({"params":p}))
            .unwrap();
        let id = app.selected[0];
        let original = app.body(id).unwrap().shape.unwrap();
        p.fill = true;
        assert!(app
            .agent_equation_surface("surface.equation.apply", json!({"params":p,"body_id":id}))
            .is_err());
        assert_eq!(app.body(id).unwrap().shape, Some(original));
        p.fill = false;
        p.expressions[2] = "0.01*(u^2-v^2)".into();
        app.agent_equation_surface("surface.equation.apply", json!({"params":p,"body_id":id}))
            .unwrap();
        assert_eq!(app.doc.bodies.len(), 1);
        assert_eq!(app.selected, vec![id]);
        assert!(app.undo());
        assert_eq!(app.body(id).unwrap().shape, Some(original));
        assert!(app.redo());
        let saved = app.project_to_s33d().unwrap();
        app.new_project();
        app.load_s33d_project(&saved).unwrap();
        assert!(app
            .kernel
            .exact_shape(app.body(id).unwrap().shape.unwrap())
            .is_some());
        let Feature::EquationSurface { params } = &app.body(id).unwrap().feature else {
            panic!()
        };
        assert_eq!(params, &p);
        app.start_equation_surface(Some(id));
        assert!(app.surface_ctx.is_some());
        app.cancel_equation_curve();
        assert!(app.surface_ctx.is_none());
        assert_eq!(app.doc.bodies.len(), 1);
        app.selected = vec![id];
        app.start_extrude();
        assert_eq!(
            app.offset_ctx.as_ref().unwrap().operation,
            OffsetMode::SymmetricThickenSurface
        );
        app.cancel_offset_shell();
        let mut invalid = p;
        invalid.resolution = 10000;
        assert!(build(&invalid).is_err());
        assert!(evaluate("ans+u", 1.0, 2.0, false).is_err());
        assert_eq!(evaluate("2*u+v", 3.0, 4.0, false).unwrap(), 10.0);
    }
    #[test]
    fn equation_surface_open_fill_and_symmetric_thickness() {
        let mut p = EquationSurfaceParams {
            resolution: 8,
            ..Default::default()
        };
        let saddle = build(&p).unwrap();
        assert_eq!(saddle.solid_count, 0);
        p.fill = true;
        assert!(build(&p).is_err(), "Open saddle must never get a cap");
        p.fill = false;
        p.expressions[2] = "0".into();
        let plane = build(&p).unwrap();
        let thick = plane
            .offset(2.0, 1e-6, &[], OffsetMode::SymmetricThickenSurface)
            .unwrap();
        assert_eq!(thick.solid_count, 1);
        let (min, max) = thick.mesh.bounds().unwrap();
        assert!(
            (min.z + 1.0).abs() < 0.01 && (max.z - 1.0).abs() < 0.01,
            "{min:?} {max:?}"
        );
        let curved = saddle
            .offset(0.5, 1e-6, &[], OffsetMode::SymmetricThickenSurface)
            .unwrap();
        assert_eq!(curved.solid_count, 1);
    }
    #[test]
    fn equation_surface_closed_sphere_and_torus_fill() {
        let mut p = EquationSurfaceParams {
            expressions: ["10".into(), "u".into(), "v".into()],
            spherical: true,
            u_domain: [0.0, std::f64::consts::TAU],
            v_domain: [0.0, std::f64::consts::PI],
            resolution: 16,
            fill: true,
            ..Default::default()
        };
        let sphere = build(&p).unwrap();
        assert_eq!(sphere.solid_count, 1);
        assert!(sphere.export_step().unwrap().contains("B_SPLINE_SURFACE"));
        p.spherical = false;
        p.v_domain[1] = std::f64::consts::TAU;
        p.expressions = [
            "(10+3*cos(v))*cos(u)".into(),
            "(10+3*cos(v))*sin(u)".into(),
            "3*sin(v)".into(),
        ];
        let torus = build(&p).unwrap();
        assert_eq!(torus.solid_count, 1);
    }
}
