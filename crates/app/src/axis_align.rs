//! Coaxial alignment with a picked line or P(t) = P0 + t D.
use super::*;

#[derive(Clone, Debug)]
pub struct AxisAlign {
    pub enabled: bool,
    pub picking: bool,
    pub point: [f64; 3],
    pub direction: [f64; 3],
    pub source_axis: usize,
    pub cylinder_axes: bool,
    pub fixed_body: Option<BodyId>,
}
impl Default for AxisAlign {
    fn default() -> Self {
        Self {
            enabled: false,
            picking: false,
            point: [0.; 3],
            direction: [0., 1., 0.],
            source_axis: 1,
            cylinder_axes: true,
            fixed_body: None,
        }
    }
}

/// Rotate a source axis onto the target line; preserve its station along the
/// target, translating only the perpendicular component. No geometry scaling.
pub fn coaxial_transform(
    center: glam::Vec3,
    source: glam::Vec3,
    point: glam::Vec3,
    direction: glam::Vec3,
) -> Option<glam::Mat4> {
    if !center.is_finite()
        || !point.is_finite()
        || !source.is_finite()
        || !direction.is_finite()
        || source.length_squared() < 1e-12
        || direction.length_squared() < 1e-12
    {
        return None;
    }
    let source = source.normalize();
    let direction = direction.normalize();
    // A center axis is an unoriented line. Avoid flipping a cylinder end for
    // an arbitrary sign chosen by the importer or picked edge traversal.
    let target = if source.dot(direction) < 0. {
        -direction
    } else {
        direction
    };
    let rotation = glam::Quat::from_rotation_arc(source, target);
    let projected = point + direction * (center - point).dot(direction);
    Some(
        glam::Mat4::from_translation(projected)
            * glam::Mat4::from_quat(rotation)
            * glam::Mat4::from_translation(-center),
    )
}

/// Axis alignment is rigid, so dimensions remain valid while the persisted
/// primitive frame must follow the shape (including copied primitives).
fn aligned_primitive(mut params: Primitive3DParams, matrix: &glam::Mat4) -> Primitive3DParams {
    let frame = params.frame.unwrap_or_else(|| {
        if matches!(
            params.kind,
            Primitive3DKind::Box | Primitive3DKind::Cylinder
        ) {
            DatumPlane::Top.def()
        } else {
            DatumPlane::Front.def()
        }
    });
    params.position = matrix
        .transform_point3(glam::DVec3::from_array(params.position).as_vec3())
        .as_dvec3()
        .to_array();
    params.frame = Some(PlaneDef {
        origin: params.position,
        x_axis: matrix
            .transform_vector3(glam::DVec3::from_array(frame.x_axis).as_vec3())
            .as_dvec3()
            .to_array(),
        y_axis: matrix
            .transform_vector3(glam::DVec3::from_array(frame.y_axis).as_vec3())
            .as_dvec3()
            .to_array(),
    });
    params
}

impl S33DApp {
    pub fn align_axis_from_reference(&mut self) {
        let Some(id) = self.align_reference else {
            return;
        };
        if let Ok(targets) =
            self.thread_targets_for_body(id, &mut HashMap::new(), &mut HashSet::new())
        {
            if let Some(t) = targets
                .iter()
                .filter(|t| !t.internal)
                .max_by(|a, b| a.length.total_cmp(&b.length))
                .or_else(|| targets.first())
            {
                self.axis_align.point = t.origin;
                self.axis_align.direction = t.axis;
                self.axis_align.fixed_body = Some(id);
                self.axis_align.picking = false;
                self.bump_scene();
                return;
            }
        }
        if let Some(mesh) = self
            .body(id)
            .and_then(|b| b.shape)
            .and_then(|s| self.kernel.tessellate(s).ok())
        {
            if let Some((lo, hi)) = mesh.bounds() {
                self.axis_align.point = ((lo + hi) * 0.5).as_dvec3().to_array();
                self.axis_align.direction = [0.; 3];
                self.axis_align.direction[self.axis_align.source_axis.min(2)] = 1.;
                self.axis_align.fixed_body = Some(id);
                self.axis_align.picking = false;
                self.bump_scene();
            }
        }
    }

    pub(super) fn pick_alignment_axis(&mut self, pos: egui::Pos2, rect: egui::Rect) {
        let cylinder = self
            .pick_face(pos, rect)
            .and_then(|(f, _)| self.thread_target_for_face(&f));
        if let Some(t) = cylinder {
            self.axis_align.point = t.origin;
            self.axis_align.direction = t.axis;
            self.axis_align.fixed_body = Some(t.body_id);
            self.align_reference = self.selected.contains(&t.body_id).then_some(t.body_id);
        } else if let Some(axis) = self.pick_rotation_axis(pos, rect) {
            self.axis_align.point = axis.point.map(f64::from);
            self.axis_align.direction = axis.direction.map(f64::from);
            self.axis_align.fixed_body = None;
        } else {
            self.toast(self.settings.language.text(
                "Pick a cylinder wall or a straight edge",
                "请选择圆柱面或直边",
            ));
            return;
        }
        self.axis_align.picking = false;
        self.bump_scene();
    }

    pub fn align_selected_to_axis(&mut self) {
        if !self.align_active || !self.axis_align.enabled || self.axis_align.picking {
            return;
        }
        let point = glam::DVec3::from_array(self.axis_align.point).as_vec3();
        let direction = glam::DVec3::from_array(self.axis_align.direction).as_vec3();
        if !point.is_finite() || !direction.is_finite() || direction.length_squared() < 1e-12 {
            self.toast(self.settings.language.text(
                "Axis direction must be finite and nonzero",
                "轴方向必须有限且非零",
            ));
            return;
        }
        let ids = self.selected.clone();
        let mut changes = Vec::new();
        for id in ids {
            if self.axis_align.fixed_body == Some(id) || self.align_reference == Some(id) {
                continue;
            }
            let Some(shape) = self
                .body(id)
                .filter(|b| b.kind == BodyKind::Solid)
                .and_then(|b| b.shape)
            else {
                continue;
            };
            let result = (|| -> Result<(ShapeId, glam::Mat4), String> {
                let mesh = self.kernel.tessellate(shape).map_err(|e| e.to_string())?;
                let (lo, hi) = mesh.bounds().ok_or("Object has no bounds")?;
                let mut center = (lo + hi) * 0.5;
                let mut source = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z]
                    [self.axis_align.source_axis.min(2)];
                if self.axis_align.cylinder_axes {
                    // Hidden Objects-list members remain valid transform
                    // targets, even though threading only accepts visible ones.
                    let ts =
                        self.thread_targets_for_body(id, &mut HashMap::new(), &mut HashSet::new())?;
                    if let Some(t) = ts
                        .iter()
                        .filter(|t| !t.internal)
                        .max_by(|a, b| a.length.total_cmp(&b.length))
                        .or_else(|| ts.first())
                    {
                        source = glam::DVec3::from_array(t.axis).as_vec3();
                        center = glam::DVec3::from_array(t.origin).as_vec3()
                            + source * (t.length as f32 * 0.5);
                    }
                }
                let matrix = coaxial_transform(center, source, point, direction)
                    .ok_or("Invalid source axis")?;
                self.kernel
                    .transform(shape, &matrix)
                    .map(|s| (s, matrix))
                    .map_err(|e| e.to_string())
            })();
            match result {
                Ok((new_shape, matrix)) => changes.push((id, new_shape, matrix)),
                Err(error) => {
                    for (_, shape, _) in changes {
                        self.kernel.delete(shape);
                    }
                    self.toast(format!(
                        "{}: {error}",
                        self.settings
                            .language
                            .text("Axis alignment failed", "轴线对齐失败")
                    ));
                    return;
                }
            }
        }
        if changes.is_empty() {
            self.toast(self.settings.language.text(
                "Select an object to move onto the axis",
                "请选择需要向轴线对齐的对象",
            ));
            return;
        }
        self.begin_document_change();
        let count = changes.len();
        for (id, shape, matrix) in changes {
            let original = self.doc.body(id).unwrap().shape.unwrap();
            self.transform_thread_state(id, original, shape, &matrix);
            let body = self.doc.body_mut(id).unwrap();
            body.shape = Some(shape);
            match &mut body.feature {
                Feature::Primitive3D { params } => *params = aligned_primitive(*params, &matrix),
                Feature::Derived {
                    operation:
                        Some(DerivedOperation::Copy {
                            primitive_snapshot: Some(params),
                            ..
                        }),
                    ..
                } => *params = aligned_primitive(*params, &matrix),
                _ => {}
            }
            if let Feature::Fastener { params } = body.feature {
                body.feature = Feature::Fastener {
                    params: crate::fasteners::transformed_params(params, &matrix),
                };
            }
        }
        self.face_pick = None;
        self.selected_edge = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!("Center-axis align ({count} objects)"));
        self.bump_scene();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coaxial_alignment_preserves_station_and_radius() {
        let c = glam::Vec3::new(8., 12., 3.);
        let p = glam::Vec3::new(1., 2., 4.);
        let m = coaxial_transform(c, glam::Vec3::Y, p, glam::Vec3::X).unwrap();
        assert!(m
            .transform_point3(c)
            .abs_diff_eq(glam::Vec3::new(8., 2., 4.), 1e-5));
        assert!(m
            .transform_vector3(glam::Vec3::Y)
            .abs_diff_eq(glam::Vec3::X, 1e-5));
        assert!((m.transform_vector3(glam::Vec3::Z * 3.).length() - 3.).abs() < 1e-5);
        assert!(coaxial_transform(c, glam::Vec3::Y, p, glam::Vec3::ZERO).is_none());
    }

    #[test]
    fn axis_alignment_moves_cylinder_and_preserves_locked_reference_and_undo() {
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        let mut create = |position| {
            app.execute_agent_request(AgentRequest::new(
                serde_json::json!(1),
                "primitive.create",
                serde_json::json!({"kind":"cylinder","radius":3.,"height":20.,"position":position}),
            ))
            .unwrap()["id"]
                .as_u64()
                .unwrap()
        };
        let reference = create([0., 0., 0.]);
        let movable = create([15., 7., 9.]);
        let original = app.doc.bodies.clone();
        app.selected = vec![reference, movable];
        app.align_active = true;
        app.align_reference = Some(reference);
        app.axis_align.enabled = true;
        app.align_axis_from_reference();
        assert_eq!(app.axis_align.fixed_body, Some(reference));
        app.align_selected_to_axis();
        assert_eq!(app.body(reference).unwrap(), &original[0]);
        let mesh = app
            .kernel
            .tessellate(app.body(movable).unwrap().shape.unwrap())
            .unwrap();
        let (lo, hi) = mesh.bounds().unwrap();
        assert!(
            lo.abs_diff_eq(glam::Vec3::new(-3., 7., -3.), 1e-4),
            "{lo:?}"
        );
        assert!(hi.abs_diff_eq(glam::Vec3::new(3., 27., 3.), 1e-4), "{hi:?}");
        let t = app.thread_targets(&[movable]).unwrap();
        assert_eq!(t.len(), 1);
        let t = &t[0];
        assert!(t.origin[0].abs() < 1e-4 && t.origin[2].abs() < 1e-4);
        assert!((t.origin[1] - 7.).abs() < 1e-4);
        assert!((t.radius - 3.).abs() < 1e-4);
        assert!(app.doc.undo());
        assert_eq!(app.doc.bodies, original);
        assert!(app.doc.redo());
        app.selected = vec![movable];
        app.align_reference = None;
        app.axis_align.fixed_body = None;
        app.axis_align.point = [2., 4., 6.];
        app.axis_align.direction = [1., 1., 0.];
        app.align_selected_to_axis();
        let target = app.thread_targets(&[movable]).unwrap().remove(0);
        let axis = glam::DVec3::from_array(target.axis);
        let desired = glam::DVec3::new(1., 1., 0.).normalize();
        assert!(axis.dot(desired).abs() > 0.99999);
        let radial = glam::DVec3::from_array(target.origin) - glam::DVec3::new(2., 4., 6.);
        assert!((radial - desired * radial.dot(desired)).length() < 1e-4);
        assert!((target.radius - 3.).abs() < 1e-4);
        let Feature::Primitive3D { params } = app.body(movable).unwrap().feature else {
            panic!()
        };
        let rebuilt = S33DApp::build_insert_3d_shape(&mut app.kernel, &params).unwrap();
        assert!(target.matches_full_wall(&app.kernel.tessellate(rebuilt).unwrap()));
        let before = app.doc.bodies.clone();
        app.axis_align.direction = [0.; 3];
        app.align_selected_to_axis();
        assert_eq!(app.doc.bodies, before);
    }
}
