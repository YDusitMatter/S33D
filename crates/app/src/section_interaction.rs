//! View-only section manipulation. No document or undo mutations.
use super::*;

pub(super) fn crosses(mesh: &MeshData, plane: &PlaneDef) -> bool {
    let n = plane.normal();
    let d = plane.origin().dot(n);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    // Use referenced vertices: unused vertices must not turn a miss into a cut.
    for &index in &mesh.indices {
        if let Some(p) = mesh.positions.get(index as usize) {
            let s = glam::DVec3::from_array(p.map(f64::from)).dot(n) - d;
            lo = lo.min(s);
            hi = hi.max(s);
        }
    }
    let tolerance = ((hi - lo).abs() * 1e-6).max(1e-5);
    lo < -tolerance && hi > tolerance
}

pub(crate) fn equation(plane: &PlaneDef) -> [f64; 4] {
    let n = plane.normal();
    [n.x, n.y, n.z, n.dot(plane.origin())]
}

impl S33DApp {
    fn section_handle(&mut self, rect: egui::Rect) -> Option<(glam::Vec3, egui::Pos2)> {
        let plane = self.section_view_ctx.as_ref()?.plane_def?;
        let center = self
            .visible_solid_bounds()
            .map(|(min, max)| (min + max) * 0.5)
            .unwrap_or(glam::Vec3::ZERO);
        let n = plane.normal().as_vec3();
        let origin = plane.origin().as_vec3();
        let point = center - n * (center - origin).dot(n);
        Some((point, self.world_to_viewport(point, rect)?))
    }

    pub(crate) fn paint_section_handle(&mut self, painter: &egui::Painter, rect: egui::Rect) {
        if self.mode != Mode::SectionView {
            return;
        }
        if let Some((_, screen)) = self.section_handle(rect) {
            let normal = self
                .section_view_ctx
                .as_ref()
                .unwrap()
                .plane_def
                .unwrap()
                .normal()
                .as_vec3();
            let delta = egui::vec2(normal.dot(self.camera.right()), -normal.dot(self.camera.up));
            let direction = if delta.length() > 0.15 {
                delta.normalized()
            } else {
                egui::vec2(0., -1.)
            };
            let color = self.settings.accent();
            painter.arrow(
                screen - direction * 24.,
                direction * 48.,
                egui::Stroke::new(2.0_f32, color),
            );
            painter.circle_filled(screen, 7., color);
            painter.text(
                screen + egui::vec2(12., 12.),
                egui::Align2::LEFT_TOP,
                self.settings
                    .language
                    .text("Drag section plane", "拖动剖面"),
                egui::FontId::proportional(12.),
                color,
            );
        }
    }

    pub(crate) fn section_pointer(&mut self, response: &egui::Response) -> bool {
        if self.mode != Mode::SectionView {
            return false;
        }
        let dragging = self
            .section_view_ctx
            .as_ref()
            .is_some_and(|c| c.drag_start.is_some());
        if !dragging
            && ((response.is_pointer_button_down_on()
                && response.ctx.input(|i| i.pointer.primary_down()))
                || response.drag_started_by(egui::PointerButton::Primary))
        {
            let start = response.ctx.input(|i| i.pointer.press_origin());
            if let (Some(start), Some((_, screen))) = (start, self.section_handle(response.rect)) {
                if start.distance(screen) <= 30. {
                    let ctx = self.section_view_ctx.as_mut().unwrap();
                    ctx.drag_start = ctx.plane_def.map(|plane| (start, plane));
                    response.ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
            }
        }
        let Some((start, original)) = self.section_view_ctx.as_ref().and_then(|c| c.drag_start)
        else {
            return false;
        };
        if let Some(current) = response
            .interact_pointer_pos()
            .filter(|p| p.distance(start) > 0.1)
        {
            let point = original.origin().as_vec3();
            let offset =
                self.normal_drag_distance(current - start, original.normal().as_vec3(), point);
            let mut plane = original;
            plane.origin = (original.origin() + original.normal() * offset).to_array();
            let ctx = self.section_view_ctx.as_mut().unwrap();
            ctx.plane_def = Some(plane);
            ctx.plane = None;
            ctx.plane_source = Some(MirrorPlaneSource::Equation);
            ctx.equation = equation(&plane);
            ctx.equation_enabled = true;
            self.bump_scene();
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            || !response.ctx.input(|i| i.pointer.primary_down())
        {
            self.section_view_ctx.as_mut().unwrap().drag_start = None;
        }
        true
    }

    pub(super) fn normal_drag_distance(
        &self,
        delta: egui::Vec2,
        normal: glam::Vec3,
        center: glam::Vec3,
    ) -> f64 {
        let projected = egui::vec2(normal.dot(self.camera.right()), -normal.dot(self.camera.up));
        let direction = if projected.length() > 0.15 {
            projected.normalized()
        } else {
            egui::vec2(0., -1.)
        };
        (delta.dot(direction) * self.world_per_pixel_at(center)) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn misses_and_tangencies_show_whole_body_on_either_side() {
        let mesh = MeshData::new(
            vec![[0., 0., -2.], [1., 0., 2.], [0., 1., 2.], [0., 0., 1000.]],
            vec![0, 1, 2],
        );
        let mut plane = PlaneDef::from_normal([0., 0., 1.]);
        assert!(crosses(&mesh, &plane));
        for z in [-10., -2., 2., 10.] {
            plane.origin[2] = z;
            assert!(!crosses(&mesh, &plane), "z={z}");
        }
        plane.origin[2] = 1.;
        assert_eq!(equation(&plane), [0., 0., 1., 1.]);
    }
}
