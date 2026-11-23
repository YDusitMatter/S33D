// SPDX-License-Identifier: MPL-2.0
use super::*;
pub(super) fn fields(d: &mut Drawing, ui: &mut egui::Ui, lang: crate::app::UiLanguage) {
    ui.collapsing(lang.text("Title block", "标题栏"), |ui| {
        for (en, zh, field) in [
            ("Drawing number", "图号", edit::Field::Number),
            ("Revision", "版本", edit::Field::Revision),
            ("Material", "材料", edit::Field::Material),
            ("Drawn by", "制图", edit::Field::Drawn),
            ("Checked by", "审核", edit::Field::Checked),
            ("Date", "日期", edit::Field::Date),
        ] {
            ui.label(lang.text(en, zh));
            edit::field_input(d, ui, field);
        }
    });
}

pub(super) fn title_block(d: &Drawing, s: &mut Sheet, unit: UiUnit, zh: bool, page: usize) {
    if page == 0 {
        s.texts.push((
            Vec2::new(100., 12.),
            if zh {
                "位置基准：各视图左下包围框角点。虚线表示隐藏边。"
            } else {
                "POSITION DATUM: VIEW LOWER-LEFT BOUNDS · DASHED: HIDDEN"
            }
            .into(),
            0.,
        ));
    }
    let cells = [
        (
            Vec2::new(140., 181.),
            Vec2::new(250., 191.),
            if zh { "名称" } else { "TITLE" },
            d.title.clone(),
        ),
        (
            Vec2::new(250., 181.),
            Vec2::new(290., 191.),
            if zh { "图号" } else { "DWG NO." },
            d.drawing_number.clone(),
        ),
        (
            Vec2::new(140., 191.),
            Vec2::new(185., 197.),
            if zh { "材料" } else { "MATERIAL" },
            d.material.clone(),
        ),
        (
            Vec2::new(185., 191.),
            Vec2::new(222., 197.),
            if zh { "制图" } else { "DRAWN" },
            d.drawn_by.clone(),
        ),
        (
            Vec2::new(222., 191.),
            Vec2::new(260., 197.),
            if zh { "审核" } else { "CHECKED" },
            d.checked_by.clone(),
        ),
        (
            Vec2::new(260., 191.),
            Vec2::new(290., 197.),
            if zh { "版本" } else { "REV" },
            d.revision.clone(),
        ),
    ];
    for ((a, b, label, value), field) in cells.into_iter().zip([
        edit::Field::Title,
        edit::Field::Number,
        edit::Field::Material,
        edit::Field::Drawn,
        edit::Field::Checked,
        edit::Field::Revision,
    ]) {
        s.bindings.insert(s.texts.len(), field);
        s.cells.push((a, b, field));
        for line in [
            [a, Vec2::new(b.x, a.y)],
            [a, Vec2::new(a.x, b.y)],
            [Vec2::new(a.x, b.y), b],
            [Vec2::new(b.x, a.y), b],
        ] {
            s.lines.push((line, true));
        }
        let width = ((b.x - a.x) / 1.7) as usize;
        let value: String = value.chars().take(width).collect();
        s.texts
            .push(((a + b) * 0.5, format!("{label}: {value}"), 0.));
    }
    let scale = if d.views.len() == 3 {
        fit(&d.views)
    } else {
        1.
    };
    s.lines
        .push(([Vec2::new(140., 197.), Vec2::new(140., 203.)], true));
    s.texts.push((
        Vec2::new(215., 200.),
        format!(
            "A4 | {} {:.3}:1 | {} | {} {}/{}",
            if zh { "比例" } else { "SCALE" },
            scale,
            unit.symbol(),
            if zh { "页" } else { "SHEET" },
            page + 1,
            features::pages(d, unit, zh)
        ),
        0.,
    ));
    s.texts.push((
        Vec2::new(68., 186.),
        format!(
            "S33D · {}",
            if zh {
                "第三角投影"
            } else {
                "THIRD ANGLE PROJECTION"
            }
        ),
        0.,
    ));
    s.bindings.insert(s.texts.len(), edit::Field::Date);
    s.texts.push((
        Vec2::new(68., 193.),
        format!("{} {}", if zh { "日期" } else { "DATE" }, d.drawing_date),
        0.,
    ));
    s.texts.push((
        Vec2::new(68., 200.),
        if zh {
            "尺寸以标注为准；未指定公差。"
        } else {
            "DO NOT SCALE DRAWING · TOLERANCES NOT SPECIFIED"
        }
        .into(),
        0.,
    ));
}
pub(super) fn paint_dashed(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, zoom: f32) {
    let delta = b - a;
    let len = delta.length();
    let direction = delta / len.max(1e-6);
    let mut t = 0.;
    while t < len {
        p.line_segment(
            [a + direction * t, a + direction * (t + 1.8 * zoom).min(len)],
            Stroke::new(0.7, Color32::GRAY),
        );
        t += 2.8 * zoom;
    }
}
