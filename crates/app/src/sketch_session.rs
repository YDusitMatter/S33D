//! 草图会话：绘制工具、吸附、实体编辑（移植 Super3D `sketch.js` SketchSession）。

use crate::mode::AlignMode;
use s33d_kernel::{PlaneDef, Region2D};
use s33d_model::SketchEntity;
use std::collections::{BTreeMap, HashMap, HashSet};

pub type Point2 = [f64; 2];

#[derive(Clone, Default, PartialEq)]
pub struct SketchSnapshot {
    pub entities: Vec<SketchEntity>,
    pub vertex_locks: BTreeMap<usize, Option<usize>>,
}

/// One sketch-local axis bound to the world XYZ axis it most closely represents.
/// `sign` makes the displayed positive direction agree with the world Gizmo even
/// when PlaneDef's internal U/V basis points the other way (for example Right
/// plane U = -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SketchAxisBinding {
    pub local_axis: usize,
    pub world_axis: usize,
    pub sign: f64,
}

/// A world X/Y/Z direction projected into the active sketch plane. Datum
/// planes expose two such axes; an equation-defined oblique plane exposes all
/// three. `local_direction` keeps geometry operations in the existing 2D
/// sketch coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SketchDisplayAxis {
    pub world_axis: usize,
    pub local_direction: Point2,
}

impl SketchDisplayAxis {
    pub fn label(self) -> &'static str {
        ["X", "Y", "Z"][self.world_axis.min(2)]
    }
}

impl SketchAxisBinding {
    pub fn label(self) -> &'static str {
        ["X", "Y", "Z"][self.world_axis.min(2)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SketchTool {
    Trim,
    Freehand,
    Text,
    Move,
    Rotate,
    Line,
    Arc,
    Circle,
    Rect,
    Polygon,
    Pick,
    Align,
}

impl SketchTool {
    pub fn name(&self) -> &'static str {
        match self {
            SketchTool::Trim => "trim",
            SketchTool::Freehand => "freehand",
            SketchTool::Text => "text",
            SketchTool::Move => "move",
            SketchTool::Rotate => "rotate",
            SketchTool::Line => "line",
            SketchTool::Arc => "arc",
            SketchTool::Circle => "circle",
            SketchTool::Rect => "rect",
            SketchTool::Polygon => "polygon",
            SketchTool::Pick => "pick",
            SketchTool::Align => "align",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            SketchTool::Trim => {
                "Trim: click the highlighted segment between vertices/intersections; Ctrl+Z to undo"
            }
            SketchTool::Freehand => {
                "Freehand: drag each new stroke; switch to Pick to move or resize existing strokes"
            }
            SketchTool::Text => {
                "Text: click to place an input box; edit text here or in the sidebar"
            }
            SketchTool::Move => "Move: select entities, set the offset, then Apply",
            SketchTool::Rotate => "Rotate: select entities, set the angle, then Apply",
            SketchTool::Line => "Line: click start and end (chain; click start point to close)",
            SketchTool::Arc => "Arc: start, end, then a point on the arc",
            SketchTool::Circle => "Circle: center, then radius",
            SketchTool::Rect => "Rectangle: two opposite corners",
            SketchTool::Polygon => "Polygon: center, then radius (+/- changes sides)",
            SketchTool::Pick => "Pick: click to select, drag to move, Del deletes",
            SketchTool::Align => "Align: select entities, lock one reference, then Apply",
        }
    }
}

/// 拖拽移动信息。
#[derive(Debug, Clone)]
struct DragInfo {
    entity: usize,
    grab: Point2, // 抓取点（局部坐标）
    mode: DragMode,
    moved: bool,
}

#[derive(Debug, Clone, Copy)]
enum DragMode {
    Move,
    Resize {
        handle: ResizeHandle,
        handle_start: Point2,
    },
}

#[derive(Debug, Clone, Copy)]
enum ResizeHandle {
    PathScale,
    LineStart,
    LineEnd,
    RectCorner(usize),
    RectEdge(usize),
    RotatedRectCorner(usize),
    RotatedRectEdge(usize),
    CircleRadius,
    ArcStart,
    ArcEnd,
    ArcRadius,
    PolygonVertex(usize),
    PolygonEdge(usize),
}

/// 草图会话（平面局部坐标交互）。
/// 参数化 Line 状态（Line 工具激活时的两套输入体制共享数据）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamLineState {
    /// 起点（两套体制共用）。
    pub start: Point2,
    /// 终点（Set 1 直接编辑；Set 2 由角度+长度推导）。
    pub end: Point2,
    /// 已提交到 entities 的索引（编辑时原位更新，不堆积实体）。
    pub entity_idx: Option<usize>,
}

pub struct SketchSession {
    pub trim_preview: Option<s33d_model::trim::TrimPlan>,
    pub text_content: String,
    pub text_font: String,
    pub text_size: f64,
    pub text_focus_requested: bool,
    pub drawing_error: Option<String>,
    pub freehand_close: bool,
    stroke: Vec<Point2>,
    stroke_tolerance: f64,
    pub plane: PlaneDef,
    /// Equation planes show/edit world X, Y and Z simultaneously. Geometry is
    /// still stored in the plane's local 2D coordinates.
    pub world_xyz_coordinates: bool,
    pub entities: Vec<SketchEntity>,
    pub vertex_locks: BTreeMap<usize, Option<usize>>,
    pub tool: SketchTool,
    /// 绘制中的点（多边形：中心+半径；弧：起点+终点）。
    pub pending: Vec<Point2>,
    /// 参数化 Line 状态（选 Line 工具时激活；None = 未开始）。
    pub param_line: Option<ParamLineState>,
    pub hover: Option<Point2>,
    /// Currently expanded/active Pick entity. Kept as a single index for the
    /// existing drag and parameter-edit paths.
    pub selected_entity: Option<usize>,
    pub list_selection_anchor: Option<usize>,
    /// Ordered Pick selection. Earlier entries stay selected but collapsed in
    /// Properties; the newest entry becomes `selected_entity` and expands.
    pub pick_selection: Vec<usize>,
    pub poly_sides: u32,
    drag: Option<DragInfo>,
    /// true when Pick is hovering a parameter-changing key point.
    hover_resize_handle: bool,
    /// 实体编辑预览（虚线显示，Apply 才生效）。
    pub edit_preview: Option<SketchEntity>,
    /// 吸附标记（用于 UI 提示）。
    pub snap_kind: Option<SnapKind>,
    /// 连续线链的起点（点击起点闭合）。
    pub chain_start: Option<Point2>,
    /// 链已绘制的线段数。
    pub chain_segs: u32,
    /// Surface Sketch 可见轮廓提供的外部关键点（不属于草图实体）。
    pub external_key_points: Vec<Point2>,
    /// Sketch Align 当前选单及固定参考实体。
    pub align_selection: Vec<usize>,
    pub align_reference: Option<usize>,
    /// World-axis index (0 = X, 1 = Y, 2 = Z). The direction used by geometry
    /// is that world axis projected into the sketch plane.
    pub align_axis: usize,
    pub align_mode: AlignMode,
    /// Move/Rotate share one ordered selection so switching between them does
    /// not lose the objects chosen in Pick, the viewport or Objects.
    pub transform_selection: Vec<usize>,
    pub move_delta: Point2,
    pub rotate_angle_deg: f64,
    /// When enabled, Sketch Rotate uses `rotate_pivot` as the point where the
    /// plane-normal rotation axis crosses the active sketch plane. Otherwise
    /// the shared selection center is used.
    pub rotate_custom_pivot: bool,
    pub rotate_pivot: Point2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
    Grid,
}

impl SketchSession {
    pub fn new(plane: PlaneDef, entities: Vec<SketchEntity>) -> Self {
        Self::new_with_coordinate_display(plane, entities, false)
    }

    pub fn new_with_coordinate_display(
        plane: PlaneDef,
        entities: Vec<SketchEntity>,
        world_xyz_coordinates: bool,
    ) -> Self {
        Self {
            trim_preview: None,
            text_content: String::new(),
            text_focus_requested: false,
            text_font: crate::sketch_text::FONTS[0].into(),
            text_size: 10.0,
            drawing_error: None,
            freehand_close: false,
            stroke: Vec::new(),
            stroke_tolerance: 0.1,
            plane,
            world_xyz_coordinates,
            entities,
            vertex_locks: BTreeMap::new(),
            tool: SketchTool::Pick, // 进入草图默认是选择（可选中/编辑已有实体）
            pending: Vec::new(),
            param_line: None,
            hover: None,
            selected_entity: None,
            list_selection_anchor: None,
            pick_selection: Vec::new(),
            poly_sides: 6,
            drag: None,
            hover_resize_handle: false,
            edit_preview: None,
            snap_kind: None,
            chain_start: None,
            chain_segs: 0,
            external_key_points: Vec::new(),
            align_selection: Vec::new(),
            align_reference: None,
            align_axis: 0,
            align_mode: AlignMode::Center,
            transform_selection: Vec::new(),
            move_delta: [0.0, 0.0],
            rotate_angle_deg: 0.0,
            rotate_custom_pivot: false,
            rotate_pivot: [0.0, 0.0],
        }
    }

    pub fn snapshot(&self) -> SketchSnapshot {
        SketchSnapshot {
            entities: self.entities.clone(),
            vertex_locks: self.vertex_locks.clone(),
        }
    }

    pub fn restore_snapshot(&mut self, snapshot: SketchSnapshot) {
        self.entities = snapshot.entities;
        self.vertex_locks = snapshot.vertex_locks;
    }

    pub fn vertex_count(&self, index: usize) -> usize {
        self.entities.get(index).map_or(0, vertex_count)
    }

    pub fn locked_vertex(&self, index: usize) -> Option<usize> {
        let entity = self.entities.get(index)?;
        let count = vertex_count(entity);
        if count == 0 {
            return None;
        }
        self.vertex_locks
            .get(&index)
            .copied()
            .unwrap_or_else(|| Some(first_placed_point(entity)))
            .map(|vertex| vertex.min(count - 1))
    }

    pub fn locked_vertex_point(&self, index: usize) -> Option<Point2> {
        self.locked_vertex(index)
            .map(|vertex| vertex_point(&self.entities[index], vertex))
    }

    pub fn set_locked_vertex(&mut self, index: usize, vertex: Option<usize>) {
        let Some(entity) = self.entities.get(index) else {
            return;
        };
        let count = vertex_count(entity);
        if count == 0 || vertex.is_some_and(|n| n >= count) {
            return;
        }
        if vertex == Some(first_placed_point(entity)) {
            self.vertex_locks.remove(&index);
        } else {
            self.vertex_locks.insert(index, vertex);
        }
    }

    fn keep_locked_vertex(&mut self, index: usize, mut next: SketchEntity) {
        if let Some(locked) = self.locked_vertex(index) {
            let next_vertex = locked.min(vertex_count(&next) - 1);
            let fixed = vertex_point(&self.entities[index], locked);
            let moved = vertex_point(&next, next_vertex);
            next = move_entity(&next, [fixed[0] - moved[0], fixed[1] - moved[1]])
                .expect("all sketch entities support translation");
            self.entities[index] = next;
            self.set_locked_vertex(index, Some(next_vertex));
        } else {
            self.entities[index] = next;
        }
    }

    fn remap_vertex_locks(&mut self, index: usize, replacements: usize) {
        self.vertex_locks = std::mem::take(&mut self.vertex_locks)
            .into_iter()
            .filter_map(|(slot, lock)| match slot.cmp(&index) {
                std::cmp::Ordering::Less => Some((slot, lock)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some((slot - 1 + replacements, lock)),
            })
            .collect();
    }

    pub fn set_tool(&mut self, tool: SketchTool) {
        self.trim_preview = None;
        let previous_tool = self.tool;
        let previous_active = self.selected_entity;
        let previous_pick_selection = self.selected_entities();
        let previous_align_selection = self.align_selection.clone();
        let previous_transform_selection = self.transform_selection.clone();
        let previous_selection = match previous_tool {
            SketchTool::Align => previous_align_selection.clone(),
            SketchTool::Move | SketchTool::Rotate => previous_transform_selection.clone(),
            _ => previous_pick_selection.clone(),
        };
        self.tool = tool;
        self.stroke.clear();
        self.drawing_error = None;
        self.pending.clear();
        self.chain_start = None;
        self.chain_segs = 0;
        self.selected_entity = None;
        self.pick_selection.clear();
        self.drag = None;
        self.hover_resize_handle = false;
        self.edit_preview = None;
        if tool == SketchTool::Align {
            // Default to the first displayed world axis (X before Y before Z),
            // not blindly to internal U.
            self.align_axis = self.displayed_world_axes()[0].world_axis;
            if previous_tool != SketchTool::Align {
                // Pick and Align are two views of the same user selection.
                // Carry every ordered Pick item across instead of keeping only
                // the last/expanded Properties card.
                self.align_selection = previous_selection;
                self.align_reference = previous_active
                    .filter(|index| self.align_selection.contains(index))
                    .or_else(|| self.align_selection.last().copied());
            }
            self.transform_selection.clear();
        } else if matches!(tool, SketchTool::Move | SketchTool::Rotate) {
            self.transform_selection = previous_selection
                .into_iter()
                .filter(|index| *index < self.entities.len())
                .collect();
            self.move_delta = [0.0, 0.0];
            self.rotate_angle_deg = 0.0;
            self.rotate_custom_pivot = false;
            self.rotate_pivot = self.transform_center().unwrap_or([0.0, 0.0]);
            self.align_selection.clear();
            self.align_reference = None;
        } else if tool == SketchTool::Pick
            && matches!(
                previous_tool,
                SketchTool::Align | SketchTool::Move | SketchTool::Rotate
            )
        {
            // Returning from a multi-entity operation keeps the same objects
            // selected in Pick; operation-specific state is discarded.
            self.pick_selection = previous_selection
                .into_iter()
                .filter(|index| *index < self.entities.len())
                .collect();
            self.selected_entity = self.pick_selection.last().copied();
            self.align_selection.clear();
            self.align_reference = None;
            self.transform_selection.clear();
        } else {
            self.align_selection.clear();
            self.align_reference = None;
            self.transform_selection.clear();
        }
        // 离开 Line 工具时结束参数线（实体已提交）
        if tool != SketchTool::Line {
            self.param_line = None;
        }
    }

    /// 全部最小封闭区域（供拉伸选择）。
    pub fn regions(&self) -> Vec<Region2D> {
        s33d_model::compute_regions(&self.entities)
    }

    /// 世界坐标 → 平面局部坐标。
    pub fn to_local(&self, p: [f64; 3]) -> Point2 {
        let (u, v) = self.plane.to_local(p);
        [u, v]
    }

    /// 平面局部坐标 → 世界坐标。
    pub fn to_world(&self, p: Point2) -> [f64; 3] {
        self.plane.to_world(p[0], p[1])
    }

    /// 吸附：几何关键点和网格分别开关，便于 Sketch 顶部快速控制。
    pub fn snap(
        &mut self,
        p: Point2,
        geometry_snap_enabled: bool,
        grid_snap_enabled: bool,
        snap_step: f64,
        snap_radius: f64,
    ) -> Point2 {
        self.snap_kind = None;
        let mut best = p;
        let mut best_d = if geometry_snap_enabled {
            snap_radius.max(1e-6)
        } else {
            f64::INFINITY
        };
        if geometry_snap_enabled {
            for (i, entity) in self.entities.iter().enumerate() {
                if Some(i) == self.selected_entity {
                    continue;
                }
                for (point, kind) in entity_key_points(entity) {
                    let d = dist(point, p);
                    if d < best_d {
                        best_d = d;
                        best = point;
                        self.snap_kind = Some(kind);
                    }
                }
            }
            for &point in &self.external_key_points {
                let d = dist(point, p);
                if d < best_d {
                    best_d = d;
                    best = point;
                    self.snap_kind = Some(SnapKind::Endpoint);
                }
            }
        }
        // 网格吸附（次优先）
        if grid_snap_enabled && snap_step > 0.0 {
            let gx = (best[0] / snap_step).round() * snap_step;
            let gy = (best[1] / snap_step).round() * snap_step;
            if dist([gx, gy], p) < snap_radius.max(1e-6) && self.snap_kind.is_none() {
                best = [gx, gy];
                self.snap_kind = Some(SnapKind::Grid);
            }
        }
        best
    }

    /// 当前草图可显示/可吸附的关键点。
    pub fn key_points(&self) -> Vec<Point2> {
        let mut points = Vec::new();
        for entity in &self.entities {
            for (point, _) in entity_key_points(entity) {
                if !points.iter().any(|existing| dist(*existing, point) < 1e-8) {
                    points.push(point);
                }
            }
        }
        for &point in &self.external_key_points {
            if !points.iter().any(|existing| dist(*existing, point) < 1e-8) {
                points.push(point);
            }
        }
        points
    }

    pub fn set_external_key_points(&mut self, points: Vec<Point2>) {
        let scale = 1_000_000.0;
        let mut seen = HashSet::with_capacity(points.len());
        self.external_key_points = points
            .into_iter()
            .filter(|point| {
                seen.insert((
                    (point[0] * scale).round() as i64,
                    (point[1] * scale).round() as i64,
                ))
            })
            .collect();
    }

    pub fn clear_snap(&mut self) {
        self.snap_kind = None;
    }

    /// Current ordered Pick selection. The fallback preserves compatibility
    /// with older direct assignments to `selected_entity` in tests and callers.
    pub fn selected_entities(&self) -> Vec<usize> {
        if self.pick_selection.is_empty() {
            return self.selected_entity.into_iter().collect();
        }
        self.pick_selection
            .iter()
            .copied()
            .filter(|index| *index < self.entities.len())
            .collect()
    }

    pub fn is_entity_selected(&self, index: usize) -> bool {
        self.pick_selection.contains(&index)
            || (self.pick_selection.is_empty() && self.selected_entity == Some(index))
    }

    fn select_only(&mut self, index: usize) {
        self.pick_selection.clear();
        self.pick_selection.push(index);
        self.selected_entity = Some(index);
    }

    /// Objects tree and viewport Pick share this ordered multi-selection.
    /// Normal click replaces the selection; Ctrl/Shift-click toggles one entity.
    pub fn select_entity(&mut self, index: usize, additive: bool) -> bool {
        if index >= self.entities.len() {
            return false;
        }
        self.tool = SketchTool::Pick;
        self.pending.clear();
        self.param_line = None;
        self.chain_start = None;
        self.chain_segs = 0;
        self.drag = None;
        self.hover_resize_handle = false;
        self.edit_preview = None;
        if additive {
            if let Some(position) = self
                .pick_selection
                .iter()
                .position(|selected| *selected == index)
            {
                self.pick_selection.remove(position);
                if self.selected_entity == Some(index) {
                    self.selected_entity = self.pick_selection.last().copied();
                }
            } else {
                // A legacy single selection may not yet be mirrored into the
                // ordered collection. Preserve it before appending the new one.
                if self.pick_selection.is_empty() {
                    if let Some(previous) =
                        self.selected_entity.filter(|previous| *previous != index)
                    {
                        self.pick_selection.push(previous);
                    }
                }
                self.pick_selection.push(index);
                self.selected_entity = Some(index);
            }
        } else {
            self.select_only(index);
        }
        true
    }

    pub fn select_list_range(&mut self, index: usize, additive: bool) -> bool {
        if index >= self.entities.len() {
            return false;
        }
        let previous = match self.tool {
            SketchTool::Align => self.align_selection.clone(),
            SketchTool::Move | SketchTool::Rotate => self.transform_selection.clone(),
            _ => self.selected_entities(),
        };
        let anchor = self
            .list_selection_anchor
            .filter(|i| *i < self.entities.len())
            .or_else(|| previous.first().copied())
            .unwrap_or(index);
        self.list_selection_anchor = Some(anchor);
        let mut next = if additive { previous } else { Vec::new() };
        for i in anchor.min(index)..=anchor.max(index) {
            if !next.contains(&i) {
                next.push(i);
            }
        }
        match self.tool {
            SketchTool::Align => {
                self.align_selection = next;
                if !self
                    .align_reference
                    .is_some_and(|i| self.align_selection.contains(&i))
                {
                    self.align_reference = self.align_selection.first().copied();
                }
            }
            SketchTool::Move | SketchTool::Rotate => self.transform_selection = next,
            _ => {
                self.select_entity(index, false);
                self.pick_selection = next;
            }
        }
        true
    }

    /// Open one selected entity's accordion without changing the selection.
    pub fn activate_entity(&mut self, index: usize) -> bool {
        if index >= self.entities.len() || !self.is_entity_selected(index) {
            return false;
        }
        self.selected_entity = Some(index);
        self.drag = None;
        self.hover_resize_handle = false;
        self.edit_preview = None;
        true
    }

    pub fn toggle_entity_expanded(&mut self, index: usize) -> bool {
        if index >= self.entities.len() || !self.is_entity_selected(index) {
            return false;
        }
        if self.selected_entity == Some(index) {
            self.selected_entity = None;
            self.drag = None;
            self.edit_preview = None;
            true
        } else {
            self.activate_entity(index)
        }
    }

    /// Remove one entity from Pick selection, leaving the remaining selection
    /// intact and expanding the newest remaining entity.
    pub fn deselect_entity(&mut self, index: usize) -> bool {
        let mut selected = self.selected_entities();
        let Some(position) = selected.iter().position(|selected| *selected == index) else {
            return false;
        };
        selected.remove(position);
        self.pick_selection = selected;
        if self.selected_entity == Some(index) {
            self.selected_entity = self.pick_selection.last().copied();
        }
        self.drag = None;
        self.edit_preview = None;
        true
    }

    /// 指针按下（局部坐标）——完整移植 Super3D `sketch.js onPointerDown`。
    /// 返回是否有新实体创建（调用方据此刷新参数面板）。
    pub fn pointer_down(&mut self, p: Point2, snap_step: f64) -> bool {
        self.pointer_down_with_additive(p, snap_step, false)
    }

    /// Arm direct manipulation for a completed entity without forcing the
    /// user out of the current drawing tool. This is what lets a rectangle,
    /// circle, arc or polygon be moved/resized immediately after its last
    /// construction click, before the user presses Done or exits the sketch.
    pub fn begin_entity_drag(&mut self, p: Point2, tol: f64, additive: bool) -> bool {
        if !self.direct_entity_drag_available() {
            return false;
        }
        let Some((index, resize_handle)) = self.pick_entity_for_drag(p, tol) else {
            return false;
        };

        // select_entity intentionally enters Pick for ordinary selection.
        // A direct drag during construction keeps the drawing tool active so
        // the user can continue creating more entities after releasing it.
        let drawing_tool = self.tool;
        self.select_entity(index, additive);
        self.tool = drawing_tool;
        if !self.is_entity_selected(index) {
            return false;
        }
        self.drag = Some(DragInfo {
            entity: index,
            grab: p,
            mode: resize_handle.map_or(DragMode::Move, |(handle, handle_start)| DragMode::Resize {
                handle,
                handle_start,
            }),
            moved: false,
        });
        self.hover_resize_handle = false;
        true
    }

    pub fn pointer_down_with_additive(
        &mut self,
        p: Point2,
        snap_step: f64,
        additive: bool,
    ) -> bool {
        self.hover = Some(p);
        let n_before = self.entities.len();
        if self.tool == SketchTool::Trim {
            return self.trim_at(p, (snap_step * 1.2).max(1.5));
        }
        if self.tool == SketchTool::Align {
            let tol = (snap_step * 1.2).max(1.5);
            if let Some(index) = self.pick_entity(p, tol) {
                self.toggle_align_entity(index);
            }
            return false;
        }
        if matches!(self.tool, SketchTool::Move | SketchTool::Rotate) {
            let tol = (snap_step * 1.2).max(1.5);
            if let Some(index) = self.pick_entity(p, tol) {
                self.select_transform_entity(index, additive);
            } else if !additive {
                self.transform_selection.clear();
            }
            return false;
        }
        if self.tool == SketchTool::Pick {
            let tol = (snap_step * 1.2).max(1.5);
            if !self.begin_entity_drag(p, tol, additive) && !additive {
                self.deselect();
            }
            self.hover_resize_handle = false;
            return false;
        }
        match self.tool {
            SketchTool::Freehand => return false, // A stroke is committed only on pointer release.
            SketchTool::Text => {
                match crate::sketch_text::editable_text(
                    &self.text_content,
                    &self.text_font,
                    self.text_size,
                    p,
                ) {
                    Ok(entity) => {
                        self.entities.push(entity);
                        self.text_focus_requested = true;
                        self.drawing_error = None;
                    }
                    Err(error) => {
                        self.drawing_error = Some(error);
                        return false;
                    }
                }
            }
            SketchTool::Line => {
                // 点击链起点：闭合轮廓
                if let Some(cs) = self.chain_start {
                    if self.chain_segs >= 2 && dist(p, cs) < (snap_step * 0.75).max(0.2) {
                        let last = *self.pending.last().unwrap_or(&cs);
                        self.entities.push(SketchEntity::Line { a: last, b: cs });
                        self.finish_chain();
                        self.param_line = None; // 链闭合，参数面板复位
                        return true;
                    }
                }
                self.pending.push(p);
                if self.chain_start.is_none() {
                    self.chain_start = Some(p);
                    self.chain_segs = 0;
                    // 参数面板：起点已定，等待终点
                    self.param_line = Some(ParamLineState {
                        start: p,
                        end: p,
                        entity_idx: None,
                    });
                }
                if self.pending.len() >= 2 {
                    let a = self.pending[self.pending.len() - 2];
                    self.entities.push(SketchEntity::Line { a, b: p });
                    let idx = self.entities.len() - 1;
                    self.pending = vec![p];
                    self.chain_segs += 1;
                    // 参数面板同步：当前段（编辑时原位更新）
                    self.param_line = Some(ParamLineState {
                        start: a,
                        end: p,
                        entity_idx: Some(idx),
                    });
                }
            }
            SketchTool::Arc => {
                self.pending.push(p);
                if self.pending.len() == 3 {
                    let arc = SketchEntity::arc_from_3_points(
                        self.pending[0],
                        self.pending[1],
                        self.pending[2],
                    );
                    self.entities.push(arc.unwrap_or(SketchEntity::Line {
                        a: self.pending[0],
                        b: self.pending[1],
                    }));
                    self.pending.clear();
                }
            }
            SketchTool::Circle => {
                self.pending.push(p);
                if self.pending.len() == 2 {
                    let c = self.pending[0];
                    let r = dist(p, c);
                    if r > 1e-6 {
                        self.entities.push(SketchEntity::Circle { c, r });
                    }
                    self.pending.clear();
                }
            }
            SketchTool::Rect => {
                self.pending.push(p);
                if self.pending.len() == 2 {
                    let c1 = self.pending[0];
                    if (p[0] - c1[0]).abs() > 1e-6 && (p[1] - c1[1]).abs() > 1e-6 {
                        self.entities.push(SketchEntity::Rect { c1, c2: p });
                    }
                    self.pending.clear();
                }
            }
            SketchTool::Polygon => {
                self.pending.push(p);
                if self.pending.len() == 2 {
                    let c = self.pending[0];
                    let r = dist(p, c);
                    if r > 1e-6 {
                        self.entities.push(SketchEntity::Polygon {
                            c,
                            r,
                            n: self.poly_sides,
                            rot: (p[1] - c[1]).atan2(p[0] - c[0]),
                        });
                    }
                    self.pending.clear();
                }
            }
            SketchTool::Trim
            | SketchTool::Pick
            | SketchTool::Align
            | SketchTool::Move
            | SketchTool::Rotate => {
                unreachable!()
            }
        }
        // 新实体自动选中（参数面板立即可编辑）；无新实体则清空选择（与 Super3D 一致）
        if self.entities.len() > n_before {
            self.select_only(self.entities.len() - 1);
            self.drag = None;
            self.edit_preview = None;
            true
        } else {
            self.selected_entity = None;
            self.pick_selection.clear();
            false
        }
    }

    /// Compute again at click time so a stale hover can never trim another edge.
    pub fn trim_at(&mut self, p: Point2, tolerance: f64) -> bool {
        let Some(plan) = s33d_model::trim::plan_trim(&self.entities, p, tolerance) else {
            self.trim_preview = None;
            return false;
        };
        self.remap_vertex_locks(plan.entity, plan.replacement.len());
        self.entities
            .splice(plan.entity..=plan.entity, plan.replacement);
        self.reset_after_history_restore();
        self.snap_kind = None;
        true
    }

    pub fn pointer_move(&mut self, p: Point2) {
        self.hover = Some(p);
        // Line 参数面板实时同步：段未生成（等待终点）时跟随鼠标
        if self.tool == SketchTool::Line {
            if let Some(pl) = &mut self.param_line {
                if pl.entity_idx.is_none() {
                    pl.end = p;
                }
            }
        }
        if let Some(drag) = self.drag.clone() {
            let e = self.entities[drag.entity].clone();
            let delta = [p[0] - drag.grab[0], p[1] - drag.grab[1]];
            if !drag.moved && dist(delta, [0.0, 0.0]) > 0.5 {
                if let Some(active) = self.drag.as_mut() {
                    active.moved = true;
                }
            }
            if self.drag.as_ref().is_some_and(|active| active.moved) {
                let preview = match drag.mode {
                    DragMode::Move => move_entity(&e, delta),
                    DragMode::Resize {
                        handle,
                        handle_start,
                    } => resize_entity(
                        &e,
                        handle,
                        [handle_start[0] + delta[0], handle_start[1] + delta[1]],
                    ),
                };
                if let Some(moved) = preview {
                    self.edit_preview = Some(moved);
                }
            }
        }
    }

    /// 指针移动并处理拖拽。拖动实体时，以实体的所有关键点寻找最近的静止
    /// 关键点，而不是只吸附鼠标抓取点，因此端点/中心可以精确重合。
    pub fn pointer_move_with_snap(
        &mut self,
        p: Point2,
        geometry_snap_enabled: bool,
        grid_snap_enabled: bool,
        snap_step: f64,
        snap_radius: f64,
    ) {
        if self.tool == SketchTool::Trim {
            self.hover = Some(p);
            self.snap_kind = None;
            self.hover_resize_handle = false;
            self.trim_preview = s33d_model::trim::plan_trim(&self.entities, p, snap_radius);
            return;
        }
        if !self.stroke.is_empty() {
            if p.iter().all(|v| v.is_finite())
                && dist(*self.stroke.last().unwrap(), p) >= self.stroke_tolerance
            {
                if self.stroke.len() >= 4096 {
                    self.stroke = self.stroke.iter().step_by(2).copied().collect();
                    self.stroke_tolerance *= 2.0;
                }
                self.stroke.push(p);
            }
            self.hover = Some(p);
            self.snap_kind = None;
            return;
        }
        let Some(drag) = self.drag.clone() else {
            if self.tool == SketchTool::Freehand {
                self.hover_resize_handle = false;
                self.snap_kind = None;
                self.pointer_move(p);
                return;
            }
            self.hover_resize_handle = self.direct_entity_drag_available()
                && self
                    .entities
                    .iter()
                    .any(|entity| resize_handle_at(entity, p, snap_radius).is_some());
            let snapped = self.snap(
                p,
                geometry_snap_enabled,
                grid_snap_enabled,
                snap_step,
                snap_radius,
            );
            self.pointer_move(snapped);
            return;
        };

        self.hover_resize_handle = false;
        self.snap_kind = None;
        let raw_delta = [p[0] - drag.grab[0], p[1] - drag.grab[1]];
        if let DragMode::Resize { handle_start, .. } = drag.mode {
            let mut target = [
                handle_start[0] + raw_delta[0],
                handle_start[1] + raw_delta[1],
            ];
            let mut best_distance = snap_radius.max(1e-6);
            if geometry_snap_enabled {
                let mut target_points: Vec<(Point2, SnapKind)> = self
                    .entities
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != drag.entity)
                    .flat_map(|(_, entity)| entity_key_points(entity))
                    .collect();
                target_points.extend(
                    self.external_key_points
                        .iter()
                        .copied()
                        .map(|point| (point, SnapKind::Endpoint)),
                );
                for (candidate, kind) in target_points {
                    let distance = dist(target, candidate);
                    if distance < best_distance {
                        best_distance = distance;
                        target = candidate;
                        self.snap_kind = Some(kind);
                    }
                }
            }
            if self.snap_kind.is_none() && grid_snap_enabled && snap_step > 0.0 {
                let grid = [
                    (target[0] / snap_step).round() * snap_step,
                    (target[1] / snap_step).round() * snap_step,
                ];
                if dist(target, grid) < snap_radius.max(1e-6) {
                    target = grid;
                    self.snap_kind = Some(SnapKind::Grid);
                }
            }
            self.pointer_move([
                drag.grab[0] + target[0] - handle_start[0],
                drag.grab[1] + target[1] - handle_start[1],
            ]);
            return;
        }

        let mut corrected_delta = raw_delta;
        let mut best_distance = snap_radius.max(1e-6);
        if geometry_snap_enabled {
            let moving_points = entity_key_points(&self.entities[drag.entity]);
            let mut target_points: Vec<(Point2, SnapKind)> = self
                .entities
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != drag.entity)
                .flat_map(|(_, entity)| entity_key_points(entity))
                .collect();
            target_points.extend(
                self.external_key_points
                    .iter()
                    .copied()
                    .map(|point| (point, SnapKind::Endpoint)),
            );
            for (moving, _) in moving_points {
                let translated = [moving[0] + raw_delta[0], moving[1] + raw_delta[1]];
                for (target, kind) in &target_points {
                    let distance = dist(translated, *target);
                    if distance < best_distance {
                        best_distance = distance;
                        corrected_delta = [
                            raw_delta[0] + target[0] - translated[0],
                            raw_delta[1] + target[1] - translated[1],
                        ];
                        self.snap_kind = Some(*kind);
                    }
                }
            }
        }
        let mut adjusted = [
            drag.grab[0] + corrected_delta[0],
            drag.grab[1] + corrected_delta[1],
        ];
        if self.snap_kind.is_none() && grid_snap_enabled && snap_step > 0.0 {
            let grid = [
                (adjusted[0] / snap_step).round() * snap_step,
                (adjusted[1] / snap_step).round() * snap_step,
            ];
            if dist(adjusted, grid) < snap_radius.max(1e-6) {
                adjusted = grid;
                self.snap_kind = Some(SnapKind::Grid);
            }
        }
        self.pointer_move(adjusted);
    }

    fn direct_entity_drag_available(&self) -> bool {
        matches!(
            self.tool,
            SketchTool::Pick
                | SketchTool::Text
                | SketchTool::Line
                | SketchTool::Arc
                | SketchTool::Circle
                | SketchTool::Rect
                | SketchTool::Polygon
        ) && self.pending.is_empty()
            && self.stroke.is_empty()
            && self.chain_start.is_none()
            && self.param_line.is_none()
    }

    /// Commit an armed Pick drag and report whether geometry changed.
    pub fn pointer_up(&mut self, _p: Point2) -> bool {
        if !self.stroke.is_empty() {
            let mut points =
                simplify_stroke(&std::mem::take(&mut self.stroke), self.stroke_tolerance);
            let close = points.len() >= 3
                && (self.freehand_close
                    || dist(points[0], *points.last().unwrap()) < self.stroke_tolerance * 5.0);
            if close && dist(points[0], *points.last().unwrap()) < self.stroke_tolerance * 5.0 {
                points.pop();
            }
            if points.len() >= if close { 3 } else { 2 } {
                let origin = points[0];
                for p in &mut points {
                    p[0] -= origin[0];
                    p[1] -= origin[1];
                }
                self.entities.push(SketchEntity::Path {
                    contours: vec![(points, close)],
                    origin,
                    scale: 1.0,
                    rot: 0.0,
                    text: None,
                });
                // Keep Pen ready for the next stroke, not in an implicit
                // edit state with resize handles and selected Properties.
                self.deselect();
                return true;
            }
            return false;
        }
        let mut changed = false;
        if let Some(drag) = self.drag.take() {
            if drag.moved {
                if let Some(preview) = self.edit_preview.take() {
                    self.entities[drag.entity] = preview;
                    changed = true;
                }
            }
        }
        self.hover_resize_handle = false;
        changed
    }

    /// A plain Pick click changes selection but must not leave an entity
    /// attached to later hover motion. Actual dragging is armed separately by
    /// the viewport's primary-button drag-start event.
    pub fn cancel_drag(&mut self) {
        self.stroke.clear();
        self.drag = None;
        self.hover_resize_handle = false;
        self.edit_preview = None;
    }

    pub fn hovering_resize_handle(&self) -> bool {
        self.hover_resize_handle && self.drag.is_none()
    }

    pub fn begin_freehand(&mut self, point: Point2, tolerance: f64) {
        if self.tool == SketchTool::Freehand && point.iter().all(|v| v.is_finite()) {
            self.cancel_drag();
            self.deselect();
            self.stroke_tolerance = tolerance.max(1e-5);
            self.stroke = vec![point];
        }
    }

    /// The entity still stored in `entities` is the drag origin; `edit_preview`
    /// is the live moved/resized geometry. Rendering uses this to show the
    /// origin as a dashed ghost instead of a misleading solid duplicate.
    pub fn active_drag_entity(&self) -> Option<usize> {
        self.drag
            .as_ref()
            .filter(|drag| drag.moved && self.edit_preview.is_some())
            .map(|drag| drag.entity)
    }

    /// Clear transient command/selection state after restoring an Undo/Redo
    /// entity snapshot. Geometry remains untouched.
    pub fn reset_after_history_restore(&mut self) {
        self.list_selection_anchor = None;
        self.trim_preview = None;
        self.pending.clear();
        self.param_line = None;
        self.chain_start = None;
        self.chain_segs = 0;
        self.selected_entity = None;
        self.pick_selection.clear();
        self.align_selection.clear();
        self.align_reference = None;
        self.transform_selection.clear();
        self.drag = None;
        self.hover_resize_handle = false;
        self.edit_preview = None;
    }

    pub fn toggle_align_entity(&mut self, index: usize) -> bool {
        if index >= self.entities.len() {
            return false;
        }
        if let Some(position) = self
            .align_selection
            .iter()
            .position(|value| *value == index)
        {
            self.align_selection.remove(position);
            if self.align_reference == Some(index) {
                self.align_reference = self.align_selection.first().copied();
            }
            false
        } else {
            self.align_selection.push(index);
            if self.align_reference.is_none() {
                self.align_reference = Some(index);
            }
            true
        }
    }

    pub fn set_align_reference(&mut self, reference: Option<usize>) {
        self.align_reference = reference.filter(|index| self.align_selection.contains(index));
    }

    pub fn select_transform_entity(&mut self, index: usize, additive: bool) -> bool {
        if index >= self.entities.len() {
            return false;
        }
        if additive {
            if let Some(position) = self
                .transform_selection
                .iter()
                .position(|selected| *selected == index)
            {
                self.transform_selection.remove(position);
                return false;
            }
            self.transform_selection.push(index);
        } else {
            self.transform_selection.clear();
            self.transform_selection.push(index);
        }
        true
    }

    pub fn transform_center(&self) -> Option<Point2> {
        selection_center(&self.entities, &self.transform_selection)
    }

    pub fn rotation_pivot(&self) -> Option<Point2> {
        if self.rotate_custom_pivot {
            Some(self.rotate_pivot)
        } else {
            self.transform_center()
        }
    }

    pub fn set_rotate_custom_pivot_enabled(&mut self, enabled: bool) {
        if self.tool != SketchTool::Rotate {
            return;
        }
        if enabled && !self.rotate_custom_pivot {
            self.rotate_pivot = self.transform_center().unwrap_or([0.0, 0.0]);
        }
        self.rotate_custom_pivot = enabled;
    }

    pub fn set_rotate_pivot(&mut self, pivot: Point2) {
        if self.tool == SketchTool::Rotate && pivot.iter().all(|value| value.is_finite()) {
            self.rotate_pivot = pivot;
        }
    }

    pub fn set_move_delta(&mut self, delta: Point2) {
        if self.tool == SketchTool::Move && delta.iter().all(|value| value.is_finite()) {
            self.move_delta = delta;
        }
    }

    pub fn set_rotate_angle(&mut self, angle_deg: f64) {
        if self.tool == SketchTool::Rotate && angle_deg.is_finite() {
            self.rotate_angle_deg = angle_deg;
        }
    }

    pub fn transform_preview_chains(&self) -> Vec<(Vec<Point2>, bool)> {
        if self.transform_selection.is_empty() {
            return Vec::new();
        }
        if (self.tool == SketchTool::Move
            && self.move_delta[0].abs() <= 1e-9
            && self.move_delta[1].abs() <= 1e-9)
            || (self.tool == SketchTool::Rotate && self.rotate_angle_deg.abs() <= 1e-9)
        {
            return Vec::new();
        }
        let pivot = self.rotation_pivot().unwrap_or([0.0, 0.0]);
        // User-facing positive Sketch angles rotate clockwise. The local 2D
        // geometry kernel uses the mathematical CCW-positive convention.
        let angle = -self.rotate_angle_deg.to_radians();
        self.transform_selection
            .iter()
            .filter_map(|&index| self.entities.get(index).map(|entity| (index, entity)))
            .flat_map(|(index, entity)| {
                let pivot = self.locked_vertex_point(index).unwrap_or(pivot);
                entity
                    .to_chains()
                    .into_iter()
                    .map(move |(mut points, closed)| {
                        match self.tool {
                            SketchTool::Move => {
                                for point in &mut points {
                                    point[0] += self.move_delta[0];
                                    point[1] += self.move_delta[1];
                                }
                            }
                            SketchTool::Rotate => {
                                for point in &mut points {
                                    *point = rotate_point(*point, pivot, angle);
                                }
                            }
                            _ => {}
                        }
                        (points, closed)
                    })
            })
            .collect()
    }

    pub fn apply_transform(&mut self) -> Result<usize, String> {
        self.transform_selection
            .retain(|index| *index < self.entities.len());
        if self.transform_selection.is_empty() {
            return Err("Select at least one sketch entity".into());
        }
        let selected_count = self.transform_selection.len();
        match self.tool {
            SketchTool::Move => {
                if self.move_delta.iter().any(|value| !value.is_finite()) {
                    return Err("Move offset must be finite".into());
                }
                for &index in &self.transform_selection {
                    self.entities[index] = move_entity(&self.entities[index], self.move_delta)
                        .expect("all sketch entities support translation");
                }
                self.move_delta = [0.0, 0.0];
            }
            SketchTool::Rotate => {
                if !self.rotate_angle_deg.is_finite() {
                    return Err("Rotation angle must be finite".into());
                }
                let pivot = self
                    .rotation_pivot()
                    .ok_or_else(|| "Select at least one sketch entity".to_string())?;
                let angle = -self.rotate_angle_deg.to_radians();
                if angle.abs() <= 1e-12 {
                    return Ok(selected_count);
                }
                for index in self.transform_selection.clone() {
                    let locked = self.locked_vertex(index);
                    let entity = &self.entities[index];
                    let pivot = self.locked_vertex_point(index).unwrap_or(pivot);
                    self.entities[index] = rotate_entity(entity, pivot, angle);
                    self.set_locked_vertex(index, locked);
                }
                self.rotate_angle_deg = 0.0;
            }
            _ => return Err("Choose Sketch Move or Rotate first".into()),
        }
        Ok(selected_count)
    }

    /// Apply one immediate shortcut nudge while preserving any numeric preview
    /// currently staged in the Move panel.
    pub fn apply_quick_move(&mut self, delta: Point2) -> Result<usize, String> {
        if self.tool != SketchTool::Move {
            return Err("Choose Sketch Move first".into());
        }
        if delta.iter().any(|value| !value.is_finite()) {
            return Err("Move offset must be finite".into());
        }
        let staged = self.move_delta;
        self.move_delta = delta;
        let result = self.apply_transform();
        self.move_delta = staged;
        result
    }

    /// Apply one immediate shortcut rotation while preserving any numeric
    /// preview currently staged in the Rotate panel.
    pub fn apply_quick_rotate(&mut self, angle_deg: f64) -> Result<usize, String> {
        if self.tool != SketchTool::Rotate {
            return Err("Choose Sketch Rotate first".into());
        }
        if !angle_deg.is_finite() {
            return Err("Rotation angle must be finite".into());
        }
        let staged = self.rotate_angle_deg;
        self.rotate_angle_deg = angle_deg;
        let result = self.apply_transform();
        self.rotate_angle_deg = staged;
        result
    }

    pub fn apply_align(&mut self) -> Result<usize, String> {
        self.align_selection
            .retain(|index| *index < self.entities.len());
        if self.align_selection.len() < 2 {
            return Err("Select at least two sketch entities".into());
        }
        let reference = self
            .align_reference
            .filter(|index| self.align_selection.contains(index))
            .ok_or_else(|| "Lock one sketch entity as the reference".to_string())?;
        let axis = self
            .displayed_world_axes()
            .into_iter()
            .find(|axis| axis.world_axis == self.align_axis)
            .ok_or_else(|| "Choose an axis that lies in the sketch plane".to_string())?;
        let direction = axis.local_direction;
        let anchor = |entity: &SketchEntity| {
            let (minimum, maximum) = entity_projection_bounds(entity, direction);
            match self.align_mode {
                AlignMode::Min => minimum,
                AlignMode::Center => (minimum + maximum) * 0.5,
                AlignMode::Max => maximum,
            }
        };
        let target = anchor(&self.entities[reference]);
        let mut moved = 0;
        for &index in &self.align_selection {
            if index == reference {
                continue;
            }
            let amount = target - anchor(&self.entities[index]);
            if amount.abs() <= 1e-9 {
                continue;
            }
            let delta = [direction[0] * amount, direction[1] * amount];
            if let Some(entity) = move_entity(&self.entities[index], delta) {
                self.entities[index] = entity;
                moved += 1;
            }
        }
        Ok(moved)
    }

    /// 最近实体拾取（像素距离内）。
    fn pick_entity(&self, p: Point2, tol: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, e) in self.entities.iter().enumerate() {
            let d = entity_distance(e, p);
            if d < tol && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Key points take precedence over curve proximity, so a center point can
    /// move a circle/polygon and an endpoint can resize its owning entity even
    /// though the point is not the nearest sampled outline segment.
    fn pick_entity_for_drag(
        &self,
        p: Point2,
        tol: f64,
    ) -> Option<(usize, Option<(ResizeHandle, Point2)>)> {
        let mut key_point_owner: Option<(usize, f64)> = None;
        for (index, entity) in self.entities.iter().enumerate() {
            for (point, _) in entity_key_points(entity) {
                let distance = dist(point, p);
                if distance < tol
                    && key_point_owner
                        .map(|(_, best)| distance < best)
                        .unwrap_or(true)
                {
                    key_point_owner = Some((index, distance));
                }
            }
        }
        if let Some((index, _)) = key_point_owner {
            let resize = resize_handle_at(&self.entities[index], p, tol);
            return Some((index, resize));
        }
        self.pick_entity(p, tol).map(|index| (index, None))
    }

    /// Delete all Pick-selected entities (keyboard Delete semantics).
    pub fn delete_selected(&mut self) -> bool {
        let mut selected = self.selected_entities();
        if selected.is_empty() {
            return false;
        }
        selected.sort_unstable();
        selected.dedup();
        for index in selected.into_iter().rev() {
            self.delete_entity(index);
        }
        true
    }

    /// Delete exactly one entity from its Properties card and remap every
    /// selection index after the removed slot.
    pub fn delete_entity(&mut self, index: usize) -> bool {
        if index >= self.entities.len() {
            return false;
        }
        self.entities.remove(index);
        self.remap_vertex_locks(index, 0);
        self.pick_selection.retain(|selected| *selected != index);
        for selected in &mut self.pick_selection {
            if *selected > index {
                *selected -= 1;
            }
        }
        self.align_selection.retain(|selected| *selected != index);
        for selected in &mut self.align_selection {
            if *selected > index {
                *selected -= 1;
            }
        }
        self.transform_selection
            .retain(|selected| *selected != index);
        for selected in &mut self.transform_selection {
            if *selected > index {
                *selected -= 1;
            }
        }
        self.align_reference = self.align_reference.and_then(|reference| {
            if reference == index {
                None
            } else {
                Some(if reference > index {
                    reference - 1
                } else {
                    reference
                })
            }
        });
        self.selected_entity = self.selected_entity.and_then(|selected| {
            if selected == index {
                self.pick_selection.last().copied()
            } else {
                Some(if selected > index {
                    selected - 1
                } else {
                    selected
                })
            }
        });
        self.drag = None;
        self.edit_preview = None;
        true
    }

    /// 用 Value 框尺寸输入完成/调整当前形状（返回是否消费）。
    pub fn on_dimension(&mut self, text: &str) -> bool {
        let nums: Vec<f64> = text
            .split(|c: char| c == ',' || c == '，' || c == ';' || c == '；' || c.is_whitespace())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        if nums.is_empty() {
            return false;
        }
        let n_before = self.entities.len();
        let h = self.hover;
        match self.tool {
            SketchTool::Trim | SketchTool::Freehand | SketchTool::Text => return false,
            SketchTool::Line => {
                if self.pending.is_empty() || h.is_none() || nums[0] <= 0.0 {
                    return false;
                }
                let a = self.pending[self.pending.len() - 1];
                let (dx, dy) = (h.unwrap()[0] - a[0], h.unwrap()[1] - a[1]);
                let d = (dx * dx + dy * dy).sqrt();
                if d < 1e-9 {
                    return false;
                }
                let b = [a[0] + (dx / d) * nums[0], a[1] + (dy / d) * nums[0]];
                self.entities.push(SketchEntity::Line { a, b });
                self.pending = vec![b];
            }
            SketchTool::Circle => {
                if self.pending.is_empty() || nums[0] <= 0.0 {
                    return false;
                }
                self.entities.push(SketchEntity::Circle {
                    c: self.pending[0],
                    r: nums[0],
                });
                self.pending.clear();
            }
            SketchTool::Rect => {
                if self.pending.is_empty() {
                    return false;
                }
                let c1 = self.pending[0];
                let w = nums[0].abs();
                let hh = nums.get(1).map(|v| v.abs()).unwrap_or(w);
                if w < 1e-6 || hh < 1e-6 {
                    return false;
                }
                let sx = if let Some(hp) = h {
                    if hp[0] < c1[0] {
                        -1.0
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };
                let sy = if let Some(hp) = h {
                    if hp[1] < c1[1] {
                        -1.0
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };
                self.entities.push(SketchEntity::Rect {
                    c1,
                    c2: [c1[0] + sx * w, c1[1] + sy * hh],
                });
                self.pending.clear();
            }
            SketchTool::Polygon => {
                if self.pending.is_empty() || nums[0] <= 0.0 {
                    return false;
                }
                let c = self.pending[0];
                let rot = h
                    .map(|hp| (hp[1] - c[1]).atan2(hp[0] - c[0]))
                    .unwrap_or(std::f64::consts::FRAC_PI_2);
                self.entities.push(SketchEntity::Polygon {
                    c,
                    r: nums[0],
                    n: self.poly_sides,
                    rot,
                });
                self.pending.clear();
            }
            SketchTool::Arc => {
                if self.pending.len() != 2 {
                    return false;
                }
                let a = self.pending[0];
                let b = self.pending[1];
                let r = nums[0];
                let mx = (a[0] + b[0]) / 2.0;
                let my = (a[1] + b[1]) / 2.0;
                let half = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() / 2.0;
                if r <= half {
                    return false;
                }
                let hh = (r * r - half * half).sqrt();
                let dx = (b[0] - a[0]) / (half * 2.0);
                let dy = (b[1] - a[1]) / (half * 2.0);
                let mut px = -dy;
                let mut py = dx;
                if let Some(hp) = h {
                    let side = (hp[0] - mx) * px + (hp[1] - my) * py;
                    if side > 0.0 {
                        px = -px;
                        py = -py;
                    }
                }
                let c = [mx + px * hh, my + py * hh];
                let mid_arc = [c[0] - px * r, c[1] - py * r];
                if let Some(arc) = SketchEntity::arc_from_3_points(a, b, mid_arc) {
                    self.entities.push(arc);
                }
                self.pending.clear();
            }
            SketchTool::Pick | SketchTool::Align | SketchTool::Move | SketchTool::Rotate => {
                return false;
            }
        }
        if self.entities.len() > n_before {
            self.select_only(self.entities.len() - 1);
            self.drag = None;
            self.edit_preview = None;
        }
        true
    }

    /// 结束当前链（选中最新的实体）。
    pub fn finish_chain(&mut self) {
        self.pending.clear();
        self.chain_start = None;
        self.chain_segs = 0;
        if !self.entities.is_empty() {
            self.select_only(self.entities.len() - 1);
        }
    }

    /// 若正在画链（有 chain_start 或 pending 起点）则只结束链，返回 true；
    /// 否则返回 false（无链可结束）。
    pub fn finish_chain_if_any(&mut self) -> bool {
        if self.param_line.is_some() {
            self.param_line_done();
            return true;
        }
        if self.chain_start.is_some() || !self.pending.is_empty() {
            self.finish_chain();
            true
        } else {
            false
        }
    }

    // ================= 参数化 Line（两套输入体制）=================

    /// 开始参数化线：设置起点（并清掉旧的参数线实体，若有则保留）。
    pub fn param_line_start(&mut self, p: Point2) {
        if self.tool != SketchTool::Line {
            return;
        }
        self.param_line = Some(ParamLineState {
            start: p,
            end: p,
            entity_idx: None,
        });
        self.hover = Some(p);
        self.sync_param_entity();
    }

    /// 鼠标移动同步终点。
    pub fn param_line_update_end(&mut self, p: Point2) {
        if self.tool != SketchTool::Line {
            return;
        }
        let Some(pl) = self.param_line.as_mut() else {
            return;
        };
        pl.end = p;
        self.hover = Some(p);
        drop(pl);
        self.sync_param_entity();
    }

    /// 面板编辑起点。
    pub fn param_line_set_start(&mut self, s: Point2) {
        if self.tool != SketchTool::Line {
            return;
        }
        if self.param_line.is_none() {
            self.param_line_start(s);
            return;
        }
        if let Some(pl) = self.param_line.as_mut() {
            pl.start = s;
        }
        self.sync_param_entity();
    }

    /// 面板编辑终点（Set 1）。
    pub fn param_line_set_end(&mut self, e: Point2) {
        if self.tool != SketchTool::Line {
            return;
        }
        if self.param_line.is_none() {
            return;
        }
        if let Some(pl) = self.param_line.as_mut() {
            pl.end = e;
        }
        self.sync_param_entity();
    }

    /// 面板编辑角度 + 长度（Set 2，保持起点）。
    pub fn param_line_set_polar(&mut self, angle_deg: f64, length: f64) {
        if self.tool != SketchTool::Line || self.param_line.is_none() {
            return;
        }
        if let Some(pl) = self.param_line.as_mut() {
            let a = angle_deg.to_radians();
            pl.end = [
                pl.start[0] + length * a.cos(),
                pl.start[1] + length * a.sin(),
            ];
        }
        self.sync_param_entity();
    }

    /// 当前参数线的可读状态（面板显示用）。
    pub fn param_line_state(&self) -> Option<ParamLineState> {
        self.param_line.clone()
    }

    /// Next：完成当前线，起点 = 终点，继续画下一条（与链同步）。
    pub fn param_line_next(&mut self) {
        if let Some(pl) = self.param_line.as_mut() {
            pl.start = pl.end;
            pl.entity_idx = None; // 下一条是新实体
        }
        // 链同步：pending 设为本段终点，链起点保持
        if let Some(pl) = self.param_line {
            self.pending = vec![pl.start];
            if self.chain_start.is_none() {
                self.chain_start = Some(pl.start);
            }
        }
        self.sync_param_entity();
    }

    /// Done：退出参数线模式（结束链；实体已提交）。
    pub fn param_line_done(&mut self) {
        self.param_line = None;
    }

    /// 草图平面的局部轴名（面板显示角度相对哪个轴）。
    pub fn axis_names(&self) -> (String, String) {
        plane_axis_names(&self.plane)
    }

    /// The two axes shown to the user, ordered in world XYZ order. Each entry
    /// retains the corresponding local U/V index so operations can stay in the
    /// existing plane-local architecture.
    pub fn world_axis_bindings(&self) -> [SketchAxisBinding; 2] {
        world_axis_bindings_for_plane(&self.plane)
    }

    /// World axes available to the user in the active plane. For ordinary
    /// datum sketches this is the familiar pair (XY/YZ/ZX). Equation sketches
    /// show all three projected directions, including in Align.
    pub fn displayed_world_axes(&self) -> Vec<SketchDisplayAxis> {
        if !self.world_xyz_coordinates {
            return self
                .world_axis_bindings()
                .into_iter()
                .map(|binding| {
                    let mut local_direction = [0.0, 0.0];
                    local_direction[binding.local_axis] = binding.sign;
                    SketchDisplayAxis {
                        world_axis: binding.world_axis,
                        local_direction,
                    }
                })
                .collect();
        }
        projected_world_axes_for_plane(&self.plane)
    }

    /// 取消当前选中的实体（不退出 sketch）。
    pub fn deselect(&mut self) {
        self.selected_entity = None;
        self.pick_selection.clear();
        self.drag = None;
        self.edit_preview = None;
    }

    /// 读取某实体的 Line 端点（非 Line 返回 None）。
    pub fn line_entity(&self, idx: usize) -> Option<(Point2, Point2)> {
        match self.entities.get(idx) {
            Some(SketchEntity::Line { a, b }) => Some((*a, *b)),
            _ => None,
        }
    }

    /// 直接更新某 Line 实体的端点（Pick 模式下修改选中段，允许链断开）。
    pub fn update_line_entity(&mut self, idx: usize, a: Point2, b: Point2) {
        if self.line_entity(idx).is_some() && dist(a, b).is_finite() && dist(a, b) > 1e-6 {
            self.keep_locked_vertex(idx, SketchEntity::Line { a, b });
        }
    }

    /// 把当前参数线同步到 entities（编辑时更新同一条，避免堆积）。
    fn sync_param_entity(&mut self) {
        let Some(pl) = self.param_line.clone() else {
            return;
        };
        if dist(pl.end, pl.start) < 1e-6 {
            // 零长度：不产生实体（但保留已有实体）
            return;
        }
        let line = SketchEntity::Line {
            a: pl.start,
            b: pl.end,
        };
        match pl.entity_idx {
            Some(i) => {
                if let Some(e) = self.entities.get_mut(i) {
                    *e = line;
                }
            }
            None => {
                self.entities.push(line);
                let i = self.entities.len() - 1;
                if let Some(pl) = self.param_line.as_mut() {
                    pl.entity_idx = Some(i);
                }
                // 链同步：面板生成的段同样推进链（闭合判定可用）
                self.pending = vec![pl.end];
                if self.chain_start.is_none() {
                    self.chain_start = Some(pl.start);
                }
                self.chain_segs += 1;
            }
        }
    }

    /// 实体参数编辑字段（委托给 SketchEntity，标签按草图平面的实际轴向显示）。
    pub fn entity_params(&self, idx: usize) -> Option<Vec<(&'static str, String, f64, bool)>> {
        let entity = self.entities.get(idx)?;
        if self.world_xyz_coordinates {
            return Some(self.world_xyz_entity_params(entity));
        }
        let params = entity.params();
        let (uname, vname) = plane_axis_names(&self.plane);
        Some(
            params
                .into_iter()
                .map(|(key, label, value, hidden)| {
                    // Use placeholders so swapped axes (for example U=Z, V=Y)
                    // cannot be replaced twice by cascading String::replace.
                    let label = label
                        .replace('X', "\u{0}")
                        .replace('Y', "\u{1}")
                        .replace('\u{0}', &uname)
                        .replace('\u{1}', &vname);
                    (key, label, value, hidden)
                })
                .collect(),
        )
    }

    fn world_xyz_entity_params(
        &self,
        entity: &SketchEntity,
    ) -> Vec<(&'static str, String, f64, bool)> {
        let world = |point: Point2| self.plane.to_world(point[0], point[1]);
        let point_fields = |label: &str, point: Point2, keys: [&'static str; 3], hidden: bool| {
            let value = world(point);
            vec![
                (keys[0], format!("{label} X"), value[0], hidden),
                (keys[1], format!("{label} Y"), value[1], hidden),
                (keys[2], format!("{label} Z"), value[2], hidden),
            ]
        };
        match entity {
            SketchEntity::Path { origin, .. } => {
                let mut params = point_fields(
                    "Position",
                    *origin,
                    ["center_x", "center_y", "center_z"],
                    false,
                );
                params.extend(
                    entity
                        .params()
                        .into_iter()
                        .filter(|(key, ..)| *key != "x" && *key != "y")
                        .map(|(k, l, v, h)| (k, l.to_string(), v, h)),
                );
                params
            }
            SketchEntity::Line { a, b } => {
                let mut params = point_fields("Start", *a, ["p1_x", "p1_y", "p1_z"], false);
                params.extend(point_fields("End", *b, ["p2_x", "p2_y", "p2_z"], false));
                params.push(("len", "Length".to_string(), dist(*a, *b), true));
                params
            }
            SketchEntity::Circle { c, r } => {
                let mut params =
                    point_fields("Center", *c, ["center_x", "center_y", "center_z"], false);
                params.push(("r", "Radius".to_string(), *r, false));
                params
            }
            SketchEntity::Rect { c1, c2 } => {
                let x1 = c1[0].min(c2[0]);
                let y1 = c1[1].min(c2[1]);
                let x2 = c1[0].max(c2[0]);
                let y2 = c1[1].max(c2[1]);
                let mut params = point_fields("V1", [x1, y1], ["v1_x", "v1_y", "v1_z"], false);
                params.push(("w", "Length".to_string(), x2 - x1, false));
                params.push(("h", "Width".to_string(), y2 - y1, false));
                params.extend(point_fields("V2", [x2, y1], ["v2_x", "v2_y", "v2_z"], true));
                params.extend(point_fields("V3", [x2, y2], ["v3_x", "v3_y", "v3_z"], true));
                params.extend(point_fields("V4", [x1, y2], ["v4_x", "v4_y", "v4_z"], true));
                params
            }
            SketchEntity::RotatedRect {
                c,
                width,
                height,
                rot,
            } => {
                let mut params =
                    point_fields("Center", *c, ["center_x", "center_y", "center_z"], false);
                params.push(("w", "Length".to_string(), *width, false));
                params.push(("h", "Width".to_string(), *height, false));
                params.push((
                    "rot",
                    "Rotation clockwise deg".to_string(),
                    -rot.to_degrees(),
                    false,
                ));
                params
            }
            SketchEntity::Polygon { c, r, n, rot } => {
                let mut params =
                    point_fields("Center", *c, ["center_x", "center_y", "center_z"], false);
                params.push(("n", "Sides".to_string(), *n as f64, false));
                params.push((
                    "s",
                    "Side length".to_string(),
                    2.0 * r * (std::f64::consts::PI / *n as f64).sin(),
                    false,
                ));
                params.push(("rot", "Rotation deg".to_string(), rot.to_degrees(), false));
                params
            }
            SketchEntity::Arc {
                c,
                r,
                a0,
                a1,
                ccw: _,
            } => {
                let mut params =
                    point_fields("Center", *c, ["center_x", "center_y", "center_z"], false);
                params.push(("r", "Radius".to_string(), *r, false));
                params.push(("a0", "Start deg".to_string(), a0.to_degrees(), false));
                params.push(("a1", "End deg".to_string(), a1.to_degrees(), false));
                params
            }
        }
    }

    /// 应用参数编辑（保存到实体；可能失败）。
    pub fn apply_entity_edit(
        &mut self,
        idx: usize,
        vals: &HashMap<&str, f64>,
    ) -> Result<(), String> {
        if vals.values().any(|value| !value.is_finite()) {
            return Err("Sketch parameters must be finite".into());
        }
        let e = self
            .entities
            .get(idx)
            .cloned()
            .ok_or_else(|| "Sketch entity no longer exists".to_string())?;
        // 合并完整参数：以当前实体全部参数为基准，仅覆盖传入的改动字段
        // （修复：UI 只传改动字段时其它参数被清零的 bug）
        let mut full: HashMap<&str, f64> =
            e.params().into_iter().map(|(k, _, v, _)| (k, v)).collect();
        let shape_changed = vals.iter().any(|(&key, &value)| {
            matches!(key, "w" | "h" | "r" | "n" | "s" | "a0" | "a1" | "rot")
                && full
                    .get(key)
                    .is_some_and(|before| (value - before).abs() > 1e-9)
        });
        for (k, v) in vals {
            full.insert(k, *v);
        }
        if self.world_xyz_coordinates {
            match &e {
                SketchEntity::Line { a, b } => {
                    let a =
                        world_point_with_updates(&self.plane, *a, vals, ["p1_x", "p1_y", "p1_z"]);
                    let b =
                        world_point_with_updates(&self.plane, *b, vals, ["p2_x", "p2_y", "p2_z"]);
                    full.insert("x1", a[0]);
                    full.insert("y1", a[1]);
                    full.insert("x2", b[0]);
                    full.insert("y2", b[1]);
                }
                SketchEntity::Path { origin: c, .. }
                | SketchEntity::Circle { c, .. }
                | SketchEntity::Polygon { c, .. }
                | SketchEntity::Arc { c, .. }
                | SketchEntity::RotatedRect { c, .. } => {
                    let c = world_point_with_updates(
                        &self.plane,
                        *c,
                        vals,
                        ["center_x", "center_y", "center_z"],
                    );
                    full.insert("x", c[0]);
                    full.insert("y", c[1]);
                }
                SketchEntity::Rect { c1, c2 } => {
                    let lower = [c1[0].min(c2[0]), c1[1].min(c2[1])];
                    let lower = world_point_with_updates(
                        &self.plane,
                        lower,
                        vals,
                        ["v1_x", "v1_y", "v1_z"],
                    );
                    full.insert("v1x", lower[0]);
                    full.insert("v1y", lower[1]);
                }
            }
        }
        let mut rebuilt = e.from_params(&full)?;
        if let (SketchEntity::Rect { c1, c2 }, SketchEntity::Rect { c1: lo, c2: hi }) =
            (&e, &mut rebuilt)
        {
            for axis in 0..2 {
                if c1[axis] > c2[axis] {
                    std::mem::swap(&mut lo[axis], &mut hi[axis]);
                }
            }
        }
        if shape_changed || matches!(e, SketchEntity::Line { .. }) {
            self.keep_locked_vertex(idx, rebuilt);
        } else {
            self.entities[idx] = rebuilt;
        }
        self.edit_preview = None;
        Ok(())
    }

    /// 阶段预览（不提交）。
    pub fn stage_edit(&mut self, _idx: usize, vals: Option<&HashMap<&str, f64>>) {
        if let (Some(idx), Some(vals)) = (self.selected_entity, vals) {
            let e = &self.entities[idx];
            self.edit_preview = e.from_params(vals).ok();
        } else {
            self.edit_preview = None;
        }
    }

    /// 当前绘制预览实体（未完成形状）。
    pub fn drawing_preview(&self) -> Option<(Vec<Point2>, bool)> {
        if !self.stroke.is_empty() {
            return Some((self.stroke.clone(), self.freehand_close));
        }
        if self.pending.is_empty() {
            return None;
        }
        match self.tool {
            SketchTool::Line if self.pending.len() >= 1 && self.hover.is_some() => {
                let a = self.pending[self.pending.len() - 1];
                Some((vec![a, self.hover.unwrap()], false))
            }
            SketchTool::Circle if self.pending.len() >= 1 && self.hover.is_some() => {
                let c = self.pending[0];
                let r = dist(c, self.hover.unwrap());
                if r < 1e-6 {
                    None
                } else {
                    let mut pts = Vec::new();
                    for i in 0..=64 {
                        let t = (i as f64 / 64.0) * std::f64::consts::TAU;
                        pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                    }
                    Some((pts, true))
                }
            }
            SketchTool::Rect if self.pending.len() >= 1 && self.hover.is_some() => {
                let c1 = self.pending[0];
                let h = self.hover.unwrap();
                Some((vec![c1, [h[0], c1[1]], h, [c1[0], h[1]]], true))
            }
            SketchTool::Polygon if self.pending.len() >= 1 && self.hover.is_some() => {
                let c = self.pending[0];
                let h = self.hover.unwrap();
                let r = dist(c, h);
                if r < 1e-6 {
                    None
                } else {
                    let rot = (h[1] - c[1]).atan2(h[0] - c[0]);
                    let n = self.poly_sides.max(3);
                    let mut pts = Vec::new();
                    for i in 0..n {
                        let t = rot + (i as f64 / n as f64) * std::f64::consts::TAU;
                        pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                    }
                    Some((pts, true))
                }
            }
            SketchTool::Arc if self.pending.len() == 1 && self.hover.is_some() => {
                Some((vec![self.pending[0], self.hover.unwrap()], false))
            }
            SketchTool::Arc if self.pending.len() == 2 && self.hover.is_some() => {
                let a = self.pending[0];
                let b = self.pending[1];
                if let Some(arc) = SketchEntity::arc_from_3_points(a, b, self.hover.unwrap()) {
                    Some(arc.to_chain())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Super3D 风格的光标随动尺寸，只在一个实体正在放置时显示。
    pub fn live_dimension(&self) -> Option<String> {
        self.live_dimension_in(1.0, "mm", 1)
    }

    pub fn live_dimension_in(
        &self,
        display_units_per_mm: f64,
        symbol: &str,
        decimals: usize,
    ) -> Option<String> {
        let hover = self.hover?;
        let first = *self.pending.first()?;
        let value =
            |millimeters: f64| format!("{:.*}", decimals, millimeters * display_units_per_mm);
        match self.tool {
            SketchTool::Line => {
                let start = *self.pending.last()?;
                Some(format!("L {} {symbol}", value(dist(start, hover))))
            }
            SketchTool::Circle => Some(format!("⌀ {} {symbol}", value(2.0 * dist(first, hover)))),
            SketchTool::Rect => Some(format!(
                "{} × {} {symbol}",
                value((hover[0] - first[0]).abs()),
                value((hover[1] - first[1]).abs())
            )),
            SketchTool::Polygon => Some(format!(
                "R {} {symbol}  sides {}",
                value(dist(first, hover)),
                self.poly_sides.max(3)
            )),
            SketchTool::Arc if self.pending.len() == 2 => {
                match SketchEntity::arc_from_3_points(self.pending[0], self.pending[1], hover) {
                    Some(SketchEntity::Arc { r, .. }) => Some(format!("R {} {symbol}", value(r))),
                    _ => None,
                }
            }
            SketchTool::Freehand
            | SketchTool::Trim
            | SketchTool::Text
            | SketchTool::Arc
            | SketchTool::Pick
            | SketchTool::Align
            | SketchTool::Move
            | SketchTool::Rotate => None,
        }
    }

    /// 始终显示世界 XYZ 分量，使方向与右下角 Gizmo 完全一致。即使任意
    /// 曲面草图内部仍使用 U/V，也只呈现其绑定的两个世界轴。
    pub fn cursor_coordinate_label(&self) -> Option<String> {
        self.cursor_coordinate_label_in(1.0, "", 4)
    }

    pub fn cursor_coordinate_label_in(
        &self,
        display_units_per_mm: f64,
        symbol: &str,
        decimals: usize,
    ) -> Option<String> {
        let hover = self.hover?;
        let snapped_keypoint = matches!(
            self.snap_kind,
            Some(SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Center)
        );
        let prefix = if snapped_keypoint { "● " } else { "" };
        let world = self.to_world(hover);
        let suffix = if symbol.is_empty() {
            String::new()
        } else {
            format!(" {symbol}")
        };
        let value =
            |millimeters: f64| format!("{:.*}", decimals, millimeters * display_units_per_mm);
        if self.world_xyz_coordinates {
            return Some(format!(
                "{prefix}X {}   Y {}   Z {}{suffix}",
                value(world[0]),
                value(world[1]),
                value(world[2])
            ));
        }
        let [first, second] = self.world_axis_bindings();
        Some(format!(
            "{prefix}{} {}   {} {}{suffix}",
            first.label(),
            value(world[first.world_axis]),
            second.label(),
            value(world[second.world_axis])
        ))
    }

    pub fn cursor_readout(&self) -> Option<String> {
        let coordinate = self.cursor_coordinate_label()?;
        Some(match self.live_dimension() {
            Some(dimension) => format!("{dimension}\n{coordinate}"),
            None => coordinate,
        })
    }

    pub fn cursor_readout_in(
        &self,
        display_units_per_mm: f64,
        symbol: &str,
        decimals: usize,
    ) -> Option<String> {
        let coordinate = self.cursor_coordinate_label_in(display_units_per_mm, symbol, decimals)?;
        Some(
            match self.live_dimension_in(display_units_per_mm, symbol, decimals) {
                Some(dimension) => format!("{dimension}\n{coordinate}"),
                None => coordinate,
            },
        )
    }
}

/// Iterative Ramer-Douglas-Peucker keeps one stroke compact without recursion.
fn simplify_stroke(points: &[Point2], tolerance: f64) -> Vec<Point2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let farthest = ((a + 1)..b)
            .map(|i| (i, dist_point_seg(points[i], points[a], points[b])))
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((index, d)) = farthest.filter(|(_, d)| *d > tolerance) {
            let _ = d;
            keep[index] = true;
            stack.push((a, index));
            stack.push((index, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

fn entity_bounds(entity: &SketchEntity) -> (Point2, Point2) {
    let points: Vec<_> = entity
        .to_chains()
        .into_iter()
        .flat_map(|(p, _)| p)
        .collect();
    points.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(mut minimum, mut maximum), point| {
            for axis in 0..2 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
            (minimum, maximum)
        },
    )
}

/// 返回建模常用的端点、中点、中心和圆象限点。
fn entity_key_points(entity: &SketchEntity) -> Vec<(Point2, SnapKind)> {
    let midpoint = |a: Point2, b: Point2| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    match entity {
        SketchEntity::Path { origin, .. } => {
            let (lo, hi) = entity_bounds(entity);
            vec![
                (*origin, SnapKind::Center),
                (lo, SnapKind::Endpoint),
                (hi, SnapKind::Endpoint),
                ([lo[0], hi[1]], SnapKind::Endpoint),
                ([hi[0], lo[1]], SnapKind::Endpoint),
            ]
        }
        SketchEntity::Line { a, b } => vec![
            (*a, SnapKind::Endpoint),
            (*b, SnapKind::Endpoint),
            (midpoint(*a, *b), SnapKind::Midpoint),
        ],
        SketchEntity::Rect { c1, c2 } => {
            let corners = [
                [c1[0], c1[1]],
                [c2[0], c1[1]],
                [c2[0], c2[1]],
                [c1[0], c2[1]],
            ];
            let mut points: Vec<_> = corners
                .iter()
                .copied()
                .map(|point| (point, SnapKind::Endpoint))
                .collect();
            for index in 0..4 {
                points.push((
                    midpoint(corners[index], corners[(index + 1) % 4]),
                    SnapKind::Midpoint,
                ));
            }
            points.push((midpoint(*c1, *c2), SnapKind::Center));
            points
        }
        SketchEntity::RotatedRect { c, .. } => {
            let (corners, _) = entity.to_chain();
            let mut points: Vec<_> = corners
                .iter()
                .copied()
                .map(|point| (point, SnapKind::Endpoint))
                .collect();
            for index in 0..4 {
                points.push((
                    midpoint(corners[index], corners[(index + 1) % 4]),
                    SnapKind::Midpoint,
                ));
            }
            points.push((*c, SnapKind::Center));
            points
        }
        SketchEntity::Circle { c, r } => vec![
            (*c, SnapKind::Center),
            ([c[0] + r, c[1]], SnapKind::Endpoint),
            ([c[0] - r, c[1]], SnapKind::Endpoint),
            ([c[0], c[1] + r], SnapKind::Endpoint),
            ([c[0], c[1] - r], SnapKind::Endpoint),
        ],
        SketchEntity::Arc { c, r, a0, a1, ccw } => {
            let mut sweep = a1 - a0;
            if *ccw {
                while sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
            } else {
                while sweep >= 0.0 {
                    sweep -= std::f64::consts::TAU;
                }
            }
            let point_at = |angle: f64| [c[0] + r * angle.cos(), c[1] + r * angle.sin()];
            vec![
                (*c, SnapKind::Center),
                (point_at(*a0), SnapKind::Endpoint),
                (point_at(*a1), SnapKind::Endpoint),
                (point_at(*a0 + sweep * 0.5), SnapKind::Midpoint),
            ]
        }
        SketchEntity::Polygon { c, r, n, rot } => {
            let count = (*n).max(3) as usize;
            let vertices: Vec<Point2> = (0..count)
                .map(|index| {
                    let angle = rot + index as f64 / count as f64 * std::f64::consts::TAU;
                    [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
                })
                .collect();
            let mut points: Vec<_> = vertices
                .iter()
                .copied()
                .map(|point| (point, SnapKind::Endpoint))
                .collect();
            for index in 0..count {
                points.push((
                    midpoint(vertices[index], vertices[(index + 1) % count]),
                    SnapKind::Midpoint,
                ));
            }
            points.push((*c, SnapKind::Center));
            points
        }
    }
}

fn resize_handles(entity: &SketchEntity) -> Vec<(Point2, ResizeHandle)> {
    let midpoint = |a: Point2, b: Point2| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    match entity {
        SketchEntity::Path { origin, .. } => {
            let (lo, hi) = entity_bounds(entity);
            let farthest = [lo, hi, [lo[0], hi[1]], [hi[0], lo[1]]]
                .into_iter()
                .max_by(|a, b| dist(*a, *origin).total_cmp(&dist(*b, *origin)))
                .unwrap();
            vec![(farthest, ResizeHandle::PathScale)]
        }
        SketchEntity::Line { a, b } => {
            vec![(*a, ResizeHandle::LineStart), (*b, ResizeHandle::LineEnd)]
        }
        SketchEntity::Rect { .. } => {
            let (corners, _) = entity.to_chain();
            let mut handles: Vec<_> = corners
                .iter()
                .enumerate()
                .map(|(index, point)| (*point, ResizeHandle::RectCorner(index)))
                .collect();
            for index in 0..4 {
                handles.push((
                    midpoint(corners[index], corners[(index + 1) % 4]),
                    ResizeHandle::RectEdge(index),
                ));
            }
            handles
        }
        SketchEntity::RotatedRect { .. } => {
            let (corners, _) = entity.to_chain();
            let mut handles: Vec<_> = corners
                .iter()
                .enumerate()
                .map(|(index, point)| (*point, ResizeHandle::RotatedRectCorner(index)))
                .collect();
            for index in 0..4 {
                handles.push((
                    midpoint(corners[index], corners[(index + 1) % 4]),
                    ResizeHandle::RotatedRectEdge(index),
                ));
            }
            handles
        }
        SketchEntity::Circle { c, r } => vec![
            ([c[0] + r, c[1]], ResizeHandle::CircleRadius),
            ([c[0] - r, c[1]], ResizeHandle::CircleRadius),
            ([c[0], c[1] + r], ResizeHandle::CircleRadius),
            ([c[0], c[1] - r], ResizeHandle::CircleRadius),
        ],
        SketchEntity::Arc { c, r, a0, a1, ccw } => {
            let mut sweep = a1 - a0;
            if *ccw {
                while sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
            } else {
                while sweep >= 0.0 {
                    sweep -= std::f64::consts::TAU;
                }
            }
            let point_at = |angle: f64| [c[0] + r * angle.cos(), c[1] + r * angle.sin()];
            vec![
                (point_at(*a0), ResizeHandle::ArcStart),
                (point_at(*a1), ResizeHandle::ArcEnd),
                (point_at(*a0 + sweep * 0.5), ResizeHandle::ArcRadius),
            ]
        }
        SketchEntity::Polygon { c, r, n, rot } => {
            let count = (*n).max(3) as usize;
            let vertices: Vec<Point2> = (0..count)
                .map(|index| {
                    let angle = rot + index as f64 / count as f64 * std::f64::consts::TAU;
                    [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
                })
                .collect();
            let mut handles: Vec<_> = vertices
                .iter()
                .enumerate()
                .map(|(index, point)| (*point, ResizeHandle::PolygonVertex(index)))
                .collect();
            for index in 0..count {
                handles.push((
                    midpoint(vertices[index], vertices[(index + 1) % count]),
                    ResizeHandle::PolygonEdge(index),
                ));
            }
            handles
        }
    }
}

fn resize_handle_at(
    entity: &SketchEntity,
    point: Point2,
    tolerance: f64,
) -> Option<(ResizeHandle, Point2)> {
    resize_handles(entity)
        .into_iter()
        .filter_map(|(handle_point, handle)| {
            let distance = dist(handle_point, point);
            (distance < tolerance).then_some((handle, handle_point, distance))
        })
        .min_by(|left, right| left.2.total_cmp(&right.2))
        .map(|(handle, handle_point, _)| (handle, handle_point))
}

fn resize_entity(
    entity: &SketchEntity,
    handle: ResizeHandle,
    target: Point2,
) -> Option<SketchEntity> {
    if target.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let minimum = 1e-6;
    match (entity, handle) {
        (SketchEntity::Path { origin, .. }, ResizeHandle::PathScale) => {
            let start = resize_handles(entity).first()?.0;
            let mut resized = entity.clone();
            if let SketchEntity::Path { scale, .. } = &mut resized {
                *scale *= dist(target, *origin) / dist(start, *origin).max(minimum);
                if *scale <= minimum {
                    return None;
                }
            }
            Some(resized)
        }
        (SketchEntity::Line { a: _, b }, ResizeHandle::LineStart) => {
            (dist(target, *b) > minimum).then_some(SketchEntity::Line { a: target, b: *b })
        }
        (SketchEntity::Line { a, b: _ }, ResizeHandle::LineEnd) => {
            (dist(*a, target) > minimum).then_some(SketchEntity::Line { a: *a, b: target })
        }
        (SketchEntity::Rect { .. }, ResizeHandle::RectCorner(index)) => {
            let (corners, _) = entity.to_chain();
            let opposite = corners[(index + 2) % 4];
            ((target[0] - opposite[0]).abs() > minimum && (target[1] - opposite[1]).abs() > minimum)
                .then_some(SketchEntity::Rect {
                    c1: opposite,
                    c2: target,
                })
        }
        (SketchEntity::Rect { c1, c2 }, ResizeHandle::RectEdge(index)) => {
            let mut first = *c1;
            let mut second = *c2;
            match index % 4 {
                0 => first[1] = target[1],
                1 => second[0] = target[0],
                2 => second[1] = target[1],
                _ => first[0] = target[0],
            }
            ((second[0] - first[0]).abs() > minimum && (second[1] - first[1]).abs() > minimum)
                .then_some(SketchEntity::Rect {
                    c1: first,
                    c2: second,
                })
        }
        (SketchEntity::RotatedRect { rot, .. }, ResizeHandle::RotatedRectCorner(index)) => {
            let (corners, _) = entity.to_chain();
            let opposite = corners[(index + 2) % 4];
            let delta = [target[0] - opposite[0], target[1] - opposite[1]];
            let (sin, cos) = rot.sin_cos();
            let local_x = delta[0] * cos + delta[1] * sin;
            let local_y = -delta[0] * sin + delta[1] * cos;
            (local_x.abs() > minimum && local_y.abs() > minimum).then_some(
                SketchEntity::RotatedRect {
                    c: [
                        (target[0] + opposite[0]) * 0.5,
                        (target[1] + opposite[1]) * 0.5,
                    ],
                    width: local_x.abs(),
                    height: local_y.abs(),
                    rot: *rot,
                },
            )
        }
        (
            SketchEntity::RotatedRect {
                c,
                width,
                height,
                rot,
            },
            ResizeHandle::RotatedRectEdge(index),
        ) => {
            let (sin, cos) = rot.sin_cos();
            let axis_x = [cos, sin];
            let axis_y = [-sin, cos];
            let (axis, old_extent) = if index % 2 == 0 {
                (axis_y, *height)
            } else {
                (axis_x, *width)
            };
            let side_sign = if matches!(index % 4, 0 | 3) {
                -1.0
            } else {
                1.0
            };
            let opposite = [
                c[0] - axis[0] * side_sign * old_extent * 0.5,
                c[1] - axis[1] * side_sign * old_extent * 0.5,
            ];
            let signed_extent =
                (target[0] - opposite[0]) * axis[0] + (target[1] - opposite[1]) * axis[1];
            (signed_extent.abs() > minimum).then_some(SketchEntity::RotatedRect {
                c: [
                    opposite[0] + axis[0] * signed_extent * 0.5,
                    opposite[1] + axis[1] * signed_extent * 0.5,
                ],
                width: if index % 2 == 1 {
                    signed_extent.abs()
                } else {
                    *width
                },
                height: if index % 2 == 0 {
                    signed_extent.abs()
                } else {
                    *height
                },
                rot: *rot,
            })
        }
        (SketchEntity::Circle { c, .. }, ResizeHandle::CircleRadius) => {
            let radius = dist(*c, target);
            (radius > minimum).then_some(SketchEntity::Circle { c: *c, r: radius })
        }
        (SketchEntity::Arc { c, a1, ccw, .. }, ResizeHandle::ArcStart) => {
            let radius = dist(*c, target);
            if radius <= minimum {
                return None;
            }
            let new_start = (target[1] - c[1]).atan2(target[0] - c[0]);
            let (new_start, new_end) = normalize_arc_angles(new_start, *a1, *ccw);
            Some(SketchEntity::Arc {
                c: *c,
                r: radius,
                a0: new_start,
                a1: new_end,
                ccw: *ccw,
            })
        }
        (
            SketchEntity::Arc {
                c, r: _, a0, ccw, ..
            },
            ResizeHandle::ArcEnd,
        ) => {
            let radius = dist(*c, target);
            if radius <= minimum {
                return None;
            }
            let new_end = (target[1] - c[1]).atan2(target[0] - c[0]);
            let (new_start, new_end) = normalize_arc_angles(*a0, new_end, *ccw);
            Some(SketchEntity::Arc {
                c: *c,
                r: radius,
                a0: new_start,
                a1: new_end,
                ccw: *ccw,
            })
        }
        (SketchEntity::Arc { c, a0, a1, ccw, .. }, ResizeHandle::ArcRadius) => {
            let radius = dist(*c, target);
            (radius > minimum).then_some(SketchEntity::Arc {
                c: *c,
                r: radius,
                a0: *a0,
                a1: *a1,
                ccw: *ccw,
            })
        }
        (SketchEntity::Polygon { c, n, .. }, ResizeHandle::PolygonVertex(index)) => {
            let radius = dist(*c, target);
            if radius <= minimum {
                return None;
            }
            let step = std::f64::consts::TAU / (*n).max(3) as f64;
            let angle = (target[1] - c[1]).atan2(target[0] - c[0]);
            Some(SketchEntity::Polygon {
                c: *c,
                r: radius,
                n: *n,
                rot: angle - index as f64 * step,
            })
        }
        (SketchEntity::Polygon { c, n, .. }, ResizeHandle::PolygonEdge(index)) => {
            let count = (*n).max(3) as f64;
            let apothem = dist(*c, target);
            let cosine = (std::f64::consts::PI / count).cos();
            let radius = apothem / cosine.max(minimum);
            if radius <= minimum {
                return None;
            }
            let step = std::f64::consts::TAU / count;
            let angle = (target[1] - c[1]).atan2(target[0] - c[0]);
            Some(SketchEntity::Polygon {
                c: *c,
                r: radius,
                n: *n,
                rot: angle - (index as f64 + 0.5) * step,
            })
        }
        _ => None,
    }
}

fn normalize_arc_angles(a0: f64, a1: f64, ccw: bool) -> (f64, f64) {
    let mut end = a1;
    if ccw {
        while end <= a0 {
            end += std::f64::consts::TAU;
        }
    } else {
        while end >= a0 {
            end -= std::f64::consts::TAU;
        }
    }
    (a0, end)
}

/// Assign the two local plane axes to two distinct world axes. Standard datum
/// planes map exactly; an arbitrary planar surface uses the best two-component
/// assignment while keeping all geometry calculations in its local plane.
pub fn world_axis_bindings_for_plane(plane: &PlaneDef) -> [SketchAxisBinding; 2] {
    let local = [plane.x_axis(), plane.y_axis()];
    let component = |local_axis: usize, world_axis: usize| match world_axis {
        0 => local[local_axis].x,
        1 => local[local_axis].y,
        _ => local[local_axis].z,
    };

    let mut best_axes = [0usize, 1usize];
    let mut best_score = f64::NEG_INFINITY;
    for first in 0..3 {
        for second in 0..3 {
            if first == second {
                continue;
            }
            let score = component(0, first).abs() + component(1, second).abs();
            if score > best_score {
                best_score = score;
                best_axes = [first, second];
            }
        }
    }

    let mut bindings = [
        SketchAxisBinding {
            local_axis: 0,
            world_axis: best_axes[0],
            sign: if component(0, best_axes[0]) < 0.0 {
                -1.0
            } else {
                1.0
            },
        },
        SketchAxisBinding {
            local_axis: 1,
            world_axis: best_axes[1],
            sign: if component(1, best_axes[1]) < 0.0 {
                -1.0
            } else {
                1.0
            },
        },
    ];
    bindings.sort_by_key(|binding| binding.world_axis);
    bindings
}

/// Project each usable world coordinate axis into a plane and express the
/// normalized result in local sketch coordinates. A world axis parallel to the
/// plane normal has no in-plane direction and is therefore omitted.
pub fn projected_world_axes_for_plane(plane: &PlaneDef) -> Vec<SketchDisplayAxis> {
    let u = plane.x_axis();
    let v = plane.y_axis();
    let world = [glam::DVec3::X, glam::DVec3::Y, glam::DVec3::Z];
    world
        .into_iter()
        .enumerate()
        .filter_map(|(world_axis, direction)| {
            let local = [direction.dot(u), direction.dot(v)];
            let length = (local[0] * local[0] + local[1] * local[1]).sqrt();
            (length > 1e-9).then_some(SketchDisplayAxis {
                world_axis,
                local_direction: [local[0] / length, local[1] / length],
            })
        })
        .collect()
}

fn entity_projection_bounds(entity: &SketchEntity, direction: Point2) -> (f64, f64) {
    let points: Vec<_> = entity
        .to_chains()
        .into_iter()
        .flat_map(|(p, _)| p)
        .collect();
    points.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), point| {
            let projected = point[0] * direction[0] + point[1] * direction[1];
            (minimum.min(projected), maximum.max(projected))
        },
    )
}

fn world_point_with_updates(
    plane: &PlaneDef,
    local: Point2,
    values: &HashMap<&str, f64>,
    keys: [&str; 3],
) -> Point2 {
    let mut world = plane.to_world(local[0], local[1]);
    for axis in 0..3 {
        if let Some(value) = values.get(keys[axis]) {
            world[axis] = *value;
        }
    }
    let (u, v) = plane.to_local(world);
    [u, v]
}

/// Local parameter fields still store U/V values. Label them with the bound XYZ
/// axis and an explicit minus sign when positive local values run opposite to
/// the corresponding Gizmo direction.
fn plane_axis_names(plane: &PlaneDef) -> (String, String) {
    let bindings = world_axis_bindings_for_plane(plane);
    let name_for_local = |local_axis| {
        let binding = bindings
            .iter()
            .find(|binding| binding.local_axis == local_axis)
            .copied()
            .expect("both local axes are bound");
        if binding.sign < 0.0 {
            format!("-{}", binding.label())
        } else {
            binding.label().to_string()
        }
    };
    (name_for_local(0), name_for_local(1))
}
/// 点到点距离。
pub fn dist(a: Point2, b: Point2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// 点到实体的最小距离（折线近似）。
fn entity_distance(e: &SketchEntity, p: Point2) -> f64 {
    let mut best = f64::INFINITY;
    for (pts, closed) in e.to_chains() {
        let n = pts.len();
        let cnt = if closed { n } else { n.saturating_sub(1) };
        for i in 0..cnt {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            best = best.min(dist_point_seg(p, a, b));
        }
    }
    best
}

fn dist_point_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let l2 = abx * abx + aby * aby;
    let mut t = if l2 > 1e-12 {
        ((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / l2
    } else {
        0.0
    };
    t = t.clamp(0.0, 1.0);
    let qx = a[0] + abx * t;
    let qy = a[1] + aby * t;
    ((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt()
}

/// 平移实体（拖拽移动）。
fn move_entity(e: &SketchEntity, delta: Point2) -> Option<SketchEntity> {
    let d = delta;
    match e {
        SketchEntity::Path { .. } => {
            let mut moved = e.clone();
            if let SketchEntity::Path { origin, .. } = &mut moved {
                origin[0] += d[0];
                origin[1] += d[1];
            }
            Some(moved)
        }
        SketchEntity::Line { a, b } => Some(SketchEntity::Line {
            a: [a[0] + d[0], a[1] + d[1]],
            b: [b[0] + d[0], b[1] + d[1]],
        }),
        SketchEntity::Circle { c, r } => Some(SketchEntity::Circle {
            c: [c[0] + d[0], c[1] + d[1]],
            r: *r,
        }),
        SketchEntity::Rect { c1, c2 } => Some(SketchEntity::Rect {
            c1: [c1[0] + d[0], c1[1] + d[1]],
            c2: [c2[0] + d[0], c2[1] + d[1]],
        }),
        SketchEntity::RotatedRect {
            c,
            width,
            height,
            rot,
        } => Some(SketchEntity::RotatedRect {
            c: [c[0] + d[0], c[1] + d[1]],
            width: *width,
            height: *height,
            rot: *rot,
        }),
        SketchEntity::Polygon { c, r, n, rot } => Some(SketchEntity::Polygon {
            c: [c[0] + d[0], c[1] + d[1]],
            r: *r,
            n: *n,
            rot: *rot,
        }),
        SketchEntity::Arc { c, r, a0, a1, ccw } => Some(SketchEntity::Arc {
            c: [c[0] + d[0], c[1] + d[1]],
            r: *r,
            a0: *a0,
            a1: *a1,
            ccw: *ccw,
        }),
    }
}

fn selection_center(entities: &[SketchEntity], selection: &[usize]) -> Option<Point2> {
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    let mut found = false;
    for entity in selection.iter().filter_map(|index| entities.get(*index)) {
        let points: Vec<_> = entity
            .to_chains()
            .into_iter()
            .flat_map(|(p, _)| p)
            .collect();
        for point in points {
            found = true;
            for axis in 0..2 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
        }
    }
    found.then_some([
        (minimum[0] + maximum[0]) * 0.5,
        (minimum[1] + maximum[1]) * 0.5,
    ])
}

fn rotate_point(point: Point2, pivot: Point2, angle: f64) -> Point2 {
    let (sin, cos) = angle.sin_cos();
    let x = point[0] - pivot[0];
    let y = point[1] - pivot[1];
    [pivot[0] + x * cos - y * sin, pivot[1] + x * sin + y * cos]
}

fn vertex_count(entity: &SketchEntity) -> usize {
    match entity {
        SketchEntity::Line { .. } | SketchEntity::Arc { .. } => 2,
        SketchEntity::Rect { .. } | SketchEntity::RotatedRect { .. } => 4,
        SketchEntity::Polygon { n, .. } => (*n).max(3) as usize + 1,
        SketchEntity::Circle { .. } | SketchEntity::Path { .. } => 0,
    }
}

fn first_placed_point(entity: &SketchEntity) -> usize {
    match entity {
        SketchEntity::Rect { c1, c2 } => match (c1[0] > c2[0], c1[1] > c2[1]) {
            (false, false) => 0,
            (true, false) => 1,
            (true, true) => 2,
            (false, true) => 3,
        },
        _ => 0,
    }
}

fn vertex_point(entity: &SketchEntity, vertex: usize) -> Point2 {
    match entity {
        SketchEntity::Line { a, b } => [*a, *b][vertex],
        SketchEntity::Arc { c, r, a0, a1, .. } => {
            let angle = [*a0, *a1][vertex];
            [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
        }
        SketchEntity::Rect { c1, c2 } => {
            // Properties labels the rectangle starting at its low corner.
            let [x, y] = [c1[0].min(c2[0]), c1[1].min(c2[1])];
            let [u, v] = [c1[0].max(c2[0]), c1[1].max(c2[1])];
            [[x, y], [u, y], [u, v], [x, v]][vertex]
        }
        SketchEntity::Polygon { c, r, n, rot } => {
            if vertex == 0 {
                return *c;
            }
            let angle = rot + std::f64::consts::TAU * (vertex - 1) as f64 / (*n).max(3) as f64;
            [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
        }
        SketchEntity::RotatedRect { .. } => entity.to_chain().0[vertex],
        _ => unreachable!("entity has no vertices"),
    }
}

fn rotate_entity(entity: &SketchEntity, pivot: Point2, angle: f64) -> SketchEntity {
    match entity {
        SketchEntity::Path { .. } => {
            let mut rotated = entity.clone();
            if let SketchEntity::Path { origin, rot, .. } = &mut rotated {
                *origin = rotate_point(*origin, pivot, angle);
                *rot += angle;
            }
            rotated
        }
        SketchEntity::Line { a, b } => SketchEntity::Line {
            a: rotate_point(*a, pivot, angle),
            b: rotate_point(*b, pivot, angle),
        },
        SketchEntity::Circle { c, r } => SketchEntity::Circle {
            c: rotate_point(*c, pivot, angle),
            r: *r,
        },
        SketchEntity::Arc { c, r, a0, a1, ccw } => SketchEntity::Arc {
            c: rotate_point(*c, pivot, angle),
            r: *r,
            a0: *a0 + angle,
            a1: *a1 + angle,
            ccw: *ccw,
        },
        SketchEntity::Polygon { c, r, n, rot } => SketchEntity::Polygon {
            c: rotate_point(*c, pivot, angle),
            r: *r,
            n: *n,
            rot: *rot + angle,
        },
        SketchEntity::Rect { c1, c2 } => {
            let center = [(c1[0] + c2[0]) * 0.5, (c1[1] + c2[1]) * 0.5];
            SketchEntity::RotatedRect {
                c: rotate_point(center, pivot, angle),
                width: (c2[0] - c1[0]).abs(),
                height: (c2[1] - c1[1]).abs(),
                rot: angle,
            }
        }
        SketchEntity::RotatedRect {
            c,
            width,
            height,
            rot,
        } => SketchEntity::RotatedRect {
            c: rotate_point(*c, pivot, angle),
            width: *width,
            height: *height,
            rot: *rot + angle,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freehand_commits_one_object_per_stroke_and_can_be_dragged_repeatedly() {
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Freehand);
        s.begin_freehand([0.0, 0.0], 0.05);
        for p in [[10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]] {
            s.pointer_move_with_snap(p, true, true, 1.0, 1.0);
        }
        assert!(s.entities.is_empty(), "Do not commit during the stroke");
        assert!(s.pointer_up([0.0; 2]));
        assert_eq!(s.entities.len(), 1);
        assert_eq!(s.regions().len(), 1);
        assert!(s.entities[0].to_chains()[0].1);
        assert!(s.selected_entities().is_empty());
        assert!(!s.begin_entity_drag([5.0, 0.0], 0.2, false));
        s.set_tool(SketchTool::Pick);
        for x in [5.0, 8.0] {
            assert!(s.begin_entity_drag([x, 0.0], 0.2, false));
            s.pointer_move([x + 3.0, 0.0]);
            assert!(s.pointer_up([0.0; 2]));
            assert_eq!(s.entities.len(), 1);
        }
        s.set_tool(SketchTool::Freehand);
        s.begin_freehand([20.0, 20.0], 0.05);
        s.pointer_move_with_snap([25.0, 22.0], false, false, 1.0, 1.0);
        s.pointer_up([0.0; 2]);
        assert_eq!(s.entities.len(), 2);
        assert!(!s.entities[1].to_chains()[0].1);
        assert_eq!(s.regions().len(), 1);
        s.begin_freehand([50.0, 50.0], 0.05);
        s.set_tool(SketchTool::Pick);
        assert!(!s.pointer_up([0.0; 2]));
        assert_eq!(s.entities.len(), 2);
    }

    #[test]
    fn text_transform_resize_and_oblique_coordinate_edit_preserve_identity() {
        let text = crate::sketch_text::outline_text("OB", "Noto Sans SC", 10.0, [0.0; 2]).unwrap();
        let mut s = SketchSession::new_with_coordinate_display(
            PlaneDef::from_normal([1.0, 2.0, 3.0]),
            vec![text.clone()],
            true,
        );
        let moved = move_entity(&text, [4.0, 6.0]).unwrap();
        let rotated = rotate_entity(&moved, [4.0, 6.0], 0.5);
        assert_eq!(rotated.to_chains().len(), 5);
        let (handle, kind) = resize_handles(&rotated)[0];
        let target = [4.0 + (handle[0] - 4.0) * 2.0, 6.0 + (handle[1] - 6.0) * 2.0];
        let resized = resize_entity(&rotated, kind, target).unwrap();
        assert!(matches!(resized,SketchEntity::Path { scale,.. } if (scale-20.0).abs()<1e-8));
        s.apply_entity_edit(0, &HashMap::from([("center_x", 5.0), ("h", 20.0)]))
            .unwrap();
        assert_eq!(s.entities.len(), 1);
        assert_eq!(s.entities[0].kind(), "text");
        assert_eq!(s.entities[0].to_chains().len(), 5);
        assert_ne!(
            s33d_model::entities_fingerprint(&[text]),
            s33d_model::entities_fingerprint(&s.entities)
        );
    }

    #[test]
    fn snap_to_endpoint() {
        let mut s = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0],
            }],
        );
        let p = s.snap([0.4, 0.3], true, false, 1.0, 1.0);
        assert!((p[0]).abs() < 1e-9 && (p[1]).abs() < 1e-9);
    }

    #[test]
    fn geometry_and_grid_snap_are_independent() {
        let mut s = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [2.0, 2.0],
                b: [8.0, 2.0],
            }],
        );
        let unchanged = s.snap([2.35, 2.2], false, false, 1.0, 1.0);
        assert_eq!(unchanged, [2.35, 2.2]);
        assert_eq!(s.snap_kind, None);

        let grid = s.snap([2.35, 2.2], false, true, 1.0, 1.0);
        assert_eq!(grid, [2.0, 2.0]);
        assert_eq!(s.snap_kind, Some(SnapKind::Grid));
    }

    #[test]
    fn rectangle_key_points_include_center_and_edge_midpoints() {
        let s = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Rect {
                c1: [0.0, 0.0],
                c2: [10.0, 6.0],
            }],
        );
        let points = s.key_points();
        assert!(points.contains(&[5.0, 3.0]));
        assert!(points.contains(&[5.0, 0.0]));
        assert!(points.contains(&[10.0, 3.0]));
        assert_eq!(points.len(), 9);
    }

    #[test]
    fn draw_line_and_dimension() {
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Line);
        s.pointer_down([0.0, 0.0], 1.0);
        s.hover = Some([1.0, 0.0]);
        assert!(s.on_dimension("5"));
        assert_eq!(s.entities.len(), 1);
        if let SketchEntity::Line { a, b } = s.entities[0] {
            assert_eq!(a, [0.0, 0.0]);
            assert!((b[0] - 5.0).abs() < 1e-9);
        }
    }

    #[test]
    fn sketch_live_dimensions_and_coordinates_follow_display_units() {
        let mut session = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), Vec::new());
        session.set_tool(SketchTool::Line);
        session.pointer_down([0.0, 0.0], 1.0);
        session.hover = Some([25.4, 0.0]);
        let readout = session
            .cursor_readout_in(1.0 / 25.4, "in", 4)
            .expect("active line should have a readout");
        assert!(readout.contains("L 1.0000 in"));
        assert!(readout.contains("1.0000"));
        assert!(readout.contains("in"));
    }

    #[test]
    fn click_click_draws_line() {
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Line);
        assert!(!s.pointer_down([0.0, 0.0], 1.0)); // 起点：无实体
        assert!(s.pointer_down([10.0, 0.0], 1.0)); // 第二点：生成 line
        assert_eq!(s.entities.len(), 1);
        assert_eq!(
            s.entities[0],
            SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0]
            }
        );
        // 新实体自动选中（进入参数编辑）
        assert_eq!(s.selected_entity, Some(0));
        // 链式继续：第三次点击生成第二条线
        assert!(s.pointer_down([10.0, 5.0], 1.0));
        assert_eq!(s.entities.len(), 2);
        assert_eq!(
            s.entities[1],
            SketchEntity::Line {
                a: [10.0, 0.0],
                b: [10.0, 5.0]
            }
        );
    }

    #[test]
    fn click_click_closes_chain() {
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Line);
        s.pointer_down([0.0, 0.0], 1.0);
        s.pointer_down([10.0, 0.0], 1.0);
        s.pointer_down([10.0, 5.0], 1.0);
        // 点击链起点：闭合，生成第三条线 + finish_chain
        assert!(s.pointer_down([0.0, 0.0], 1.0));
        assert_eq!(s.entities.len(), 3);
        assert_eq!(
            s.entities[2],
            SketchEntity::Line {
                a: [10.0, 5.0],
                b: [0.0, 0.0]
            }
        );
        assert!(s.pending.is_empty());
        assert!(s.chain_start.is_none());
        // 闭合的矩形轮廓 → 1 个区域
        assert_eq!(s.regions().len(), 1);
    }

    #[test]
    fn click_click_draws_circle_rect_polygon() {
        // circle
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Circle);
        s.pointer_down([2.0, 2.0], 1.0);
        assert!(s.pointer_down([2.0, 5.0], 1.0));
        assert_eq!(s.entities.len(), 1);
        if let SketchEntity::Circle { c, r } = s.entities[0] {
            assert_eq!(c, [2.0, 2.0]);
            assert!((r - 3.0).abs() < 1e-9);
        } else {
            panic!("not circle");
        }
        // rect
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Rect);
        s.pointer_down([0.0, 0.0], 1.0);
        assert!(s.pointer_down([10.0, 5.0], 1.0));
        assert_eq!(
            s.entities[0],
            SketchEntity::Rect {
                c1: [0.0, 0.0],
                c2: [10.0, 5.0]
            }
        );
        // polygon（6 边）
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.set_tool(SketchTool::Polygon);
        s.poly_sides = 6;
        s.pointer_down([0.0, 0.0], 1.0);
        assert!(s.pointer_down([10.0, 0.0], 1.0));
        if let SketchEntity::Polygon { c, r, n, .. } = s.entities[0] {
            assert_eq!(c, [0.0, 0.0]);
            assert!((r - 10.0).abs() < 1e-9);
            assert_eq!(n, 6);
        } else {
            panic!("not polygon");
        }
    }

    #[test]
    fn edit_single_field_keeps_others() {
        // 回归：只改一个参数不得清零其它参数
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.entities.push(SketchEntity::Circle {
            c: [5.0, 5.0],
            r: 10.0,
        });
        s.selected_entity = Some(0);
        let mut vals: HashMap<&str, f64> = Default::default();
        vals.insert("r", 20.0);
        s.apply_entity_edit(0, &vals).unwrap();
        if let SketchEntity::Circle { c, r } = s.entities[0] {
            assert_eq!(c, [5.0, 5.0], "center must be preserved");
            assert!((r - 20.0).abs() < 1e-9);
        } else {
            panic!("not circle");
        }
        // rect 只改长度
        s.entities = vec![SketchEntity::Rect {
            c1: [1.0, 2.0],
            c2: [6.0, 8.0],
        }];
        s.selected_entity = Some(0);
        let mut vals: HashMap<&str, f64> = Default::default();
        vals.insert("w", 10.0);
        s.apply_entity_edit(0, &vals).unwrap();
        if let SketchEntity::Rect { c1, c2 } = s.entities[0] {
            assert_eq!(c1, [1.0, 2.0], "c1 must be preserved");
            assert!((c2[0] - 11.0).abs() < 1e-9, "c2={c2:?}");
            assert!((c2[1] - 8.0).abs() < 1e-9, "c2={c2:?}");
        } else {
            panic!("not rect");
        }
    }
    #[test]
    fn regions_from_rect() {
        let mut s = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        s.entities.push(SketchEntity::Rect {
            c1: [0.0, 0.0],
            c2: [10.0, 5.0],
        });
        let regions = s.regions();
        assert_eq!(regions.len(), 1);
        assert!((regions[0].signed_area() - 50.0).abs() < 1e-6);
    }

    #[test]
    fn sketch_align_uses_locked_reference_and_selected_axis() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![
                SketchEntity::Line {
                    a: [0.0, 0.0],
                    b: [10.0, 0.0],
                },
                SketchEntity::Rect {
                    c1: [20.0, 4.0],
                    c2: [24.0, 8.0],
                },
            ],
        );
        session.set_tool(SketchTool::Align);
        session.align_selection = vec![0, 1];
        session.align_reference = Some(0);
        session.align_axis = 1;
        session.align_mode = AlignMode::Min;
        assert_eq!(session.apply_align().unwrap(), 1);
        assert_eq!(
            session.entities[1],
            SketchEntity::Rect {
                c1: [20.0, 0.0],
                c2: [24.0, 4.0]
            }
        );
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0]
            }
        );
    }

    #[test]
    fn sketch_move_transfers_pick_multi_selection_previews_then_applies() {
        let original = vec![
            SketchEntity::Line {
                a: [0.0, 0.0],
                b: [2.0, 0.0],
            },
            SketchEntity::Circle {
                c: [5.0, 3.0],
                r: 1.0,
            },
        ];
        let mut session =
            SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), original.clone());
        session.pick_selection = vec![0, 1];
        session.selected_entity = Some(1);
        session.set_tool(SketchTool::Move);
        assert_eq!(session.transform_selection, vec![0, 1]);
        session.set_move_delta([4.0, -2.0]);
        assert_eq!(session.transform_preview_chains().len(), 2);
        assert_eq!(
            session.entities, original,
            "preview must not mutate geometry"
        );
        assert_eq!(session.apply_transform().unwrap(), 2);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [4.0, -2.0],
                b: [6.0, -2.0]
            }
        );
        assert_eq!(
            session.entities[1],
            SketchEntity::Circle {
                c: [9.0, 1.0],
                r: 1.0
            }
        );
    }

    #[test]
    fn sketch_rotate_uses_group_center_and_preserves_selection() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![
                SketchEntity::Line {
                    a: [-2.0, 0.0],
                    b: [0.0, 0.0],
                },
                SketchEntity::Line {
                    a: [0.0, 0.0],
                    b: [2.0, 0.0],
                },
            ],
        );
        session.set_locked_vertex(0, None);
        session.set_locked_vertex(1, None);
        session.pick_selection = vec![0, 1];
        session.set_tool(SketchTool::Rotate);
        session.set_rotate_angle(90.0);
        assert_eq!(session.apply_transform().unwrap(), 2);
        let expected = [
            SketchEntity::Line {
                a: [0.0, 2.0],
                b: [0.0, 0.0],
            },
            SketchEntity::Line {
                a: [0.0, 0.0],
                b: [0.0, -2.0],
            },
        ];
        for (actual, expected) in session.entities.iter().zip(expected) {
            let (actual_points, _) = actual.to_chain();
            let (expected_points, _) = expected.to_chain();
            for (actual, expected) in actual_points.iter().zip(expected_points) {
                assert!(dist(*actual, expected) < 1e-9);
            }
        }
        session.set_tool(SketchTool::Pick);
        assert_eq!(session.selected_entities(), vec![0, 1]);
    }

    #[test]
    fn sketch_rotate_custom_pivot_uses_plane_axis_point() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [1.0, 0.0],
                b: [2.0, 0.0],
            }],
        );
        session.set_locked_vertex(0, None);
        session.pick_selection = vec![0];
        session.set_tool(SketchTool::Rotate);
        session.set_rotate_custom_pivot_enabled(true);
        session.set_rotate_pivot([0.0, 0.0]);
        session.set_rotate_angle(90.0);
        session.apply_transform().unwrap();
        let SketchEntity::Line { a, b } = session.entities[0] else {
            panic!("rotation must preserve a line");
        };
        assert!(dist(a, [0.0, -1.0]) < 1e-9, "a={a:?}");
        assert!(dist(b, [0.0, -2.0]) < 1e-9, "b={b:?}");
        assert_eq!(session.rotate_pivot, [0.0, 0.0]);
    }

    #[test]
    fn sketch_transform_shortcuts_apply_immediately_and_keep_numeric_preview() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [0.0, 0.0],
                b: [1.0, 0.0],
            }],
        );
        session.pick_selection = vec![0];
        session.set_tool(SketchTool::Move);
        session.set_move_delta([7.0, 8.0]);
        session.apply_quick_move([1.0, 0.0]).unwrap();
        assert_eq!(session.move_delta, [7.0, 8.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [1.0, 0.0],
                b: [2.0, 0.0]
            }
        );

        session.set_locked_vertex(0, None);
        session.set_tool(SketchTool::Rotate);
        session.set_rotate_custom_pivot_enabled(true);
        session.set_rotate_pivot([0.0, 0.0]);
        session.set_rotate_angle(12.0);
        session.apply_quick_rotate(90.0).unwrap();
        assert!((session.rotate_angle_deg - 12.0).abs() < 1e-9);
        let SketchEntity::Line { a, b } = session.entities[0] else {
            panic!("rotation must preserve a line");
        };
        assert!(dist(a, [0.0, -1.0]) < 1e-9, "a={a:?}");
        assert!(dist(b, [0.0, -2.0]) < 1e-9, "b={b:?}");
    }

    #[test]
    fn vertex_locks_follow_shape_edits_rotations_and_moves() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0., 0., 1.]),
            vec![
                SketchEntity::Rect {
                    c1: [9., 8.],
                    c2: [3., 2.],
                },
                SketchEntity::Polygon {
                    c: [5., 7.],
                    r: 3.,
                    n: 6,
                    rot: 0.,
                },
                SketchEntity::Line {
                    a: [2., 1.],
                    b: [4., 1.],
                },
                SketchEntity::Arc {
                    c: [3., 6.],
                    r: 2.,
                    a0: 0.,
                    a1: 1.,
                    ccw: true,
                },
            ],
        );
        assert_eq!(session.locked_vertex_point(0), Some([9., 8.]));
        assert_eq!(session.locked_vertex_point(1), Some([5., 7.]));
        assert_eq!(session.locked_vertex_point(2), Some([2., 1.]));
        assert_eq!(session.locked_vertex_point(3), Some([5., 6.]));
        session.set_locked_vertex(1, Some(6));
        session.set_locked_vertex(2, Some(1));
        let anchors: Vec<_> = (0..4)
            .map(|i| session.locked_vertex_point(i).unwrap())
            .collect();
        session
            .apply_entity_edit(0, &HashMap::from([("w", 10.)]))
            .unwrap();
        session
            .apply_entity_edit(1, &HashMap::from([("n", 4.), ("s", 5.)]))
            .unwrap();
        session.update_line_entity(2, [2., 1.], [2., 5.]);
        session
            .apply_entity_edit(3, &HashMap::from([("r", 3.), ("a0", 25.)]))
            .unwrap();
        assert_eq!(session.locked_vertex(1), Some(4));
        for (index, anchor) in anchors.iter().enumerate() {
            assert!(dist(session.locked_vertex_point(index).unwrap(), *anchor) < 1e-9);
        }
        session.set_tool(SketchTool::Rotate);
        session.transform_selection = vec![0, 1, 2, 3];
        session.set_rotate_angle(42.);
        assert_eq!(session.transform_preview_chains().len(), 4);
        session.apply_transform().unwrap();
        session.set_tool(SketchTool::Move);
        for (index, anchor) in anchors.iter().enumerate() {
            assert!(dist(session.locked_vertex_point(index).unwrap(), *anchor) < 1e-9);
        }
        session.set_move_delta([2., -3.]);
        session.apply_transform().unwrap();
        for (index, [x, y]) in anchors.into_iter().enumerate() {
            assert!(
                dist(
                    session.locked_vertex_point(index).unwrap(),
                    [x + 2., y - 3.]
                ) < 1e-9
            );
        }
    }

    #[test]
    fn arbitrary_rectangle_rotation_keeps_one_rect_with_the_same_object_index() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Rect {
                c1: [-2.0, -1.0],
                c2: [2.0, 1.0],
            }],
        );
        session.pick_selection = vec![0];
        session.set_tool(SketchTool::Rotate);
        session.set_rotate_angle(30.0);
        session.apply_transform().unwrap();
        assert_eq!(session.entities.len(), 1);
        assert_eq!(session.entities[0].kind(), "rect");
        assert_eq!(session.transform_selection, vec![0]);
        assert!(matches!(
            session.entities[0],
            SketchEntity::RotatedRect { rot, .. }
                if (rot.to_degrees() + 30.0).abs() < 1e-9
        ));

        // A second operation targets the same list slot/object id.
        session.set_rotate_angle(15.0);
        session.apply_transform().unwrap();
        assert_eq!(session.entities.len(), 1);
        assert_eq!(session.transform_selection, vec![0]);
        assert!(matches!(
            session.entities[0],
            SketchEntity::RotatedRect { rot, .. }
                if (rot.to_degrees() + 45.0).abs() < 1e-9
        ));
    }

    #[test]
    fn datum_sketch_axes_are_presented_in_world_xyz_order_and_direction() {
        let front =
            SketchSession::new(s33d_kernel::DatumPlane::Front.def(), vec![]).world_axis_bindings();
        assert_eq!(front.map(|axis| axis.label()), ["X", "Y"]);
        assert_eq!(front.map(|axis| axis.local_axis), [0, 1]);
        assert_eq!(front.map(|axis| axis.sign), [1.0, 1.0]);

        let right =
            SketchSession::new(s33d_kernel::DatumPlane::Right.def(), vec![]).world_axis_bindings();
        assert_eq!(right.map(|axis| axis.label()), ["Y", "Z"]);
        assert_eq!(right.map(|axis| axis.local_axis), [1, 0]);
        assert_eq!(right.map(|axis| axis.sign), [1.0, -1.0]);

        let top =
            SketchSession::new(s33d_kernel::DatumPlane::Top.def(), vec![]).world_axis_bindings();
        assert_eq!(top.map(|axis| axis.label()), ["X", "Z"]);
        assert_eq!(top.map(|axis| axis.local_axis), [0, 1]);
        assert_eq!(top.map(|axis| axis.sign), [1.0, -1.0]);
    }

    #[test]
    fn sketch_cursor_readout_never_exposes_internal_uv_axes() {
        let mut right = SketchSession::new(s33d_kernel::DatumPlane::Right.def(), vec![]);
        right.hover = Some([1.0, 2.0]);
        assert_eq!(
            right.cursor_coordinate_label().as_deref(),
            Some("Y 2.0000   Z -1.0000")
        );

        let mut arbitrary = SketchSession::new(PlaneDef::from_normal([1.0, 1.0, 1.0]), vec![]);
        arbitrary.hover = Some([3.0, 4.0]);
        let label = arbitrary.cursor_coordinate_label().unwrap();
        assert!(!label.contains('U') && !label.contains('V'), "{label}");
    }

    #[test]
    fn equation_sketch_displays_all_world_coordinates_and_axes() {
        let plane = PlaneDef::from_normal([1.0, 1.0, 1.0]);
        let mut session = SketchSession::new_with_coordinate_display(plane, Vec::new(), true);
        session.hover = Some([3.0, 4.0]);
        let label = session.cursor_coordinate_label().unwrap();
        assert!(label.contains('X') && label.contains('Y') && label.contains('Z'));
        assert_eq!(
            session
                .displayed_world_axes()
                .iter()
                .map(|axis| axis.label())
                .collect::<Vec<_>>(),
            vec!["X", "Y", "Z"]
        );
    }

    #[test]
    fn equation_sketch_world_property_edit_stays_on_plane() {
        let plane = PlaneDef::from_normal([1.0, 2.0, 3.0]);
        let mut session = SketchSession::new_with_coordinate_display(
            plane,
            vec![SketchEntity::Circle {
                c: [2.0, -1.0],
                r: 4.0,
            }],
            true,
        );
        let params = session.entity_params(0).unwrap();
        assert!(params.iter().any(|(_, label, _, _)| label == "Center X"));
        assert!(params.iter().any(|(_, label, _, _)| label == "Center Y"));
        assert!(params.iter().any(|(_, label, _, _)| label == "Center Z"));

        let mut values = HashMap::new();
        values.insert("center_x", 9.0);
        session.apply_entity_edit(0, &values).unwrap();
        let SketchEntity::Circle { c, .. } = session.entities[0] else {
            panic!("circle expected");
        };
        let world = plane.to_world(c[0], c[1]);
        let offset = glam::DVec3::from_array(world) - plane.origin();
        assert!(offset.dot(plane.normal()).abs() < 1e-9);
    }

    #[test]
    fn sketch_align_min_max_follow_world_axis_when_local_axis_is_reversed() {
        let mut session = SketchSession::new(
            s33d_kernel::DatumPlane::Right.def(),
            vec![
                SketchEntity::Rect {
                    c1: [-10.0, 0.0],
                    c2: [-6.0, 4.0],
                },
                SketchEntity::Rect {
                    c1: [0.0, 8.0],
                    c2: [2.0, 12.0],
                },
            ],
        );
        session.set_tool(SketchTool::Align);
        session.align_selection = vec![0, 1];
        session.align_reference = Some(0);
        let z_axis = session
            .world_axis_bindings()
            .into_iter()
            .find(|axis| axis.label() == "Z")
            .unwrap();
        assert_eq!(z_axis.local_axis, 0);
        assert_eq!(z_axis.sign, -1.0);
        session.align_axis = z_axis.world_axis;
        session.align_mode = AlignMode::Max;
        assert_eq!(session.apply_align().unwrap(), 1);
        assert_eq!(
            session.entities[1],
            SketchEntity::Rect {
                c1: [-10.0, 8.0],
                c2: [-8.0, 12.0]
            }
        );
    }

    #[test]
    fn pick_ctrl_multi_selection_is_ordered_and_latest_entity_expands() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![
                SketchEntity::Line {
                    a: [0.0, 0.0],
                    b: [10.0, 0.0],
                },
                SketchEntity::Line {
                    a: [0.0, 10.0],
                    b: [10.0, 10.0],
                },
                SketchEntity::Line {
                    a: [0.0, 20.0],
                    b: [10.0, 20.0],
                },
            ],
        );
        session.pointer_down_with_additive([5.0, 0.0], 1.0, false);
        assert_eq!(session.selected_entities(), vec![0]);
        assert_eq!(session.selected_entity, Some(0));

        session.pointer_down_with_additive([5.0, 10.0], 1.0, true);
        assert_eq!(session.selected_entities(), vec![0, 1]);
        assert_eq!(session.selected_entity, Some(1));

        // Opening the first card collapses the second without changing either
        // entity's selected state; clicking it again folds all cards.
        assert!(session.toggle_entity_expanded(0));
        assert_eq!(session.selected_entity, Some(0));
        assert_eq!(session.selected_entities(), vec![0, 1]);
        assert!(session.toggle_entity_expanded(0));
        assert_eq!(session.selected_entity, None);
        assert_eq!(session.selected_entities(), vec![0, 1]);

        session.pointer_down_with_additive([5.0, 20.0], 1.0, true);
        assert_eq!(session.selected_entities(), vec![0, 1, 2]);
        assert_eq!(session.selected_entity, Some(2));
        session.pointer_down_with_additive([5.0, 10.0], 1.0, true);
        assert_eq!(session.selected_entities(), vec![0, 2]);
        assert_eq!(session.selected_entity, Some(2));
    }

    #[test]
    fn pick_multi_selection_roundtrips_through_align_without_losing_entities() {
        let entities = (0..4)
            .map(|index| SketchEntity::Circle {
                c: [index as f64 * 10.0, 0.0],
                r: 2.0,
            })
            .collect();
        let mut session = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), entities);
        session.select_entity(0, false);
        session.select_entity(2, true);
        session.select_entity(3, true);
        assert_eq!(session.selected_entities(), vec![0, 2, 3]);
        assert_eq!(session.selected_entity, Some(3));

        session.set_tool(SketchTool::Align);
        assert_eq!(session.align_selection, vec![0, 2, 3]);
        assert_eq!(session.align_reference, Some(3));
        assert!(session.selected_entities().is_empty());

        session.set_align_reference(Some(0));
        session.set_tool(SketchTool::Pick);
        assert_eq!(session.selected_entities(), vec![0, 2, 3]);
        assert_eq!(session.selected_entity, Some(3));
        assert!(session.align_selection.is_empty());
        assert_eq!(session.align_reference, None);
    }

    #[test]
    fn deleting_one_pick_card_remaps_remaining_selection_indices() {
        let entities = (0..4)
            .map(|index| SketchEntity::Circle {
                c: [index as f64 * 10.0, 0.0],
                r: 2.0,
            })
            .collect();
        let mut session = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), entities);
        session.select_entity(0, false);
        session.select_entity(2, true);
        session.select_entity(3, true);
        assert_eq!(session.selected_entities(), vec![0, 2, 3]);
        assert!(session.delete_entity(1));
        assert_eq!(session.entities.len(), 3);
        assert_eq!(session.selected_entities(), vec![0, 1, 2]);
        assert_eq!(session.selected_entity, Some(2));

        assert!(session.delete_selected());
        assert!(session.entities.is_empty());
        assert!(session.selected_entities().is_empty());
        assert_eq!(session.selected_entity, None);
    }

    #[test]
    fn dragging_entity_snaps_any_moving_keypoint_to_stationary_keypoint() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![
                SketchEntity::Line {
                    a: [0.0, 0.0],
                    b: [10.0, 0.0],
                },
                SketchEntity::Line {
                    a: [20.0, 0.0],
                    b: [30.0, 0.0],
                },
            ],
        );
        assert!(!session.pointer_down([5.0, 0.0], 1.0));
        session.pointer_move_with_snap([14.6, 0.0], true, false, 1.0, 1.0);
        assert_eq!(session.snap_kind, Some(SnapKind::Endpoint));
        session.pointer_up([14.6, 0.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [10.0, 0.0],
                b: [20.0, 0.0]
            }
        );
    }

    #[test]
    fn dragging_line_endpoint_resizes_without_moving_the_other_endpoint() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0],
            }],
        );
        session.pointer_down([0.0, 0.0], 1.0);
        session.pointer_move([0.0, 5.0]);
        assert_eq!(session.active_drag_entity(), Some(0));
        session.pointer_up([0.0, 5.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [0.0, 5.0],
                b: [10.0, 0.0]
            }
        );
    }

    #[test]
    fn dragging_shape_keypoints_updates_parameters_without_changing_identity() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![
                SketchEntity::Circle {
                    c: [0.0, 0.0],
                    r: 5.0,
                },
                SketchEntity::Rect {
                    c1: [20.0, 0.0],
                    c2: [30.0, 5.0],
                },
            ],
        );

        session.pointer_down([5.0, 0.0], 1.0);
        session.pointer_move([8.0, 0.0]);
        session.pointer_up([8.0, 0.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Circle {
                c: [0.0, 0.0],
                r: 8.0
            }
        );

        session.pointer_down([30.0, 5.0], 1.0);
        session.pointer_move([34.0, 9.0]);
        session.pointer_up([34.0, 9.0]);
        assert_eq!(
            session.entities[1],
            SketchEntity::Rect {
                c1: [20.0, 0.0],
                c2: [34.0, 9.0]
            }
        );
        assert_eq!(session.entities[1].kind(), "rect");
    }

    #[test]
    fn center_keypoint_moves_shape_and_resize_keypoint_reports_hover_hint() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Circle {
                c: [0.0, 0.0],
                r: 5.0,
            }],
        );
        session.pointer_move_with_snap([5.2, 0.0], false, false, 1.0, 1.0);
        assert!(session.hovering_resize_handle());

        session.pointer_down([0.0, 0.0], 1.0);
        session.pointer_move([3.0, 4.0]);
        session.pointer_up([3.0, 4.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Circle {
                c: [3.0, 4.0],
                r: 5.0
            }
        );
    }

    #[test]
    fn same_entity_can_be_dragged_twice_without_reselecting_another_entity() {
        let mut session = SketchSession::new(
            PlaneDef::from_normal([0.0, 0.0, 1.0]),
            vec![SketchEntity::Line {
                a: [0.0, 0.0],
                b: [10.0, 0.0],
            }],
        );

        session.pointer_down([5.0, 0.0], 1.0);
        session.pointer_move([15.0, 0.0]);
        session.pointer_up([15.0, 0.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [10.0, 0.0],
                b: [20.0, 0.0]
            }
        );

        // The second press targets the already-selected and already-moved
        // entity directly; no intervening selection of another entity occurs.
        session.pointer_down([15.0, 0.0], 1.0);
        session.pointer_move([20.0, 5.0]);
        session.pointer_up([20.0, 5.0]);
        assert_eq!(
            session.entities[0],
            SketchEntity::Line {
                a: [15.0, 5.0],
                b: [25.0, 5.0]
            }
        );
        assert_eq!(session.selected_entity, Some(0));
    }

    #[test]
    fn newly_drawn_shape_moves_and_resizes_before_done_without_leaving_its_tool() {
        let mut session = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        session.set_tool(SketchTool::Rect);

        assert!(!session.pointer_down([0.0, 0.0], 1.0));
        assert!(session.pointer_down([10.0, 6.0], 1.0));
        assert_eq!(session.tool, SketchTool::Rect);

        // Dragging the center moves the complete rectangle while Rect remains
        // active, so another rectangle can be drawn immediately afterwards.
        assert!(session.begin_entity_drag([5.0, 3.0], 1.0, false));
        session.pointer_move([8.0, 7.0]);
        assert!(session.pointer_up([8.0, 7.0]));
        assert_eq!(
            session.entities[0],
            SketchEntity::Rect {
                c1: [3.0, 4.0],
                c2: [13.0, 10.0]
            }
        );
        assert_eq!(session.tool, SketchTool::Rect);

        // A corner drag uses the resize path and retains the same entity slot
        // and kind instead of recreating the object.
        session.pointer_move_with_snap([13.0, 10.0], false, false, 1.0, 1.0);
        assert!(session.hovering_resize_handle());
        assert!(session.begin_entity_drag([13.0, 10.0], 1.0, false));
        session.pointer_move([16.0, 12.0]);
        assert!(session.pointer_up([16.0, 12.0]));
        assert_eq!(
            session.entities[0],
            SketchEntity::Rect {
                c1: [3.0, 4.0],
                c2: [16.0, 12.0]
            }
        );
        assert_eq!(session.entities[0].kind(), "rect");
        assert_eq!(session.tool, SketchTool::Rect);
    }

    #[test]
    fn surface_reference_points_are_displayed_and_snappable() {
        let mut session = SketchSession::new(PlaneDef::from_normal([0.0, 0.0, 1.0]), vec![]);
        session.set_external_key_points(vec![[3.0, 4.0], [3.0, 4.0]]);
        assert_eq!(session.key_points(), vec![[3.0, 4.0]]);
        let snapped = session.snap([3.2, 4.1], true, false, 1.0, 1.0);
        assert_eq!(snapped, [3.0, 4.0]);
        assert_eq!(session.snap_kind, Some(SnapKind::Endpoint));
    }
}
