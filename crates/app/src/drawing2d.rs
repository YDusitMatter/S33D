// SPDX-License-Identifier: MPL-2.0
//! Orthographic, tessellation-based engineering sheets. Never mutates geometry.
use crate::{
    app::{S33DApp, UiUnit},
    mode::Mode,
};
use eframe::egui::{self, Color32, Stroke};
use glam::{Vec2, Vec3};
use s33d_kernel::{GeometryKernel, MeshData};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};
#[path = "drawing_edit.rs"]
mod edit;
#[path = "drawing_export.rs"]
mod export;
#[path = "drawing_features.rs"]
mod features;
#[path = "drawing_format.rs"]
mod format;
#[path = "drawing_interaction.rs"]
mod interaction;
#[path = "drawing_preview.rs"]
mod preview;
pub(crate) use export::export_to_path;
pub use export::menu as export_menu;
use features::Component;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DimensionKind {
    #[default]
    Smart,
    Horizontal,
    Vertical,
    Aligned,
    Diameter,
    Radius,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dimension {
    pub view: usize,
    pub a: Vec2,
    pub b: Vec2,
    pub label: Vec2,
    pub kind: DimensionKind,
}
impl Dimension {
    fn value(&self) -> f64 {
        (match self.kind {
            DimensionKind::Horizontal => (self.b.x - self.a.x).abs(),
            DimensionKind::Vertical => (self.b.y - self.a.y).abs(),
            DimensionKind::Aligned | DimensionKind::Smart | DimensionKind::Radius => {
                self.a.distance(self.b)
            }
            DimensionKind::Diameter => self.a.distance(self.b),
        }) as f64
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Projection {
    pub lines: Vec<[Vec2; 2]>,
    pub snaps: Vec<Vec2>,
    pub hidden: Vec<[Vec2; 2]>,
    pub min: Vec2,
    pub max: Vec2,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Drawing {
    pub open: bool,
    pub manual: bool,
    pub kind: DimensionKind,
    pub views: Vec<Projection>,
    pub dimensions: Vec<Dimension>,
    pub title: String,
    pub drawing_number: String,
    pub revision: String,
    pub material: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub drawing_date: String,
    pub show_hidden: bool,
    #[serde(skip)]
    pending_kind: Option<DimensionKind>,
    #[serde(skip)]
    rounds: Vec<Vec<interaction::Round>>,
    #[serde(skip)]
    pub message: String,
    #[serde(skip)]
    pub pending: Vec<(usize, Vec2)>,
    components: Vec<Component>,
    #[serde(skip)]
    preview: preview::Preview,
    page: usize,
    #[serde(skip)]
    schedule_cache: std::cell::RefCell<Option<(UiUnit, bool, Vec<String>)>>,
    #[serde(with = "text_map")]
    text_edits: HashMap<edit::TextKey, String>,
    #[serde(with = "text_map")]
    text_offsets: HashMap<edit::TextKey, Vec2>,
    #[serde(skip)]
    editor: Option<edit::Editor>,
    #[serde(skip)]
    dimension_drag: Option<edit::AnnotationDrag>,
    #[serde(skip)]
    undo: Vec<edit::Snapshot>,
    #[serde(skip)]
    redo: Vec<edit::Snapshot>,
    #[serde(skip)]
    job: Option<ProjectionJob>,
}
impl Drawing {
    pub(crate) fn projection_pending(&self) -> bool {
        self.job.is_some()
    }

    fn checkpoint(&mut self) {
        if self.undo.len() == 200 {
            self.undo.remove(0);
        }
        self.undo.push(edit::Snapshot::capture(self));
        self.redo.clear();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(edit::Snapshot::capture(self));
            previous.restore(self);
        }
        self.pending.clear();
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(edit::Snapshot::capture(self));
            next.restore(self);
        }
        self.pending.clear();
    }
    fn clear(&mut self) {
        if !self.dimensions.is_empty() {
            self.checkpoint();
            self.dimensions.clear();
            self.text_edits.retain(|k, _| k.dimension.is_none());
            self.editor = None;
        }
        self.pending.clear();
    }
}
struct ProjectionJob {
    started: std::time::Instant,
    rx: Receiver<Result<Vec<Projection>, String>>,
    cancelled: Arc<AtomicBool>,
    components: Vec<Component>,
}
impl Drop for ProjectionJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl S33DApp {
    pub(crate) fn drawing_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = self.drawing.job.as_ref() else {
            return;
        };
        let elapsed = job.started.elapsed().as_secs_f64();
        if elapsed < 0.25 {
            return;
        }
        if crate::app::model_progress::progress_modal(
            ctx,
            self.settings.language,
            ("3-view drawing", "三视图"),
            ("Projecting model…", "正在计算投影…"),
            None,
            elapsed,
        ) {
            self.drawing.job = None; // Drop signals cooperative cancellation.
            self.drawing.message = self
                .settings
                .language
                .text("Projection cancelled", "已取消投影")
                .into();
        }
    }

    pub fn toggle_drawing_menu(&mut self) {
        if self.drawing.open {
            self.drawing.open = false;
            self.drawing.pending.clear();
            return;
        }
        if !self.finish_creation_for_tool_switch() || self.mode != Mode::Idle {
            return;
        }
        self.exit_sidebar_tool();
        self.sketch_tools_open = false;
        self.draw3d_tools_open = false;
        self.drawing.open = true;
        self.drawing.show_hidden = true;
        self.drawing.kind = DimensionKind::Smart;
        if self.drawing.title.is_empty() {
            self.drawing.title = self.doc.name().to_owned();
        }
    }
    pub fn generate_drawing(&mut self) {
        if self.drawing.job.is_some() {
            return;
        }
        let ids = self.selected.clone();
        let mut meshes = Vec::new();
        let mut components = Vec::new();
        let mut count = 0;
        for body in &self.doc.bodies {
            if !(if ids.is_empty() {
                body.visible
            } else {
                ids.contains(&body.id)
            }) {
                continue;
            }
            let Some(shape) = body.shape else { continue };
            let mesh = match self
                .mesh_cache
                .get(&shape)
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| self.kernel.tessellate(shape))
            {
                Ok(mesh) => mesh,
                Err(error) => {
                    self.drawing.message = format!(
                        "{} {}: {error}",
                        self.settings
                            .language
                            .text("Projection failed for", "无法投影对象"),
                        body.name
                    );
                    return;
                }
            };
            count += mesh.triangle_count();
            if count > 200_000 {
                self.drawing.message = self
                    .settings
                    .language
                    .text(
                        "Too many triangles (limit 200000); select fewer objects.",
                        "三角面超过 200000，请减少选择对象。",
                    )
                    .into();
                return;
            }
            if !mesh.is_empty() {
                components.push(features::component(self, body, &mesh));
                meshes.push(mesh);
            }
        }
        if meshes.is_empty() {
            self.drawing.message = self
                .settings
                .language
                .text(
                    "Select visible 3D geometry first.",
                    "请先选择可见的三维对象。",
                )
                .into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.drawing.job = Some(ProjectionJob {
            started: std::time::Instant::now(),
            rx,
            cancelled: cancelled.clone(),
            components,
        });
        self.drawing.message.clear();
        // Each request captures meshes only: workers never own kernel/native handles.
        std::thread::spawn(move || {
            let _ = tx.send(project(&meshes, &cancelled));
        });
    }

    pub fn sync_drawing_preview(&mut self) {
        preview::sync(self);
    }
}

// Test visibility at the actual projected point. Sampling a nearby depth pixel
// puts cylinder tangents behind their own rounded wall and drops whole outlines.
struct DepthNode {
    min: Vec2,
    max: Vec2,
    max_z: f32,
    triangles: Vec<[Vec3; 3]>,
    children: Option<Box<[DepthNode; 2]>>,
}
impl DepthNode {
    fn build(mut triangles: Vec<[Vec3; 3]>) -> Self {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        let mut max_z = f32::NEG_INFINITY;
        for t in &triangles {
            for p in t {
                min = min.min(p.truncate());
                max = max.max(p.truncate());
                max_z = max_z.max(p.z);
            }
        }
        let children = if triangles.len() > 12 {
            let axis = if max.x - min.x > max.y - min.y { 0 } else { 1 };
            triangles.sort_unstable_by(|a, b| {
                ((a[0][axis] + a[1][axis] + a[2][axis]) / 3.)
                    .total_cmp(&((b[0][axis] + b[1][axis] + b[2][axis]) / 3.))
            });
            let right = triangles.split_off(triangles.len() / 2);
            let left = std::mem::take(&mut triangles);
            Some(Box::new([Self::build(left), Self::build(right)]))
        } else {
            None
        };
        Self {
            min,
            max,
            max_z,
            triangles,
            children,
        }
    }
    fn obscures(&self, p: Vec3, tolerance: f32) -> bool {
        if p.x < self.min.x - tolerance
            || p.y < self.min.y - tolerance
            || p.x > self.max.x + tolerance
            || p.y > self.max.y + tolerance
            || self.max_z <= p.z + tolerance
        {
            return false;
        }
        if let Some(c) = &self.children {
            return c[0].obscures(p, tolerance) || c[1].obscures(p, tolerance);
        }
        for [a, b, c] in &self.triangles {
            let area = cross(b.truncate() - a.truncate(), c.truncate() - a.truncate());
            if area.abs() < 1e-12 {
                continue;
            }
            let u = cross(b.truncate() - p.truncate(), c.truncate() - p.truncate()) / area;
            let v = cross(c.truncate() - p.truncate(), a.truncate() - p.truncate()) / area;
            let w = 1. - u - v;
            if u >= -1e-6
                && v >= -1e-6
                && w >= -1e-6
                && a.z * u + b.z * v + c.z * w > p.z + tolerance
            {
                return true;
            }
        }
        false
    }
}
fn projected(p: Vec3, view: usize) -> Vec3 {
    match view {
        0 => Vec3::new(p.x, p.y, p.z),
        1 => Vec3::new(p.x, -p.z, p.y),
        _ => Vec3::new(-p.z, p.y, p.x),
    }
}
fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}
fn merge_straight_lines(lines: Vec<[Vec2; 2]>, epsilon: f32) -> Vec<[Vec2; 2]> {
    // End circles viewed edge-on consist of many collinear tessellation
    // chords. Union their intervals so widths/steps are actual full lengths,
    // not dozens of spurious chord dimensions. Never bridge a visible gap.
    let mut axes: HashMap<(usize, i64), Vec<(f32, f32, f32)>> = HashMap::new();
    let mut out = Vec::new();
    for [a, b] in lines {
        let delta = (b - a).abs();
        let axis = if delta.y < epsilon {
            0
        } else if delta.x < epsilon {
            1
        } else {
            out.push([a, b]);
            continue;
        };
        let station = (a[1 - axis] + b[1 - axis]) * 0.5;
        axes.entry((axis, (station / epsilon).round() as i64))
            .or_default()
            .push((a[axis].min(b[axis]), a[axis].max(b[axis]), station));
    }
    for ((axis, _), mut intervals) in axes {
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut lo, mut hi, station) = intervals[0];
        let segment = |lo, hi| {
            let mut a = Vec2::splat(station);
            let mut b = a;
            a[axis] = lo;
            b[axis] = hi;
            [a, b]
        };
        for (a, b, _) in intervals.into_iter().skip(1) {
            if a <= hi + epsilon {
                hi = hi.max(b);
            } else {
                out.push(segment(lo, hi));
                lo = a;
                hi = b;
            }
        }
        out.push(segment(lo, hi));
    }
    out
}
fn project(meshes: &[MeshData], cancelled: &AtomicBool) -> Result<Vec<Projection>, String> {
    let mut out = Vec::new();
    for view in 0..3 {
        let mut points = Vec::new();
        let mut faces = Vec::new();
        let mut candidates = Vec::new();
        for mesh in meshes {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            if mesh.positions.iter().flatten().any(|x| !x.is_finite())
                || mesh.indices.len() % 3 != 0
                || mesh
                    .indices
                    .iter()
                    .any(|&i| i as usize >= mesh.positions.len())
            {
                return Err("Invalid mesh".into());
            }
            let mut mesh = mesh.clone();
            mesh.weld(1e-5);
            let base = points.len();
            points.extend(
                mesh.positions
                    .iter()
                    .map(|p| projected(Vec3::from_array(*p), view)),
            );
            let mut edges: HashMap<(u32, u32), Vec<Vec3>> = HashMap::new();
            for t in mesh.indices.chunks_exact(3) {
                let a = points[base + t[0] as usize];
                let b = points[base + t[1] as usize];
                let c = points[base + t[2] as usize];
                let normal = (b - a).cross(c - a).normalize_or_zero();
                faces.push([a, b, c]);
                for (x, y) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    edges.entry((x.min(y), x.max(y))).or_default().push(normal);
                }
            }
            for ((a, b), n) in edges {
                if n.len() != 2
                    || n[0].dot(n[1]) < 0.866
                    || (n[0].z * n[1].z <= 0. && (n[0].z - n[1].z).abs() > 1e-6)
                {
                    candidates.push([points[base + a as usize], points[base + b as usize]]);
                }
            }
        }
        if points.is_empty() {
            return Err("Empty geometry".into());
        }
        let min = points
            .iter()
            .fold(Vec2::splat(f32::INFINITY), |a, p| a.min(p.truncate()));
        let max = points
            .iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), |a, p| a.max(p.truncate()));
        let extent = (max - min).max(Vec2::splat(1e-4));
        const N: usize = 384;
        let pixel = extent / (N as f32 - 1.);
        let depth = DepthNode::build(faces);
        let mut lines = Vec::new();
        let mut hidden = Vec::new();
        let mut snaps = Vec::new();
        let tolerance = extent.length() * 2e-6 + 1e-5;
        for [a, b] in candidates {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let steps = (((b - a).truncate() / pixel).length().ceil() as usize).clamp(1, 768);
            let mut start = 0;
            let mut previous = !depth.obscures(a, tolerance);
            for i in 1..=steps {
                let p = a.lerp(b, i as f32 / steps as f32);
                let visible = !depth.obscures(p, tolerance);
                if visible != previous || i == steps {
                    let end = if i == steps { i } else { i.saturating_sub(1) };
                    let begin = a.lerp(b, start as f32 / steps as f32).truncate();
                    let finish = a.lerp(b, end as f32 / steps as f32).truncate();
                    if begin.distance(finish) > 1e-5 {
                        if previous {
                            lines.push([begin, finish]);
                        } else {
                            hidden.push([begin, finish]);
                        }
                    }
                    start = i;
                    previous = visible;
                }
            }
            for point in [a, b] {
                if !depth.obscures(point, tolerance) {
                    snaps.push(point.truncate());
                }
            }
        }
        snaps.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        snaps.dedup_by(|a, b| a.distance(*b) < 1e-5);
        let mut lines = merge_straight_lines(lines, 1e-5_f32.max(extent.max_element() * 1e-7));
        for line in &mut lines {
            if line[0]
                .x
                .total_cmp(&line[1].x)
                .then(line[0].y.total_cmp(&line[1].y))
                .is_gt()
            {
                line.swap(0, 1);
            }
        }
        lines.sort_by(|a, b| {
            a[0].x
                .total_cmp(&b[0].x)
                .then(a[0].y.total_cmp(&b[0].y))
                .then(a[1].x.total_cmp(&b[1].x))
                .then(a[1].y.total_cmp(&b[1].y))
        });
        lines.dedup_by(|a, b| a[0].distance(b[0]) < 1e-5 && a[1].distance(b[1]) < 1e-5);
        out.push(Projection {
            lines,
            hidden: merge_straight_lines(hidden, tolerance),
            snaps,
            min,
            max,
        });
    }
    Ok(out)
}
fn centers(i: usize) -> Vec2 {
    match i {
        0 => Vec2::new(80., 137.),
        1 => Vec2::new(80., 60.),
        _ => Vec2::new(215., 137.),
    }
}
fn fit(views: &[Projection]) -> f32 {
    views.iter().fold(f32::INFINITY, |s, v| {
        let e = (v.max - v.min).max(Vec2::splat(1e-4));
        s.min(96. / e.x).min(52. / e.y)
    })
}
fn sheet_point(p: Vec2, i: usize, v: &Projection, scale: f32) -> Vec2 {
    centers(i) + (p - (v.min + v.max) * 0.5) * Vec2::new(1., -1.) * scale
}
fn dimension_lines(d: &Dimension) -> ([Vec2; 2], Vec<[Vec2; 2]>) {
    let (a, b) = match d.kind {
        DimensionKind::Horizontal => (Vec2::new(d.a.x, d.label.y), Vec2::new(d.b.x, d.label.y)),
        DimensionKind::Vertical => (Vec2::new(d.label.x, d.a.y), Vec2::new(d.label.x, d.b.y)),
        DimensionKind::Diameter | DimensionKind::Radius => {
            return ([d.b, d.label], vec![[d.a, d.b], [d.b, d.label]]);
        }
        DimensionKind::Aligned | DimensionKind::Smart => {
            let n = (d.b - d.a).normalize_or_zero().perp();
            let offset = (d.label - d.a).dot(n);
            (d.a + n * offset, d.b + n * offset)
        }
    };
    ([a, b], vec![[d.a, a], [d.b, b], [a, b]])
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
#[derive(Default)]
struct Sheet {
    lines: Vec<([Vec2; 2], bool)>,
    texts: Vec<(Vec2, String, f32)>,
    dashed: Vec<[Vec2; 2]>,
    bindings: HashMap<usize, edit::Field>,
    cells: Vec<(Vec2, Vec2, edit::Field)>,
    dimension_labels: HashMap<usize, usize>,
    // Text index -> leader segment; moving a feature label keeps its anchor fixed.
    leaders: HashMap<usize, usize>,
}
fn sheet(d: &Drawing, unit: UiUnit, zh: bool) -> Sheet {
    sheet_page(d, unit, zh, d.page)
}
fn sheet_page(d: &Drawing, unit: UiUnit, zh: bool, page: usize) -> Sheet {
    let mut sheet = raw_sheet_page(d, unit, zh, page);
    edit::apply_overrides(d, &mut sheet, page);
    sheet
}
fn raw_sheet_page(d: &Drawing, unit: UiUnit, zh: bool, page: usize) -> Sheet {
    if page > 0 {
        return features::schedule(d, unit, zh, page);
    }
    let mut out = Sheet::default();
    if d.views.len() != 3 {
        return out;
    }
    let scale = fit(&d.views);
    for (i, v) in d.views.iter().enumerate() {
        if d.show_hidden {
            for line in &v.hidden {
                out.dashed.push(line.map(|p| sheet_point(p, i, v, scale)));
            }
        }
        for line in &v.lines {
            out.lines.push((
                [
                    sheet_point(line[0], i, v, scale),
                    sheet_point(line[1], i, v, scale),
                ],
                false,
            ));
        }
        out.texts.push((
            centers(i) + Vec2::new(0., -(v.max.y - v.min.y) * scale * 0.5 - 7.),
            if zh {
                ["前视图", "俯视图", "右视图"][i]
            } else {
                ["FRONT", "TOP", "RIGHT"][i]
            }
            .into(),
            0.,
        ));
    }
    for (dimension_index, dim) in d.dimensions.iter().enumerate() {
        let v = &d.views[dim.view];
        let (ends, lines) = dimension_lines(dim);
        for line in lines {
            out.lines.push((
                [
                    sheet_point(line[0], dim.view, v, scale),
                    sheet_point(line[1], dim.view, v, scale),
                ],
                true,
            ));
        }
        let a = sheet_point(ends[0], dim.view, v, scale);
        let b = sheet_point(ends[1], dim.view, v, scale);
        let dir = (b - a).normalize_or_zero();
        let n = dir.perp();
        for (p, sign) in [(a, 1.), (b, -1.)].into_iter().take(
            if matches!(dim.kind, DimensionKind::Diameter | DimensionKind::Radius) {
                1
            } else {
                2
            },
        ) {
            for side in [-1., 1.] {
                out.lines
                    .push(([p, p + dir * (sign * 2.) + n * (side * 0.7)], true));
            }
        }
        let mut angle = dir.y.atan2(dir.x);
        if angle > std::f32::consts::FRAC_PI_2 {
            angle -= std::f32::consts::PI;
        }
        if angle < -std::f32::consts::FRAC_PI_2 {
            angle += std::f32::consts::PI;
        }
        out.dimension_labels
            .insert(out.texts.len(), dimension_index);
        out.texts.push((
            (a + b) * 0.5 + Vec2::new(angle.sin(), -angle.cos()) * 2.5,
            format!(
                "{}{}",
                match dim.kind {
                    DimensionKind::Diameter => "Ø",
                    DimensionKind::Radius => "R",
                    _ => "",
                },
                unit.format_mm(dim.value())
            ),
            angle,
        ));
    }
    for (view, [a, b]) in features::center_marks(d) {
        let v = &d.views[view];
        out.lines.push((
            [
                sheet_point(a, view, v, scale),
                sheet_point(b, view, v, scale),
            ],
            true,
        ));
    }
    out.texts.push((
        Vec2::new(148., 176.),
        format!(
            "{} 2–{}",
            if zh {
                "关键尺寸明细见图纸"
            } else {
                "Key dimension schedule: sheets"
            },
            features::pages(d, unit, zh)
        ),
        0.,
    ));
    features::callouts(d, &mut out, unit, zh);
    format::title_block(d, &mut out, unit, zh, 0);
    out
}
#[cfg(test)]
fn svg(d: &Drawing, unit: UiUnit, zh: bool) -> String {
    svg_page(d, unit, zh, 0)
}
fn svg_page(d: &Drawing, unit: UiUnit, zh: bool, page: usize) -> String {
    let s = sheet_page(d, unit, zh, page);
    let mut result=String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"297mm\" height=\"210mm\" viewBox=\"0 0 297 210\"><rect width=\"297\" height=\"210\" fill=\"white\"/><rect x=\"7\" y=\"7\" width=\"283\" height=\"196\" fill=\"none\" stroke=\"black\" stroke-width=\"0.3\"/>");
    for [a, b] in s.dashed {
        result += &format!("<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"#666\" stroke-width=\"0.18\" stroke-dasharray=\"1.8 1\"/>", a.x,a.y,b.x,b.y);
    }
    for ([a, b], thin) in s.lines {
        result += &format!(
            "<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"{}\"/>",
            a.x,
            a.y,
            b.x,
            b.y,
            if thin { 0.18 } else { 0.3 }
        );
    }
    for (p, t, angle) in s.texts {
        result+=&format!("<text x=\"{}\" y=\"{}\" transform=\"rotate({} {} {})\" font-size=\"3\" font-family=\"sans-serif\" text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>",p.x,p.y,angle.to_degrees(),p.x,p.y,xml(&t));
    }
    result += "</svg>";
    result
}
#[cfg(test)]
fn export_files(
    d: &Drawing,
    unit: UiUnit,
    zh: bool,
    path: &std::path::Path,
) -> std::io::Result<usize> {
    export::files(d, unit, zh, path, export::ExportFormat::Svg)
}
pub fn panel(app: &mut S33DApp, ui: &mut egui::Ui) {
    let lang = app.settings.language;
    let unit = app.settings.unit;
    ui.heading(lang.text("3-view Drawing", "三视图工程图"));
    if ui
        .add_enabled(
            app.drawing.job.is_none(),
            egui::Button::new(lang.text("Generate / Refresh", "生成 / 刷新")),
        )
        .clicked()
    {
        app.generate_drawing();
    }
    if app.drawing.editor.is_some() {
        edit::panel(&mut app.drawing, ui, lang);
    }
    if ui
        .button(lang.text("Automatic dimensions", "自动标注"))
        .clicked()
    {
        auto_dimensions(&mut app.drawing);
    }
    if ui
        .checkbox(
            &mut app.drawing.manual,
            lang.text("Manual dimensions", "手动标注"),
        )
        .changed()
    {
        app.drawing.pending.clear();
    }
    ui.horizontal_wrapped(|ui| {
        for (kind, en, zh) in [
            (DimensionKind::Smart, "Smart dimension", "智能尺寸"),
            (
                DimensionKind::Horizontal,
                "Width / horizontal",
                "宽度 / 水平",
            ),
            (DimensionKind::Vertical, "Height / vertical", "高度 / 垂直"),
            (DimensionKind::Aligned, "Length / aligned", "长度 / 对齐"),
            (DimensionKind::Diameter, "Diameter Ø", "直径 Ø"),
            (DimensionKind::Radius, "Radius R", "半径 R"),
        ] {
            if ui
                .selectable_value(&mut app.drawing.kind, kind, lang.text(en, zh))
                .changed()
            {
                app.drawing.pending.clear();
                app.drawing.manual = true;
            }
        }
    });
    edit::selection_summary(app, ui);
    ui.horizontal_wrapped(|ui| {
        if ui.button(lang.text("Undo dimension", "撤销标注")).clicked() {
            app.drawing.undo();
        }
        if ui
            .add_enabled(
                app.drawing.can_redo(),
                egui::Button::new(lang.text("Redo", "重做")),
            )
            .clicked()
        {
            app.drawing.redo();
        }
        if ui
            .button(lang.text("Clear dimensions", "清除标注"))
            .clicked()
        {
            app.drawing.clear();
        }
    });
    export_menu(app, ui, lang.text("Export…", "导出…"));
    let pages = features::pages(
        &app.drawing,
        unit,
        lang == crate::app::UiLanguage::ChineseSimplified,
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(app.drawing.page > 0, egui::Button::new("◀"))
            .clicked()
        {
            app.drawing.page -= 1;
            app.drawing.pending.clear();
        }
        ui.label(format!(
            "{} {} / {pages}",
            lang.text("Sheet", "图纸"),
            app.drawing.page + 1
        ));
        if ui
            .add_enabled(app.drawing.page + 1 < pages, egui::Button::new("▶"))
            .clicked()
        {
            app.drawing.page += 1;
            app.drawing.pending.clear();
        }
    });
    ui.checkbox(
        &mut app.drawing.show_hidden,
        lang.text("Hidden edges", "隐藏边虚线"),
    );
    ui.label(lang.text("Drawing title", "图纸标题"));
    edit::field_input(&mut app.drawing, ui, edit::Field::Title);
    format::fields(&mut app.drawing, ui, lang);
    if app.drawing.editor.is_none() {
        edit::panel(&mut app.drawing, ui, lang);
    }
    ui.label(lang.text(
        "A4 landscape · third-angle projection · fitted scale",
        "A4 横向 · 第三角投影 · 自动适配比例",
    ));
    app.sync_drawing_preview();
    let (preview_rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 160.), egui::Sense::hover());
    preview::paint(
        &app.drawing.preview,
        ui,
        preview_rect,
        lang == crate::app::UiLanguage::ChineseSimplified,
    );
    ui.separator();
    ui.collapsing(lang.text("Guide / Limits", "用法 / 限制"), |ui| {
        ui.label(lang.text("Uses the Objects selection directly. Empty selection uses all visible geometry. The 3D workspace stays visible until generation succeeds.", "直接使用对象列表的选择；未选择时使用全部可见几何。生成成功前保留三维工作区。"));
        ui.label(lang.text("Manual: select a highlighted edge or circle, then place the dimension. Linear tools also accept two endpoints or circle centers. Esc cancels.","手动：点击高亮边或圆，再点击放置尺寸；线性工具也可依次选端点或圆心。Esc 取消。"));
        ui.label(lang.text("Mesh-based visibility, not exact HLR. Auto dimensions include verified cylinders/bores, center spacing and straight steps. All source sizes, positions and known features are on additional dimension sheets; no design tolerances are inferred. SVG/DXF/EPS export one file per sheet; PDF exports all sheets in one document. DXF uses paper-space mm at the stated scale. Native DWG is not included; convert DXF with a separately licensed CAD tool. The 3D selection inset is a UI preview, not printed. Refresh after geometry edits.","遮挡基于网格，并非精确 HLR。自动标注包含已确认的圆柱/孔径、中心间距和直线台阶。各对象尺寸、位置和已知特征列于附加明细页；不推断设计公差。SVG/DXF/EPS 每页一个文件；PDF 全部图纸合为一份文档。DXF 使用纸面毫米和标示比例。未内置 DWG，可用另行许可的 CAD 工具转换 DXF。三维选择预览仅用于界面核对，不打印。模型修改后请刷新。"));
    });
    ui.small(lang.text(
        "Drawing and annotation edits are saved with the .s33d project.",
        "图纸和标注编辑随 .s33d 工程保存。",
    ));
    if app
        .drawing
        .components
        .iter()
        .any(|c| c.thread.is_some() && c.thread_annotations.is_empty())
    {
        ui.small(lang.text("Older thread metadata has no surface location. Its sizes remain in the schedule; regenerate the thread for located callouts.","旧螺纹记录没有柱面位置。尺寸仍在明细页；重新生成螺纹后可显示位置引线。"));
    }
    if !app.drawing.message.is_empty() {
        ui.label(&app.drawing.message);
    }
    if ui.button(lang.text("Exit Drawing", "退出工程图")).clicked() {
        app.drawing.open = false;
        app.drawing.pending.clear();
    }
}
pub fn auto_dimensions(d: &mut Drawing) {
    if d.views.len() != 3 {
        return;
    }
    d.checkpoint();
    d.dimensions.clear();
    d.text_edits.retain(|k, _| k.dimension.is_none());
    d.editor = None;
    d.pending.clear();
    let scale = fit(&d.views);
    let offset = 8. / scale;
    for (i, v) in d.views.iter().enumerate() {
        d.dimensions.push(Dimension {
            view: i,
            a: v.min,
            b: Vec2::new(v.max.x, v.min.y),
            label: v.min - Vec2::Y * offset,
            kind: DimensionKind::Horizontal,
        });
        d.dimensions.push(Dimension {
            view: i,
            a: v.min,
            b: Vec2::new(v.min.x, v.max.y),
            label: v.min - Vec2::X * offset,
            kind: DimensionKind::Vertical,
        });
    }
    features::annotate(d);
    for (view, rounds) in d.rounds.iter().enumerate() {
        for (i, c) in rounds
            .iter()
            .filter(|c| {
                c.arc.is_some()
                    || !d.components.iter().flat_map(|c| &c.cylinders).any(|t| {
                        let Some(r) = features::rim(t, view) else {
                            return false;
                        };
                        let end = r.center
                            + projected(Vec3::from_array(t.axis.map(|v| v as f32)), view)
                                .truncate()
                                * t.length as f32;
                        (r.center.distance(c.center) < 1e-3 || end.distance(c.center) < 1e-3)
                            && (r.radius - c.radius).abs() < 1e-3
                    })
            })
            .take(4)
            .enumerate()
        {
            let direction = Vec2::from_angle(
                c.arc
                    .map(|(start, sweep)| start + sweep * 0.5)
                    .unwrap_or(0.),
            );
            let kind = if c.arc.is_some() {
                DimensionKind::Radius
            } else {
                DimensionKind::Diameter
            };
            d.dimensions.push(Dimension {
                view,
                a: if kind == DimensionKind::Radius {
                    c.center
                } else {
                    c.center - direction * c.radius
                },
                b: c.center + direction * c.radius,
                label: c.center
                    + direction * (c.radius + 8. / scale)
                    + Vec2::Y * i as f32 * 4. / scale,
                kind,
            });
        }
    }
    d.page = 0;
}
pub fn poll(d: &mut Drawing, ctx: &egui::Context) {
    if let Some(rx) = &d.job {
        match rx.rx.try_recv() {
            Ok(Ok(views)) => {
                d.views = views;
                d.components = rx.components.clone();
                d.rounds = (0..3).map(|i| interaction::rounds(d, i)).collect();
                *d.schedule_cache.get_mut() = None;
                d.page = 0;
                d.dimensions.clear();
                d.undo.clear();
                d.redo.clear();
                d.pending.clear();
                d.text_edits.clear();
                d.text_offsets.clear();
                d.editor = None;
                d.dimension_drag = None;
                d.job = None;
            }
            Ok(Err(e)) => {
                d.message = e;
                d.job = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                d.message = "Projection worker stopped".into();
                d.job = None;
            }
            _ => {
                ctx.request_repaint_after(std::time::Duration::from_millis(33));
            }
        }
    }
}
pub fn canvas(d: &mut Drawing, ui: &mut egui::Ui, unit: UiUnit, zh: bool) {
    poll(d, ui.ctx());
    let size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let zoom = (size.x / 307.).min(size.y / 220.).max(0.01);
    let origin = rect.center() - egui::vec2(297., 210.) * zoom * 0.5;
    let screen = |p: Vec2| origin + egui::vec2(p.x, p.y) * zoom;
    let paint = ui.painter_at(rect);
    paint.rect_filled(
        egui::Rect::from_min_size(origin, egui::vec2(297., 210.) * zoom),
        0.,
        Color32::WHITE,
    );
    let border = [
        Vec2::new(7., 7.),
        Vec2::new(290., 7.),
        Vec2::new(290., 203.),
        Vec2::new(7., 203.),
    ];
    for i in 0..4 {
        paint.line_segment(
            [screen(border[i]), screen(border[(i + 1) % 4])],
            Stroke::new(1., Color32::BLACK),
        );
    }
    let sheet = sheet(d, unit, zh);
    let mut hits = Vec::new();
    for [a, b] in &sheet.dashed {
        format::paint_dashed(&paint, screen(*a), screen(*b), zoom);
    }
    for ([a, b], thin) in &sheet.lines {
        paint.line_segment(
            [screen(*a), screen(*b)],
            Stroke::new(if *thin { 0.7 } else { 1.1 }, Color32::BLACK),
        );
    }
    for (index, (p, text, angle)) in sheet.texts.iter().enumerate() {
        let galley = paint.layout_no_wrap(
            text.clone(),
            egui::FontId::proportional((3. * zoom).clamp(9., 16.)),
            Color32::BLACK,
        );
        let half = galley.size() * 0.5;
        let offset = egui::vec2(
            half.x * angle.cos() - half.y * angle.sin(),
            half.x * angle.sin() + half.y * angle.cos(),
        );
        paint.add(
            egui::epaint::TextShape::new(screen(*p) - offset, galley.clone(), Color32::BLACK)
                .with_angle(*angle),
        );
        hits.push(edit::TextHit {
            index,
            center: screen(*p),
            size: galley.size(),
            angle: *angle,
        });
    }
    if !d.manual {
        edit::canvas(d, ui, &response, &sheet, &hits, origin, zoom, unit, zh);
    }
    let preview_rect = egui::Rect::NOTHING;
    if d.views.is_empty() {
        paint.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if zh {
                "选择对象后点击生成"
            } else {
                "Select objects, then Generate"
            },
            egui::FontId::proportional(18.),
            Color32::DARK_GRAY,
        );
    }
    if d.job.is_some() {
        paint.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if zh {
                "正在投影…"
            } else {
                "Projecting…"
            },
            egui::FontId::proportional(18.),
            Color32::DARK_GRAY,
        );
    }
    if d.manual && d.views.len() == 3 && d.page == 0 {
        interaction::canvas(d, ui, &response, origin, zoom, preview_rect, unit, zh);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enclosed_cavity_is_hidden_but_dimensionable_and_exported_dashed() {
        let outer = box_mesh([30., 20., 16.]);
        let mut cavity = box_mesh([8., 6., 5.]);
        for p in &mut cavity.positions {
            p[0] += 7.;
            p[1] += 5.;
            p[2] += 4.;
        }
        for face in cavity.indices.chunks_exact_mut(3) {
            face.swap(0, 1);
        }
        let views = project(&[outer, cavity], &AtomicBool::new(false)).unwrap();
        for v in &views {
            assert!(
                !v.hidden.is_empty(),
                "closed cavity must retain hidden edges"
            );
        }
        let mut d = Drawing {
            views,
            show_hidden: true,
            ..Default::default()
        };
        let svg = svg(&d, UiUnit::Millimeter, false);
        assert!(svg.contains("stroke-dasharray"));
        d.show_hidden = false;
        assert!(!self::svg(&d, UiUnit::Millimeter, false).contains("stroke-dasharray"));
    }

    #[test]
    fn title_block_and_located_hole_thread_callouts_use_real_coordinates() {
        let views = project(&[box_mesh([30., 20., 16.])], &AtomicBool::new(false)).unwrap();
        let target = crate::threads::Target {
            body_id: 1,
            face_id: 0,
            origin: [7., 5., 4.],
            axis: [0., 0., 1.],
            radius: 2.,
            length: 5.,
            internal: true,
        };
        let mut d = Drawing {
            views,
            title: "Cavity part".into(),
            drawing_number: "DWG-042".into(),
            revision: "B".into(),
            material: "Aluminum".into(),
            drawn_by: "YDZM".into(),
            show_hidden: true,
            ..Default::default()
        };
        d.components.push(Component {
            id: 1,
            name: "Part".into(),
            cylinders: vec![target.clone()],
            thread_annotations: vec![crate::threads::ThreadAnnotation {
                target,
                params: crate::threads::Params {
                    diameter: 4.,
                    pitch: 0.7,
                    length: 3.,
                    offset: 1.,
                    ..Default::default()
                },
            }],
            ..Default::default()
        });
        auto_dimensions(&mut d);
        let output = svg(&d, UiUnit::Millimeter, false);
        for text in [
            "DWG-042",
            "Aluminum",
            "REV: B",
            "DRAWN: YDZM",
            "H1 Ø4.000 mm",
            "L 5.000 mm",
            "X 7.000 mm  Y 5.000 mm",
            "P0.700 mm",
            "OFFSET 1.000 mm",
        ] {
            assert!(output.contains(text), "{text}");
        }
        assert!(svg(&d, UiUnit::Inch, false).contains("H1 Ø0.1575 in"));
        if let Some(path) = std::env::var_os("S33D_DRAWING_FORMAT_OUTPUT") {
            std::fs::write(path, output).unwrap();
        }
    }
    fn cylinder_mesh(center: Vec3) -> MeshData {
        let n = 96u32;
        let mut positions = vec![center.to_array(), (center + Vec3::Y * 30.).to_array()];
        for y in [0., 30.] {
            for i in 0..n {
                let a = i as f32 * std::f32::consts::TAU / n as f32;
                positions.push((center + Vec3::new(a.cos() * 3., y, a.sin() * 3.)).to_array());
            }
        }
        let mut indices = Vec::new();
        for i in 0..n {
            let a = 2 + i;
            let b = 2 + (i + 1) % n;
            indices.extend([0, a, b, 1, b + n, a + n, a, a + n, b + n, a, b + n, b]);
        }
        MeshData {
            positions,
            indices,
            ..Default::default()
        }
    }
    #[test]
    fn cylinder_array_keeps_full_tangent_outlines_in_front_and_right() {
        let meshes: Vec<_> = (0..3)
            .flat_map(|x| {
                (0..3).map(move |z| cylinder_mesh(Vec3::new(x as f32 * 10., 0., z as f32 * 10.)))
            })
            .collect();
        let views = project(&meshes, &AtomicBool::new(false)).unwrap();
        for view in [0, 2] {
            let v = &views[view];
            for center in if view == 0 {
                [0., 10., 20.]
            } else {
                [0., -10., -20.]
            } {
                for x in [center - 3., center + 3.] {
                    assert!(
                        v.lines.iter().any(|[a, b]| (a.x - x).abs() < 1e-4
                            && (b.x - x).abs() < 1e-4
                            && a.y.min(b.y) < 1e-4
                            && a.y.max(b.y) > 29.999),
                        "view {view}, missing tangent {x}"
                    );
                }
            }
        }
    }
    #[test]
    fn key_dimensions_include_diameters_spacing_and_complete_schedule() {
        let meshes: Vec<_> = (0..3)
            .flat_map(|x| {
                (0..3).map(move |z| cylinder_mesh(Vec3::new(x as f32 * 10., 0., z as f32 * 10.)))
            })
            .collect();
        let mut d = Drawing {
            views: project(&meshes, &AtomicBool::new(false)).unwrap(),
            title: "Cylinder array".into(),
            ..Default::default()
        };
        for (i, m) in meshes.iter().enumerate() {
            let (min, max) = m.bounds().unwrap();
            let base = (min + max) * 0.5 - Vec3::Y * 15.;
            d.components.push(Component {
                id: (i + 1) as u64,
                name: format!("Cylinder {}", i + 1),
                min,
                max,
                cylinders: vec![crate::threads::Target {
                    body_id: (i + 1) as u64,
                    face_id: 0,
                    origin: base.as_dvec3().to_array(),
                    axis: [0., 1., 0.],
                    radius: 3.,
                    length: 30.,
                    internal: false,
                }],
                ..Default::default()
            });
        }
        auto_dimensions(&mut d);
        assert!(d
            .dimensions
            .iter()
            .any(|v| v.kind == DimensionKind::Diameter && (v.value() - 6.).abs() < 1e-4));
        assert!(
            d.dimensions
                .iter()
                .filter(|v| v.view == 1 && (v.value() - 10.).abs() < 1e-4)
                .count()
                >= 4
        );
        assert!(d.dimensions.iter().any(|v| (v.value() - 30.).abs() < 1e-4));
        let pages = features::pages(&d, UiUnit::Millimeter, false);
        assert!(pages >= 3, "no truncation of a long assembly schedule");
        let combined = (1..pages)
            .map(|p| svg_page(&d, UiUnit::Millimeter, false, p))
            .collect::<String>();
        for i in 1..=9 {
            assert!(combined.contains(&format!("#{i}  Cylinder {i}")));
        }
        assert!(combined.contains("Ø6.000 mm"));
        assert!(combined.contains("World minimum"));
        assert!(svg_page(&d, UiUnit::Inch, false, 1).contains("Ø0.2362 in"));
        let cn = svg_page(&d, UiUnit::Millimeter, true, 1);
        assert!(cn.contains("关键尺寸明细") && cn.contains("轴起点"));
        let path = std::env::temp_dir().join(format!("s33d-drawing-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        assert_eq!(
            export_files(&d, UiUnit::Millimeter, false, &path.join("array.svg")).unwrap(),
            pages
        );
        for page in 1..pages {
            assert!(path.join(format!("array-dimensions-{page}.svg")).is_file());
        }
        // Optional human review artifacts stay under the ignored target tree.
        if let Some(output) = std::env::var_os("S33D_DRAWING_ARRAY_OUTPUT") {
            export_files(&d, UiUnit::Millimeter, false, std::path::Path::new(&output)).unwrap();
            for format in [export::ExportFormat::Dxf, export::ExportFormat::Pdf] {
                export::files(
                    &d,
                    UiUnit::Millimeter,
                    false,
                    &std::path::Path::new(&output).with_extension(format.extension()),
                    format,
                )
                .unwrap();
            }
        }
        for entry in std::fs::read_dir(&path).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn selection_preview_tracks_objects_without_mutating_sheet_or_model() {
        let ctx = egui::Context::default();
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx));
        for i in 0..2 {
            let shape = app
                .kernel
                .insert_mesh(cylinder_mesh(Vec3::X * i as f32 * 10.));
            let id = app.doc.alloc_id();
            app.doc.add_body(s33d_model::Body {
                id,
                name: format!("Source {id}"),
                kind: s33d_model::BodyKind::Solid,
                feature: s33d_model::Feature::Imported {
                    path: "test".into(),
                },
                shape: Some(shape),
                material: Default::default(),
                visible: true,
            });
        }
        app.doc.body_mut(2).unwrap().feature = s33d_model::Feature::Chamfer {
            target: 1,
            base: Some(1),
            edges: vec![],
            reference_bounds: None,
            chamfer: None,
            fillet: Some(2.),
        };
        let before = app.doc.bodies.clone();
        app.select_from_objects(2, false);
        let camera = app.camera.view_matrix();
        app.toggle_drawing_menu();
        assert_eq!(app.selected, vec![2]);
        assert_eq!(app.camera.view_matrix(), camera);
        assert!(!app.drawing.show_sheet());
        app.select_from_objects(1, false);
        app.sync_drawing_preview();
        assert_eq!(app.drawing.preview.names, vec!["Source 1"]);
        app.select_from_objects(2, false);
        app.sync_drawing_preview();
        assert_eq!(app.drawing.preview.names, vec!["Source 2"]);
        app.select_from_objects(1, true);
        app.sync_drawing_preview();
        assert_eq!(app.drawing.preview.names.len(), 2);
        assert!(app.drawing.open && !app.chamfer_active);
        assert!(app.drawing.views.is_empty());
        assert_eq!(app.doc.bodies, before);
        app.doc.body_mut(2).unwrap().visible = false;
        app.sync_drawing_preview();
        assert_eq!(app.drawing.preview.names.len(), 2);
        app.generate_drawing();
        assert!(!app.drawing.show_sheet());
        let job = app.drawing.job.take().unwrap();
        assert_eq!(
            job.components.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            job.rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .unwrap()
                .len(),
            3
        );
    }
    #[test]
    fn saved_cylinder_session_regression_if_requested() {
        let Some(path) = std::env::var_os("S33D_DRAWING_QA_PROJECT") else {
            return;
        };
        let ctx = egui::Context::default();
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx));
        app.open_project_from_path(std::path::Path::new(&path))
            .unwrap();
        let before = app.doc.bodies.clone();
        app.selected = app
            .doc
            .bodies
            .iter()
            .filter(|b| b.visible && b.shape.is_some())
            .map(|b| b.id)
            .collect();
        app.generate_drawing();
        let job = app.drawing.job.take().unwrap();
        app.drawing.views = job
            .rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .unwrap();
        app.drawing.components = job.components.clone();
        assert_eq!(app.drawing.components.len(), 9);
        assert!(
            app.drawing
                .components
                .iter()
                .map(|c| c.cylinders.len())
                .sum::<usize>()
                >= 8
        );
        auto_dimensions(&mut app.drawing);
        for (view, x) in [(0, -4.), (0, 22.), (2, -23.), (2, 3.)] {
            assert!(
                app.drawing.views[view]
                    .lines
                    .iter()
                    .any(|[a, b]| (a.x - x).abs() < 1e-4
                        && (b.x - x).abs() < 1e-4
                        && (a.y - b.y).abs() > 21.),
                "source outline missing in view {view} at {x}"
            );
        }
        let rows = features::rows(&app.drawing, UiUnit::Millimeter, false).join("\n");
        assert!(rows.contains("Thread: Ø6.000 mm   P 1.000 mm   L 8.000 mm"));
        if let Some(output) = std::env::var_os("S33D_DRAWING_SOURCE_OUTPUT") {
            export_files(
                &app.drawing,
                UiUnit::Millimeter,
                false,
                std::path::Path::new(&output),
            )
            .unwrap();
        }
        assert_eq!(app.doc.bodies, before);
    }
    #[test]
    fn dimensions_measure_geometry_not_paper_scale() {
        for (kind, value) in [
            (DimensionKind::Horizontal, 3.),
            (DimensionKind::Vertical, 4.),
            (DimensionKind::Aligned, 5.),
        ] {
            let d = Dimension {
                view: 0,
                a: Vec2::ZERO,
                b: Vec2::new(3., 4.),
                label: Vec2::splat(8.),
                kind,
            };
            assert_eq!(d.value(), value);
        }
    }
    #[test]
    fn box_projections_units_svg_and_snapshot() {
        let mut kernel = s33d_kernel::TruckKernel::new();
        let r = s33d_kernel::Region2D {
            outer: vec![[0., 0.], [10., 0.], [10., 5.], [0., 5.]],
            holes: vec![],
        };
        let id = kernel
            .extrude_region(&r, &s33d_kernel::PlaneDef::from_normal([0., 0., 1.]), 20.)
            .unwrap();
        let mesh = kernel.tessellate(id).unwrap();
        let views = project(&[mesh], &AtomicBool::new(false)).unwrap();
        for (i, e) in [Vec2::new(10., 5.), Vec2::new(10., 20.), Vec2::new(20., 5.)]
            .iter()
            .enumerate()
        {
            assert!((views[i].max - views[i].min - *e).length() < 1e-4);
            assert!(!views[i].lines.is_empty());
        }
        let mut d = Drawing {
            views,
            title: "<part & screw>".into(),
            ..Default::default()
        };
        auto_dimensions(&mut d);
        assert_eq!(d.dimensions.len(), 6);
        let data = svg(&d, UiUnit::Millimeter, false);
        if let Some(path) = std::env::var_os("S33D_DRAWING_QA_OUTPUT") {
            std::fs::write(path, &data).unwrap();
        }
        assert!(data.contains("10.000 mm"));
        assert!(data.contains("&lt;part &amp; screw&gt;"));
        assert!(data.contains("297mm"));
        assert!(svg(&d, UiUnit::Inch, false).contains("0.3937 in"));
        auto_dimensions(&mut d);
        assert_eq!(d.dimensions.len(), 6);
        d.undo();
        assert_eq!(d.dimensions.len(), 6);
        d.undo();
        assert!(d.dimensions.is_empty());
        d.redo();
        assert_eq!(d.dimensions.len(), 6);
        d.clear();
        assert!(d.dimensions.is_empty());
        d.undo();
        assert_eq!(d.dimensions.len(), 6);
    }

    pub(super) fn box_mesh(size: [f64; 3]) -> MeshData {
        let mut kernel = s33d_kernel::TruckKernel::new();
        let region = s33d_kernel::Region2D {
            outer: vec![[0., 0.], [size[0], 0.], [size[0], size[1]], [0., size[1]]],
            holes: vec![],
        };
        let id = kernel
            .extrude_region(
                &region,
                &s33d_kernel::PlaneDef::from_normal([0., 0., 1.]),
                size[2],
            )
            .unwrap();
        kernel.tessellate(id).unwrap()
    }

    #[test]
    fn projection_omits_hidden_edges_and_does_not_snap_to_visibility_cuts() {
        let front = box_mesh([10., 10., 20.]);
        let mut rear = box_mesh([4., 4., 5.]);
        for p in &mut rear.positions {
            p[0] += 3.;
            p[1] += 3.;
            p[2] -= 10.;
        }
        let views = project(&[front, rear], &AtomicBool::new(false)).unwrap();
        let boundary = |p: Vec2| {
            p.x.abs() < 1e-5
                || p.y.abs() < 1e-5
                || (p.x - 10.).abs() < 1e-5
                || (p.y - 10.).abs() < 1e-5
        };
        assert!(!views[0].lines.is_empty());
        assert!(views[0].lines.iter().flatten().all(|p| boundary(*p)));
        assert!(views[0].snaps.iter().all(|p| boundary(*p)));
        assert!(project(&[], &AtomicBool::new(false)).is_err());
        assert!(project(&[box_mesh([1., 1., 1.])], &AtomicBool::new(true)).is_err());
        let invalid = MeshData {
            positions: vec![[0., 0., 0.]],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        assert!(project(&[invalid], &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn drawing_menus_snapshot_refresh_and_project_reset_are_safe() {
        let ctx = egui::Context::default();
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.settings = crate::app::AppSettings::default();
        let shape = app.kernel.insert_mesh(box_mesh([12., 8., 6.]));
        let id = app.doc.alloc_id();
        app.doc.add_body(s33d_model::Body {
            id,
            name: "Fixture".into(),
            kind: s33d_model::BodyKind::Solid,
            feature: s33d_model::Feature::Imported {
                path: "fixture".into(),
            },
            shape: Some(shape),
            material: Default::default(),
            visible: true,
        });
        let before = app.doc.bodies.clone();
        app.toggle_draw3d_menu();
        app.toggle_drawing_menu();
        assert!(app.drawing.open && !app.draw3d_tools_open && !app.sketch_tools_open);
        app.selected = vec![id];
        app.generate_drawing();
        let job = app.drawing.job.take().unwrap();
        app.drawing.views = job
            .rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(app.drawing.views.len(), 3);
        auto_dimensions(&mut app.drawing);
        app.toggle_drawing_menu();
        assert!(!app.drawing.open);
        app.toggle_drawing_menu();
        assert_eq!(app.drawing.dimensions.len(), 6);
        assert_eq!(app.doc.bodies, before);
        app.toggle_sketch_menu();
        assert!(!app.drawing.open && app.sketch_tools_open);
        app.toggle_drawing_menu();
        assert!(app.drawing.open && !app.sketch_tools_open);
        app.generate_drawing();
        let cancelled = app.drawing.job.as_ref().unwrap().cancelled.clone();
        app.doc.body_mut(id).unwrap().visible = false;
        app.generate_drawing(); // A running request must not be duplicated.
        assert!(app.drawing.job.is_some());
        app.mark_project_saved(None);
        app.request_project_action(crate::app::project_session::ProjectAction::New, &ctx);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(app.drawing.views.is_empty() && app.drawing.dimensions.is_empty());
        assert!(app.doc.bodies.is_empty());
        app.generate_drawing();
        assert!(app.drawing.job.is_none());
        assert!(!app.drawing.message.is_empty());
    }

    #[test]
    fn manual_dimension_three_clicks_and_undo_use_real_geometry() {
        let ctx = egui::Context::default();
        let views = project(&[box_mesh([10., 5., 20.])], &AtomicBool::new(false)).unwrap();
        let mut d = Drawing {
            views,
            manual: true,
            ..Default::default()
        };
        let mut position = egui::Pos2::ZERO;
        let mut run = |d: &mut Drawing, events: Vec<egui::Event>, point: Option<Vec2>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 800.),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(point) = point {
                        let size = ui.available_size();
                        let zoom = (size.x / 307.).min(size.y / 220.);
                        let origin = ui.max_rect().center() - egui::vec2(297., 210.) * zoom * 0.5;
                        let page = sheet_point(point, 0, &d.views[0], fit(&d.views));
                        position = origin + egui::vec2(page.x, page.y) * zoom;
                    }
                    canvas(d, ui, UiUnit::Millimeter, false);
                });
            });
            position
        };
        for point in [Vec2::ZERO, Vec2::new(10., 0.), Vec2::new(5., -5.)] {
            let pos = run(&mut d, vec![], Some(point));
            run(
                &mut d,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                None,
            );
            run(
                &mut d,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                None,
            );
        }
        assert_eq!(d.dimensions.len(), 1);
        assert!((d.dimensions[0].value() - 10.).abs() < 1e-6);
        assert!(d.pending.is_empty());
        d.undo();
        assert!(d.dimensions.is_empty());
        d.redo();
        assert_eq!(d.dimensions.len(), 1);
    }
}

mod text_map {
    use super::*;
    pub fn serialize<S: serde::Serializer, V: Serialize>(
        values: &HashMap<edit::TextKey, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        values.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>, V: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<HashMap<edit::TextKey, V>, D::Error> {
        Ok(Vec::<(edit::TextKey, V)>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn snapshot_roundtrip() {
        let mut d = Drawing::default();
        d.title = "三视图".into();
        d.open = true;
        d.views = vec![
            Projection {
                lines: vec![[Vec2::ZERO, Vec2::ONE]],
                ..Default::default()
            };
            3
        ];
        d.dimensions.push(Dimension {
            b: Vec2::ONE,
            ..Default::default()
        });
        d.components.push(Component {
            notes: vec![("Depth".into(), "深度".into(), 12.)],
            ..Default::default()
        });
        d.pending.push((0, Vec2::ONE));
        let value = serde_json::to_value(&d).unwrap();
        let restored: Drawing = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), value);
        assert_eq!(restored.views.len(), 3);
        assert!(restored.pending.is_empty());
    }
}

impl Drawing {
    pub fn is_generating(&self) -> bool {
        self.job.is_some()
    }
    pub fn saved_content(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        if let Some(object) = value.as_object_mut() {
            for key in ["open", "manual", "kind", "page"] {
                object.remove(key);
            }
        }
        value
    }
}
