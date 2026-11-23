//! 草图实体模型（与 Super3D `sketch.js` 的数据模型一致）。
//!
//! 实体在草图平面局部 (u, v) 坐标下定义：
//! - `Line`：端点 a、b
//! - `Arc`：圆心 c、半径 r、起止角 a0/a1（弧度）、方向 ccw
//! - `Circle`：圆心 c、半径 r
//! - `Rect`：两个对角 c1、c2
//! - `Polygon`：中心 c、半径 r（外接圆）、边数 n、旋转角 rot

use serde::{Deserialize, Serialize};

pub type Point2 = [f64; 2];

/// Editable text metadata. Outlines are embedded in the entity so projects do
/// not depend on installed fonts when reopened on another machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchText {
    pub content: String,
    pub font: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SketchEntity {
    Path {
        contours: Vec<(Vec<Point2>, bool)>,
        origin: Point2,
        scale: f64,
        rot: f64,
        text: Option<SketchText>,
    },
    Line {
        a: Point2,
        b: Point2,
    },
    Arc {
        c: Point2,
        r: f64,
        a0: f64,
        a1: f64,
        ccw: bool,
    },
    Circle {
        c: Point2,
        r: f64,
    },
    Rect {
        c1: Point2,
        c2: Point2,
    },
    /// Rectangle with a stable identity after an in-plane rotation.
    /// `rot` is radians around `c`; width/height remain its editable local size.
    RotatedRect {
        c: Point2,
        width: f64,
        height: f64,
        rot: f64,
    },
    Polygon {
        c: Point2,
        r: f64,
        n: u32,
        rot: f64,
    },
}

impl SketchEntity {
    pub fn kind(&self) -> &'static str {
        match self {
            SketchEntity::Path { text, .. } => {
                if text.is_some() {
                    "text"
                } else {
                    "freehand"
                }
            }
            SketchEntity::Line { .. } => "line",
            SketchEntity::Arc { .. } => "arc",
            SketchEntity::Circle { .. } => "circle",
            SketchEntity::Rect { .. } | SketchEntity::RotatedRect { .. } => "rect",
            SketchEntity::Polygon { .. } => "polygon",
        }
    }

    /// 实体 → 折线链（与 `sketch.js entityToChains` 一致）。
    /// 返回 (点列, 是否闭合)。
    pub fn to_chain(&self) -> (Vec<Point2>, bool) {
        match self {
            SketchEntity::Path { .. } => self.to_chains().into_iter().next().unwrap_or_default(),
            SketchEntity::Line { a, b } => (vec![*a, *b], false),
            SketchEntity::Arc { c, r, a0, a1, ccw } => {
                let sweep = if *ccw {
                    let mut s = a1 - a0;
                    while s <= 0.0 {
                        s += std::f64::consts::TAU;
                    }
                    s
                } else {
                    let mut s = a1 - a0;
                    while s >= 0.0 {
                        s -= std::f64::consts::TAU;
                    }
                    s
                };
                let n = (sweep.abs() / (std::f64::consts::PI / 36.0))
                    .ceil()
                    .max(6.0) as usize;
                let mut pts = Vec::with_capacity(n + 1);
                for i in 0..=n {
                    let t = a0 + sweep * (i as f64 / n as f64);
                    pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                }
                (pts, false)
            }
            SketchEntity::Circle { c, r } => {
                let mut pts = Vec::with_capacity(72);
                for i in 0..72 {
                    let t = (i as f64 / 72.0) * std::f64::consts::TAU;
                    pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                }
                (pts, true)
            }
            SketchEntity::Rect { c1, c2 } => {
                let (x1, y1) = (c1[0], c1[1]);
                let (x2, y2) = (c2[0], c2[1]);
                (vec![[x1, y1], [x2, y1], [x2, y2], [x1, y2]], true)
            }
            SketchEntity::RotatedRect {
                c,
                width,
                height,
                rot,
            } => (
                rotated_rect_corners(*c, *width, *height, *rot).to_vec(),
                true,
            ),
            SketchEntity::Polygon { c, r, n, rot } => {
                let n = (*n).max(3);
                let mut pts = Vec::with_capacity(n as usize);
                for i in 0..n {
                    let t = rot + (i as f64 / n as f64) * std::f64::consts::TAU;
                    pts.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                }
                (pts, true)
            }
        }
    }

    /// Multiple glyph contours must never be joined by artificial segments.
    pub fn to_chains(&self) -> Vec<(Vec<Point2>, bool)> {
        if let Self::Path {
            contours,
            origin,
            scale,
            rot,
            ..
        } = self
        {
            let (sin, cos) = rot.sin_cos();
            contours
                .iter()
                .map(|(points, closed)| {
                    (
                        points
                            .iter()
                            .map(|p| {
                                [
                                    origin[0] + scale * (p[0] * cos - p[1] * sin),
                                    origin[1] + scale * (p[0] * sin + p[1] * cos),
                                ]
                            })
                            .collect(),
                        *closed,
                    )
                })
                .collect()
        } else {
            vec![self.to_chain()]
        }
    }

    /// 弧的方向修正：ccw 时 a1 > a0，否则 a1 < a0。
    fn arc_sweep_angles(a0: f64, a1: f64, ccw: bool) -> (f64, f64) {
        if ccw {
            let mut a1 = a1;
            while a1 <= a0 {
                a1 += std::f64::consts::TAU;
            }
            (a0, a1)
        } else {
            let mut a1 = a1;
            while a1 >= a0 {
                a1 -= std::f64::consts::TAU;
            }
            (a0, a1)
        }
    }

    /// 三点 → 圆弧（与 `sketch.js arcFrom3Points` 一致）。
    pub fn arc_from_3_points(a: Point2, b: Point2, through: Point2) -> Option<SketchEntity> {
        let cc = circum_center(a, b, through)?;
        let r = ((a[0] - cc[0]).powi(2) + (a[1] - cc[1]).powi(2)).sqrt();
        let a0 = (a[1] - cc[1]).atan2(a[0] - cc[0]);
        let mut a1 = (b[1] - cc[1]).atan2(b[0] - cc[0]);
        let cross = (b[0] - a[0]) * (through[1] - a[1]) - (b[1] - a[1]) * (through[0] - a[0]);
        let ccw = cross > 0.0;
        if ccw {
            while a1 <= a0 {
                a1 += std::f64::consts::TAU;
            }
        } else {
            while a1 >= a0 {
                a1 -= std::f64::consts::TAU;
            }
        }
        Some(SketchEntity::Arc {
            c: cc,
            r,
            a0,
            a1,
            ccw,
        })
    }

    /// 参数编辑字段（与 `sketch.js entityParams` 一致）。
    /// 返回字段列表 (key, 标签, 值, 是否隐藏)。角度为度。
    pub fn params(&self) -> Vec<(&'static str, &'static str, f64, bool)> {
        let deg = |r: f64| r.to_degrees();
        match self {
            SketchEntity::Path {
                origin,
                scale,
                rot,
                text,
                ..
            } => vec![
                ("x", "Position X", origin[0], false),
                ("y", "Position Y", origin[1], false),
                (
                    if text.is_some() { "h" } else { "scale" },
                    if text.is_some() {
                        "Font size"
                    } else {
                        "Scale factor"
                    },
                    *scale,
                    false,
                ),
                ("rot", "Rotation deg", -deg(*rot), false),
            ],
            SketchEntity::Line { a, b } => vec![
                ("x1", "Start X", a[0], false),
                ("y1", "Start Y", a[1], false),
                ("x2", "End X", b[0], false),
                ("y2", "End Y", b[1], false),
                // 联动字段：由端点决定，仅显示不编辑
                (
                    "len",
                    "Length",
                    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt(),
                    true,
                ),
            ],
            SketchEntity::Circle { c, r } => vec![
                ("x", "Center X", c[0], false),
                ("y", "Center Y", c[1], false),
                ("r", "Radius", *r, false),
            ],
            SketchEntity::Rect { c1, c2 } => {
                let x1 = c1[0].min(c2[0]);
                let y1 = c1[1].min(c2[1]);
                let x2 = c1[0].max(c2[0]);
                let y2 = c1[1].max(c2[1]);
                vec![
                    ("v1x", "V1 X", x1, false),
                    ("v1y", "V1 Y", y1, false),
                    ("w", "Length", x2 - x1, false),
                    ("h", "Width", y2 - y1, false),
                    // 联动字段：由 V1 + 长宽决定，仅显示不编辑
                    ("v2x", "V2 X", x2, true),
                    ("v2y", "V2 Y", y1, true),
                    ("v3x", "V3 X", x2, true),
                    ("v3y", "V3 Y", y2, true),
                    ("v4x", "V4 X", x1, true),
                    ("v4y", "V4 Y", y2, true),
                ]
            }
            SketchEntity::RotatedRect {
                c,
                width,
                height,
                rot,
            } => vec![
                ("x", "Center X", c[0], false),
                ("y", "Center Y", c[1], false),
                ("w", "Length", *width, false),
                ("h", "Width", *height, false),
                ("rot", "Rotation clockwise deg", -deg(*rot), false),
            ],
            SketchEntity::Polygon { c, r, n, rot } => vec![
                ("x", "Center X", c[0], false),
                ("y", "Center Y", c[1], false),
                ("n", "Sides", *n as f64, false),
                (
                    "s",
                    "Side length",
                    2.0 * r * (std::f64::consts::PI / *n as f64).sin(),
                    false,
                ),
                ("rot", "Rotation deg", deg(*rot), false),
            ],
            SketchEntity::Arc {
                c,
                r,
                a0,
                a1,
                ccw: _,
            } => vec![
                ("x", "Center X", c[0], false),
                ("y", "Center Y", c[1], false),
                ("r", "Radius", *r, false),
                ("a0", "Start deg", deg(*a0), false),
                ("a1", "End deg", deg(*a1), false),
            ],
        }
    }

    /// 从参数字典重建实体（校验，可能失败——与 `sketch.js entityFromParams` 一致）。
    pub fn from_params(
        &self,
        vals: &std::collections::HashMap<&str, f64>,
    ) -> Result<SketchEntity, String> {
        let rad = |d: f64| d.to_radians();
        let pos = |v: f64| v.is_finite() && v > 1e-6;
        match self {
            SketchEntity::Path { contours, text, .. } => {
                let scale = get(vals, if text.is_some() { "h" } else { "scale" });
                if !pos(scale) || vals.values().any(|v| !v.is_finite()) {
                    return Err("Size must be finite and greater than 0".into());
                }
                Ok(Self::Path {
                    contours: contours.clone(),
                    text: text.clone(),
                    origin: [get(vals, "x"), get(vals, "y")],
                    scale,
                    rot: -rad(get(vals, "rot")),
                })
            }
            SketchEntity::Line { .. } => {
                let out = SketchEntity::Line {
                    a: [get(vals, "x1"), get(vals, "y1")],
                    b: [get(vals, "x2"), get(vals, "y2")],
                };
                if let SketchEntity::Line { a, b } = &out {
                    if ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() < 1e-6 {
                        return Err("Line length must be greater than 0".into());
                    }
                }
                Ok(out)
            }
            SketchEntity::Circle { .. } => {
                if !pos(get(vals, "r")) {
                    return Err("Radius must be greater than 0".into());
                }
                Ok(SketchEntity::Circle {
                    c: [get(vals, "x"), get(vals, "y")],
                    r: get(vals, "r"),
                })
            }
            SketchEntity::Rect { .. } => {
                let w = get(vals, "w");
                let h = get(vals, "h");
                if !pos(w) || !pos(h) {
                    return Err("Length and width must be greater than 0".into());
                }
                Ok(SketchEntity::Rect {
                    c1: [get(vals, "v1x"), get(vals, "v1y")],
                    c2: [get(vals, "v1x") + w, get(vals, "v1y") + h],
                })
            }
            SketchEntity::RotatedRect { .. } => {
                let w = get(vals, "w");
                let h = get(vals, "h");
                if !pos(w) || !pos(h) {
                    return Err("Length and width must be greater than 0".into());
                }
                Ok(SketchEntity::RotatedRect {
                    c: [get(vals, "x"), get(vals, "y")],
                    width: w,
                    height: h,
                    rot: -rad(get(vals, "rot")),
                })
            }
            SketchEntity::Polygon { .. } => {
                let n = get(vals, "n").round();
                let s = get(vals, "s");
                if !pos(s) {
                    return Err("Side length must be greater than 0".into());
                }
                if !(3.0..=64.0).contains(&n) {
                    return Err("Sides must be between 3 and 64".into());
                }
                Ok(SketchEntity::Polygon {
                    c: [get(vals, "x"), get(vals, "y")],
                    r: s / (2.0 * (std::f64::consts::PI / n).sin()),
                    n: n as u32,
                    rot: rad(get(vals, "rot")),
                })
            }
            SketchEntity::Arc { ccw, .. } => {
                let r = get(vals, "r");
                if !pos(r) {
                    return Err("Radius must be greater than 0".into());
                }
                let a0 = rad(get(vals, "a0"));
                let a1 = rad(get(vals, "a1"));
                if ((a1 - a0).abs()) < 1e-6 {
                    return Err("Arc angle must be greater than 0".into());
                }
                let (a0, a1) = Self::arc_sweep_angles(a0, a1, *ccw);
                Ok(SketchEntity::Arc {
                    c: [get(vals, "x"), get(vals, "y")],
                    r,
                    a0,
                    a1,
                    ccw: *ccw,
                })
            }
        }
    }

    /// 尺寸文本框的短文本（与 `sketch.js entityDimsText` 一致）：
    /// line=长度, circle/arc=半径, polygon=边长, rect=宽,高。
    pub fn dims_text(&self) -> String {
        let r1 = |x: f64| format!("{:.3}", (x * 1000.0).round() / 1000.0);
        match self {
            SketchEntity::Path { scale, .. } => r1(*scale),
            SketchEntity::Line { a, b } => {
                r1(((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt())
            }
            SketchEntity::Circle { r, .. } | SketchEntity::Arc { r, .. } => r1(*r),
            SketchEntity::Polygon { c: _, r, n, .. } => {
                r1(2.0 * r * (std::f64::consts::PI / *n as f64).sin())
            }
            SketchEntity::Rect { c1, c2 } => format!(
                "{}, {}",
                r1((c2[0] - c1[0]).abs()),
                r1((c2[1] - c1[1]).abs())
            ),
            SketchEntity::RotatedRect { width, height, .. } => {
                format!("{}, {}", r1(*width), r1(*height))
            }
        }
    }

    /// 解析 Value 框文本为参数字典（中心/轴保持——与 `sketch.js dimsToParams` 一致）。
    pub fn dims_to_params(&self, text: &str) -> Option<Vec<(String, f64)>> {
        let nums: Vec<f64> = text
            .split(|c: char| c == ',' || c == '，' || c == ';' || c == '；' || c.is_whitespace())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        if nums.is_empty() || nums[0] <= 0.0 {
            return None;
        }
        match self {
            SketchEntity::Path { .. } => {
                let mut params: Vec<_> = self
                    .params()
                    .into_iter()
                    .map(|(key, _, value, _)| (key.to_owned(), value))
                    .collect();
                for (key, value) in &mut params {
                    if key == "h" || key == "scale" {
                        *value = nums[0];
                    }
                }
                Some(params)
            }
            SketchEntity::Line { a, b } => {
                let mx = (a[0] + b[0]) / 2.0;
                let my = (a[1] + b[1]) / 2.0;
                let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                if len < 1e-9 {
                    return None;
                }
                let dx = ((b[0] - a[0]) / len) * (nums[0] / 2.0);
                let dy = ((b[1] - a[1]) / len) * (nums[0] / 2.0);
                Some(vec![
                    ("x1".into(), mx - dx),
                    ("y1".into(), my - dy),
                    ("x2".into(), mx + dx),
                    ("y2".into(), my + dy),
                ])
            }
            SketchEntity::Circle { c, .. } => Some(vec![
                ("x".into(), c[0]),
                ("y".into(), c[1]),
                ("r".into(), nums[0]),
            ]),
            SketchEntity::Rect { c1, c2 } => {
                let w = nums[0].abs();
                let h = nums.get(1).map(|v| v.abs()).unwrap_or(w);
                if w < 1e-6 || h < 1e-6 {
                    return None;
                }
                Some(vec![
                    ("x".into(), (c1[0] + c2[0]) / 2.0),
                    ("y".into(), (c1[1] + c2[1]) / 2.0),
                    ("w".into(), w),
                    ("h".into(), h),
                ])
            }
            SketchEntity::RotatedRect { c, rot, .. } => {
                let w = nums[0].abs();
                let h = nums.get(1).map(|v| v.abs()).unwrap_or(w);
                if w < 1e-6 || h < 1e-6 {
                    return None;
                }
                Some(vec![
                    ("x".into(), c[0]),
                    ("y".into(), c[1]),
                    ("w".into(), w),
                    ("h".into(), h),
                    ("rot".into(), -rot.to_degrees()),
                ])
            }
            SketchEntity::Polygon { c, n, rot, .. } => Some(vec![
                ("x".into(), c[0]),
                ("y".into(), c[1]),
                ("n".into(), *n as f64),
                ("s".into(), nums[0]),
                ("rot".into(), rot.to_degrees()),
            ]),
            SketchEntity::Arc { c, a0, a1, .. } => Some(vec![
                ("x".into(), c[0]),
                ("y".into(), c[1]),
                ("r".into(), nums[0]),
                ("a0".into(), a0.to_degrees()),
                ("a1".into(), a1.to_degrees()),
            ]),
        }
    }
}

fn rotated_rect_corners(c: Point2, width: f64, height: f64, rot: f64) -> [Point2; 4] {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let (sin, cos) = rot.sin_cos();
    let rotate = |x: f64, y: f64| [c[0] + x * cos - y * sin, c[1] + x * sin + y * cos];
    [
        rotate(-half_width, -half_height),
        rotate(half_width, -half_height),
        rotate(half_width, half_height),
        rotate(-half_width, half_height),
    ]
}

fn get(vals: &std::collections::HashMap<&str, f64>, key: &str) -> f64 {
    vals.get(key).copied().unwrap_or(0.0)
}

/// 三点外接圆圆心（与 `sketch.js circumCenter` 一致）。
fn circum_center(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-9 {
        return None;
    }
    let a2 = a[0] * a[0] + a[1] * a[1];
    let b2 = b[0] * b[0] + b[1] * b[1];
    let c2 = c[0] * c[0] + c[1] * c[1];
    Some([
        (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
        (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_chain_closed() {
        let r = SketchEntity::Rect {
            c1: [0.0, 0.0],
            c2: [10.0, 5.0],
        };
        let (pts, closed) = r.to_chain();
        assert!(closed);
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], [0.0, 0.0]);
        assert_eq!(pts[2], [10.0, 5.0]);
    }

    #[test]
    fn rotated_rect_keeps_rect_identity_parameters_and_serialization() {
        let rect = SketchEntity::RotatedRect {
            c: [3.0, -2.0],
            width: 10.0,
            height: 4.0,
            rot: -30.0_f64.to_radians(),
        };
        assert_eq!(rect.kind(), "rect");
        let (points, closed) = rect.to_chain();
        assert!(closed);
        assert_eq!(points.len(), 4);
        let params: std::collections::HashMap<&str, f64> = rect
            .params()
            .into_iter()
            .map(|(key, _, value, _)| (key, value))
            .collect();
        assert!((params["w"] - 10.0).abs() < 1e-9);
        assert!((params["h"] - 4.0).abs() < 1e-9);
        assert!((params["rot"] - 30.0).abs() < 1e-9);
        assert_eq!(rect.from_params(&params).unwrap(), rect);
        let json = serde_json::to_string(&rect).unwrap();
        assert_eq!(serde_json::from_str::<SketchEntity>(&json).unwrap(), rect);
    }

    #[test]
    fn circle_chain_closed() {
        let c = SketchEntity::Circle {
            c: [0.0, 0.0],
            r: 5.0,
        };
        let (pts, closed) = c.to_chain();
        assert!(closed);
        assert_eq!(pts.len(), 72);
    }

    #[test]
    fn line_params_roundtrip() {
        let l = SketchEntity::Line {
            a: [0.0, 0.0],
            b: [3.0, 4.0],
        };
        let params = l.params();
        let map: std::collections::HashMap<&str, f64> =
            params.iter().map(|(k, _, v, _)| (*k, *v)).collect();
        assert!((map["len"] - 5.0).abs() < 1e-9);
        let rebuilt = l.from_params(&map).unwrap();
        assert_eq!(rebuilt, l);
    }

    #[test]
    fn rect_params_and_apply() {
        let r = SketchEntity::Rect {
            c1: [1.0, 2.0],
            c2: [6.0, 8.0],
        };
        let params = r.params();
        let map: std::collections::HashMap<&str, f64> =
            params.iter().map(|(k, _, v, _)| (*k, *v)).collect();
        assert!((map["w"] - 5.0).abs() < 1e-9);
        assert!((map["h"] - 6.0).abs() < 1e-9);
        // 用 V1 + w/h 重建
        let mut m2 = map.clone();
        m2.insert("w", 10.0);
        let rebuilt = r.from_params(&m2).unwrap();
        if let SketchEntity::Rect { c1, c2 } = rebuilt {
            assert_eq!(c1, [1.0, 2.0]);
            assert_eq!(c2, [11.0, 2.0 + 6.0]);
        } else {
            panic!("not rect");
        }
    }

    #[test]
    fn arc_from_three_points_semicircle() {
        let arc = SketchEntity::arc_from_3_points([-1.0, 0.0], [1.0, 0.0], [0.0, 1.0]).unwrap();
        if let SketchEntity::Arc { c, r, ccw, .. } = arc {
            assert!((c[0]).abs() < 1e-9 && (c[1]).abs() < 1e-9);
            assert!((r - 1.0).abs() < 1e-9);
            assert!(ccw);
        } else {
            panic!("not arc");
        }
    }

    #[test]
    fn dims_to_params_line_length() {
        let l = SketchEntity::Line {
            a: [0.0, 0.0],
            b: [3.0, 4.0],
        };
        let vals = l.dims_to_params("10").unwrap();
        let map: std::collections::HashMap<&str, f64> =
            vals.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        // 中点 (1.5, 2.0) 保持，长度 10
        assert!(
            ((map["x2"] - map["x1"]).powi(2) + (map["y2"] - map["y1"]).powi(2)).sqrt() - 10.0
                < 1e-9
        );
    }
}
