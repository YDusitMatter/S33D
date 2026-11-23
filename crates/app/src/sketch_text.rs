//! Text is outlined once, then stored as portable, editable sketch geometry.
use s33d_model::{sketch::SketchText, SketchEntity};
use ttf_parser::{Face, OutlineBuilder};

pub const FONTS: [&str; 3] = ["Noto Sans SC", "Ubuntu Light", "Hack"];
const NOTO: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSC.ttf");
type Point = [f64; 2];

/// Baseline caret positions use the very same advances as outline generation,
/// including spaces, so the canvas editor never substitutes UI-font metrics.
pub fn caret_positions(content: &str, font: &str) -> Vec<Point> {
    let defaults = egui::FontDefinitions::default();
    let bytes = match font {
        "Noto Sans SC" => NOTO,
        "Ubuntu Light" => defaults.font_data["Ubuntu-Light"].font.as_ref(),
        "Hack" => defaults.font_data["Hack"].font.as_ref(),
        _ => return vec![[0.0; 2]],
    };
    let Ok(mut face) = Face::parse(bytes, 0) else {
        return vec![[0.0; 2]];
    };
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), 400.0);
    let em = f64::from(face.units_per_em());
    let mut pen = [0.0; 2];
    let mut result = vec![pen];
    for ch in content.chars() {
        if ch == '\n' {
            pen = [0.0, pen[1] - 1.4];
        } else if ch != '\r' {
            pen[0] += face
                .glyph_index(ch)
                .and_then(|g| face.glyph_hor_advance(g))
                .map(|advance| f64::from(advance) / em)
                .unwrap_or(1.0);
        }
        result.push(pen);
    }
    result
}

pub fn filled_triangles(entity: &SketchEntity) -> Result<Vec<[[f64; 2]; 3]>, String> {
    let regions = s33d_model::compute_regions(&[entity.clone()]);
    let mut triangles = Vec::new();
    for i in filled_regions(entity, &regions)? {
        let mesh = s33d_model::triangulate_region(&regions[i].outer, &regions[i].holes)
            .map_err(|error| format!("Text region {i}: {error}"))?;
        triangles.extend(mesh.triangles);
    }
    Ok(triangles)
}

/// Select the filled parts of one text object, not its enclosed counters.
/// Region centroids can lie in holes; a checked interior witness cannot.
/// A failed region aborts the whole group rather than silently dropping letters.
pub fn filled_regions(
    entity: &SketchEntity,
    regions: &[s33d_kernel::Region2D],
) -> Result<Vec<usize>, String> {
    if !matches!(entity, SketchEntity::Path { text: Some(_), .. }) {
        return Ok(Vec::new());
    }
    let contours = entity.to_chains();
    let mut selected = Vec::new();
    for (index, region) in regions.iter().enumerate() {
        let mesh = s33d_model::triangulate_region(&region.outer, &region.holes)
            .map_err(|error| format!("Text region {index}: {error}"))?;
        let p = mesh
            .representable_interior_point()
            .ok_or_else(|| format!("Text region {index}: no representable interior point"))?;
        let mut winding = 0i32;
        for (points, closed) in &contours {
            if !closed || points.is_empty() {
                continue;
            }
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
                if a[1] <= p[1] && b[1] > p[1] && cross > 0.0 {
                    winding += 1;
                }
                if a[1] > p[1] && b[1] <= p[1] && cross < 0.0 {
                    winding -= 1;
                }
            }
        }
        if winding != 0 {
            selected.push(index);
        }
    }
    Ok(selected)
}

/// An empty editor is a placed text object, but has no filled geometry yet.
pub fn editable_text(
    content: &str,
    font: &str,
    size: f64,
    origin: Point,
) -> Result<SketchEntity, String> {
    if content.trim().is_empty() && size.is_finite() && size > 1e-6 {
        return Ok(SketchEntity::Path {
            contours: Vec::new(),
            origin,
            scale: size,
            rot: 0.0,
            text: Some(SketchText {
                content: content.into(),
                font: font.into(),
            }),
        });
    }
    outline_text(content, font, size, origin)
}

pub fn outline_text(
    content: &str,
    font: &str,
    size: f64,
    origin: Point,
) -> Result<SketchEntity, String> {
    if content.trim().is_empty() {
        return Err("Enter text first".into());
    }
    if content.chars().count() > 64 {
        return Err("Use at most 64 characters per text object".into());
    }
    if !size.is_finite() || size <= 1e-6 || origin.iter().any(|v| !v.is_finite()) {
        return Err("Font size must be greater than 0".into());
    }
    let defaults = egui::FontDefinitions::default();
    let bytes = match font {
        "Noto Sans SC" => NOTO,
        "Ubuntu Light" => defaults
            .font_data
            .get("Ubuntu-Light")
            .ok_or("Font unavailable")?
            .font
            .as_ref(),
        "Hack" => defaults
            .font_data
            .get("Hack")
            .ok_or("Font unavailable")?
            .font
            .as_ref(),
        _ => return Err("Unknown outline font".into()),
    };
    let mut face = Face::parse(bytes, 0).map_err(|_| "Unable to read outline font")?;
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), 400.0);
    let em = f64::from(face.units_per_em());
    let mut pen = [0.0, 0.0];
    let mut contours = Vec::new();
    for ch in content.chars() {
        if ch == '\n' {
            pen[0] = 0.0;
            pen[1] -= 1.4;
            continue;
        }
        if ch == '\r' {
            continue;
        }
        let glyph = face
            .glyph_index(ch)
            .ok_or_else(|| format!("Font has no glyph for '{ch}'; try Noto Sans SC"))?;
        let mut builder = Outline {
            contours: Vec::new(),
            current: Vec::new(),
            em,
            pen,
        };
        let outlined = face.outline_glyph(glyph, &mut builder).is_some();
        if !outlined && !ch.is_whitespace() {
            return Err(format!("No outline for '{ch}'"));
        }
        contours.extend(builder.contours);
        pen[0] += f64::from(face.glyph_hor_advance(glyph).unwrap_or(face.units_per_em())) / em;
        // Bound region construction cost; never silently drop glyph strokes.
        if contours
            .iter()
            .map(|(p, _): &(Vec<Point>, bool)| p.len())
            .sum::<usize>()
            > 8000
        {
            return Err("Text is too complex; split it into shorter text objects".into());
        }
    }
    if contours.is_empty() {
        return Err("Text contains no drawable outlines".into());
    }
    Ok(SketchEntity::Path {
        contours,
        origin,
        scale: size,
        rot: 0.0,
        text: Some(SketchText {
            content: content.to_owned(),
            font: font.to_owned(),
        }),
    })
}

struct Outline {
    contours: Vec<(Vec<Point>, bool)>,
    current: Vec<Point>,
    em: f64,
    pen: Point,
}
impl Outline {
    fn point(&self, x: f32, y: f32) -> Point {
        [
            x as f64 / self.em + self.pen[0],
            y as f64 / self.em + self.pen[1],
        ]
    }
    fn push(&mut self, p: Point) {
        if self
            .current
            .last()
            .is_none_or(|last| distance(*last, p) > 1e-9)
        {
            self.current.push(p);
        }
    }
    fn cubic(&mut self, a: Point, b: Point, c: Point, d: Point, depth: u8) {
        if depth >= 12
            || (line_distance(b, a, d).max(line_distance(c, a, d)) < 0.0008
                && distance(a, b) + distance(b, c) + distance(c, d) - distance(a, d) < 0.002)
        {
            self.push(d);
            return;
        }
        let ab = mid(a, b);
        let bc = mid(b, c);
        let cd = mid(c, d);
        let abc = mid(ab, bc);
        let bcd = mid(bc, cd);
        let m = mid(abc, bcd);
        self.cubic(a, ab, abc, m, depth + 1);
        self.cubic(m, bcd, cd, d, depth + 1);
    }
}
fn mid(a: Point, b: Point) -> Point {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn line_distance(p: Point, a: Point, b: Point) -> f64 {
    let length = distance(a, b);
    if length < 1e-12 {
        distance(p, a)
    } else {
        ((p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])).abs() / length
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.push(self.point(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(self.point(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(a) = self.current.last().copied() else {
            return;
        };
        let q = self.point(x1, y1);
        let d = self.point(x, y);
        let b = [(a[0] + 2.0 * q[0]) / 3.0, (a[1] + 2.0 * q[1]) / 3.0];
        let c = [(d[0] + 2.0 * q[0]) / 3.0, (d[1] + 2.0 * q[1]) / 3.0];
        self.cubic(a, b, c, d, 0);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if let Some(a) = self.current.last().copied() {
            self.cubic(
                a,
                self.point(x1, y1),
                self.point(x2, y2),
                self.point(x, y),
                0,
            );
        }
    }
    fn close(&mut self) {
        if self.current.len() > 1 && distance(self.current[0], *self.current.last().unwrap()) < 1e-9
        {
            self.current.pop();
        }
        if self.current.len() >= 3 {
            self.contours
                .push((std::mem::take(&mut self.current), true));
        } else {
            self.current.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_carets_and_preview_use_outline_coordinates() {
        let carets = caret_positions("你是猪猪", FONTS[0]);
        assert_eq!(carets.len(), 5);
        assert!(carets.windows(2).all(|p| p[1][0] > p[0][0]));
        let spaced = caret_positions("A A", FONTS[0]);
        assert!(spaced[2][0] > spaced[1][0]);
        let mut entity = outline_text("OB8", FONTS[0], 10.0, [-52.0, 18.0]).unwrap();
        if let SketchEntity::Path { rot, .. } = &mut entity {
            *rot = 37.0;
        }
        let triangles = filled_triangles(&entity).unwrap();
        assert!(!triangles.is_empty());
        let bounds = entity.to_chains().into_iter().flat_map(|(p, _)| p).fold(
            ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
            |(mut min, mut max), p| {
                for axis in 0..2 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
                (min, max)
            },
        );
        assert!(triangles
            .iter()
            .flatten()
            .all(|p| (0..2).all(|a| p[a] >= bounds.0[a] - 1e-8 && p[a] <= bounds.1[a] + 1e-8)));
    }
    #[test]
    fn text_group_includes_all_letters_but_not_counters_or_other_objects() {
        for font in FONTS {
            let text = outline_text("OB8", font, 10.0, [0.0; 2]).unwrap();
            let other = outline_text("A", font, 10.0, [50.0, 0.0]).unwrap();
            let regions = s33d_model::compute_regions(&[text.clone(), other.clone()]);
            let selected = filled_regions(&text, &regions).unwrap();
            assert_eq!(selected.len(), 3, "{font}: one filled region per letter");
            assert_eq!(
                selected
                    .iter()
                    .map(|&i| regions[i].holes.len())
                    .sum::<usize>(),
                5
            );
            let others = filled_regions(&other, &regions).unwrap();
            // Some fonts construct A from overlapping closed strokes.
            assert!(!others.is_empty());
            assert!(selected.iter().all(|i| !others.contains(i)));
        }
        let text = outline_text("中文", FONTS[0], 12.0, [0.0; 2]).unwrap();
        let regions = s33d_model::compute_regions(&[text.clone()]);
        assert!(
            filled_regions(&text, &regions).unwrap().len() >= 3,
            "All separate Chinese strokes are grouped"
        );
    }
    #[test]
    fn font_outlines_preserve_counters_and_are_portable() {
        for font in FONTS {
            let entity = outline_text("OB8", font, 10.0, [2.0, 3.0]).unwrap();
            assert_eq!(
                entity.to_chains().len(),
                8,
                "{font}: each glyph counter is independent"
            );
            let encoded = serde_json::to_string(&entity).unwrap();
            let decoded: SketchEntity = serde_json::from_str(&encoded).unwrap();
            for ((a, ca), (b, cb)) in decoded.to_chains().into_iter().zip(entity.to_chains()) {
                assert_eq!(ca, cb);
                assert_eq!(a.len(), b.len());
                for (p, q) in a.into_iter().zip(b) {
                    assert!(distance(p, q) < 1e-10);
                }
            }
        }
    }
    #[test]
    fn chinese_and_invalid_input() {
        assert!(
            outline_text("中文", FONTS[0], 12.0, [0.0; 2])
                .unwrap()
                .to_chains()
                .len()
                >= 4
        );
        assert!(outline_text("中文", FONTS[2], 12.0, [0.0; 2]).is_err());
        assert!(outline_text(" ", FONTS[0], 12.0, [0.0; 2]).is_err());
        assert!(outline_text("A", FONTS[0], f64::NAN, [0.0; 2]).is_err());
    }

    #[test]
    fn outlined_regions_highlight_and_extrude_with_holes() {
        use s33d_kernel::{GeometryKernel, PlaneDef, TruckKernel};
        for content in ["O", "B", "8", "中文"] {
            let entity = outline_text(content, FONTS[0], 12.0, [0.0; 2]).unwrap();
            let regions = s33d_model::compute_regions(&[entity]);
            assert!(!regions.is_empty(), "{content}");
            if content == "O" {
                assert!(regions.iter().any(|r| r.holes.len() == 1));
            }
            if content == "B" || content == "8" {
                assert!(regions.iter().any(|r| r.holes.len() == 2));
            }
            let mut kernel = TruckKernel::new();
            for region in &regions {
                let triangles = s33d_model::triangulate_region(&region.outer, &region.holes)
                    .unwrap()
                    .triangles;
                assert!(!triangles.is_empty(), "{content}: missing highlight");
                let shape = kernel
                    .extrude_region(region, &PlaneDef::from_normal([0.0, 0.0, 1.0]), 3.0)
                    .unwrap();
                let mesh = kernel.tessellate(shape).unwrap();
                assert!(!mesh.indices.is_empty());
                assert!(mesh.positions.iter().flatten().all(|v| v.is_finite()));
                kernel.delete(shape);
            }
        }
    }
}
