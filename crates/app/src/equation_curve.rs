//! Spatial parametric curves: definitions are independent of display sampling.
use super::*;
use s33d_model::document::EquationCurveParams;

pub struct CurveContext {
    pub editing: Option<BodyId>,
    pub params: EquationCurveParams,
    pub preview: Option<Vec<[f64; 3]>>,
    pub error: Option<String>,
}

/// A planar equation polyline participates in the same region arrangement as
/// Sketch. Never flatten a spatial curve or close an open endpoint by force.
pub(super) fn planar_profile(points: &[[f64; 3]]) -> Option<SketchFeature> {
    use glam::DVec3;
    let origin = DVec3::from_array(*points.first()?);
    let vectors: Vec<_> = points
        .iter()
        .map(|p| DVec3::from_array(*p) - origin)
        .collect();
    if vectors.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let longest = *vectors
        .iter()
        .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))?;
    let scale = longest.length();
    if scale <= 1e-8 {
        return None;
    }
    let x = longest / scale;
    let cross = vectors
        .iter()
        .map(|p| x.cross(*p))
        .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))?;
    if cross.length() <= scale * 1e-8 {
        return None;
    }
    let n = cross.normalize();
    let tolerance = (scale * 1e-8).max(1e-7);
    if vectors.iter().any(|p| p.dot(n).abs() > tolerance) {
        return None;
    }
    let plane = PlaneDef {
        origin: origin.to_array(),
        x_axis: x.to_array(),
        y_axis: n.cross(x).to_array(),
    };
    let mut local: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            let (u, v) = plane.to_local(*p);
            [u, v]
        })
        .collect();
    if DVec3::from_array(*points.last()?).distance(origin) <= tolerance {
        *local.last_mut()? = local[0];
    }
    let entities = local
        .windows(2)
        .filter(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) > tolerance)
        .map(|w| SketchEntity::Line { a: w[0], b: w[1] })
        .collect();
    Some(SketchFeature {
        vertex_locks: Default::default(),
        plane,
        entities,
        world_xyz_coordinates: true,
        on_body: None,
    })
}

impl S33DApp {
    pub fn profile_sketch(&self, id: BodyId) -> Option<SketchFeature> {
        let body = self.body(id)?;
        if body.is_sketch() {
            return body.sketch().cloned();
        }
        if matches!(body.feature, Feature::EquationCurve { .. }) {
            return self.equation_profile_group(id).map(|(sketch, _)| sketch);
        }
        None
    }

    /// Composite profiles are frozen in the resulting feature; a single source
    /// link would incorrectly rebuild them from only one constituent curve.
    pub(super) fn profile_source_link(&self, id: BodyId) -> Option<BodyId> {
        let body = self.body(id)?;
        if body.is_sketch() {
            return Some(id);
        }
        self.equation_profile_group(id)
            .filter(|(_, ids)| ids.len() == 1)
            .map(|_| id)
    }

    fn equation_profile_group(&self, id: BodyId) -> Option<(SketchFeature, Vec<BodyId>)> {
        use glam::DVec3;
        let Feature::EquationCurve { points: seed, .. } = &self.body(id)?.feature else {
            return None;
        };
        if seed.len() < 2 {
            return None;
        }
        let candidates: Vec<_> = self
            .doc
            .bodies
            .iter()
            .filter_map(|b| {
                if b.id != id && !b.visible {
                    return None;
                }
                if let Feature::EquationCurve { points, .. } = &b.feature {
                    Some((b.id, points))
                } else {
                    None
                }
            })
            .collect();
        let single = planar_profile(seed);
        // A straight segment has no unique plane by itself, but another curve
        // can define a plane containing it. Do not add the synthetic join.
        let defining = single.clone().or_else(|| {
            candidates.iter().find_map(|(other, points)| {
                if *other == id {
                    return None;
                }
                let combined: Vec<_> = seed.iter().chain(points.iter()).copied().collect();
                planar_profile(&combined)
            })
        })?;
        let mut normal = defining.plane.normal();
        let axis = if normal.x.abs() >= normal.y.abs() && normal.x.abs() >= normal.z.abs() {
            0
        } else if normal.y.abs() >= normal.z.abs() {
            1
        } else {
            2
        };
        if normal[axis] < 0.0 {
            normal = -normal;
        }
        let offset = DVec3::from_array(defining.plane.origin).dot(normal);
        let mut plane = PlaneDef::from_normal(normal.to_array());
        plane.origin = (normal * offset).to_array();
        let scale = seed
            .iter()
            .map(|p| DVec3::from_array(*p).distance(DVec3::from_array(seed[0])))
            .fold(1.0_f64, f64::max);
        let tolerance = (scale * 1e-8).max(1e-7);
        let mut ids = Vec::new();
        let mut entities = Vec::new();
        for (source, points) in candidates {
            if points.len() < 2 {
                continue;
            }
            if points.iter().any(|p| {
                !DVec3::from_array(*p).is_finite()
                    || (DVec3::from_array(*p).dot(normal) - offset).abs() > tolerance
            }) {
                continue;
            }
            ids.push(source);
            let mut local: Vec<_> = points
                .iter()
                .map(|p| {
                    let (u, v) = plane.to_local(*p);
                    [u, v]
                })
                .collect();
            if points.len() > 2
                && DVec3::from_array(points[0]).distance(DVec3::from_array(*points.last()?))
                    <= tolerance
            {
                let first = local[0];
                *local.last_mut()? = first;
            }
            entities.extend(
                local
                    .windows(2)
                    .filter(|w| (w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) > tolerance)
                    .map(|w| SketchEntity::Line { a: w[0], b: w[1] }),
            );
        }
        if ids.len() == 1 {
            return single.map(|sketch| (sketch, ids));
        }
        Some((
            SketchFeature {
                vertex_locks: Default::default(),
                plane,
                entities,
                world_xyz_coordinates: true,
                on_body: None,
            },
            ids,
        ))
    }
}

pub fn sample(params: &EquationCurveParams) -> Result<Vec<[f64; 3]>, String> {
    let [start, end] = params.domain;
    if !start.is_finite()
        || !end.is_finite()
        || start >= end
        || !params.unit_mm.is_finite()
        || params.unit_mm <= 0.0
        || params.transform.iter().any(|x| !x.is_finite())
    {
        return Err("Invalid domain, length unit or transform".into());
    }
    let transform = glam::DMat4::from_cols_array(&params.transform);
    if transform.x_axis.w != 0.0
        || transform.y_axis.w != 0.0
        || transform.z_axis.w != 0.0
        || transform.w_axis.w != 1.0
        || transform.determinant().abs() < 1e-12
    {
        return Err("Expected an invertible affine transform".into());
    }
    let evaluate = |t: f64| -> Result<glam::DVec3, String> {
        let mut v = [0.0; 3];
        for i in 0..3 {
            v[i] = crate::plugins::evaluate_curve_expression(
                &params.expressions[i],
                t,
                params.degrees,
            )
            .map_err(|e| format!("t={t:.8}, component {}: {e}", i + 1))?;
        }
        let point = if params.spherical {
            if v[0] < 0.0 {
                return Err(format!("Negative radius at t={t}"));
            }
            let theta = if params.degrees {
                v[1].to_radians()
            } else {
                v[1]
            };
            let phi = if params.degrees {
                v[2].to_radians()
            } else {
                v[2]
            };
            glam::DVec3::new(
                v[0] * phi.sin() * theta.cos(),
                v[0] * phi.sin() * theta.sin(),
                v[0] * phi.cos(),
            )
        } else {
            glam::DVec3::from_array(v)
        };
        let p = transform.transform_point3(point * params.unit_mm);
        if !p.is_finite() || p.abs().max_element() > 1e7 {
            return Err(format!("Coordinate outside supported range at t={t}"));
        }
        Ok(p)
    };
    fn refine(
        eval: &impl Fn(f64) -> Result<glam::DVec3, String>,
        a: f64,
        b: f64,
        pa: glam::DVec3,
        pb: glam::DVec3,
        depth: u8,
        out: &mut Vec<[f64; 3]>,
    ) -> Result<(), String> {
        if out.len() >= 16384 {
            return Err("Curve exceeds sampling budget; narrow the domain".into());
        }
        let m = a + (b - a) * 0.5;
        let pm = eval(m)?;
        let q1 = eval(a + (b - a) * 0.25)?;
        let q3 = eval(a + (b - a) * 0.75)?;
        let error = pm
            .distance((pa + pb) * 0.5)
            .max(q1.distance(pa.lerp(pb, 0.25)))
            .max(q3.distance(pa.lerp(pb, 0.75)));
        if error > 0.02 {
            if depth >= 12 {
                return Err("Unresolved discontinuity / curvature; narrow the domain".into());
            }
            refine(eval, a, m, pa, pm, depth + 1, out)?;
            refine(eval, m, b, pm, pb, depth + 1, out)?;
        } else {
            out.push(pb.to_array());
        }
        Ok(())
    }
    let mut points = vec![evaluate(start)?.to_array()];
    for i in 0..256 {
        let a = start + (end - start) * (i as f64 / 256.0);
        let b = start + (end - start) * ((i + 1) as f64 / 256.0);
        refine(
            &evaluate,
            a,
            b,
            glam::DVec3::from_array(*points.last().unwrap()),
            evaluate(b)?,
            0,
            &mut points,
        )?;
    }
    if points
        .iter()
        .all(|p| glam::DVec3::from_array(*p).distance(glam::DVec3::from_array(points[0])) < 1e-8)
    {
        return Err("Curve is a single point".into());
    }
    Ok(points)
}

impl S33DApp {
    pub(super) fn pick_curve(&self, pos: egui::Pos2, rect: egui::Rect) -> Option<BodyId> {
        let mut best = (6.0, None);
        for b in self.doc.bodies.iter().filter(|b| b.visible) {
            let Feature::EquationCurve { points, .. } = &b.feature else {
                continue;
            };
            for w in points.windows(2) {
                let (Some(a), Some(z)) = (
                    self.world_to_viewport(glam::DVec3::from_array(w[0]).as_vec3(), rect),
                    self.world_to_viewport(glam::DVec3::from_array(w[1]).as_vec3(), rect),
                ) else {
                    continue;
                };
                let d = z - a;
                let t = ((pos - a).dot(d) / d.length_sq().max(1e-10)).clamp(0.0, 1.0);
                let distance = pos.distance(a + d * t);
                if distance < best.0 {
                    best = (distance, Some(b.id));
                }
            }
        }
        best.1
    }
    pub(super) fn agent_equation_curve(
        &mut self,
        method: &str,
        input: Value,
    ) -> AgentCommandResult {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            params: EquationCurveParams,
            #[serde(default)]
            body_id: Option<BodyId>,
        }
        self.agent_require_idle()?;
        let input = Self::agent_parse::<Input>(input)?;
        if input
            .body_id
            .is_some_and(|id| !self.body(id).is_some_and(|b| b.kind == BodyKind::Curve))
        {
            return Err(Self::agent_fault(
                "invalid_selection",
                "Expected an existing curve object",
            ));
        }
        let points = sample(&input.params).map_err(|e| Self::agent_fault("invalid_curve", e))?;
        if method == "curve.equation.preview" {
            return Ok(json!({"point_count":points.len(),"points":points,"committed":false}));
        }
        self.curve_ctx = Some(CurveContext {
            editing: input.body_id,
            params: input.params,
            preview: Some(points),
            error: None,
        });
        self.commit_equation_curve();
        if self.curve_ctx.is_some() {
            let error = self
                .curve_ctx
                .as_ref()
                .and_then(|c| c.error.clone())
                .unwrap_or_else(|| "Equation edit could not be committed".into());
            self.cancel_equation_curve();
            return Err(Self::agent_fault("geometry_error", error));
        }
        self.agent_body_value_by_id(self.selected[0])
    }
    pub fn start_equation_curve(&mut self, editing: Option<BodyId>) {
        if !self.finish_creation_for_tool_switch() || self.mode != Mode::Idle {
            return;
        }
        let params = if let Some(id) = editing {
            match self.body(id).map(|b| &b.feature) {
                Some(Feature::EquationCurve { params, .. }) => params.clone(),
                _ => return,
            }
        } else {
            let mut p = EquationCurveParams::default();
            p.unit_mm = self.settings.unit.millimeters_per_unit();
            p
        };
        self.deactivate_sidebar_tools();
        self.curve_ctx = Some(CurveContext {
            editing,
            params,
            preview: None,
            error: None,
        });
        self.mode = Mode::EquationCurve;
        self.preview_equation_curve();
    }
    pub fn preview_equation_curve(&mut self) {
        if let Some(ctx) = self.curve_ctx.as_mut() {
            match sample(&ctx.params) {
                Ok(points) => {
                    ctx.preview = Some(points);
                    ctx.error = None;
                }
                Err(e) => {
                    ctx.preview = None;
                    ctx.error = Some(e);
                }
            }
        }
        self.bump_scene();
    }
    pub fn commit_equation_curve(&mut self) {
        let Some(ctx) = self.curve_ctx.as_ref() else {
            return;
        };
        let Some(points) = ctx.preview.clone() else {
            return;
        };
        let params = ctx.params.clone();
        let editing = ctx.editing;
        if editing.is_some_and(|id| self.body(id).is_none()) {
            return;
        }
        let rebuilt = if let Some(id) = editing {
            let has_dependents = self.doc.bodies.iter().any(|b| {
                matches!(&b.feature,
                Feature::Extrude { source_sketch: Some(source), .. } |
                Feature::Revolve { source_sketch: Some(source), .. } if *source == id)
            });
            if has_dependents {
                let result = planar_profile(&points)
                    .ok_or_else(|| {
                        "The curve has dependent features and must remain planar".to_string()
                    })
                    .and_then(|sketch| self.rebuild_sketch_dependents(id, &sketch));
                match result {
                    Ok(rebuilt) => rebuilt,
                    Err(error) => {
                        self.curve_ctx.as_mut().unwrap().error = Some(error);
                        return;
                    }
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        self.begin_document_change();
        let feature = Feature::EquationCurve { params, points };
        let id = if let Some(id) = editing {
            self.doc.body_mut(id).unwrap().feature = feature;
            id
        } else {
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("Curve {id}"),
                kind: BodyKind::Curve,
                feature,
                shape: None,
                material: Material::default(),
                visible: true,
            });
            id
        };
        for (id, shape, feature) in rebuilt {
            if let Some(body) = self.doc.body_mut(id) {
                body.shape = Some(shape);
                body.feature = feature;
            }
        }
        self.curve_ctx = None;
        self.mode = Mode::Idle;
        self.selected = vec![id];
        self.clear_region_selection();
        self.face_pick = None;
        self.selected_edge = None;
        self.regions_cache.remove(&id);
        self.bump_scene();
        self.record_history("3D equation curve");
    }
    pub fn cancel_equation_curve(&mut self) {
        self.pending_tool_exit = None;
        self.curve_ctx = None;
        self.surface_ctx = None;
        self.mode = Mode::Idle;
        self.bump_scene();
    }
    pub(super) fn curve_scene(&self, scene: &mut SceneData) {
        let add = |scene: &mut SceneData, points: &[[f64; 3]], color| {
            scene.body_edge_lines.push(SceneLines {
                positions: points
                    .windows(2)
                    .flat_map(|w| w.iter().map(|p| p.map(|v| v as f32)))
                    .collect(),
                color,
            });
        };
        for b in &self.doc.bodies {
            if !b.visible
                || self
                    .curve_ctx
                    .as_ref()
                    .is_some_and(|c| c.editing == Some(b.id) && c.preview.is_some())
            {
                continue;
            }
            if let Feature::EquationCurve { points, .. } = &b.feature {
                add(
                    scene,
                    points,
                    if self.selected.contains(&b.id) {
                        [1.0, 0.55, 0.1]
                    } else {
                        [
                            b.material.color[0],
                            b.material.color[1],
                            b.material.color[2],
                        ]
                    },
                );
            }
        }
        if let Some(points) = self.curve_ctx.as_ref().and_then(|c| c.preview.as_ref()) {
            add(scene, points, [1.0, 0.7, 0.2]);
        }
    }
}

pub(super) fn bounds(feature: &Feature) -> Option<(glam::Vec3, glam::Vec3)> {
    let Feature::EquationCurve { points, .. } = feature else {
        return None;
    };
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = -min;
    for p in points {
        let p = glam::DVec3::from_array(*p).as_vec3();
        min = min.min(p);
        max = max.max(p);
    }
    (!points.is_empty()).then_some((min, max))
}

pub fn panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let lang = app.settings.language;
    let Some(ctx) = app.curve_ctx.as_mut() else {
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
        "Parameter: t · use *, /, ^ and elementary functions",
        "参数：t · 支持 *、/、^ 及初等函数",
    ));
    for i in 0..3 {
        ui.label(if ctx.params.spherical {
            ["R(t)", "theta(t)", "phi(t)"][i]
        } else {
            ["x(t)", "y(t)", "z(t)"][i]
        });
        changed |= ui
            .text_edit_singleline(&mut ctx.params.expressions[i])
            .changed();
    }
    ui.horizontal(|ui| {
        ui.label(lang.text("Domain", "定义域"));
        for v in &mut ctx.params.domain {
            changed |= ui.add(egui::DragValue::new(v).speed(0.1)).changed();
        }
    });
    changed |= ui
        .checkbox(
            &mut ctx.params.degrees,
            lang.text(
                "Angles in degrees (otherwise radians)",
                "角度使用度（不勾选为弧度）",
            ),
        )
        .changed();
    ui.horizontal(|ui| {
        ui.label(lang.text("Expression length unit", "表达式长度单位"));
        changed |= ui
            .selectable_value(&mut ctx.params.unit_mm, 1.0, "mm")
            .changed();
        changed |= ui
            .selectable_value(&mut ctx.params.unit_mm, 25.4, "inch")
            .changed();
    });
    if ctx.params.spherical {
        ui.label(lang.text(
            "theta: +X toward +Y in XY; phi: from +Z. World XYZ, independent of the view.",
            "theta：XY 内 +X 朝 +Y；phi：从 +Z 量起。使用世界 XYZ，不随视图改变。",
        ));
    }
    if changed {
        ctx.preview = None;
        ctx.error = None;
    }
    if let Some(e) = &ctx.error {
        ui.colored_label(egui::Color32::from_rgb(200, 65, 45), e);
    }
    ui.label(lang.text(
        "Planar enclosed regions can be selected and extruded; spatial curves stay 3D.",
        "平面闭合区域可选中并拉伸；非平面曲线保持三维。",
    ));
    let ready = ctx.preview.is_some();
    let apply = ui.button(lang.text("Apply preview", "应用预览")).clicked();
    let done = ui
        .add_enabled(ready, egui::Button::new(lang.text("Done", "完成")))
        .clicked();
    let cancel = ui.button(lang.text("Cancel", "取消")).clicked();
    let exit = ui
        .add_enabled(
            ready || changed,
            egui::Button::new(lang.text("Exit Draw Equation", "退出方程绘图")),
        )
        .clicked();
    if changed {
        app.preview_equation_curve();
    }
    if apply {
        app.preview_equation_curve();
    }
    if done || exit {
        app.commit_equation_curve();
    }
    if cancel {
        app.cancel_equation_curve();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn add_curve(app: &mut S33DApp, expressions: [&str; 3], domain: [f64; 2]) -> BodyId {
        let mut params = EquationCurveParams::default();
        params.expressions = expressions.map(str::to_owned);
        params.domain = domain;
        app.agent_equation_curve("curve.equation.apply", json!({"params":params}))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    }

    #[test]
    fn coplanar_equation_curves_form_annulus_and_snapshot_extrusion() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let a = add_curve(
            &mut app,
            ["20*cos(t)", "20*sin(t)", "5"],
            [0.0, std::f64::consts::TAU],
        );
        let b = add_curve(
            &mut app,
            ["10*cos(t)", "10*sin(t)", "5"],
            [0.0, std::f64::consts::TAU],
        );
        let regions = app.regions_of(a);
        assert_eq!(regions.len(), 2);
        assert_eq!(app.regions_of(b).len(), 2);
        let ring = regions.iter().position(|r| r.holes.len() == 1).unwrap();
        let area = regions[ring].signed_area()
            - regions[ring]
                .holes
                .iter()
                .map(|hole| s33d_kernel::region::signed_area(hole).abs())
                .sum::<f64>();
        assert!((area / (300.0 * std::f64::consts::PI) - 1.0).abs() < 0.002);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0));
        app.viewport_rect = rect;
        let pos = app
            .world_to_viewport(glam::Vec3::new(15.0, 0.0, 5.0), rect)
            .unwrap();
        let (picked, picked_region, ..) = app.pick_sketch_region(pos, rect).unwrap();
        assert_eq!(app.regions_of(picked)[picked_region].holes.len(), 1);
        app.selected = vec![b];
        assert!(app.set_region_selection(app.selected[0], vec![ring]));
        app.start_extrude();
        app.set_extrude_depth(7.0);
        assert_eq!(app.extrude_ctx.as_ref().unwrap().regions[0].holes.len(), 1);
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.commit_extrude();
        let solid = app.selected[0];
        assert!(
            matches!(&app.body(solid).unwrap().feature,Feature::Extrude{source_sketch:None,sketch,..} if sketch.entities.len()>256)
        );
        let shape = app.body(solid).unwrap().shape.unwrap();
        let mesh = app.kernel.tessellate(shape).unwrap();
        assert!((mesh.bounds().unwrap().1.z - mesh.bounds().unwrap().0.z - 7.0).abs() < 0.01);
        assert!(app.undo());
        assert!(app.body(solid).is_none());
        assert!(app.redo());
        let payload = app.project_to_s33d().unwrap();
        let (doc, kernel, ..) = decode_s33d_project(&payload).unwrap();
        assert!(!kernel
            .tessellate(doc.body(solid).unwrap().shape.unwrap())
            .unwrap()
            .is_empty());
        app.doc.body_mut(a).unwrap().visible = false;
        assert_eq!(app.regions_of(b).len(), 1);
        assert_eq!(app.profile_source_link(b), Some(b));
    }

    #[test]
    fn open_equation_segments_close_together_without_flattening_spatial_curves() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let a = add_curve(
            &mut app,
            ["10*cos(t)", "10*sin(t)", "10*cos(t)"],
            [0.0, std::f64::consts::PI],
        );
        assert!(app.regions_of(a).is_empty());
        let b = add_curve(&mut app, ["t", "0", "t"], [-10.0, 10.0]);
        assert_eq!(app.regions_of(a).len(), 1);
        assert_eq!(app.regions_of(b).len(), 1);
        let area = app.regions_of(a)[0].signed_area();
        let spatial = add_curve(
            &mut app,
            ["5*cos(t)", "5*sin(t)", "t"],
            [0.0, std::f64::consts::TAU],
        );
        assert!(app.profile_sketch(spatial).is_none());
        assert!((app.regions_of(a)[0].signed_area() - area).abs() < 1e-6);
        app.agent_extrude_feature(ExtrudeFeatureParams {
            sketch_id: b,
            depth: 3.0,
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            &app.doc.bodies.last().unwrap().feature,
            Feature::Extrude {
                source_sketch: None,
                ..
            }
        ));
    }

    #[test]
    fn planar_equation_region_extrudes_rebuilds_and_survives_undo() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_equation_curve(None);
        let params = &mut app.curve_ctx.as_mut().unwrap().params;
        params.expressions = ["20*cos(t)".into(), "20*sin(t)".into(), "5".into()];
        params.domain = [0.0, std::f64::consts::TAU];
        app.preview_equation_curve();
        app.commit_equation_curve();
        let curve_id = app.selected[0];
        assert_eq!(app.regions_of(curve_id).len(), 1);
        assert!(app.set_region_selection(app.selected[0], vec![0]));
        app.start_extrude();
        assert_eq!(app.mode, Mode::Extrude);
        assert!(!app.extrude_ctx.as_ref().unwrap().symmetric);
        assert!(app.extrude_ctx.as_ref().unwrap().preview_error.is_none());
        app.commit_extrude();
        let solid_id = app.selected[0];
        assert!(
            matches!(app.body(solid_id).unwrap().feature, Feature::Extrude { source_sketch: Some(id), .. } if id == curve_id)
        );
        let old_shape = app.body(solid_id).unwrap().shape;
        app.start_equation_curve(Some(curve_id));
        app.curve_ctx.as_mut().unwrap().params.expressions[0] = "30*cos(t)".into();
        app.preview_equation_curve();
        app.commit_equation_curve();
        assert_eq!(app.selected, vec![curve_id]);
        assert_ne!(app.body(solid_id).unwrap().shape, old_shape);
        assert!(app.doc.undo());
        assert_eq!(app.body(solid_id).unwrap().shape, old_shape);
        assert!(app.doc.redo());
        let payload = app.project_to_s33d().unwrap();
        let (doc, _, ..) = decode_s33d_project(&payload).unwrap();
        assert!(matches!(
            doc.body(curve_id).unwrap().feature,
            Feature::EquationCurve { .. }
        ));
    }

    #[test]
    fn profile_detection_does_not_flatten_or_force_close_curves() {
        assert!(planar_profile(&sample(&EquationCurveParams::default()).unwrap()).is_none());
        let points = vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        let profile = planar_profile(&points).unwrap();
        assert!(s33d_model::compute_regions(&profile.entities).is_empty());
        let closed = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 10.0],
            [10.0, 10.0, 10.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let profile = planar_profile(&closed).unwrap();
        assert_eq!(s33d_model::compute_regions(&profile.entities).len(), 1);
    }
    #[test]
    fn agent_planar_curve_extrudes_and_failed_spatial_edit_is_atomic() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        let mut params = EquationCurveParams::default();
        params.expressions[2] = "0".into();
        params.domain = [0.0, std::f64::consts::TAU];
        let body = app
            .agent_equation_curve("curve.equation.apply", json!({"params":params}))
            .unwrap();
        let id = body["id"].as_u64().unwrap();
        assert_eq!(body["sketch"]["regions"].as_array().unwrap().len(), 1);
        app.agent_extrude_feature(ExtrudeFeatureParams {
            sketch_id: id,
            ..Default::default()
        })
        .unwrap();
        let before = app.doc.archive();
        params.expressions[2] = "t".into();
        assert!(app
            .agent_equation_curve(
                "curve.equation.apply",
                json!({"params":params,"body_id":id})
            )
            .is_err());
        assert_eq!(
            serde_json::to_value(app.doc.archive()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert!(app.curve_ctx.is_none());
        assert_eq!(app.mode, Mode::Idle);
    }
    #[test]
    fn curve_edit_transform_and_archive_keep_id_and_rebuild_samples() {
        let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut app = S33DApp::new(&creation);
        app.start_equation_curve(None);
        app.commit_equation_curve();
        let id = app.selected[0];
        assert_eq!(app.doc.body(id).unwrap().kind, BodyKind::Curve);
        app.start_equation_curve(Some(id));
        app.curve_ctx.as_mut().unwrap().params.expressions[2] = "3*t".into();
        app.preview_equation_curve();
        app.commit_equation_curve();
        assert_eq!(app.selected[0], id);
        app.apply_transform(
            &glam::Mat4::from_translation(glam::Vec3::new(10.0, 0.0, 0.0)),
            "test move",
        );
        let payload = app.project_to_s33d().unwrap();
        let mut value: Value = serde_json::from_str(&payload).unwrap();
        value["document"]["current"]["bodies"][0]["feature"]["EquationCurve"]["points"] = json!([]);
        let (mut doc, _, ..) = decode_s33d_project(&value.to_string()).unwrap();
        let Feature::EquationCurve { points, .. } = &doc.body(id).unwrap().feature else {
            panic!()
        };
        assert!((points[0][0] - 30.0).abs() < 1e-6);
        assert!(points.last().unwrap()[2] > 30.0);
        assert!(doc.undo());
        assert!(doc.redo());
        assert_eq!(doc.body(id).unwrap().id, id);
    }
    #[test]
    fn spatial_curves_preserve_three_coordinates_and_definition() {
        let p = EquationCurveParams::default();
        let points = sample(&p).unwrap();
        assert!(points.last().unwrap()[2] > 20.0);
        assert!(points.iter().any(|p| p[0] < -19.0));
        assert!(points.iter().any(|p| p[1] > 19.0));
        let encoded = serde_json::to_string(&p).unwrap();
        assert_eq!(
            sample(&serde_json::from_str(&encoded).unwrap()).unwrap(),
            points
        );
        let mut sphere = p;
        sphere.spherical = true;
        sphere.expressions = ["10".into(), "t".into(), "pi/3".into()];
        for p in sample(&sphere).unwrap() {
            assert!((glam::DVec3::from_array(p).length() - 10.0).abs() < 1e-8);
        }
    }
    #[test]
    fn rejects_discontinuities_and_invalid_input() {
        let mut p = EquationCurveParams::default();
        p.domain = [-1.0, 1.0];
        p.expressions[0] = "1/t".into();
        assert!(sample(&p).is_err());
        p.expressions[0] = "sqrt(-1)".into();
        assert!(sample(&p).is_err());
        p.expressions[0] = "(".repeat(257);
        assert!(sample(&p).is_err());
        p.expressions = ["-1".into(), "t".into(), "t".into()];
        p.spherical = true;
        assert!(sample(&p).is_err());
    }
}
