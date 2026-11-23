// SPDX-License-Identifier: MPL-2.0
//! Editable drawing annotations. Changes never alter measured source geometry.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Field {
    Title,
    Number,
    Revision,
    Material,
    Drawn,
    Checked,
    Date,
}
impl Field {
    fn value(self, d: &Drawing) -> &String {
        match self {
            Self::Title => &d.title,
            Self::Number => &d.drawing_number,
            Self::Revision => &d.revision,
            Self::Material => &d.material,
            Self::Drawn => &d.drawn_by,
            Self::Checked => &d.checked_by,
            Self::Date => &d.drawing_date,
        }
    }
    fn value_mut(self, d: &mut Drawing) -> &mut String {
        match self {
            Self::Title => &mut d.title,
            Self::Number => &mut d.drawing_number,
            Self::Revision => &mut d.revision,
            Self::Material => &mut d.material,
            Self::Drawn => &mut d.drawn_by,
            Self::Checked => &mut d.checked_by,
            Self::Date => &mut d.drawing_date,
        }
    }
}
const FIELDS: [Field; 7] = [
    Field::Title,
    Field::Number,
    Field::Revision,
    Field::Material,
    Field::Drawn,
    Field::Checked,
    Field::Date,
];
#[derive(Clone)]
pub(super) struct Snapshot {
    dimensions: Vec<Dimension>,
    texts: HashMap<TextKey, String>,
    offsets: HashMap<TextKey, Vec2>,
    fields: [String; 7],
}
impl Snapshot {
    pub fn capture(d: &Drawing) -> Self {
        Self {
            dimensions: d.dimensions.clone(),
            texts: d.text_edits.clone(),
            offsets: d.text_offsets.clone(),
            fields: FIELDS.map(|f| f.value(d).clone()),
        }
    }
    pub fn restore(self, d: &mut Drawing) {
        d.dimensions = self.dimensions;
        d.text_edits = self.texts;
        d.text_offsets = self.offsets;
        for (field, value) in FIELDS.into_iter().zip(self.fields) {
            *field.value_mut(d) = value;
        }
        d.editor = None;
        d.dimension_drag = None;
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) struct TextKey {
    page: usize,
    position: [u32; 2],
    original: String,
    pub dimension: Option<usize>,
}
impl TextKey {
    pub(super) fn new(s: &Sheet, page: usize, index: usize) -> Self {
        let (position, original, _) = &s.texts[index];
        let dimension = s.dimension_labels.get(&index).copied();
        Self {
            page,
            position: if dimension.is_some() {
                [0, 0]
            } else {
                [position.x.to_bits(), position.y.to_bits()]
            },
            original: s
                .bindings
                .get(&index)
                .map(|f| format!("field:{f:?}"))
                .unwrap_or_else(|| original.clone()),
            dimension,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Field(Field),
    Text(TextKey),
}
#[derive(Clone)]
pub(super) struct Editor {
    target: Target,
    draft: String,
    reveal: bool,
}
pub(super) struct AnnotationDrag {
    target: DragTarget,
    pointer: egui::Pos2,
}
enum DragTarget {
    Dimension { index: usize, label: Vec2 },
    Text { key: TextKey, offset: Vec2 },
}
pub(super) struct TextHit {
    pub index: usize,
    pub center: egui::Pos2,
    pub size: egui::Vec2,
    pub angle: f32,
}
impl TextHit {
    fn contains(&self, pos: egui::Pos2) -> bool {
        let v = pos - self.center;
        let local = egui::vec2(
            v.x * self.angle.cos() + v.y * self.angle.sin(),
            -v.x * self.angle.sin() + v.y * self.angle.cos(),
        );
        local.x.abs() <= self.size.x * 0.5 + 4. && local.y.abs() <= self.size.y * 0.5 + 4.
    }
}
impl Drawing {
    pub fn show_sheet(&self) -> bool {
        self.open && self.views.len() == 3
    }
    pub fn cancel_interaction(&mut self) -> bool {
        if !self.pending.is_empty() {
            self.pending.clear();
            return true;
        }
        self.dimension_drag = None;
        self.editor.take().is_some()
    }
}
pub(super) fn apply_overrides(d: &Drawing, s: &mut Sheet, page: usize) {
    // Compute keys before replacing text; rendering and export share this path.
    for index in 0..s.texts.len() {
        let key = TextKey::new(s, page, index);
        if let Some(offset) = d.text_offsets.get(&key) {
            s.texts[index].0 += *offset;
            if let Some(&line) = s.leaders.get(&index) {
                s.lines[line].0[1] += *offset;
            }
        }
        if !s.bindings.contains_key(&index) {
            if let Some(text) = d.text_edits.get(&key) {
                s.texts[index].1 = text.clone();
            }
        }
    }
}
pub(super) fn field_input(d: &mut Drawing, ui: &mut egui::Ui, field: Field) {
    let mut value = field.value(d).clone();
    if ui.text_edit_singleline(&mut value).changed() {
        d.checkpoint();
        *field.value_mut(d) = value;
        d.editor = None;
    }
}
pub(super) fn selection_summary(app: &S33DApp, ui: &mut egui::Ui) {
    let lang = app.settings.language;
    let bodies: Vec<_> = app
        .doc
        .bodies
        .iter()
        .filter(|b| {
            b.shape.is_some()
                && if app.selected.is_empty() {
                    b.visible
                } else {
                    app.selected.contains(&b.id)
                }
        })
        .collect();
    ui.label(format!(
        "{}: {}",
        lang.text("Objects selection", "对象列表选择"),
        bodies.len()
    ));
    if app.selected.is_empty() {
        ui.small(lang.text(
            "No selection: all visible objects.",
            "未选择：使用全部可见对象。",
        ));
    }
    if !bodies.is_empty() {
        ui.collapsing(lang.text("Selected objects", "所选对象"), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("drawing-source-names")
                .max_height(64.)
                .show(ui, |ui| {
                    for b in bodies {
                        ui.small(format!(
                            "{}{}",
                            b.name,
                            if b.visible {
                                ""
                            } else {
                                lang.text(" (hidden)", "（已隐藏）")
                            }
                        ));
                    }
                });
        });
    }
    if !app.drawing.show_sheet() {
        ui.small(lang.text(
            "The workspace stays in 3D until Generate finishes.",
            "生成成功前保留原三维工作区。",
        ));
    }
    if app.drawing.job.is_some() {
        ui.label(lang.text("Projecting…", "正在投影…"));
    }
}
fn select(d: &mut Drawing, s: &Sheet, index: usize) {
    let target = s
        .bindings
        .get(&index)
        .map(|f| Target::Field(*f))
        .unwrap_or_else(|| Target::Text(TextKey::new(s, d.page, index)));
    let draft = match &target {
        Target::Field(f) => f.value(d).clone(),
        Target::Text(key) => d
            .text_edits
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.original.clone()),
    };
    d.editor = Some(Editor {
        target,
        draft,
        reveal: true,
    });
}
fn commit(d: &mut Drawing) -> bool {
    let Some(editor) = d.editor.clone() else {
        return false;
    };
    let changed = match &editor.target {
        Target::Field(f) => f.value(d) != &editor.draft,
        Target::Text(key) => d.text_edits.get(key).unwrap_or(&key.original) != &editor.draft,
    };
    if !changed {
        return false;
    }
    d.checkpoint();
    match editor.target {
        Target::Field(f) => *f.value_mut(d) = editor.draft,
        Target::Text(key) => {
            if editor.draft == key.original {
                d.text_edits.remove(&key);
            } else {
                d.text_edits.insert(key, editor.draft);
            }
        }
    }
    true
}
pub(super) fn panel(d: &mut Drawing, ui: &mut egui::Ui, lang: crate::app::UiLanguage) {
    if d.views.len() != 3 {
        return;
    }
    if let Some(editor) = &mut d.editor {
        if editor.reveal {
            ui.scroll_to_cursor(Some(egui::Align::Min));
            editor.reveal = false;
        }
    }
    ui.separator();
    if ui
        .selectable_label(
            !d.manual,
            lang.text("Edit drawing text / dimensions", "编辑图中文字 / 尺寸"),
        )
        .clicked()
    {
        d.manual = false;
        d.pending.clear();
    }
    ui.small(lang.text("Click text to edit it below. Drag text or feature labels to reposition them; drag a dimension label to move its dimension line. Projected model lines are locked.","单击文字可在下方修改；拖动文字或特征编号可调整位置，拖动尺寸数字可移动尺寸线。模型投影线已锁定。"));
    let Some(editor) = d.editor.as_mut() else {
        return;
    };
    ui.label(lang.text("Selected text", "所选文字"));
    let response =
        ui.add(egui::TextEdit::singleline(&mut editor.draft).desired_width(f32::INFINITY));
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        d.editor = None;
        return;
    }
    if matches!(editor.target, Target::Text(_)) {
        ui.small(lang.text(
            "Text overrides are annotations; measured geometry is unchanged.",
            "文字替换仅修改标注，不改变几何测量值。",
        ));
    }
    let mut apply = enter;
    let mut reset = false;
    let mut close = false;
    ui.horizontal(|ui| {
        apply |= ui.button(lang.text("Apply text", "应用文字")).clicked();
        reset = ui.button(lang.text("Reset text", "还原文字")).clicked();
        close = ui.button(lang.text("Close", "关闭")).clicked();
    });
    if apply {
        commit(d);
    }
    if reset {
        if let Some(Editor {
            target: Target::Text(key),
            ..
        }) = d.editor.clone()
        {
            if d.text_edits.contains_key(&key) {
                d.checkpoint();
                d.text_edits.remove(&key);
            }
            d.editor = Some(Editor {
                draft: key.original.clone(),
                target: Target::Text(key),
                reveal: false,
            });
        } else if let Some(Editor {
            target: Target::Field(f),
            ..
        }) = &d.editor
        {
            let draft = f.value(d).clone();
            d.editor.as_mut().unwrap().draft = draft;
        }
    }
    let dimension = d.editor.as_ref().and_then(|e| match &e.target {
        Target::Text(k) => k.dimension,
        _ => None,
    });
    if let Some(index) = dimension {
        if ui
            .button(lang.text("Delete dimension", "删除此尺寸"))
            .clicked()
            && index < d.dimensions.len()
        {
            d.checkpoint();
            d.dimensions.remove(index);
            d.text_edits = std::mem::take(&mut d.text_edits)
                .into_iter()
                .filter_map(|(mut key, value)| {
                    if key.dimension == Some(index) {
                        return None;
                    }
                    if let Some(n) = key.dimension {
                        if n > index {
                            key.dimension = Some(n - 1);
                        }
                    }
                    Some((key, value))
                })
                .collect();
            d.editor = None;
        }
    }
    if close {
        d.editor = None;
    }
}
pub(super) fn canvas(
    d: &mut Drawing,
    ui: &egui::Ui,
    response: &egui::Response,
    s: &Sheet,
    hits: &[TextHit],
    origin: egui::Pos2,
    zoom: f32,
    unit: UiUnit,
    zh: bool,
) {
    // Raw strings keep override keys stable after a displayed value changes.
    let raw = raw_sheet_page(d, unit, zh, d.page);
    let pointer = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos());
    let hit_pointer = if response.drag_started() {
        ui.input(|i| i.pointer.press_origin())
    } else {
        pointer
    };
    let hit = hit_pointer.and_then(|p| hits.iter().rev().find(|h| h.contains(p)).map(|h| h.index));
    if let Some(index) = hit {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        if response.clicked() {
            select(d, &raw, index);
        }
        if response.drag_started() {
            if let Some(pointer) = hit_pointer {
                let target = if let Some(&index) = s.dimension_labels.get(&index) {
                    DragTarget::Dimension {
                        index,
                        label: d.dimensions[index].label,
                    }
                } else {
                    let key = TextKey::new(&raw, d.page, index);
                    let offset = d.text_offsets.get(&key).copied().unwrap_or_default();
                    DragTarget::Text { key, offset }
                };
                select(d, &raw, index);
                d.checkpoint();
                d.dimension_drag = Some(AnnotationDrag { target, pointer });
            }
        }
    } else if response.clicked() {
        if let Some(pointer) = pointer {
            let point = Vec2::new((pointer.x - origin.x) / zoom, (pointer.y - origin.y) / zoom);
            let field = s
                .cells
                .iter()
                .find(|(a, b, _)| point.cmpge(*a).all() && point.cmple(*b).all())
                .map(|(_, _, f)| *f);
            d.editor = field.map(|f| Editor {
                target: Target::Field(f),
                draft: f.value(d).clone(),
                reveal: true,
            });
        }
    }
    if let Some(drag) = &d.dimension_drag {
        if let Some(pointer) = pointer {
            let delta = pointer - drag.pointer;
            match &drag.target {
                DragTarget::Dimension { index, label } => {
                    d.dimensions[*index].label =
                        *label + Vec2::new(delta.x, -delta.y) / (zoom * fit(&d.views));
                }
                DragTarget::Text { key, offset } => {
                    d.text_offsets
                        .insert(key.clone(), *offset + Vec2::new(delta.x, delta.y) / zoom);
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }
    if response.drag_stopped() {
        d.dimension_drag = None;
    }
    if let Some(editor) = &d.editor {
        let selected = hits.iter().find(|h| match &editor.target {
            Target::Field(f) => raw.bindings.get(&h.index) == Some(f),
            Target::Text(key) => TextKey::new(&raw, d.page, h.index) == *key,
        });
        if let Some(hit) = selected {
            let size = egui::vec2(
                hit.size.x * hit.angle.cos().abs() + hit.size.y * hit.angle.sin().abs(),
                hit.size.x * hit.angle.sin().abs() + hit.size.y * hit.angle.cos().abs(),
            );
            ui.painter().rect_stroke(
                egui::Rect::from_center_size(hit.center, size + egui::vec2(6., 4.)),
                2.,
                Stroke::new(1., Color32::from_rgb(230, 130, 20)),
                egui::StrokeKind::Outside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Drawing {
        let mut d = Drawing {
            open: true,
            views: project(
                &[super::super::tests::box_mesh([20., 15., 10.])],
                &AtomicBool::new(false),
            )
            .unwrap(),
            ..Default::default()
        };
        auto_dimensions(&mut d);
        d
    }
    fn frame(ctx: &egui::Context, d: &mut Drawing, events: Vec<egui::Event>) -> (egui::Pos2, f32) {
        let mut transform = (egui::Pos2::ZERO, 1.);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let size = ui.available_size();
                    let zoom = (size.x / 307.).min(size.y / 220.);
                    transform = (
                        ui.max_rect().center() - egui::vec2(297., 210.) * zoom * 0.5,
                        zoom,
                    );
                    super::super::canvas(d, ui, UiUnit::Millimeter, false);
                });
            },
        );
        transform
    }
    fn click(ctx: &egui::Context, d: &mut Drawing, page: Vec2) {
        let (origin, zoom) = frame(ctx, d, vec![]);
        let pos = origin + egui::vec2(page.x, page.y) * zoom;
        frame(
            ctx,
            d,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            d,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    fn drag(ctx: &egui::Context, d: &mut Drawing, page: Vec2, delta: Vec2) {
        let (origin, zoom) = frame(ctx, d, vec![]);
        let pos = origin + egui::vec2(page.x, page.y) * zoom;
        frame(
            ctx,
            d,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let moved = pos + egui::vec2(delta.x, delta.y) * zoom;
        frame(ctx, d, vec![egui::Event::PointerMoved(moved)]);
        frame(
            ctx,
            d,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn drawing_text_drag_persists_across_edit_save_and_undo_with_projection_locked() {
        let ctx = egui::Context::default();
        let mut d = fixture();
        let before = serde_json::to_value(&d.views).unwrap();
        let raw = raw_sheet_page(&d, UiUnit::Millimeter, false, 0);
        let index = raw.texts.iter().position(|(_, t, _)| t == "FRONT").unwrap();
        let start = raw.texts[index].0;
        let delta = Vec2::new(8., 3.);
        drag(&ctx, &mut d, start, delta);
        let s = sheet_page(&d, UiUnit::Millimeter, false, 0);
        assert!(s.texts[index].0.distance(start + delta) < 1e-4);
        d.editor.as_mut().unwrap().draft = "Front inspection".into();
        assert!(commit(&mut d));
        let encoded = serde_json::to_value(&d).unwrap();
        let restored: Drawing = serde_json::from_value(encoded).unwrap();
        let s = sheet_page(&restored, UiUnit::Millimeter, false, 0);
        assert_eq!(s.texts[index].1, "Front inspection");
        assert!(s.texts[index].0.distance(start + delta) < 1e-4);
        d.undo(); // text
        d.undo(); // drag
        assert_eq!(
            sheet_page(&d, UiUnit::Millimeter, false, 0).texts[index].0,
            start
        );
        d.redo();
        assert!(
            sheet_page(&d, UiUnit::Millimeter, false, 0).texts[index]
                .0
                .distance(start + delta)
                < 1e-4
        );
        // Dragging a projected line in edit mode has no mutation route.
        let mid = (raw.lines[0].0[0] + raw.lines[0].0[1]) * 0.5;
        drag(&ctx, &mut d, mid, Vec2::splat(6.));
        assert_eq!(serde_json::to_value(&d.views).unwrap(), before);
        // Legacy drawings without offsets still deserialize normally.
        let mut legacy = serde_json::to_value(&d).unwrap();
        legacy.as_object_mut().unwrap().remove("text_offsets");
        assert!(serde_json::from_value::<Drawing>(legacy)
            .unwrap()
            .text_offsets
            .is_empty());
    }
    #[test]
    fn drawing_feature_label_drag_keeps_leader_anchor_and_title_edit_keeps_position() {
        let ctx = egui::Context::default();
        let mut d = fixture();
        d.components.push(Component {
            id: 1,
            cylinders: vec![crate::threads::Target {
                body_id: 1,
                face_id: 0,
                origin: [7., 5., 4.],
                axis: [0., 0., 1.],
                radius: 2.,
                length: 5.,
                internal: true,
            }],
            ..Default::default()
        });
        let raw = raw_sheet_page(&d, UiUnit::Millimeter, false, 0);
        let (&index, &line) = raw.leaders.iter().next().unwrap();
        let delta = Vec2::new(12., 5.);
        drag(&ctx, &mut d, raw.texts[index].0, delta);
        let s = sheet_page(&d, UiUnit::Millimeter, false, 0);
        assert_eq!(s.lines[line].0[0], raw.lines[line].0[0]);
        assert!(s.lines[line].0[1].distance(raw.lines[line].0[1] + delta) < 1e-4);
        let index = *raw
            .bindings
            .iter()
            .find(|(_, f)| **f == Field::Title)
            .unwrap()
            .0;
        drag(&ctx, &mut d, raw.texts[index].0, delta);
        d.editor.as_mut().unwrap().draft = "New title".into();
        assert!(commit(&mut d));
        let s = sheet_page(&d, UiUnit::Millimeter, false, 0);
        assert!(s.texts[index].1.contains("New title"));
        assert!(s.texts[index].0.distance(raw.texts[index].0 + delta) < 1e-4);
    }
    #[test]
    fn drawing_text_cells_and_schedule_edits_export_and_undo_without_changing_geometry() {
        let ctx = egui::Context::default();
        let mut d = fixture();
        let lines = d.views.iter().map(|v| v.lines.clone()).collect::<Vec<_>>();
        // Click blank space inside the title cell, not merely its label.
        click(&ctx, &mut d, Vec2::new(145., 185.));
        assert!(matches!(
            d.editor.as_ref().unwrap().target,
            Target::Field(Field::Title)
        ));
        d.editor.as_mut().unwrap().draft = "Drawing A & <B>".into();
        assert!(commit(&mut d));
        assert_eq!(d.title, "Drawing A & <B>");
        assert!(svg(&d, UiUnit::Millimeter, false).contains("Drawing A &amp; &lt;B&gt;"));
        d.undo();
        assert!(d.title.is_empty());
        d.redo();
        assert_eq!(d.title, "Drawing A & <B>");
        d.page = 1;
        let raw = raw_sheet_page(&d, UiUnit::Millimeter, false, 1);
        let i = raw
            .texts
            .iter()
            .position(|(_, t, _)| t.starts_with("FRONT"))
            .unwrap();
        click(&ctx, &mut d, raw.texts[i].0);
        assert!(matches!(d.editor.as_ref().unwrap().target, Target::Text(_)));
        d.editor.as_mut().unwrap().draft = "Inspection note".into();
        assert!(commit(&mut d));
        assert!(svg_page(&d, UiUnit::Millimeter, false, 1).contains("Inspection note"));
        assert!(!svg(&d, UiUnit::Millimeter, false).contains("Inspection note"));
        d.undo();
        assert!(!svg_page(&d, UiUnit::Millimeter, false, 1).contains("Inspection note"));
        d.redo();
        assert!(svg_page(&d, UiUnit::Millimeter, false, 1).contains("Inspection note"));
        assert_eq!(
            d.views.iter().map(|v| v.lines.clone()).collect::<Vec<_>>(),
            lines
        );
    }
    #[test]
    fn drawing_dimension_label_drags_and_edited_value_survives_undo_and_repositioning() {
        let ctx = egui::Context::default();
        let mut d = fixture();
        let raw = raw_sheet_page(&d, UiUnit::Millimeter, false, 0);
        let (&text_index, &dimension_index) = raw
            .dimension_labels
            .iter()
            .find(|(_, index)| **index == 0)
            .unwrap();
        click(&ctx, &mut d, raw.texts[text_index].0);
        d.editor.as_mut().unwrap().draft = "20 REF".into();
        assert!(commit(&mut d));
        let label = d.dimensions[dimension_index].label;
        let (origin, zoom) = frame(&ctx, &mut d, vec![]);
        let pos = origin + egui::vec2(raw.texts[text_index].0.x, raw.texts[text_index].0.y) * zoom;
        frame(
            &ctx,
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
        );
        let moved = pos + egui::vec2(15., 12.);
        frame(&ctx, &mut d, vec![egui::Event::PointerMoved(moved)]);
        frame(
            &ctx,
            &mut d,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(d.dimensions[dimension_index].label.distance(label) > 1.);
        assert!(svg(&d, UiUnit::Millimeter, false).contains("20 REF"));
        d.undo();
        assert_eq!(d.dimensions[dimension_index].label, label);
        assert!(svg(&d, UiUnit::Millimeter, false).contains("20 REF"));
        d.undo();
        assert!(!svg(&d, UiUnit::Millimeter, false).contains("20 REF"));
    }
}
