//! Sketch trim planning. Analytic lines/circles/arcs stay analytic; composite
//! outlines are broken at their actual vertices, not at circle tessellation points.
use crate::sketch::{Point2, SketchEntity};
use std::f64::consts::TAU;

const EPS: f64 = 1e-8;
const PARAM_EPS: f64 = 1e-9;

#[derive(Debug, Clone)]
pub struct TrimPlan {
    pub entity: usize,
    pub replacement: Vec<SketchEntity>,
    pub removed: SketchEntity,
}

#[derive(Clone, Copy)]
enum Curve {
    Line(Point2, Point2),
    Arc {
        c: Point2,
        r: f64,
        start: f64,
        sweep: f64,
        closed: bool,
    },
}
fn sub(a: Point2, b: Point2) -> Point2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: Point2, b: Point2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: Point2, b: Point2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn distance(a: Point2, b: Point2) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

impl Curve {
    fn point(self, t: f64) -> Point2 {
        match self {
            Self::Line(a, b) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            Self::Arc {
                c, r, start, sweep, ..
            } => {
                let angle = start + sweep * t;
                [c[0] + r * angle.cos(), c[1] + r * angle.sin()]
            }
        }
    }
    fn parameter(self, p: Point2) -> Option<f64> {
        let t = match self {
            Self::Line(a, b) => {
                let d = sub(b, a);
                if dot(d, d) <= EPS * EPS {
                    return None;
                }
                dot(sub(p, a), d) / dot(d, d)
            }
            Self::Arc {
                c, start, sweep, ..
            } => {
                let angle = (p[1] - c[1]).atan2(p[0] - c[0]);
                let delta = if sweep > 0.0 {
                    (angle - start).rem_euclid(TAU)
                } else {
                    (start - angle).rem_euclid(TAU)
                };
                // Roundoff just behind the start must not become a full turn.
                if TAU - delta < PARAM_EPS {
                    0.0
                } else {
                    delta / sweep.abs()
                }
            }
        };
        if (-PARAM_EPS..=1.0 + PARAM_EPS).contains(&t) {
            Some(t.clamp(0.0, 1.0))
        } else {
            None
        }
    }
    fn nearest(self, p: Point2) -> (f64, f64) {
        let t = match self {
            Self::Line(a, b) => {
                let d = sub(b, a);
                (dot(sub(p, a), d) / dot(d, d)).clamp(0.0, 1.0)
            }
            Self::Arc { .. } => self.parameter(p).unwrap_or_else(|| {
                if distance(p, self.point(0.0)) < distance(p, self.point(1.0)) {
                    0.0
                } else {
                    1.0
                }
            }),
        };
        (t, distance(p, self.point(t)))
    }
    fn piece(self, a: f64, b: f64) -> SketchEntity {
        match self {
            Self::Line(..) => SketchEntity::Line {
                a: self.point(a),
                b: self.point(b),
            },
            Self::Arc {
                c,
                r,
                start,
                sweep,
                closed,
            } => {
                if closed && b - a >= 1.0 - PARAM_EPS {
                    SketchEntity::Circle { c, r }
                } else {
                    SketchEntity::Arc {
                        c,
                        r,
                        a0: start + sweep * a,
                        a1: start + sweep * b,
                        ccw: sweep > 0.0,
                    }
                }
            }
        }
    }
    fn closed(self) -> bool {
        matches!(self, Self::Arc { closed: true, .. })
    }
}

fn curves(entity: &SketchEntity) -> Vec<Curve> {
    match entity {
        SketchEntity::Circle { c, r } if *r > EPS => vec![Curve::Arc {
            c: *c,
            r: *r,
            start: 0.0,
            sweep: TAU,
            closed: true,
        }],
        SketchEntity::Arc { c, r, a0, a1, ccw } if *r > EPS => {
            let amount = if *ccw {
                (a1 - a0).rem_euclid(TAU)
            } else {
                (a0 - a1).rem_euclid(TAU)
            };
            let amount = if amount < PARAM_EPS { TAU } else { amount };
            vec![Curve::Arc {
                c: *c,
                r: *r,
                start: *a0,
                sweep: if *ccw { amount } else { -amount },
                closed: false,
            }]
        }
        _ => entity
            .to_chains()
            .into_iter()
            .flat_map(|(pts, closed)| {
                let count = if closed {
                    pts.len()
                } else {
                    pts.len().saturating_sub(1)
                };
                (0..count)
                    .filter_map(|i| {
                        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                        (distance(a, b) > EPS).then_some(Curve::Line(a, b))
                    })
                    .collect::<Vec<_>>()
            })
            .collect(),
    }
}

/// Includes tangencies and the endpoints of collinear/coincident overlap.
fn intersections(a: Curve, b: Curve) -> Vec<Point2> {
    match (a, b) {
        (Curve::Line(p, q), Curve::Line(u, v)) => {
            let d = sub(q, p);
            let e = sub(v, u);
            let den = cross(d, e);
            if den.abs() <= PARAM_EPS * (dot(d, d) * dot(e, e)).sqrt() {
                if cross(sub(u, p), d).abs() <= EPS * dot(d, d).sqrt() {
                    vec![p, q, u, v]
                } else {
                    vec![]
                }
            } else {
                vec![a.point(cross(sub(u, p), e) / den)]
            }
        }
        (Curve::Line(p, q), Curve::Arc { c, r, .. }) => {
            let d = sub(q, p);
            let f = sub(p, c);
            let aa = dot(d, d);
            let t = -dot(f, d) / aa;
            let nearest = a.point(t);
            let h2 = r * r - dot(sub(nearest, c), sub(nearest, c));
            if h2 < -EPS * r.max(1.0) {
                vec![]
            } else {
                let dt = (h2.max(0.0) / aa).sqrt();
                vec![a.point(t - dt), a.point(t + dt)]
            }
        }
        (Curve::Arc { .. }, Curve::Line(..)) => intersections(b, a),
        (Curve::Arc { c, r, .. }, Curve::Arc { c: d, r: s, .. }) => {
            let delta = sub(d, c);
            let len = dot(delta, delta).sqrt();
            if len < EPS {
                if (r - s).abs() < EPS {
                    // A full circle has no geometric seam/end vertex. Only
                    // real endpoints of coincident open arcs divide a circle.
                    [a, b]
                        .into_iter()
                        .filter(|curve| !curve.closed())
                        .flat_map(|curve| [curve.point(0.0), curve.point(1.0)])
                        .collect()
                } else {
                    vec![]
                }
            } else if len > r + s + EPS || len < (r - s).abs() - EPS {
                vec![]
            } else {
                let x = (r * r - s * s + len * len) / (2.0 * len);
                let h = (r * r - x * x).max(0.0).sqrt();
                let base = [c[0] + x * delta[0] / len, c[1] + x * delta[1] / len];
                vec![
                    [base[0] - h * delta[1] / len, base[1] + h * delta[0] / len],
                    [base[0] + h * delta[1] / len, base[1] - h * delta[0] / len],
                ]
            }
        }
    }
}

/// Pick in real, unsnapped plane coordinates. The tolerance is supplied by the
/// viewport in world units, independently of the drawing grid/snap step.
pub fn plan_trim(entities: &[SketchEntity], cursor: Point2, tolerance: f64) -> Option<TrimPlan> {
    if !cursor.iter().all(|v| v.is_finite()) || !tolerance.is_finite() || tolerance <= 0.0 {
        return None;
    }
    let all: Vec<_> = entities.iter().map(curves).collect();
    let mut best = None;
    let mut best_distance = tolerance;
    for (owner, parts) in all.iter().enumerate() {
        for (index, curve) in parts.iter().enumerate() {
            let (t, d) = curve.nearest(cursor);
            if d < best_distance {
                best = Some((owner, index, *curve, t));
                best_distance = d;
            }
        }
    }
    let (owner, index, curve, t) = best?;
    let mut cuts = if curve.closed() {
        vec![]
    } else {
        vec![0.0, 1.0]
    };
    for (other_owner, parts) in all.iter().enumerate() {
        for (other_index, other) in parts.iter().enumerate() {
            if other_owner == owner && other_index == index {
                continue;
            }
            for p in intersections(curve, *other) {
                if let (Some(t), Some(_)) = (curve.parameter(p), other.parameter(p)) {
                    // Collinear overlap candidates can lie beyond either end.
                    if distance(curve.point(t), p) <= EPS * 4.0 {
                        cuts.push(t);
                    }
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < PARAM_EPS);
    let (start, end) = if curve.closed() {
        if cuts.len() < 2 {
            (0.0, 1.0)
        } else {
            let upper = cuts.partition_point(|cut| *cut <= t + PARAM_EPS);
            let start = if upper == 0 {
                cuts[cuts.len() - 1] - 1.0
            } else {
                cuts[upper - 1]
            };
            let end = if upper == cuts.len() {
                cuts[0] + 1.0
            } else {
                cuts[upper]
            };
            (start, end)
        }
    } else {
        let upper = cuts
            .partition_point(|cut| *cut <= t + PARAM_EPS)
            .clamp(1, cuts.len() - 1);
        (cuts[upper - 1], cuts[upper])
    };
    let mut replacement = Vec::new();
    for (i, part) in all[owner].iter().enumerate() {
        if i != index {
            replacement.push(part.piece(0.0, 1.0));
            continue;
        }
        if curve.closed() {
            if end - start < 1.0 - PARAM_EPS {
                replacement.push(curve.piece(end, start + 1.0));
            }
        } else {
            if start > PARAM_EPS {
                replacement.push(curve.piece(0.0, start));
            }
            if end < 1.0 - PARAM_EPS {
                replacement.push(curve.piece(end, 1.0));
            }
        }
    }
    Some(TrimPlan {
        entity: owner,
        replacement,
        removed: curve.piece(start, end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(a: Point2, b: Point2) -> SketchEntity {
        SketchEntity::Line { a, b }
    }
    #[test]
    fn trim_middle_and_tail_between_intersections() {
        let e = vec![
            line([0., 0.], [10., 0.]),
            line([3., -2.], [3., 2.]),
            line([7., -2.], [7., 2.]),
        ];
        let p = plan_trim(&e, [5., 0.1], 0.5).unwrap();
        assert_eq!(
            p.replacement,
            vec![line([0., 0.], [3., 0.]), line([7., 0.], [10., 0.])]
        );
        let p = plan_trim(&e, [9., 0.], 0.5).unwrap();
        assert_eq!(p.replacement, vec![line([0., 0.], [7., 0.])]);
    }
    #[test]
    fn trim_isolated_line_and_miss() {
        let e = vec![line([0., 0.], [10., 0.])];
        assert!(plan_trim(&e, [5., 0.], 0.5).unwrap().replacement.is_empty());
        assert!(plan_trim(&e, [5., 2.], 0.5).is_none());
    }
    #[test]
    fn trim_circle_preserves_analytic_arc_and_wraps_seam() {
        let e = vec![
            SketchEntity::Circle { c: [0., 0.], r: 5. },
            line([0., -8.], [0., 8.]),
        ];
        let p = plan_trim(&e, [5., 0.], 0.5).unwrap();
        assert_eq!(p.replacement.len(), 1);
        let SketchEntity::Arc { r, a0, a1, .. } = p.replacement[0] else {
            panic!()
        };
        assert_eq!(r, 5.);
        assert!((a1 - a0 - std::f64::consts::PI).abs() < EPS);
        assert!(a0.cos().abs() < EPS);
    }
    #[test]
    fn trim_rectangle_side_and_intersected_side() {
        let mut e = vec![SketchEntity::Rect {
            c1: [0., 0.],
            c2: [10., 8.],
        }];
        assert_eq!(plan_trim(&e, [5., 0.], 0.5).unwrap().replacement.len(), 3);
        e.push(line([4., -2.], [4., 2.]));
        let p = plan_trim(&e, [6., 0.], 0.5).unwrap();
        assert_eq!(p.replacement.len(), 4);
        assert_eq!(p.replacement[0], line([0., 0.], [4., 0.]));
    }
    #[test]
    fn extension_of_other_line_is_not_a_cut() {
        let e = vec![line([0., 0.], [10., 0.]), line([5., 2.], [5., 4.])];
        assert!(plan_trim(&e, [2., 0.], 0.5).unwrap().replacement.is_empty());
    }
    #[test]
    fn overlapping_lines_cut_at_overlap_endpoints() {
        let e = vec![line([0., 0.], [10., 0.]), line([3., 0.], [7., 0.])];
        assert_eq!(
            plan_trim(&e, [1., 0.], 0.5).unwrap().replacement,
            vec![line([3., 0.], [10., 0.])]
        );
    }
    #[test]
    fn clockwise_arc_and_tangent_line() {
        let e = vec![
            SketchEntity::Arc {
                c: [0., 0.],
                r: 5.,
                a0: std::f64::consts::PI,
                a1: 0.,
                ccw: false,
            },
            line([0., -8.], [0., 8.]),
        ];
        let p = plan_trim(&e, [3., 4.], 0.5).unwrap();
        assert!(matches!(
            p.replacement[0],
            SketchEntity::Arc { ccw: false, .. }
        ));
        let e = vec![
            line([-10., 5.], [10., 5.]),
            SketchEntity::Circle { c: [0., 0.], r: 5. },
        ];
        let p = plan_trim(&e, [-7., 5.], 0.5).unwrap();
        assert_eq!(p.replacement, vec![line([0., 5.], [10., 5.])]);
    }
    #[test]
    fn coincident_arc_does_not_add_a_fake_circle_seam() {
        let e = vec![
            SketchEntity::Circle { c: [0., 0.], r: 5. },
            SketchEntity::Arc {
                c: [0., 0.],
                r: 5.,
                a0: 0.5,
                a1: 1.0,
                ccw: true,
            },
        ];
        let p = plan_trim(&e, [5., 0.], 0.2).unwrap();
        let SketchEntity::Arc { a0, a1, .. } = p.replacement[0] else {
            panic!()
        };
        assert!((a1 - a0 - 0.5).abs() < EPS);
    }

    #[test]
    fn circle_circle_intersection_and_path_vertices() {
        let e = vec![
            SketchEntity::Circle { c: [0., 0.], r: 5. },
            SketchEntity::Circle { c: [6., 0.], r: 5. },
        ];
        let p = plan_trim(&e, [5., 0.], 0.5).unwrap();
        assert_eq!(p.replacement.len(), 1);
        let SketchEntity::Arc { a0, a1, .. } = p.replacement[0] else {
            panic!()
        };
        assert!(a1 - a0 > std::f64::consts::PI);
        let e = vec![SketchEntity::Path {
            contours: vec![(vec![[0., 0.], [5., 0.], [5., 5.]], false)],
            origin: [0., 0.],
            scale: 1.,
            rot: 0.,
            text: None,
        }];
        assert_eq!(
            plan_trim(&e, [3., 0.], 0.5).unwrap().replacement,
            vec![line([5., 0.], [5., 5.])]
        );
    }
}
