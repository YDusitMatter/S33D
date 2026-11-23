// SPDX-License-Identifier: MPL-2.0
//! Project-authored CAD tool pictograms. All marks are geometry, never font glyphs.
//!
//! Coordinates use a 20-unit artboard, with an 18-point display size. The same
//! paths work in light and dark themes and at fractional display scaling.
//! Source/provenance and the limits of the IP review: ../TOOL_ICONS.md.
//! Independent drawing is not a patent or trademark clearance opinion.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Response, Shape, Stroke, Ui, Vec2};

macro_rules! icons {
    ($($name:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum ToolIcon { $($name),+ }
        impl ToolIcon {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
            pub const fn name(self) -> &'static str {
                match self { $(Self::$name => stringify!($name)),+ }
            }
        }
    };
}

icons! {
    Move, Rotate, Scale, Align, Sketch, Pick, Trim, Line, Arc, Circle, Rect,
    Poly, Pen, Text, Exit, Draw3D, Box, Cylinder, Sphere, Cone, Torus, Equation,
    Extrude, Pocket, Revolve, Union, Subtract, Intersect, Split, Section,
    Mirror, Fillet, Chamfer, OffsetShell, Copy, Paste, Delete, Array, Group, Ungroup, Objects,
    Iso, Top, Front, Right, Fit, Recenter, EngineeringDrawing,
}

#[derive(Clone, Debug, PartialEq)]
struct Path {
    points: Vec<[f32; 2]>,
    closed: bool,
    fill: f32,
    stroke: f32,
}

#[derive(Default, Debug, PartialEq)]
struct Drawing(Vec<Path>);

impl Drawing {
    fn path(&mut self, points: &[[f32; 2]]) {
        self.0.push(Path {
            points: points.to_vec(),
            closed: false,
            fill: 0.0,
            stroke: 1.0,
        });
    }

    fn polygon(&mut self, points: &[[f32; 2]], fill: f32, stroke: f32) {
        self.0.push(Path {
            points: points.to_vec(),
            closed: true,
            fill,
            stroke,
        });
    }

    fn line(&mut self, a: [f32; 2], b: [f32; 2]) {
        self.path(&[a, b]);
    }

    fn rect(&mut self, min: [f32; 2], max: [f32; 2], fill: f32) {
        self.polygon(&[min, [max[0], min[1]], max, [min[0], max[1]]], fill, 1.0);
    }

    fn filled_rect(&mut self, min: [f32; 2], max: [f32; 2], fill: f32) {
        self.polygon(&[min, [max[0], min[1]], max, [min[0], max[1]]], fill, 0.0);
    }

    fn ellipse(&mut self, center: [f32; 2], radii: [f32; 2]) {
        self.ellipse_arc(center, radii, 0.0, std::f32::consts::TAU, true);
    }

    fn ellipse_arc(&mut self, center: [f32; 2], radii: [f32; 2], from: f32, to: f32, closed: bool) {
        let steps = ((to - from).abs() * 5.0).ceil().max(6.0) as usize;
        let points = (0..=steps)
            .map(|i| {
                let a = from + (to - from) * i as f32 / steps as f32;
                [
                    center[0] + radii[0] * a.cos(),
                    center[1] + radii[1] * a.sin(),
                ]
            })
            .collect();
        self.0.push(Path {
            points,
            closed,
            fill: 0.0,
            stroke: 1.0,
        });
    }

    fn arrow(&mut self, from: [f32; 2], to: [f32; 2], head: f32) {
        self.line(from, to);
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let length = dx.hypot(dy);
        let u = [dx / length, dy / length];
        self.path(&[
            [
                to[0] - head * u[0] + head * 0.65 * u[1],
                to[1] - head * u[1] - head * 0.65 * u[0],
            ],
            to,
            [
                to[0] - head * u[0] - head * 0.65 * u[1],
                to[1] - head * u[1] + head * 0.65 * u[0],
            ],
        ]);
    }

    fn dot(&mut self, point: [f32; 2]) {
        self.polygon(
            &[
                [point[0] - 0.85, point[1] - 0.85],
                [point[0] + 0.85, point[1] - 0.85],
                [point[0] + 0.85, point[1] + 0.85],
                [point[0] - 0.85, point[1] + 0.85],
            ],
            1.0,
            0.0,
        );
    }

    fn cube(&mut self) {
        self.polygon(
            &[
                [3.0, 6.0],
                [10.0, 2.5],
                [17.0, 6.0],
                [17.0, 14.0],
                [10.0, 17.5],
                [3.0, 14.0],
            ],
            0.0,
            1.0,
        );
        self.path(&[[3.0, 6.0], [10.0, 9.5], [17.0, 6.0]]);
        self.line([10.0, 9.5], [10.0, 17.5]);
    }
}

fn drawing(icon: ToolIcon) -> Drawing {
    use std::f32::consts::{FRAC_PI_2, PI};
    use ToolIcon::*;
    let mut d = Drawing::default();
    match icon {
        EngineeringDrawing => {
            d.rect([3.0, 2.0], [17.0, 18.0], 0.0);
            d.rect([5.0, 5.0], [10.0, 10.0], 0.0);
            d.ellipse([13.0, 7.5], [1.5, 2.5]);
            d.line([5.0, 13.0], [15.0, 13.0]);
            d.line([5.0, 11.5], [5.0, 14.5]);
            d.line([15.0, 11.5], [15.0, 14.5]);
        }
        Move => {
            for to in [[10.0, 2.0], [18.0, 10.0], [10.0, 18.0], [2.0, 10.0]] {
                d.arrow([10.0, 10.0], to, 2.7);
            }
        }
        Rotate => {
            d.ellipse_arc([10.0, 10.0], [6.7, 6.7], -2.45, 2.5, false);
            d.path(&[[3.1, 3.6], [4.9, 6.0], [7.5, 4.9]]);
            d.dot([10.0, 10.0]);
        }
        Scale => {
            d.rect([3.0, 10.0], [10.0, 17.0], 0.1);
            d.path(&[[3.0, 7.0], [3.0, 3.0], [7.0, 3.0]]);
            d.path(&[[13.0, 17.0], [17.0, 17.0], [17.0, 13.0]]);
            d.arrow([8.0, 12.0], [17.0, 3.0], 3.0);
        }
        Align => {
            d.line([3.0, 2.0], [3.0, 18.0]);
            d.rect([6.0, 4.0], [16.0, 8.0], 0.12);
            d.rect([6.0, 12.0], [12.0, 16.0], 0.12);
        }
        Sketch => {
            d.polygon(
                &[[2.0, 10.0], [9.0, 5.0], [18.0, 10.0], [11.0, 16.0]],
                0.08,
                1.0,
            );
            d.polygon(
                &[
                    [8.0, 11.0],
                    [9.0, 8.0],
                    [15.0, 2.0],
                    [18.0, 5.0],
                    [12.0, 11.0],
                ],
                0.15,
                1.0,
            );
            d.line([14.0, 3.0], [17.0, 6.0]);
        }
        Pick => {
            d.polygon(
                &[
                    [4.0, 2.0],
                    [16.0, 10.0],
                    [10.5, 11.0],
                    [13.0, 17.0],
                    [10.0, 18.0],
                    [7.5, 12.0],
                    [4.0, 16.0],
                ],
                0.14,
                1.0,
            );
        }
        Trim => {
            d.ellipse([4.3, 5.0], [2.3, 2.3]);
            d.ellipse([4.3, 15.0], [2.3, 2.3]);
            d.line([6.0, 6.5], [16.5, 16.0]);
            d.line([6.0, 13.5], [16.5, 4.0]);
            d.line([17.8, 8.0], [17.8, 12.0]);
        }
        Line => {
            d.line([4.0, 16.0], [16.0, 4.0]);
            d.dot([4.0, 16.0]);
            d.dot([16.0, 4.0]);
        }
        Arc => {
            d.ellipse_arc([4.0, 16.0], [12.0, 12.0], -FRAC_PI_2, 0.0, false);
            d.dot([4.0, 4.0]);
            d.dot([16.0, 16.0]);
        }
        Circle => {
            d.ellipse([10.0, 10.0], [6.5, 6.5]);
            d.dot([10.0, 10.0]);
        }
        Rect => {
            d.rect([3.0, 5.0], [17.0, 15.0], 0.0);
            d.dot([3.0, 5.0]);
            d.dot([17.0, 15.0]);
        }
        Poly => {
            d.polygon(
                &[
                    [10.0, 2.5],
                    [17.0, 7.5],
                    [14.5, 16.0],
                    [5.5, 16.0],
                    [3.0, 7.5],
                ],
                0.0,
                1.0,
            );
            d.dot([10.0, 2.5]);
        }
        Pen => {
            d.path(&[
                [2.5, 15.0],
                [4.0, 11.0],
                [6.0, 15.0],
                [8.0, 10.0],
                [10.0, 12.0],
            ]);
            d.polygon(
                &[
                    [11.0, 10.5],
                    [12.0, 6.5],
                    [16.0, 2.5],
                    [18.0, 4.5],
                    [14.0, 8.5],
                ],
                0.12,
                1.0,
            );
        }
        Text => {
            // A constructed typographic T: independent of installed font coverage.
            d.path(&[[3.0, 5.0], [3.0, 3.5], [17.0, 3.5], [17.0, 5.0]]);
            d.line([10.0, 3.5], [10.0, 16.5]);
            d.line([6.5, 16.5], [13.5, 16.5]);
        }
        Exit => {
            d.path(&[[9.0, 3.0], [3.0, 3.0], [3.0, 17.0], [9.0, 17.0]]);
            d.arrow([7.0, 10.0], [18.0, 10.0], 3.5);
        }
        Box | Iso => d.cube(),
        Draw3D => {
            d.polygon(
                &[
                    [2.0, 7.0],
                    [8.0, 4.0],
                    [14.0, 7.0],
                    [14.0, 14.0],
                    [8.0, 17.0],
                    [2.0, 14.0],
                ],
                0.0,
                1.0,
            );
            d.path(&[[2.0, 7.0], [8.0, 10.0], [14.0, 7.0]]);
            d.line([8.0, 10.0], [8.0, 17.0]);
            d.line([16.5, 1.5], [16.5, 6.5]);
            d.line([14.0, 4.0], [19.0, 4.0]);
        }
        Cylinder => {
            d.ellipse([10.0, 4.8], [6.5, 2.7]);
            d.line([3.5, 4.8], [3.5, 14.8]);
            d.line([16.5, 4.8], [16.5, 14.8]);
            d.ellipse_arc([10.0, 14.8], [6.5, 2.7], 0.0, PI, false);
        }
        Sphere => {
            d.ellipse([10.0, 10.0], [7.2, 7.2]);
            d.ellipse([10.0, 10.0], [3.1, 7.2]);
            d.ellipse([10.0, 10.0], [7.2, 2.7]);
        }
        Cone => {
            d.path(&[[3.2, 14.7], [10.0, 2.5], [16.8, 14.7]]);
            d.ellipse([10.0, 14.7], [6.8, 2.7]);
        }
        Torus => {
            d.ellipse([10.0, 10.0], [8.0, 5.7]);
            d.ellipse([10.0, 9.0], [3.8, 2.2]);
            d.ellipse_arc([10.0, 10.6], [5.4, 3.6], 0.0, PI, false);
        }
        Equation => {
            d.path(&[[2.0, 3.0], [2.0, 17.0], [18.0, 17.0]]);
            let points: Vec<_> = (0..=24)
                .map(|i| {
                    let x = 3.5 + i as f32 * 0.55;
                    [x, 10.0 - 4.5 * ((x - 3.5) * 0.45).sin()]
                })
                .collect();
            d.path(&points);
        }
        Extrude => {
            d.polygon(
                &[[2.0, 11.0], [8.0, 8.0], [14.0, 11.0], [8.0, 14.0]],
                0.14,
                1.0,
            );
            d.path(&[
                [2.0, 11.0],
                [2.0, 16.0],
                [8.0, 19.0],
                [14.0, 16.0],
                [14.0, 11.0],
            ]);
            d.line([8.0, 14.0], [8.0, 19.0]);
            d.arrow([8.0, 10.0], [8.0, 1.5], 2.8);
        }
        Pocket => {
            // A cutaway pocket with the cutter moving into its open cavity.
            // Avoid stacked isometric diamonds: they merge at 18 points.
            d.filled_rect([3.0, 5.0], [6.0, 17.0], 0.16);
            d.filled_rect([6.0, 12.0], [14.0, 17.0], 0.16);
            d.filled_rect([14.0, 5.0], [17.0, 17.0], 0.16);
            d.polygon(
                &[
                    [3.0, 5.0],
                    [6.0, 5.0],
                    [6.0, 12.0],
                    [14.0, 12.0],
                    [14.0, 5.0],
                    [17.0, 5.0],
                    [17.0, 17.0],
                    [3.0, 17.0],
                ],
                0.0,
                1.0,
            );
            d.arrow([10.0, 1.5], [10.0, 9.5], 2.7);
        }
        Revolve => {
            d.line([8.5, 2.0], [8.5, 18.0]);
            d.path(&[
                [8.5, 4.0],
                [13.5, 4.0],
                [16.0, 10.0],
                [13.5, 15.0],
                [8.5, 15.0],
            ]);
            d.ellipse_arc([8.5, 11.0], [6.0, 3.3], 0.65, 4.2, false);
            d.path(&[[3.0, 6.3], [5.6, 8.1], [3.6, 10.0]]);
        }
        Union | Subtract | Intersect => {
            // Overlapping blocks, using the actual result region, not +/- symbols.
            match icon {
                Union => {
                    d.filled_rect([3.0, 5.0], [12.0, 14.0], 0.28);
                    d.filled_rect([12.0, 2.0], [17.0, 11.0], 0.28);
                    d.filled_rect([8.0, 2.0], [12.0, 5.0], 0.28);
                }
                Subtract => {
                    d.filled_rect([3.0, 5.0], [8.0, 14.0], 0.35);
                    d.filled_rect([8.0, 11.0], [12.0, 14.0], 0.35);
                }
                Intersect => d.filled_rect([8.0, 5.0], [12.0, 11.0], 0.55),
                _ => unreachable!(),
            }
            d.rect([3.0, 5.0], [12.0, 14.0], 0.0);
            d.rect([8.0, 2.0], [17.0, 11.0], 0.0);
            if icon == Subtract {
                d.path(&[
                    [3.0, 5.0],
                    [8.0, 5.0],
                    [8.0, 11.0],
                    [12.0, 11.0],
                    [12.0, 14.0],
                    [3.0, 14.0],
                    [3.0, 5.0],
                ]);
            }
        }
        Split => {
            d.polygon(
                &[[2.0, 6.0], [8.0, 3.0], [8.0, 15.0], [2.0, 18.0]],
                0.12,
                1.0,
            );
            d.polygon(
                &[[12.0, 3.0], [18.0, 6.0], [18.0, 18.0], [12.0, 15.0]],
                0.12,
                1.0,
            );
            for y in [2.0, 6.0, 10.0, 14.0] {
                d.line([10.0, y], [10.0, y + 2.0]);
            }
        }
        Section => {
            d.polygon(
                &[
                    [3.0, 6.0],
                    [10.0, 3.0],
                    [17.0, 6.0],
                    [17.0, 15.0],
                    [3.0, 15.0],
                ],
                0.08,
                1.0,
            );
            d.rect([3.0, 9.0], [17.0, 15.0], 0.16);
            for x in [4.0, 8.0, 12.0] {
                d.line([x, 14.0], [x + 3.0, 10.0]);
            }
        }
        Mirror => {
            for y in [2.0, 6.0, 10.0, 14.0] {
                d.line([10.0, y], [10.0, y + 2.0]);
            }
            d.polygon(&[[3.0, 5.0], [7.0, 9.0], [3.0, 15.0]], 0.15, 1.0);
            d.polygon(&[[17.0, 5.0], [13.0, 9.0], [17.0, 15.0]], 0.15, 1.0);
        }
        Fillet => {
            d.path(&[[3.0, 17.0], [3.0, 9.0]]);
            d.ellipse_arc([9.0, 9.0], [6.0, 6.0], PI, PI * 1.5, false);
            d.line([9.0, 3.0], [17.0, 3.0]);
            d.path(&[[3.0, 17.0], [17.0, 17.0], [17.0, 3.0]]);
        }
        Chamfer => {
            d.polygon(
                &[
                    [3.0, 17.0],
                    [3.0, 9.0],
                    [9.0, 3.0],
                    [17.0, 3.0],
                    [17.0, 17.0],
                ],
                0.0,
                1.0,
            );
            d.line([5.0, 11.0], [11.0, 5.0]);
        }
        OffsetShell => {
            d.path(&[[3.0, 3.0], [3.0, 17.0], [17.0, 17.0], [17.0, 3.0]]);
            d.path(&[[6.0, 3.0], [6.0, 14.0], [14.0, 14.0], [14.0, 3.0]]);
            d.line([3.0, 3.0], [6.0, 3.0]);
            d.line([14.0, 3.0], [17.0, 3.0]);
            d.arrow([10.0, 10.0], [10.0, 3.0], 2.3);
        }
        Copy => {
            d.path(&[
                [7.0, 5.0],
                [7.0, 2.0],
                [17.0, 2.0],
                [17.0, 14.0],
                [14.0, 14.0],
            ]);
            d.rect([3.0, 6.0], [13.0, 18.0], 0.08);
        }
        Paste => {
            d.path(&[
                [6.0, 4.0],
                [3.0, 4.0],
                [3.0, 18.0],
                [17.0, 18.0],
                [17.0, 4.0],
                [14.0, 4.0],
            ]);
            d.rect([6.0, 2.0], [14.0, 6.0], 0.12);
            d.path(&[
                [7.0, 10.0],
                [13.0, 10.0],
                [13.0, 15.0],
                [7.0, 15.0],
                [7.0, 10.0],
            ]);
        }
        Delete => {
            d.path(&[[4.0, 6.0], [5.0, 17.5], [15.0, 17.5], [16.0, 6.0]]);
            d.line([2.5, 5.0], [17.5, 5.0]);
            d.path(&[[7.0, 4.5], [7.0, 2.5], [13.0, 2.5], [13.0, 4.5]]);
            d.line([8.0, 8.0], [8.0, 14.5]);
            d.line([12.0, 8.0], [12.0, 14.5]);
        }
        Array => {
            for x in [3.0, 11.0] {
                for y in [3.0, 11.0] {
                    d.rect([x, y], [x + 5.0, y + 5.0], 0.12);
                }
            }
        }
        Group => {
            d.rect([5., 5.], [11., 11.], 0.15);
            d.rect([9., 9.], [15., 15.], 0.15);
            d.path(&[[6., 2.], [2., 2.], [2., 6.]]);
            d.path(&[[14., 2.], [18., 2.], [18., 6.]]);
            d.path(&[[2., 14.], [2., 18.], [6., 18.]]);
            d.path(&[[14., 18.], [18., 18.], [18., 14.]]);
        }
        Ungroup => {
            d.rect([2., 3.], [8., 9.], 0.15);
            d.rect([12., 11.], [18., 17.], 0.15);
            d.arrow([10., 9.], [15., 4.], 2.);
            d.arrow([10., 11.], [5., 16.], 2.);
        }
        Objects => {
            d.line([3.5, 3.0], [3.5, 16.5]);
            for y in [4.0, 10.0, 16.0] {
                d.line([3.5, y], [7.0, y]);
                d.rect([8.5, y - 1.8], [16.5, y + 1.8], 0.1);
            }
        }
        Top | Front | Right => {
            // Same axonometric cube; the named orthographic face is shaded.
            let face: &[[f32; 2]] = match icon {
                Top => &[[3.0, 6.0], [10.0, 2.5], [17.0, 6.0], [10.0, 9.5]],
                Front => &[[3.0, 6.0], [10.0, 9.5], [10.0, 17.5], [3.0, 14.0]],
                _ => &[[10.0, 9.5], [17.0, 6.0], [17.0, 14.0], [10.0, 17.5]],
            };
            d.polygon(face, 0.35, 0.0);
            d.cube();
        }
        Fit => {
            for (x, y, sx, sy) in [
                (3.0, 3.0, 1.0, 1.0),
                (17.0, 3.0, -1.0, 1.0),
                (3.0, 17.0, 1.0, -1.0),
                (17.0, 17.0, -1.0, -1.0),
            ] {
                d.path(&[[x, y + 4.0 * sy], [x, y], [x + 4.0 * sx, y]]);
            }
            d.rect([7.0, 7.0], [13.0, 13.0], 0.1);
        }
        Recenter => {
            d.ellipse([10.0, 10.0], [4.5, 4.5]);
            d.line([10.0, 1.5], [10.0, 6.5]);
            d.line([10.0, 13.5], [10.0, 18.5]);
            d.line([1.5, 10.0], [6.5, 10.0]);
            d.line([13.5, 10.0], [18.5, 10.0]);
            d.dot([10.0, 10.0]);
        }
    }
    d
}

/// Paint an icon centered in `rect`, with the supplied foreground color.
/// A square 18-point rect is recommended; non-square rects preserve aspect ratio.
pub fn paint(painter: &Painter, rect: &Rect, icon: ToolIcon, color: Color32) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let side = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(side * 0.5);
    let scale = side / 20.0;
    let width = 1.3 * side / 18.0;
    for path in drawing(icon).0 {
        let points: Vec<Pos2> = path
            .points
            .into_iter()
            .map(|p| origin + egui::vec2(p[0], p[1]) * scale)
            .collect();
        if path.fill > 0.0 {
            painter.add(Shape::convex_polygon(
                points.clone(),
                color.gamma_multiply(path.fill),
                Stroke::NONE,
            ));
        }
        if path.stroke > 0.0 {
            let stroke = Stroke::new(width * path.stroke, color);
            painter.add(if path.closed {
                Shape::closed_line(points, stroke)
            } else {
                Shape::line(points, stroke)
            });
        }
    }
}

/// Compact, accessible button whose icon and label share the interaction color.
/// The ordinary egui label remains available to screen readers and automation.
pub fn icon_button(
    ui: &mut Ui,
    label: &str,
    icon: ToolIcon,
    selected: bool,
    enabled: bool,
    min_size: Vec2,
) -> Response {
    ui.add_enabled(enabled, |ui: &mut Ui| {
        let padding = ui.spacing().button_padding;
        let side = 18.0;
        let gap = 5.0;
        let text: egui::WidgetText = label.to_owned().into();
        let text_width = (ui.available_width() - 2.0 * padding.x - side - gap).max(1.0);
        let galley = text.into_galley(ui, None, text_width, egui::TextStyle::Button);
        let content_size = egui::vec2(side + gap + galley.size().x, side.max(galley.size().y));
        let desired = (content_size + padding * 2.0).max(min_size);
        let (rect, response) = ui.allocate_at_least(desired, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);
            let (fill, stroke) = if selected {
                // Ui::add_enabled already fades the Painter when disabled.
                // Keep these source colors intact to avoid fading twice.
                (
                    ui.visuals().selection.bg_fill,
                    ui.visuals().selection.stroke,
                )
            } else {
                (visuals.weak_bg_fill, visuals.bg_stroke)
            };
            ui.painter().rect(
                rect.expand(visuals.expansion),
                visuals.corner_radius,
                fill,
                stroke,
                egui::StrokeKind::Inside,
            );
            let color = if selected {
                ui.visuals().selection.stroke.color
            } else {
                visuals.text_color()
            };
            let start = egui::pos2(rect.min.x + padding.x, rect.center().y);
            let icon_rect =
                Rect::from_center_size(start + egui::vec2(side * 0.5, 0.0), Vec2::splat(side));
            // Keep wrapped labels inside a narrow child-tool button, including
            // unusual translated labels or users' enlarged UI font settings.
            let content_painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            paint(&content_painter, &icon_rect, icon, color);
            content_painter.galley(
                egui::pos2(
                    start.x + side + gap,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                color,
            );
        }
        response
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_finite_geometry_inside_its_artboard() {
        for &icon in ToolIcon::ALL {
            let d = drawing(icon);
            assert!(!d.0.is_empty(), "{} is empty", icon.name());
            for path in d.0 {
                assert!(
                    path.points.len() >= 2,
                    "{} has a degenerate path",
                    icon.name()
                );
                for [x, y] in path.points {
                    assert!(
                        x.is_finite() && y.is_finite(),
                        "{} has non-finite coordinates",
                        icon.name()
                    );
                    assert!(
                        (0.5..=19.5).contains(&x) && (0.5..=19.5).contains(&y),
                        "{} exceeds artboard at ({x}, {y})",
                        icon.name()
                    );
                }
            }
        }
    }

    #[test]
    fn easily_confused_cad_operations_have_distinct_geometry() {
        for (a, b) in [
            (ToolIcon::Union, ToolIcon::Subtract),
            (ToolIcon::Union, ToolIcon::Intersect),
            (ToolIcon::Subtract, ToolIcon::Intersect),
            (ToolIcon::Extrude, ToolIcon::Pocket),
            (ToolIcon::Fillet, ToolIcon::Chamfer),
            (ToolIcon::Cylinder, ToolIcon::Delete),
            (ToolIcon::Split, ToolIcon::Section),
            (ToolIcon::Move, ToolIcon::Scale),
            (ToolIcon::Sphere, ToolIcon::Torus),
            (ToolIcon::Top, ToolIcon::Front),
            (ToolIcon::Front, ToolIcon::Right),
            (ToolIcon::Fit, ToolIcon::Recenter),
        ] {
            assert_ne!(
                drawing(a),
                drawing(b),
                "{} and {} must be distinguishable",
                a.name(),
                b.name()
            );
        }
    }

    #[test]
    fn boolean_regions_encode_result_area() {
        let filled_area = |icon| {
            drawing(icon)
                .0
                .into_iter()
                .filter(|p| p.fill > 0.0)
                .map(|p| {
                    // Shoelace area; these are nonoverlapping rectangular result pieces.
                    p.points
                        .iter()
                        .zip(p.points.iter().cycle().skip(1))
                        .take(p.points.len())
                        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
                        .sum::<f32>()
                        .abs()
                        * 0.5
                })
                .sum::<f32>()
        };
        assert_eq!(filled_area(ToolIcon::Union), 138.0);
        assert_eq!(filled_area(ToolIcon::Subtract), 57.0);
        assert_eq!(filled_area(ToolIcon::Intersect), 24.0);
    }

    #[test]
    fn disabled_selected_buttons_use_egui_painter_fading() {
        for dark in [false, true] {
            let render = |enabled| {
                let context = egui::Context::default();
                context.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                let mut button_rect = Rect::NOTHING;
                let mut is_enabled = false;
                let output = context.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = icon_button(
                            ui,
                            "Selected",
                            ToolIcon::Box,
                            true,
                            enabled,
                            egui::vec2(100.0, 26.0),
                        );
                        button_rect = response.rect;
                        is_enabled = response.enabled();
                    });
                });
                let button_fill = output
                    .shapes
                    .into_iter()
                    .find_map(|clipped| match clipped.shape {
                        Shape::Rect(shape) if shape.rect == button_rect => Some(shape.fill),
                        _ => None,
                    })
                    .expect("button frame must be painted");
                (button_fill, is_enabled)
            };
            let (enabled_color, enabled) = render(true);
            let (disabled_color, disabled) = render(false);
            assert!(enabled);
            assert!(!disabled);
            assert_ne!(
                enabled_color, disabled_color,
                "selected buttons must visibly fade when disabled"
            );
        }
    }
}
