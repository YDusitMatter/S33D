// SPDX-License-Identifier: MPL-2.0
//! Feature picking in drawing coordinates; never modifies source geometry.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Round {
    pub center: Vec2,
    pub radius: f32,
    pub arc: Option<(f32, f32)>,
    pub major: Vec2,
    pub minor_ratio: f32,
}

pub(super) fn rounds(d: &Drawing, view: usize) -> Vec<Round> {
    let mut out = Vec::<Round>::new();
    for t in d.components.iter().flat_map(|c| &c.cylinders) {
        if let Some(rim) = features::rim(t, view) {
            let center = rim.center;
            let radius = rim.radius;
            if !out
                .iter()
                .any(|r| r.center.distance(center) < 1e-3 && (r.radius - radius).abs() < 1e-3)
            {
                out.push(rim);
                let end = projected(Vec3::from_array(t.axis.map(|v| v as f32)), view).truncate()
                    * t.length as f32;
                if end.length() > 1e-3 {
                    out.push(Round {
                        center: center + end,
                        ..rim
                    });
                }
            }
        }
    }
    // Closed polygonal circular rims may come from imported mesh geometry.
    // Fit only complete isolated loops with small radial residual, never a box
    // or an arbitrary set of three points. These remain mesh measurements.
    let v = &d.views[view];
    let eps = ((v.max - v.min).length() * 1e-6).max(1e-5);
    let mut remaining = v.lines.clone();
    // Visible complete rims only; trusted hidden bores already come from metadata.
    // Avoid fitting dense threaded tessellation into spurious circles.
    if remaining.len() > 10_000 {
        return out;
    }
    while let Some([a, b]) = remaining.pop() {
        let mut chain = vec![a, b];
        for direction in 0..2 {
            if direction == 1 {
                if chain.last().unwrap().distance(chain[0]) < eps {
                    break;
                }
                chain.reverse();
            }
            loop {
                let last = *chain.last().unwrap();
                let candidates: Vec<_> = remaining
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| {
                        if l[0].distance(last) < eps {
                            Some((i, l[1]))
                        } else if l[1].distance(last) < eps {
                            Some((i, l[0]))
                        } else {
                            None
                        }
                    })
                    .collect();
                if candidates.len() != 1 {
                    break;
                }
                let (index, next) = candidates[0];
                let before = last - chain[chain.len() - 2];
                let after = next - last;
                let ratio = before.length() / after.length().max(1e-9);
                if before.normalize_or_zero().dot(after.normalize_or_zero()) < 0.94
                    || !(0.25..4.).contains(&ratio)
                {
                    break;
                }
                remaining.swap_remove(index);
                chain.push(next);
                if next.distance(chain[0]) < eps || chain.len() > 4096 {
                    break;
                }
            }
        }
        if chain.len() < 6 {
            continue;
        }
        let closed = chain.last().unwrap().distance(chain[0]) < eps;
        if closed {
            chain.pop();
        }
        let first = chain[0];
        let mid = chain[chain.len() / 2];
        let last = chain[chain.len() - 1];
        let ab = mid - first;
        let ac = last - first;
        let denominator = 2. * cross(ab, ac);
        if denominator.abs() < eps * eps {
            continue;
        }
        let center = first
            + Vec2::new(
                ac.y * ab.length_squared() - ab.y * ac.length_squared(),
                ab.x * ac.length_squared() - ac.x * ab.length_squared(),
            ) / denominator;
        let radius = first.distance(center);
        if radius <= eps
            || radius > (v.max - v.min).length() * 10.
            || chain
                .iter()
                .any(|p| (p.distance(center) - radius).abs() > radius * 0.002 + eps)
        {
            continue;
        }
        let start = (first - center).y.atan2((first - center).x);
        let sweep: f32 = chain
            .windows(2)
            .map(|w| {
                let a = w[0] - center;
                let b = w[1] - center;
                cross(a, b).atan2(a.dot(b))
            })
            .sum();
        if !closed && sweep.abs() < 0.5 {
            continue;
        }
        if !out
            .iter()
            .any(|r| r.center.distance(center) < eps * 10. && (r.radius - radius).abs() < eps * 10.)
        {
            out.push(Round {
                center,
                radius,
                arc: if closed { None } else { Some((start, sweep)) },
                major: Vec2::X,
                minor_ratio: 1.,
            });
        }
    }
    out
}

impl Round {
    fn point(&self, angle: f32) -> Vec2 {
        self.center
            + self.radius
                * (self.major * angle.cos() + self.major.perp() * self.minor_ratio * angle.sin())
    }
    fn nearest_point(&self, p: Vec2) -> Vec2 {
        let d = (p - self.center) / self.radius;
        let x = d.dot(self.major);
        let y = d.dot(self.major.perp());
        self.point((y / self.minor_ratio).atan2(x))
    }
    fn contains(&self, p: Vec2) -> bool {
        self.arc.is_none_or(|(start, sweep)| {
            let angle = (p - self.center).y.atan2((p - self.center).x);
            let delta = if sweep >= 0. {
                (angle - start).rem_euclid(std::f32::consts::TAU)
            } else {
                (start - angle).rem_euclid(std::f32::consts::TAU)
            };
            delta <= sweep.abs() + 1e-4
        })
    }
}
#[derive(Clone, Debug)]
enum Pick {
    Point(Vec2),
    Edge([Vec2; 2]),
    Circle(Round),
}
fn nearest(d: &Drawing, view: usize, p: Vec2, tolerance: f32) -> Option<Pick> {
    let v = &d.views[view];
    let circles = d.rounds.get(view).map(Vec::as_slice).unwrap_or(&[]);
    let circular = matches!(d.kind, DimensionKind::Diameter | DimensionKind::Radius);
    let mut best = tolerance;
    let mut hit = None;
    for c in circles {
        if !c.contains(p) {
            continue;
        }
        let distance = p.distance(c.nearest_point(p));
        if distance < best {
            best = distance;
            hit = Some(Pick::Circle(*c));
        }
    }
    if circular || (d.kind == DimensionKind::Smart && hit.is_some()) {
        return hit;
    }
    // For linear measurements, endpoints and true circle centers take priority.
    let mut points = v.snaps.clone();
    if d.show_hidden {
        points.extend(v.hidden.iter().flatten().copied());
    }
    for point in points.into_iter().chain(circles.iter().map(|c| c.center)) {
        let distance = point.distance(p);
        if distance < best {
            best = distance;
            hit = Some(Pick::Point(point));
        }
    }
    // Circular rims are selectable as circles, not individual tessellation chords.
    for &[a, b] in v.lines.iter().chain(if d.show_hidden {
        v.hidden.iter()
    } else {
        [].iter()
    }) {
        if circles.iter().any(|c| {
            (a.distance(c.center) - c.radius).abs() < tolerance * 0.05
                && (b.distance(c.center) - c.radius).abs() < tolerance * 0.05
        }) {
            continue;
        }
        let delta = b - a;
        let q = a + delta * ((p - a).dot(delta) / delta.length_squared().max(1e-12)).clamp(0., 1.);
        let distance = p.distance(q);
        if distance < best {
            best = distance;
            hit = Some(Pick::Edge([a, b]));
        }
    }
    hit
}

pub(super) fn canvas(
    d: &mut Drawing,
    ui: &egui::Ui,
    response: &egui::Response,
    origin: egui::Pos2,
    zoom: f32,
    inset: egui::Rect,
    unit: UiUnit,
    zh: bool,
) {
    let Some(pointer) = response.hover_pos() else {
        return;
    };
    if inset.contains(pointer) {
        return;
    }
    let scale = fit(&d.views);
    let page = Vec2::new((pointer.x - origin.x) / zoom, (pointer.y - origin.y) / zoom);
    let screen = |p: Vec2| origin + egui::vec2(p.x, p.y) * zoom;
    let orange = Color32::from_rgb(220, 115, 10);
    let paint = ui.painter();
    if d.pending.is_empty() {
        d.pending_kind = None;
    }
    let mut hovered = None;
    if d.pending.len() < 2 {
        let mut distance = 10. / (zoom * scale);
        for view in 0..3 {
            if d.pending.first().is_some_and(|(v, _)| *v != view) {
                continue;
            }
            let v = &d.views[view];
            let p = (page - centers(view)) / scale * Vec2::new(1., -1.) + (v.min + v.max) * 0.5;
            if let Some(hit) = nearest(d, view, p, distance) {
                // Sheet view rectangles are separated; retain the nearest feature.
                let point = match &hit {
                    Pick::Point(p) => *p,
                    Pick::Edge([a, b]) => (*a + *b) * 0.5,
                    Pick::Circle(c) => c.nearest_point(p),
                };
                distance = distance.min(point.distance(p).max(1e-6));
                hovered = Some((view, hit));
            }
        }
    }
    if let Some((view, hit)) = &hovered {
        let pos = |p| screen(sheet_point(p, *view, &d.views[*view], scale));
        match hit {
            Pick::Point(p) => {
                paint.circle_filled(pos(*p), 4., orange);
            }
            Pick::Edge([a, b]) => {
                paint.line_segment([pos(*a), pos(*b)], Stroke::new(2., orange));
            }
            Pick::Circle(c) => {
                let (start, sweep) = c.arc.unwrap_or((0., std::f32::consts::TAU));
                let points = (0..=96)
                    .map(|i| pos(c.point(start + sweep * i as f32 / 96.)))
                    .collect();
                paint.add(egui::Shape::line(points, Stroke::new(2., orange)));
            }
        }
    }
    if d.pending.len() == 2 {
        let (view, a) = d.pending[0];
        let b = d.pending[1].1;
        let v = &d.views[view];
        let label = (page - centers(view)) / scale * Vec2::new(1., -1.) + (v.min + v.max) * 0.5;
        let dim = Dimension {
            view,
            a,
            b,
            label,
            kind: d.pending_kind.unwrap_or(d.kind),
        };
        let (_, lines) = dimension_lines(&dim);
        for l in lines {
            paint.line_segment(
                l.map(|p| screen(sheet_point(p, view, v, scale))),
                Stroke::new(1., orange),
            );
        }
        paint.text(
            pointer + egui::vec2(10., -12.),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{}{}",
                match dim.kind {
                    DimensionKind::Radius => "R",
                    DimensionKind::Diameter => "Ø",
                    _ => "",
                },
                unit.format_mm(dim.value())
            ),
            egui::FontId::proportional(13.),
            orange,
        );
        if response.clicked() && dim.value() > 1e-6 {
            d.checkpoint();
            d.dimensions.push(dim);
            d.pending.clear();
            d.pending_kind = None;
        }
    } else if response.clicked() {
        if let Some((view, hit)) = hovered {
            match hit {
                Pick::Point(p) => {
                    if d.pending
                        .last()
                        .is_none_or(|(_, last)| last.distance(p) > 1e-6)
                    {
                        d.pending.push((view, p));
                    }
                    if d.pending.len() == 2 && d.kind == DimensionKind::Smart {
                        let delta = (d.pending[1].1 - d.pending[0].1).abs();
                        d.pending_kind = Some(if delta.x >= delta.y {
                            DimensionKind::Horizontal
                        } else {
                            DimensionKind::Vertical
                        });
                    }
                }
                Pick::Edge([a, b]) => {
                    if d.pending.is_empty() {
                        d.pending = vec![(view, a), (view, b)];
                        d.pending_kind = Some(if d.kind == DimensionKind::Smart {
                            if (a.y - b.y).abs() < 1e-4 {
                                DimensionKind::Horizontal
                            } else if (a.x - b.x).abs() < 1e-4 {
                                DimensionKind::Vertical
                            } else {
                                DimensionKind::Aligned
                            }
                        } else {
                            d.kind
                        });
                    }
                }
                Pick::Circle(c) => {
                    if matches!(
                        d.kind,
                        DimensionKind::Smart | DimensionKind::Diameter | DimensionKind::Radius
                    ) && d.pending.is_empty()
                    {
                        let kind = if d.kind == DimensionKind::Radius
                            || (d.kind == DimensionKind::Smart && c.arc.is_some())
                        {
                            DimensionKind::Radius
                        } else {
                            DimensionKind::Diameter
                        };
                        let v = &d.views[view];
                        let clicked = (page - centers(view)) / scale * Vec2::new(1., -1.)
                            + (v.min + v.max) * 0.5;
                        // The major axis of a projected circular rim retains the true
                        // diameter. Never dimension the foreshortened ellipse chord.
                        let direction = if c.minor_ratio < 0.99999 {
                            c.major
                        } else {
                            (clicked - c.center).normalize_or_zero()
                        };
                        let a = if kind == DimensionKind::Radius {
                            c.center
                        } else {
                            c.center - direction * c.radius
                        };
                        d.pending = vec![(view, a), (view, c.center + direction * c.radius)];
                        d.pending_kind = Some(kind);
                    } else if d
                        .pending
                        .last()
                        .is_none_or(|(_, p)| p.distance(c.center) > 1e-6)
                    {
                        d.pending.push((view, c.center));
                    }
                }
            }
        } else {
            d.message = if zh {
                "请点击高亮的边、端点、圆或圆心。"
            } else {
                "Select a highlighted edge, endpoint, circle or center."
            }
            .into();
        }
    }
    for &(view, p) in &d.pending {
        paint.circle_filled(
            screen(sheet_point(p, view, &d.views[view], scale)),
            3.,
            orange,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclined_ellipse_pick_dimensions_true_diameter_from_minor_side() {
        let mut d = Drawing {
            views: vec![
                Projection {
                    min: Vec2::splat(-10.),
                    max: Vec2::splat(10.),
                    ..Default::default()
                };
                3
            ],
            manual: true,
            kind: DimensionKind::Diameter,
            open: true,
            ..Default::default()
        };
        let target = crate::threads::Target {
            body_id: 1,
            face_id: 0,
            origin: [0.; 3],
            axis: [0.8, 0., 0.6],
            radius: 4.,
            length: 8.,
            internal: true,
        };
        d.components.push(Component {
            cylinders: vec![target],
            ..Default::default()
        });
        d.rounds = (0..3).map(|v| rounds(&d, v)).collect();
        let r = d.rounds[0][0];
        assert!((r.minor_ratio - 0.6).abs() < 1e-6);
        let ctx = egui::Context::default();
        click(&ctx, &mut d, r.point(std::f32::consts::FRAC_PI_2));
        assert_eq!(d.pending.len(), 2);
        assert!((d.pending[0].1.distance(d.pending[1].1) - 8.).abs() < 1e-5);
        click(&ctx, &mut d, Vec2::new(6., 7.));
        assert_eq!(d.dimensions.len(), 1);
        assert!((d.dimensions[0].value() - 8.).abs() < 1e-5);
    }
    fn click(ctx: &egui::Context, d: &mut Drawing, model: Vec2) {
        let mut pos = egui::Pos2::ZERO;
        for phase in 0..3 {
            let events = match phase {
                1 => vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                2 => vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => vec![],
            };
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
                        let origin = ui.max_rect().center() - egui::vec2(297., 210.) * zoom * 0.5;
                        let page = sheet_point(model, 0, &d.views[0], fit(&d.views));
                        pos = origin + egui::vec2(page.x, page.y) * zoom;
                        super::super::canvas(d, ui, UiUnit::Millimeter, false);
                    });
                },
            );
        }
    }
    #[test]
    fn real_pointer_selects_features_then_places_each_dimension_tool() {
        for (kind, expected, point, value) in [
            (
                DimensionKind::Smart,
                DimensionKind::Diameter,
                Vec2::new(17., 8.),
                8.,
            ),
            (
                DimensionKind::Diameter,
                DimensionKind::Diameter,
                Vec2::new(17., 8.),
                8.,
            ),
            (
                DimensionKind::Radius,
                DimensionKind::Radius,
                Vec2::new(17., 8.),
                4.,
            ),
            (
                DimensionKind::Horizontal,
                DimensionKind::Horizontal,
                Vec2::new(10., 0.),
                20.,
            ),
            (
                DimensionKind::Vertical,
                DimensionKind::Vertical,
                Vec2::new(0., 10.),
                20.,
            ),
            (
                DimensionKind::Aligned,
                DimensionKind::Aligned,
                Vec2::new(10., 0.),
                20.,
            ),
        ] {
            let ctx = egui::Context::default();
            let v = Projection {
                min: Vec2::ZERO,
                max: Vec2::splat(30.),
                lines: vec![[Vec2::ZERO, Vec2::X * 20.], [Vec2::ZERO, Vec2::Y * 20.]],
                ..Default::default()
            };
            let mut d = Drawing {
                views: vec![v; 3],
                manual: true,
                kind,
                rounds: vec![
                    vec![Round {
                        center: Vec2::new(13., 8.),
                        radius: 4.,
                        arc: None,
                        major: Vec2::X,
                        minor_ratio: 1.,
                    }];
                    3
                ],
                ..Default::default()
            };
            click(&ctx, &mut d, point);
            assert_eq!(d.pending.len(), 2, "{kind:?} must select a whole feature");
            click(&ctx, &mut d, Vec2::new(24., 24.));
            assert_eq!(d.dimensions.len(), 1, "{kind:?}");
            assert_eq!(d.dimensions[0].kind, expected);
            assert!((d.dimensions[0].value() - value).abs() < 1e-4);
            d.undo();
            assert!(d.dimensions.is_empty());
            d.redo();
            assert_eq!(d.dimensions.len(), 1);
        }
    }
    #[test]
    fn imported_circle_and_open_arc_are_recognized_without_false_box_circles() {
        for sweep in [std::f32::consts::TAU, std::f32::consts::PI * 0.75] {
            let center = Vec2::new(13., 8.);
            let radius = 4.;
            let points: Vec<_> = (0..=64)
                .map(|i| center + Vec2::from_angle(sweep * i as f32 / 64.) * radius)
                .collect();
            let mut d = Drawing {
                views: vec![Projection {
                    min: Vec2::ZERO,
                    max: Vec2::splat(30.),
                    lines: points.windows(2).map(|p| [p[0], p[1]]).collect(),
                    ..Default::default()
                }],
                ..Default::default()
            };
            let found = rounds(&d, 0);
            assert!(!found.is_empty());
            assert!(
                found[0].center.distance(center) < 1e-3 && (found[0].radius - radius).abs() < 1e-3
            );
            d.rounds = vec![found];
            d.kind = DimensionKind::Smart;
            assert!(matches!(
                nearest(&d, 0, points[3], 0.1),
                Some(Pick::Circle(_))
            ));
            d.kind = DimensionKind::Radius;
            assert!(matches!(
                nearest(&d, 0, center + Vec2::from_angle(0.4) * radius, 0.1),
                Some(Pick::Circle(_))
            ));
            if sweep < std::f32::consts::TAU {
                assert!(nearest(&d, 0, center - Vec2::Y * radius, 0.1).is_none());
            }
        }
        let d = Drawing {
            views: vec![Projection {
                min: Vec2::ZERO,
                max: Vec2::splat(10.),
                lines: vec![
                    [Vec2::ZERO, Vec2::X * 10.],
                    [Vec2::X * 10., Vec2::splat(10.)],
                    [Vec2::splat(10.), Vec2::Y * 10.],
                    [Vec2::Y * 10., Vec2::ZERO],
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(rounds(&d, 0).is_empty());
    }
}
