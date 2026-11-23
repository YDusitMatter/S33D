//! Workspace multi-clicks exit tools before normal face/object selection.
use super::*;

#[derive(Debug, Clone)]
pub(super) enum ClickToolState {
    Boolean(BooleanCtx),
    Split(SplitCtx),
    Mirror(MirrorCtx),
    OffsetFaces(Vec<u32>),
    None,
}
impl ClickToolState {
    pub(super) fn capture(app: &S33DApp) -> Self {
        match app.mode {
            Mode::Boolean => app
                .boolean_ctx
                .clone()
                .map(Self::Boolean)
                .unwrap_or(Self::None),
            Mode::Split => app.split_ctx.clone().map(Self::Split).unwrap_or(Self::None),
            Mode::Mirror => app
                .mirror_ctx
                .clone()
                .map(Self::Mirror)
                .unwrap_or(Self::None),
            Mode::OffsetShell => app
                .offset_ctx
                .as_ref()
                .map(|c| Self::OffsetFaces(c.faces.clone()))
                .unwrap_or(Self::None),
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SidebarClickState {
    gizmo: Option<GizmoMode>,
    align: bool,
    reference: Option<BodyId>,
    axis: axis_align::AxisAlign,
    array: bool,
    chamfer: bool,
    chamfer_committed: bool,
}

impl SidebarClickState {
    pub(super) fn capture(app: &S33DApp) -> Self {
        Self {
            gizmo: app.gizmo,
            align: app.align_active,
            reference: app.align_reference,
            axis: app.axis_align.clone(),
            array: app.array_active,
            chamfer: app.chamfer_active,
            chamfer_committed: app.chamfer_committed,
        }
    }
    fn active(&self) -> bool {
        self.gizmo.is_some() || self.align || self.array || self.chamfer
    }
}

impl S33DApp {
    /// Follow actual operation provenance, including multiple pockets and
    /// retained Boolean/fillet bases. Never infer ownership from coincidence.
    pub(super) fn pocket_sources(&self, body_id: BodyId) -> Vec<BodyId> {
        let mut result = Vec::new();
        let mut pending = vec![body_id];
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if visited.len() >= 32 || !visited.insert(id) {
                continue;
            }
            let Some(body) = self.doc.body(id) else {
                continue;
            };
            if let Feature::Extrude {
                pocket_source_sketches,
                ..
            } = &body.feature
            {
                result.extend(pocket_source_sketches);
            }
            if let Feature::Derived { source, .. } = &body.feature {
                if let Some(saved) = source
                    .strip_prefix("Thread v1: ")
                    .and_then(|text| serde_json::from_str::<crate::threads::Saved>(text).ok())
                {
                    result.extend(saved.pocket_sources);
                }
                if let Some(saved) = source
                    .strip_prefix("Cylinder diameter v1: ")
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                {
                    if let Some(ids) = saved
                        .get("pocket_sources")
                        .and_then(|ids| serde_json::from_value::<Vec<BodyId>>(ids.clone()).ok())
                    {
                        result.extend(ids);
                    }
                }
            }
            pending.extend(rebuild_feature_dependencies(&body.feature));
        }
        result.sort_unstable();
        result.dedup();
        result
    }

    /// A matching pocket mouth is an overlay, not a cap closing its cavity.
    /// Keep its visible floor/wall reachable without hiding the consumed sketch.
    /// Other profiles retain the usual nearest-region/coplanar priority.
    pub(super) fn face_precedes_sketch_region(
        &mut self,
        face_hit: &Option<(FaceHit, f64)>,
        region_hit: &Option<(BodyId, usize, f64)>,
    ) -> bool {
        let Some((face, face_distance)) = face_hit else {
            return false;
        };
        let Some((sketch_id, region_index, region_distance)) = region_hit else {
            return true;
        };
        if face_distance + 1e-6 < *region_distance {
            return true;
        }
        let consumed_by_pocket = self.pocket_sources(face.body_id).contains(sketch_id);
        let Some(sketch) = self
            .profile_sketch(*sketch_id)
            .filter(|sketch| sketch.on_body == Some(face.body_id) || consumed_by_pocket)
        else {
            return false;
        };
        if consumed_by_pocket {
            if let Some(region) = self.regions_of(*sketch_id).get(*region_index) {
                if self.pocket_bottom_matches(face, &sketch, region) {
                    return true;
                }
            }
        }
        let Some(target) = self
            .thread_target_for_face(face)
            .filter(|target| target.internal)
        else {
            return false;
        };
        let axis = glam::DVec3::from_array(target.axis);
        if sketch.plane.normal().normalize().dot(axis).abs() < 1. - 1e-6 {
            return false;
        }
        let origin = glam::DVec3::from_array(target.origin);
        let offset = (sketch.plane.origin() - origin).dot(axis);
        let tolerance = (target.radius * 1e-4).max(1e-4);
        if offset.abs().min((offset - target.length).abs()) > tolerance {
            return false;
        }
        let regions = self.regions_of(*sketch_id);
        let Some(region) = regions.get(*region_index) else {
            return false;
        };
        // Require the entire closed ring to coincide with this particular
        // bore mouth; arbitrary profiles and neighboring holes still win.
        if !region.holes.is_empty() || region.outer.len() < 12 {
            return false;
        }
        let radial_tolerance = (target.radius * 0.01).min(0.02).max(1e-4);
        let center = origin + axis * offset;
        let (u, v) = sketch.plane.to_local(center.to_array());
        s33d_model::point_in_region([u, v], region)
            && region.outer.iter().all(|point| {
                let delta =
                    glam::DVec3::from_array(sketch.plane.to_world(point[0], point[1])) - origin;
                ((delta - axis * delta.dot(axis)).length() - target.radius).abs()
                    <= radial_tolerance
            })
    }

    pub(super) fn collapse_tool_menus(&mut self) {
        self.sketch_tools_open = false;
        self.draw3d_tools_open = false;
        self.thread_ui.expanded = false;
        self.thread_ui.print_open = false;
        self.thread_ui.save_open = false;
    }

    pub(super) fn array_exit_signature(&self) -> String {
        format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}",
            self.selected,
            self.array_axes,
            self.array_positive,
            self.array_spacing,
            self.array_copies
        )
    }

    /// Apply the current editor state exactly once. Explicit Cancel actions
    /// remain separate. A failed operation leaves its parameters available.
    pub(super) fn apply_tool_before_exit(&mut self) -> bool {
        if self.model_job.is_some() {
            return false;
        }
        if self.background_modeling && self.needs_background_finish() {
            let title = self.exit_tool_name().unwrap_or(("Finish tool", "完成工具"));
            if let Err(error) = self.start_model_job(title, false, None, |worker| {
                if worker.exit_tool_for_workspace_selection() {
                    Ok(Value::Null)
                } else {
                    Err((
                        "geometry".into(),
                        worker
                            .toasts
                            .last()
                            .map(|t| t.0.clone())
                            .unwrap_or_else(|| "Check the tool parameters / 请检查工具参数".into()),
                    ))
                }
            }) {
                self.toast(error);
            }
            return false;
        }
        match self.mode {
            Mode::Sketch => self.exit_sketch(),
            Mode::Insert3D => self.exit_insert_3d(),
            Mode::EquationCurve => {
                if self.surface_ctx.is_some() {
                    self.finish_equation_surface();
                } else {
                    self.preview_equation_curve();
                    self.commit_equation_curve();
                }
            }
            Mode::Extrude => {
                if self.face_extrude_job.is_some() {
                    return false;
                }
                let Some(mut draft) = self.extrude_ctx.clone() else {
                    return false;
                };
                if !draft.depth.is_finite()
                    || draft.depth <= 0.
                    || (draft.pocket && draft.target_body.is_none())
                {
                    self.toast("Set a valid depth and target before exiting");
                    return false;
                }
                let before = self.doc.bodies.clone();
                self.commit_extrude();
                if self.face_extrude_job.is_some() {
                    return false;
                }
                if self.doc.bodies == before {
                    // commit_extrude owns/deletes the old preview, so retain
                    // only the editable inputs when its geometry fails.
                    draft.ghost_shapes.clear();
                    self.extrude_ctx = Some(draft);
                    self.mode = Mode::Extrude;
                    return false;
                }
            }
            Mode::RevolveAxis => {
                self.toast("Choose a rotation axis before exiting");
                return false;
            }
            Mode::Revolve => {
                let draft = self.revolve_ctx.clone();
                if draft.as_ref().is_none_or(|c| {
                    !c.angle_deg.is_finite()
                        || c.angle_deg.abs() < 1e-6
                        || c.axis_dir.iter().any(|v| !v.is_finite())
                        || c.axis_dir.iter().map(|v| v * v).sum::<f64>() < 1e-12
                }) {
                    self.toast("Set a valid rotation angle and axis before exiting");
                    return false;
                }
                let before = self.doc.bodies.clone();
                self.commit_revolve();
                if self.doc.bodies == before {
                    self.revolve_ctx = draft;
                    self.mode = Mode::Revolve;
                    return false;
                }
            }
            Mode::Boolean => {
                if self
                    .boolean_ctx
                    .as_ref()
                    .is_some_and(|c| c.op == BoolOp::Union)
                {
                    self.apply_union_selected();
                } else {
                    self.apply_boolean_pair();
                }
            }
            Mode::Split => self.apply_split(),
            Mode::Mirror => self.apply_mirror(),
            Mode::OffsetShell => {
                self.poll_offset_preview();
                if self
                    .offset_ctx
                    .as_ref()
                    .is_some_and(|c| c.preview.is_none() && c.job.is_none() && c.error.is_none())
                {
                    self.apply_offset_preview();
                }
                self.commit_offset_shell();
            }
            Mode::SectionView => self.cancel_section_view(), // view only
            Mode::PickPlane => self.mode = Mode::Idle,
            Mode::Idle => {
                let before = self.doc.bodies.clone();
                if self.chamfer_active
                    && self
                        .selected_edge
                        .as_ref()
                        .is_some_and(|e| !e.segments.is_empty())
                {
                    if !self.chamfer_state_applied() {
                        self.apply_chamfer();
                        if self.doc.bodies == before {
                            return false;
                        }
                    }
                    self.chamfer_committed = true;
                } else if self.array_active {
                    if self.array_exit_applied.as_ref() != Some(&self.array_exit_signature()) {
                        self.array_selected();
                        if self.doc.bodies == before {
                            return false;
                        }
                    }
                } else if self.align_active {
                    // An empty collection has no alignment to finish.
                    if self.axis_align.enabled {
                        self.axis_align.picking = false;
                        self.align_selected_to_axis();
                        if self.doc.bodies == before {
                            return false;
                        }
                    } else if self.selected.len() >= 2 {
                        self.align_selected(self.align_axis, self.align_mode);
                        if self.align_reference.is_none() {
                            return false;
                        }
                    }
                } else if let Some(gizmo) = self.gizmo {
                    match gizmo {
                        GizmoMode::Translate => {
                            if self.move_input != [0.; 3] {
                                if self.move_input.iter().any(|v| !v.is_finite()) {
                                    return false;
                                }
                                let matrix = glam::Mat4::from_translation(glam::Vec3::from_array(
                                    self.move_input,
                                ));
                                self.apply_transform(&matrix, "Move");
                                if self.doc.bodies == before {
                                    return false;
                                }
                                self.move_input = [0.; 3];
                            } else if self.move_to_input != self.exit_move_to_baseline {
                                if self.selected_center().is_none()
                                    || self.move_to_input.iter().any(|v| !v.is_finite())
                                {
                                    return false;
                                }
                                self.move_to_selected(self.move_to_input);
                            }
                        }
                        GizmoMode::Rotate => {
                            let angle = self.rotate_input.1;
                            if angle != 0. {
                                if !angle.is_finite() {
                                    return false;
                                }
                                self.rotate_selected(angle);
                                if self.doc.bodies == before {
                                    return false;
                                }
                                self.rotate_input.1 = 0.;
                            }
                        }
                        GizmoMode::Scale => {
                            if self.scale_input != [100.; 3] {
                                self.scale_selected_by_percent(self.scale_input);
                                if self.doc.bodies == before {
                                    return false;
                                }
                            } else if self.scale_size_input != self.exit_scale_size_baseline {
                                self.scale_selected_to_size(self.scale_size_input);
                                if self.doc.bodies == before {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.mode == Mode::Idle
    }

    pub(super) fn chamfer_state_applied(&self) -> bool {
        let Some(edge) = &self.selected_edge else {
            return true;
        };
        let target = match self.doc.body(edge.body_id).map(|b| &b.feature) {
            Some(Feature::Chamfer { target, .. }) => *target,
            _ => edge.body_id,
        };
        !self.chamfer_committed
            && self
                .find_chamfer_body(target)
                .and_then(|id| self.doc.body(id))
                .is_some_and(|b| match &b.feature {
                    Feature::Chamfer {
                        edges,
                        chamfer,
                        fillet,
                        ..
                    } => {
                        same_chamfer_location(&edge.segments, edges)
                            && if self.chamfer_fillet {
                                *fillet == Some(self.chamfer_param.max(0.05) as f64)
                            } else {
                                *chamfer == Some(self.chamfer_param.max(0.05) as f64)
                            }
                    }
                    _ => false,
                })
    }

    pub(super) fn tool_exit_waiting(&self) -> bool {
        self.model_job.is_some()
            || self.face_extrude_job.is_some()
            || self.offset_ctx.as_ref().is_some_and(|c| c.job.is_some())
            || self.surface_preview_pending()
    }

    pub(super) fn poll_tool_exit(&mut self) {
        if self.tool_exit_waiting() {
            return;
        }
        let Some(response) = self.pending_tool_exit.take() else {
            return;
        };
        if !self.exit_tool_for_workspace_selection() {
            if self.tool_exit_waiting() {
                self.pending_tool_exit = Some(response);
            }
            return;
        }
        if let Some(click) = response {
            // Replay the selection through the original view after native work
            // finishes, without applying the just-completed tool a second time.
            self.replay_workspace_exit_click(click, true);
        }
    }
    pub(super) fn workspace_multiclick_available(&self) -> bool {
        if self.drawing.show_sheet()
            || self.progress_busy()
            || self.project_session.pending.is_some()
        {
            return false;
        }
        match self.mode {
            // These stages use consecutive point clicks to construct geometry.
            // Do not reinterpret the second/third point as an exit gesture.
            Mode::Sketch => self.sketch.as_ref().is_none_or(|s| s.pending.is_empty()),
            Mode::Insert3D => self.insert_3d_ctx.as_ref().is_none_or(|c| {
                matches!(
                    c.stage,
                    Draw3DPlacementStage::ChoosePrimitive | Draw3DPlacementStage::Precision
                )
            }),
            Mode::Idle
            | Mode::PickPlane
            | Mode::Extrude
            | Mode::RevolveAxis
            | Mode::Revolve
            | Mode::Boolean
            | Mode::Split
            | Mode::SectionView
            | Mode::Mirror
            | Mode::OffsetShell
            | Mode::EquationCurve => true,
        }
    }

    pub(super) fn advance_workspace_click(
        &mut self,
        pos: egui::Pos2,
        native_multiclick: bool,
    ) -> u8 {
        if !self.workspace_multiclick_available() {
            self.last_click = None;
            self.click_count = 0;
            self.click_selection_base = None;
            return 1;
        }
        let now = std::time::Instant::now();
        let continues = self.last_click.is_some_and(|(time, point)| {
            // Native input timing survives a long geometry computation between
            // clicks; wall-clock time spent inside the tool is not user delay.
            (native_multiclick || now.duration_since(time).as_secs_f64() < 0.55)
                && point.distance(pos) < 7.
        });
        let mut count = if continues {
            self.click_count.saturating_add(1)
        } else {
            1
        };
        // A native double-click may belong to a point-placement stage that
        // deliberately reset our sequence. It must not bypass that boundary.
        if native_multiclick && continues {
            count = count.max(2);
        }
        if count > 3 {
            count = 1;
        }
        if count == 1 || self.click_selection_base.is_none() {
            self.click_selection_base = Some(self.capture_click_selection());
        }
        self.last_click = Some((now, pos));
        self.click_count = count;
        count
    }

    /// Exit is Apply + Done. Collapse menus only after successful completion.
    pub(super) fn exit_tool_for_workspace_selection(&mut self) -> bool {
        let mut sequence = (
            self.last_click,
            self.click_count,
            self.click_selection_base.clone(),
        );
        let exited = self.replaying_tool_exit || self.apply_tool_before_exit();
        if exited {
            self.deactivate_sidebar_tools();
            if let Some(base) = sequence.2.as_mut() {
                // The third click belongs to the same selection sequence,
                // but must never reactivate/reapply the completed tool.
                base.sidebar = SidebarClickState::capture(self);
                base.operation = ClickToolState::None;
            }
            self.collapse_tool_menus();
            self.set_hint("Ready");
            self.bump_scene();
        } else if self.tool_exit_waiting() {
            if self.pending_tool_exit.is_none() {
                self.pending_tool_exit = Some(None);
            }
        } else {
            self.set_hint(
                "Cannot finish the current tool: check its parameters and required selections",
            );
        }
        (self.last_click, self.click_count, self.click_selection_base) = sequence;
        exited
    }

    pub(super) fn restore_sidebar_for_exit_click(&mut self) {
        if self.replaying_tool_exit {
            return;
        }
        let Some(snapshot) = self.click_selection_base.clone() else {
            return;
        };
        // The first click of an exit gesture can temporarily pick an operand
        // or replace a cutting plane. Apply the editor the user was leaving,
        // rather than that incidental first-click input.
        match snapshot.operation {
            ClickToolState::Boolean(ctx) if self.mode == Mode::Boolean => {
                self.restore_click_selection();
                self.boolean_ctx = Some(ctx);
            }
            ClickToolState::Split(mut ctx) if self.mode == Mode::Split => {
                if let Some(current) = self.split_ctx.take() {
                    for shape in current.previews {
                        self.kernel.delete(shape);
                    }
                }
                ctx.previews.clear();
                let plane = ctx.plane_def.clone();
                let source = ctx.plane_source.unwrap_or(MirrorPlaneSource::Surface);
                let face = ctx.plane.clone();
                self.split_ctx = Some(ctx);
                if let Some(plane) = plane {
                    self.rebuild_split_preview(plane, source, face);
                }
            }
            ClickToolState::Mirror(mut ctx) if self.mode == Mode::Mirror => {
                if let Some(current) = self.mirror_ctx.take() {
                    for (_, shape) in current.previews {
                        self.kernel.delete(shape);
                    }
                }
                ctx.previews.clear();
                let plane = ctx.plane_def.clone();
                let source = ctx.plane_source.unwrap_or(MirrorPlaneSource::Surface);
                let face = ctx.plane.clone();
                self.mirror_ctx = Some(ctx);
                if let Some(plane) = plane {
                    self.rebuild_mirror_preview(plane, source, face);
                }
            }
            ClickToolState::OffsetFaces(faces) if self.mode == Mode::OffsetShell => {
                if let Some(ctx) = self.offset_ctx.as_mut() {
                    if ctx.faces != faces {
                        ctx.faces = faces;
                        ctx.preview = None;
                        ctx.job = None;
                        ctx.error = None;
                    }
                }
            }
            _ => {}
        }
        if self.mode != Mode::Idle {
            return;
        }
        if !snapshot.sidebar.active() {
            return;
        }
        self.restore_click_selection();
        self.gizmo = snapshot.sidebar.gizmo;
        self.align_active = snapshot.sidebar.align;
        self.align_reference = snapshot.sidebar.reference;
        self.axis_align = snapshot.sidebar.axis;
        self.array_active = snapshot.sidebar.array;
        self.chamfer_active = snapshot.sidebar.chamfer;
        self.chamfer_committed = snapshot.sidebar.chamfer_committed;
    }
}
