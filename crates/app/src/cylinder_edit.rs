//! Certified cylindrical-wall edits. Work commits as one undo step.
use super::*;
use crate::threads::Target;
use glam::DVec3;

impl S33DApp {
    pub(crate) fn edit_cylinder_diameter(
        &mut self,
        target: Target,
        diameter: f64,
    ) -> Result<(), String> {
        if self.background_modeling {
            return self.start_model_job(
                ("Change cylinder diameter", "修改圆柱直径"),
                false,
                None,
                move |worker| {
                    worker
                        .edit_cylinder_diameter(target, diameter)
                        .map(|_| Value::Null)
                        .map_err(|e| ("geometry".into(), e))
                },
            );
        }
        let (shape, feature) = self.cylinder_diameter_result(&target, diameter)?;
        if self.doc.body(target.body_id).and_then(|b| b.shape) == Some(shape) {
            return Ok(());
        }
        self.commit_cylinder_diameter(target.body_id, shape, feature, diameter)
    }

    pub(crate) fn cylinder_diameter_editable(&self, target: &Target) -> bool {
        self.doc.body(target.body_id).is_some_and(|body| {
            matches!(&body.feature, Feature::Primitive3D { params }
                if params.kind == Primitive3DKind::Cylinder && !target.internal)
                || matches!(&body.feature, Feature::Extrude { .. })
                || body
                    .shape
                    .and_then(|shape| self.kernel.exact_shape(shape))
                    .is_some()
        })
    }

    /// Correct the geometry to the requested nominal size, then keep its new
    /// wall selected. Build, validate and resolve the display selection before
    /// committing so a failed correction never edits the document or history.
    pub(crate) fn correct_thread_blank(&mut self, target: Target) -> Result<(), String> {
        if self.background_modeling {
            return self.start_model_job(
                ("Correct according to nominal diameter", "按公称直径修正"),
                false,
                None,
                move |worker| {
                    worker
                        .correct_thread_blank(target)
                        .map(|_| Value::Null)
                        .map_err(|e| ("geometry".into(), e))
                },
            );
        }
        let expected = crate::threads::corrected_blank_target(&target, &self.thread_params)?;
        let diameter = expected.radius * 2.;
        let (shape, feature) = self.cylinder_diameter_result(&target, diameter)?;
        let corrected = if self.kernel.exact_shape(shape).is_some() {
            self.cylinder_walls_on_shape(shape, target.body_id)?
                .into_iter()
                .find(|t| t.same_wall(&expected))
                .ok_or("Corrected cylindrical wall could not be verified / 无法验证修正后的柱面")?
        } else {
            expected
        };
        crate::threads::validate(&corrected, &self.thread_params)?;
        let mesh = self.mesh_for(shape);
        let mut triangles = Vec::new();
        let mut area = 0.;
        let mut center = DVec3::ZERO;
        let mut normal = DVec3::ZERO;
        for tri in mesh
            .indices
            .chunks_exact(3)
            .filter(|tri| corrected.contains_triangle(&mesh, tri))
        {
            let points = [tri[0], tri[1], tri[2]]
                .map(|i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)));
            let cross = (points[1] - points[0]).cross(points[2] - points[0]);
            let weight = cross.length() * 0.5;
            if weight <= 1e-12 {
                continue;
            }
            if triangles.is_empty() {
                normal = cross.normalize();
            }
            triangles.extend_from_slice(tri);
            center += (points[0] + points[1] + points[2]) * (weight / 3.);
            area += weight;
        }
        if triangles.is_empty() || !area.is_finite() {
            return Err("Corrected wall cannot be selected / 无法选中修正后的柱面".into());
        }
        let face = FaceHit {
            body_id: target.body_id,
            normal: normal.as_vec3().to_array(),
            center: (center / area).as_vec3().to_array(),
            triangles,
            area_mm2: area,
        };
        self.commit_cylinder_diameter(target.body_id, shape, feature, diameter)?;
        self.face_pick = Some(face);
        self.thread_choices = vec![corrected];
        self.thread_checked = vec![true];
        self.open_thread_panel = true;
        self.thread_ui.expanded = true;
        self.thread_ui.error = None;
        Ok(())
    }

    fn cylinder_diameter_result(
        &mut self,
        target: &Target,
        diameter: f64,
    ) -> Result<(ShapeId, Feature), String> {
        self.agent_require_idle().map_err(|(_, e)| e)?;
        if !diameter.is_finite() || diameter <= 1e-5 {
            return Err("Diameter must be finite and positive / 直径必须是有限正数".into());
        }
        let old_shape = self
            .doc
            .body(target.body_id)
            .and_then(|b| b.shape)
            .ok_or("Missing solid")?;
        let current = self.thread_targets(&[target.body_id])?;
        if !current
            .iter()
            .any(|t| t.face_id == target.face_id && t.same_wall(&target))
        {
            return Err("Surface changed; select it again / 表面已变化，请重新选择".into());
        }
        let consumed_pockets = self.pocket_sources(target.body_id);
        if self.doc.bodies.iter().any(|b| {
            b.id != target.body_id
                && (rebuild_feature_dependencies(&b.feature).contains(&target.body_id)
                    || match &b.feature {
                        Feature::Sketch(s) => {
                            s.on_body == Some(target.body_id) && !consumed_pockets.contains(&b.id)
                        }
                        Feature::Extrude { sketch: s, .. } | Feature::Revolve { sketch: s, .. } => {
                            s.on_body == Some(target.body_id)
                        }
                        Feature::SurfaceTexture { source, .. } => *source == target.body_id,
                        _ => false,
                    })
        }) {
            return Err("This solid has dependent features; edit their source parameters first / 此实体有下游特征，请先修改源参数".into());
        }
        let new_radius = diameter * 0.5;
        if (new_radius - target.radius).abs() <= 1e-6 {
            return Ok((
                old_shape,
                self.doc.body(target.body_id).unwrap().feature.clone(),
            ));
        }
        self.report_model_progress(("Rebuilding cylindrical wall…", "正在重建圆柱面…"), None)?;
        // Draw 3D cylinders currently use a faceted extrusion. Rebuild
        // their own radius parameter, rather than fusing an analytic sleeve
        // to a polygon's circumscribed circle (which only touches its corners).
        if let Feature::Primitive3D { params } = &self.doc.body(target.body_id).unwrap().feature {
            if params.kind == Primitive3DKind::Cylinder && !target.internal {
                let mut params = *params;
                params.radius = new_radius;
                let shape = Self::build_insert_3d_shape(&mut self.kernel, &params)?;
                self.report_model_progress(("Validating solid…", "正在检查实体…"), None)?;
                return Ok((shape, Feature::Primitive3D { params }));
            }
        }
        if !self.cylinder_diameter_editable(target) {
            return Err("Diameter editing requires a parametric cylinder, sketch extrusion or exact solid; imported mesh cylinders are read-only / 直径编辑需要参数圆柱、草图拉伸实体或精确实体，导入网格柱面只读".into());
        }
        let mesh = self
            .kernel
            .tessellate(old_shape)
            .map_err(|e| e.to_string())?;
        // A sketch extrusion is a faceted Truck B-rep, not an OCCT analytic
        // cylinder. Validate the real wall and solid before promoting it; do
        // not trust an Extrude recipe as proof of a still-complete cylinder.
        if !target.matches_full_wall(&mesh) {
            return Err("The selected cylinder is incomplete / 所选柱面不完整".into());
        }
        let base = if self.kernel.exact_shape(old_shape).is_some() {
            old_shape
        } else {
            self.report_model_progress(
                ("Validating faceted extrusion…", "正在验证分段拉伸实体…"),
                None,
            )?;
            let exact = s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh)
                .map_err(|e| e.to_string())?;
            self.kernel.insert_exact(exact)
        };
        let solid_count = self.kernel.exact_shape(base).unwrap().solid_count;
        let axis = DVec3::from_array(target.axis);
        let radial = axis.any_orthonormal_vector();
        let plane = PlaneDef {
            origin: target.origin,
            x_axis: radial.to_array(),
            y_axis: target.axis,
        };
        // Overlap the actual chordal wall, not just its fitted circle. A
        // cutter starting at the nominal radius misses the polygon's chords
        // and leaves thin cylindrical slivers when enlarging a sketch bore.
        let (wall_min, wall_max) = wall_radial_span(&mesh, target)?;
        // Keep intersections away from cap triangulation vertices; a nearly
        // tangent micrometre sleeve produces unmeshable sliver faces in OCCT.
        let overlap = 0.002_f64.max(target.radius * 1e-6);
        let (lo, hi) = if new_radius > target.radius {
            ((wall_min - overlap).max(0.), new_radius)
        } else {
            (new_radius, wall_max + overlap)
        };
        let ring = Region2D {
            outer: vec![[lo, 0.], [hi, 0.], [hi, target.length], [lo, target.length]],
            holes: vec![],
        };
        let sleeve = self
            .kernel
            .revolve_region(
                &ring,
                &plane,
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .map_err(|e| e.to_string())?;
        let add_material = (new_radius > target.radius) != target.internal;
        let exact_result = self
            .kernel
            .exact_shape(base)
            .unwrap()
            .boolean_refined(
                self.kernel.exact_shape(sleeve).unwrap(),
                if add_material { 0 } else { 1 },
            )
            .map_err(|e| e.to_string())?;
        let result = self.kernel.insert_exact(exact_result);
        self.report_model_progress(("Validating solid…", "正在检查实体…"), None)?;
        let exact = self
            .kernel
            .exact_shape(result)
            .ok_or("Edit did not produce an exact solid")?;
        if exact.solid_count != solid_count || solid_count == 0 {
            return Err("Diameter would split or remove a solid / 此直径会分裂或删除实体".into());
        }
        let walls = self.cylinder_walls_on_shape(result, target.body_id)?;
        let expected = Target {
            radius: new_radius,
            ..target.clone()
        };
        if !walls.iter().any(|t| t.same_wall(&expected))
            || current
                .iter()
                .filter(|t| !t.same_wall(&target))
                .any(|other| !walls.iter().any(|t| t.same_wall(other)))
        {
            return Err("Diameter intersects another feature or cannot preserve the complete wall / 此直径与其他特征相交，或无法保留完整柱面".into());
        }
        Ok((
            result,
            Feature::Derived {
                source: if consumed_pockets.is_empty() {
                    format!("Cylinder diameter: {diameter} mm")
                } else {
                    format!(
                        "Cylinder diameter v1: {}",
                        json!({
                            "diameter": diameter, "pocket_sources": consumed_pockets,
                        })
                    )
                },
                operation: None,
            },
        ))
    }

    fn cylinder_walls_on_shape(
        &self,
        shape: ShapeId,
        body_id: BodyId,
    ) -> Result<Vec<Target>, String> {
        let mut walls = Vec::new();
        if let Some(exact) = self.kernel.exact_shape(shape) {
            walls.extend(
                exact
                    .cylinders()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|c| Target {
                        body_id,
                        face_id: c[0] as u32,
                        origin: [c[1], c[2], c[3]],
                        axis: [c[4], c[5], c[6]],
                        radius: c[7],
                        length: c[8],
                        internal: c[9] != 0.,
                    }),
            );
        }
        let mesh = self.kernel.tessellate(shape).map_err(|e| e.to_string())?;
        for wall in crate::mesh_cylinders::targets(&mesh, body_id) {
            if !walls.iter().any(|other| other.same_wall(&wall)) {
                walls.push(wall);
            }
        }
        Ok(walls)
    }

    fn commit_cylinder_diameter(
        &mut self,
        id: BodyId,
        shape: ShapeId,
        feature: Feature,
        diameter: f64,
    ) -> Result<(), String> {
        self.agent_prepare_mutation().map_err(|(_, e)| e)?;
        self.begin_document_change();
        let body = self.doc.body_mut(id).unwrap();
        body.shape = Some(shape);
        body.feature = feature;
        self.selected = vec![id];
        self.clear_region_selection();
        self.face_pick = None;
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!("Change cylinder diameter to {diameter} mm"));
        self.bump_scene();
        Ok(())
    }
}

/// Radial extent of the actual side triangles, including each polygon chord's
/// closest point to the axis. End caps are excluded by contains_triangle.
fn wall_radial_span(mesh: &MeshData, target: &Target) -> Result<(f64, f64), String> {
    let origin = DVec3::from_array(target.origin);
    let axis = DVec3::from_array(target.axis).normalize();
    let mut min = f64::INFINITY;
    let mut max = 0_f64;
    for tri in mesh
        .indices
        .chunks_exact(3)
        .filter(|tri| target.contains_triangle(mesh, tri))
    {
        let radial = [tri[0], tri[1], tri[2]].map(|i| {
            let relative = DVec3::from_array(mesh.positions[i as usize].map(f64::from)) - origin;
            relative - axis * relative.dot(axis)
        });
        for p in radial {
            max = max.max(p.length());
        }
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let edge = radial[b] - radial[a];
            let t = if edge.length_squared() > 1e-20 {
                (-radial[a].dot(edge) / edge.length_squared()).clamp(0., 1.)
            } else {
                0.
            };
            min = min.min((radial[a] + edge * t).length());
        }
    }
    if !min.is_finite() || min <= 0. || max < min {
        return Err("Cannot verify the selected cylindrical wall / 无法验证所选柱面".into());
    }
    Ok((min, max))
}

#[cfg(test)]
#[path = "cylinder_extrude_qa.rs"]
mod extrude_qa;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn tube() -> (S33DApp, BodyId) {
        let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
        let axis = DVec3::new(1., 2., 3.).normalize();
        let plane = PlaneDef {
            origin: [12., -8., 31.],
            x_axis: axis.any_orthonormal_vector().to_array(),
            y_axis: axis.to_array(),
        };
        let shape = app
            .kernel
            .revolve_region(
                &Region2D::new(vec![[3., 0.], [6., 0.], [6., 12.], [3., 12.]]),
                &plane,
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Tube".into(),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: "fixture.step".into(),
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        (app, id)
    }

    #[test]
    fn exact_diameter_edits_inner_outer_both_directions_and_survives_undo_reopen() {
        for (internal, diameter) in [(false, 14.), (false, 10.), (true, 8.), (true, 4.)] {
            let (mut app, id) = tube();
            let target = app
                .thread_targets(&[id])
                .unwrap()
                .into_iter()
                .find(|t| t.internal == internal)
                .unwrap();
            let original = app.doc.body(id).unwrap().clone();
            app.edit_cylinder_diameter(target.clone(), diameter)
                .unwrap();
            let expected = Target {
                radius: diameter * 0.5,
                ..target
            };
            assert!(app
                .thread_targets(&[id])
                .unwrap()
                .iter()
                .any(|t| t.same_wall(&expected)));
            let saved = app.project_to_s33d().unwrap();
            assert!(app.undo());
            assert_eq!(app.doc.body(id).unwrap(), &original);
            assert!(app.redo());
            let mut reopened = S33DApp::from_ui_state(UiStateFile::default(), false);
            reopened.load_s33d_project(&saved).unwrap();
            assert!(reopened
                .thread_targets(&[id])
                .unwrap()
                .iter()
                .any(|t| t.same_wall(&expected)));
            assert!(reopened
                .kernel
                .exact_shape(reopened.doc.body(id).unwrap().shape.unwrap())
                .is_some());
        }
    }

    #[test]
    fn invalid_intersecting_and_stale_diameter_leave_document_and_history_unchanged() {
        let (mut app, id) = tube();
        let target = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal)
            .unwrap();
        let before = app.doc.bodies.clone();
        let history = app.history.clone();
        for d in [0., -1., f64::NAN, f64::INFINITY, 13.] {
            assert!(app.edit_cylinder_diameter(target.clone(), d).is_err());
            assert_eq!(app.doc.bodies, before);
            assert_eq!(app.history, history);
        }
        let mut stale = target;
        stale.radius += 1.;
        assert!(app.edit_cylinder_diameter(stale, 8.).is_err());
        assert_eq!(app.doc.bodies, before);
    }

    fn corrected_selection(app: &mut S33DApp) -> Target {
        let face = app
            .face_pick
            .clone()
            .expect("Correct retains a selected wall");
        app.thread_target_for_face(&face)
            .expect("New display indices must resolve the resized wall")
    }

    fn finish_correction(app: &mut S33DApp) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.model_job.is_some() {
            assert!(Instant::now() < deadline, "Correction worker timed out");
            app.poll_model_job();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn correct_bore_to_nominal_shrinks_or_enlarges_then_generates_with_undo_and_reopen() {
        for preset in ["M6", "M8"] {
            let (mut app, id) = tube();
            let target = app
                .thread_targets(&[id])
                .unwrap()
                .into_iter()
                .find(|t| t.internal)
                .unwrap();
            let original = app.doc.body(id).unwrap().clone();
            let mut p = crate::threads::preset_params(preset, true).unwrap();
            p.length = 5.;
            p.offset = 1.;
            p.radial_fit = 0.05;
            p.left_hand = true;
            app.thread_params = p.clone();
            app.thread_ui.printing.process = crate::threads::config::PrintProcess::Resin;
            let printing = app.thread_ui.printing.clone();
            assert!(crate::threads::validate(&target, &p).is_err());
            app.correct_thread_blank(target).unwrap();
            let corrected = corrected_selection(&mut app);
            assert!((corrected.radius * 2. - (p.diameter - 2. * p.depth)).abs() < 1e-6);
            assert_eq!(app.thread_params, p);
            assert_eq!(app.thread_ui.printing, printing);
            assert_eq!(app.doc.body(id).unwrap().name, original.name);
            assert_eq!(app.doc.body(id).unwrap().material, original.material);
            assert_eq!(app.selected, [id]);
            assert!(app.thread_ui.expanded && app.open_thread_panel);
            assert!(app
                .thread_targets(&[id])
                .unwrap()
                .iter()
                .any(|t| !t.internal && (t.radius - 6.).abs() < 1e-6));
            crate::threads::validate(&corrected, &p).unwrap();
            let resized = app.doc.body(id).unwrap().clone();
            let project = app.project_to_s33d().unwrap();
            let mut reopened = S33DApp::from_ui_state(UiStateFile::default(), false);
            reopened.load_s33d_project(&project).unwrap();
            assert!(reopened
                .thread_targets(&[id])
                .unwrap()
                .iter()
                .any(|t| t.same_wall(&corrected)));
            // Cut the real mesh, rather than accepting only a changed number.
            app.apply_threads(vec![corrected], p.clone()).unwrap();
            let Feature::Derived { source, .. } = &app.doc.body(id).unwrap().feature else {
                panic!()
            };
            let saved: crate::threads::Saved =
                serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
            assert_eq!(saved.params, p);
            assert!(app.undo());
            assert_eq!(app.doc.body(id).unwrap(), &resized);
            assert!(app.undo());
            assert_eq!(app.doc.body(id).unwrap(), &original);
            assert!(app.redo());
            assert_eq!(app.doc.body(id).unwrap(), &resized);
        }
    }

    #[test]
    fn correct_external_taper_and_background_bore_retain_parameters_and_selection() {
        let (mut app, id) = tube();
        let outer = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| !t.internal)
            .unwrap();
        app.thread_params.diameter = 10.;
        app.thread_params.length = 5.;
        app.thread_params.taper = 0.0625;
        let p = app.thread_params.clone();
        app.correct_thread_blank(outer).unwrap();
        assert!((corrected_selection(&mut app).radius * 2. - 10.3125).abs() < 1e-6);
        assert_eq!(app.thread_params, p);
        let target = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal)
            .unwrap();
        app.thread_params = crate::threads::preset_params("M8", true).unwrap();
        let p = app.thread_params.clone();
        let before = app.doc.bodies.clone();
        let history = app.history.clone();
        app.thread_ui.expanded = true;
        app.background_modeling = true;
        app.correct_thread_blank(target.clone()).unwrap();
        assert_eq!(
            app.doc.bodies, before,
            "Must commit only after polling completion"
        );
        assert!(
            app.correct_thread_blank(target.clone()).is_err(),
            "No duplicate job"
        );
        app.cancel_model_job();
        app.poll_model_job();
        assert_eq!(app.doc.bodies, before);
        assert_eq!(app.history, history);
        assert_eq!(app.thread_params, p);
        assert!(app.thread_ui.expanded);
        app.correct_thread_blank(target).unwrap();
        finish_correction(&mut app);
        let corrected = corrected_selection(&mut app);
        crate::threads::validate(&corrected, &p).unwrap();
        assert_eq!(app.thread_params, p);
        assert!(app.thread_ui.expanded && app.open_thread_panel);
        assert!(!app.toasts.iter().any(|(s, _)| s == "Thread generated"));
        assert!(app.undo());
        assert_eq!(app.doc.bodies, before);
    }

    #[test]
    fn correct_rejects_invalid_interference_stale_and_mesh_without_document_changes() {
        let (mut app, id) = tube();
        let target = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal)
            .unwrap();
        let before = app.doc.bodies.clone();
        let history = app.history.clone();
        for diameter in [0., -1., f64::NAN, 14.] {
            app.thread_params.diameter = diameter;
            assert!(app.correct_thread_blank(target.clone()).is_err());
            assert_eq!(app.doc.bodies, before);
            assert_eq!(app.history, history);
            assert_eq!(app.thread_params.diameter.to_bits(), diameter.to_bits());
        }
        app.thread_params = crate::threads::preset_params("M8", true).unwrap();
        let mut stale = target.clone();
        stale.face_id += 100;
        assert!(app.correct_thread_blank(stale).is_err());
        assert_eq!(app.doc.bodies, before);
        let mesh = app
            .kernel
            .tessellate(app.doc.body(id).unwrap().shape.unwrap())
            .unwrap();
        app.doc.body_mut(id).unwrap().shape = Some(app.kernel.insert_mesh(mesh));
        let mesh_before = app.doc.bodies.clone();
        let mesh_target = app
            .thread_targets(&[id])
            .unwrap()
            .into_iter()
            .find(|t| t.internal)
            .unwrap();
        assert!(!app.cylinder_diameter_editable(&mesh_target));
        assert!(app
            .correct_thread_blank(mesh_target)
            .unwrap_err()
            .contains("read-only"));
        assert_eq!(app.doc.bodies, mesh_before);
        assert_eq!(app.history, history);
    }
}
