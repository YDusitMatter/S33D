// SPDX-License-Identifier: MPL-2.0
//! Dimension provenance belongs to a generated snapshot, never live selection.
use super::*;
use crate::threads::Target;
use s33d_model::{Body, Feature};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Component {
    pub id: u64,
    pub name: String,
    pub min: Vec3,
    pub max: Vec3,
    pub cylinders: Vec<Target>,
    pub(super) notes: Vec<(
        std::borrow::Cow<'static, str>,
        std::borrow::Cow<'static, str>,
        f64,
    )>,
    pub(super) thread: Option<[f64; 5]>,
    pub thread_annotations: Vec<crate::threads::ThreadAnnotation>,
}
pub(super) fn component(app: &S33DApp, body: &Body, mesh: &MeshData) -> Component {
    let (min, max) = mesh.bounds().unwrap_or_default();
    let mut c = Component {
        id: body.id,
        name: body.name.clone(),
        min,
        max,
        // Query analytic/provenance cylinders. Do not label a polygonal STL as
        // a precision circular hole merely because a circle can be fitted.
        cylinders: app.thread_targets(&[body.id]).unwrap_or_default(),
        ..Default::default()
    };
    match &body.feature {
        Feature::Chamfer {
            chamfer, fillet, ..
        } => {
            if let Some(v) = chamfer {
                c.notes.push(("Chamfer".into(), "倒角".into(), *v));
            }
            if let Some(v) = fillet {
                c.notes.push(("Fillet R".into(), "圆角 R".into(), *v));
            }
        }
        Feature::ExactOffset { distance, .. } => {
            c.notes
                .push(("Offset / shell".into(), "偏移 / 壳厚".into(), *distance))
        }
        Feature::Extrude { params, .. } => {
            c.notes
                .push(("Extrusion depth".into(), "拉伸深度".into(), params.depth))
        }
        Feature::Primitive3D { params } => {
            use s33d_model::Primitive3DKind::*;
            match params.kind {
                Sphere => c
                    .notes
                    .push(("Sphere R".into(), "球半径 R".into(), params.radius)),
                Cone => {
                    c.notes
                        .push(("Base R".into(), "底部半径 R".into(), params.radius));
                    c.notes
                        .push(("Top R".into(), "顶部半径 R".into(), params.top_radius));
                    c.notes
                        .push(("Axial height".into(), "轴向高度".into(), params.height));
                }
                Torus => {
                    c.notes
                        .push(("Major R".into(), "主半径 R".into(), params.major_radius));
                    c.notes
                        .push(("Minor R".into(), "管半径 R".into(), params.minor_radius));
                }
                _ => {}
            }
        }
        Feature::Derived { source, .. } => {
            if let Some(saved) = source
                .strip_prefix("Thread v1: ")
                .and_then(|s| serde_json::from_str::<crate::threads::Saved>(s).ok())
            {
                if crate::threads::mesh_signature(mesh) == saved.mesh_signature {
                    c.thread_annotations = saved.annotations;
                    let p = saved.params;
                    c.thread = Some([p.diameter, p.pitch, p.length, p.offset, p.clearance]);
                    if p.radial_fit > 0. {
                        c.notes.push((
                            "Radial mating allowance / part".into(),
                            "每件径向配合余量".into(),
                            p.radial_fit,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    c
}

fn distinct(values: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut v: Vec<_> = values.filter(|v| v.is_finite()).collect();
    v.sort_by(f32::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    v
}
pub(super) fn circle(c: &Target, view: usize) -> Option<(Vec2, f32)> {
    let axis = projected(Vec3::from_array(c.axis.map(|v| v as f32)), view);
    if axis.z.abs() < 0.99999 {
        return None;
    }
    Some((
        projected(Vec3::from_array(c.origin.map(|v| v as f32)), view).truncate(),
        c.radius as f32,
    ))
}
/// Projection of a certified circular rim. An inclined rim is an ellipse,
/// whose major semiaxis still equals the physical cylinder radius.
pub(super) fn rim(c: &Target, view: usize) -> Option<interaction::Round> {
    let axis = Vec3::from_array(c.axis.map(|v| v as f32)).try_normalize()?;
    let axis = projected(axis, view);
    let minor_ratio = axis.z.abs();
    if minor_ratio < 1e-3 || !c.radius.is_finite() || c.radius <= 0. {
        return None;
    }
    let major = if minor_ratio > 0.99999 {
        Vec2::X
    } else {
        axis.truncate().normalize().perp()
    };
    Some(interaction::Round {
        center: projected(Vec3::from_array(c.origin.map(|v| v as f32)), view).truncate(),
        radius: c.radius as f32,
        arc: None,
        major,
        minor_ratio,
    })
}

#[cfg(test)]
mod inclined_tests {
    use super::*;
    #[test]
    fn inclined_step_bore_and_cylinder_keep_true_diameters_and_located_callouts() {
        use s33d_kernel::occt::ExactShape;
        let mut kernel = s33d_kernel::TruckKernel::new();
        let tube_id = kernel
            .revolve_region(
                &s33d_kernel::Region2D::new(vec![[3., 0.], [5., 0.], [5., 12.], [3., 12.]]),
                &s33d_kernel::PlaneDef {
                    origin: [0.; 3],
                    x_axis: [1., 0., 0.],
                    y_axis: [0., 0., 1.],
                },
                &s33d_kernel::Axis2D {
                    point: [0.; 2],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let tube = kernel.exact_shape(tube_id).unwrap();
        let moved = tube
            .transform(&glam::Mat4::from_rotation_translation(
                glam::Quat::from_euler(glam::EulerRot::XYZ, 0.57, 0.73, 0.21),
                Vec3::new(14., 25., 8.),
            ))
            .unwrap();
        let shape = ExactShape::import_step(&moved.export_step().unwrap())
            .unwrap()
            .remove(0);
        let targets: Vec<_> = shape
            .cylinders()
            .unwrap()
            .into_iter()
            .map(|c| Target {
                body_id: 1,
                face_id: c[0] as u32,
                origin: [c[1], c[2], c[3]],
                axis: [c[4], c[5], c[6]],
                radius: c[7],
                length: c[8],
                internal: c[9] != 0.,
            })
            .collect();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets.iter().filter(|t| t.internal).count(), 1);
        for t in &targets {
            assert!((t.length - 12.).abs() < 1e-5);
            assert!((0..3).all(|v| circle(t, v).is_none()));
        }
        let views = project(&[shape.mesh.clone()], &AtomicBool::new(false)).unwrap();
        let mut d = Drawing {
            views,
            components: vec![Component {
                id: 1,
                cylinders: targets,
                ..Default::default()
            }],
            ..Default::default()
        };
        d.rounds = (0..3).map(|v| interaction::rounds(&d, v)).collect();
        auto_dimensions(&mut d);
        let diameters: Vec<_> = d
            .dimensions
            .iter()
            .filter(|d| d.kind == DimensionKind::Diameter)
            .map(Dimension::value)
            .collect();
        assert_eq!(diameters.len(), 2, "{diameters:?}");
        assert!(diameters.iter().any(|d| (*d - 6.).abs() < 1e-4));
        assert!(diameters.iter().any(|d| (*d - 10.).abs() < 1e-4));
        let output = svg(&d, UiUnit::Millimeter, false);
        assert!(output.contains("#1 H") && output.contains("#1 C"));
        assert!(output.contains("L 12.000 mm"));
        if let Some(root) = std::env::var_os("S33D_DRAWING_EXPORT_QA") {
            export::files(
                &d,
                UiUnit::Millimeter,
                false,
                &std::path::Path::new(&root).join("inclined.eps"),
                export::ExportFormat::Eps,
            )
            .unwrap();
        }
    }
}
fn best_rim(c: &Target) -> Option<(usize, interaction::Round)> {
    (0..3)
        .filter_map(|v| {
            rim(c, v).map(|mut r| {
                let axis = projected(Vec3::from_array(c.axis.map(|v| v as f32)), v);
                if axis.z > 0. {
                    r.center += axis.truncate() * c.length as f32;
                }
                (v, r)
            })
        })
        .max_by(|a, b| a.1.minor_ratio.total_cmp(&b.1.minor_ratio))
}
pub(super) fn annotate(d: &mut Drawing) {
    let scale = fit(&d.views);
    // Keep dense assembly sheets legible: one callout per distinct diameter,
    // and spacing chains. Every object's measurements remain in the schedule.
    for view in 0..3 {
        let v = &d.views[view];
        let circles: Vec<_> = d
            .components
            .iter()
            .flat_map(|c| &c.cylinders)
            .filter_map(|c| circle(c, view).map(|(p, r)| (p, r, c.internal)))
            .collect();
        let mut diameters = Vec::<(f32, bool)>::new();
        for &(center, r, internal) in &circles {
            if diameters
                .iter()
                .any(|(v, b)| (*v - r).abs() < 1e-3 && *b == internal)
            {
                continue;
            }
            let level = diameters.len().min(8) as f32;
            diameters.push((r, internal));
            if level >= 8. {
                continue;
            }
            d.dimensions.push(Dimension {
                view,
                a: center - Vec2::X * r,
                b: center + Vec2::X * r,
                label: center + Vec2::Y * (level * 4. / scale),
                kind: DimensionKind::Diameter,
            });
        }
        for (axis, kind) in [(0, DimensionKind::Horizontal), (1, DimensionKind::Vertical)] {
            let levels = distinct(circles.iter().map(|(p, _, _)| p[axis]));
            for pair in levels.windows(2) {
                let mut a = v.min;
                let mut b = v.min;
                a[axis] = pair[0];
                b[axis] = pair[1];
                let mut label = (a + b) * 0.5;
                label[1 - axis] -= 4. / scale;
                d.dimensions.push(Dimension {
                    view,
                    a,
                    b,
                    label,
                    kind,
                });
            }
        }
    }
    // Inclined axes have no face-on circle in any standard view. Label the
    // least foreshortened rim using its true major-axis diameter.
    let mut inclined = Vec::<(f32, bool)>::new();
    for c in d.components.iter().flat_map(|c| &c.cylinders) {
        let Some((view, r)) = best_rim(c) else {
            continue;
        };
        if r.minor_ratio > 0.99999
            || inclined
                .iter()
                .any(|(radius, inside)| (*radius - r.radius).abs() < 1e-3 && *inside == c.internal)
        {
            continue;
        }
        if inclined.len() >= 8 {
            break;
        }
        inclined.push((r.radius, c.internal));
        d.dimensions.push(Dimension {
            view,
            a: r.center - r.major * r.radius,
            b: r.center + r.major * r.radius,
            label: r.center
                + r.major * (r.radius + 16. / scale)
                + r.major.perp() * ((inclined.len() - 1) as f32 * 6. / scale),
            kind: DimensionKind::Diameter,
        });
    }
    // Lengths/steps on visible straight outlines, with one callout per value
    // and direction. Very short tessellation chords are not feature lengths.
    for view in 0..3 {
        let v = &d.views[view];
        let mut used: Vec<(DimensionKind, f32)> = d
            .dimensions
            .iter()
            .filter(|d| d.view == view)
            .map(|d| (d.kind, d.value() as f32))
            .collect();
        let mut added = 0;
        let mut placed = [0usize; 2];
        for &[a, b] in v.lines.iter().chain(if d.show_hidden {
            v.hidden.iter()
        } else {
            [].iter()
        }) {
            let delta = (b - a).abs();
            let kind = if delta.y < 1e-4 {
                DimensionKind::Horizontal
            } else if delta.x < 1e-4 {
                DimensionKind::Vertical
            } else {
                continue;
            };
            let value = delta.max_element();
            if value * scale < 5.
                || used
                    .iter()
                    .any(|(k, n)| *k == kind && (*n - value).abs() < 1e-3)
            {
                continue;
            }
            // Additional dense features are listed on the dimension sheets.
            if added == 8 {
                break;
            }
            let axis = if kind == DimensionKind::Horizontal {
                0
            } else {
                1
            };
            if placed[axis] >= if axis == 0 { 1 } else { 2 } {
                continue;
            }
            used.push((kind, value));
            added += 1;
            let mut label = (a + b) * 0.5;
            // Reserve one outer horizontal lane and two vertical lanes. Dense
            // details continue in the schedule instead of covering the part.
            if axis == 0 {
                label.y = v.min.y - 13. / scale;
            } else {
                label.x = v.max.x + (5. + 5. * placed[axis] as f32) / scale;
            }
            placed[axis] += 1;
            d.dimensions.push(Dimension {
                view,
                a,
                b,
                label,
                kind,
            });
        }
    }
}

pub(super) fn center_marks(d: &Drawing) -> Vec<(usize, [Vec2; 2])> {
    let mut marks = Vec::new();
    let half = 1.5 / fit(&d.views);
    for view in 0..3 {
        let mut seen = Vec::<Vec2>::new();
        for c in d.components.iter().flat_map(|c| &c.cylinders) {
            if let Some((p, _)) = circle(c, view) {
                if seen.iter().any(|q| q.distance(p) < 1e-3) {
                    continue;
                }
                seen.push(p);
                for axis in [Vec2::X, Vec2::Y] {
                    marks.push((view, [p - axis * half, p + axis * half]));
                }
            }
        }
    }
    marks
}

/// Located feature callouts, shared by SVG and on-screen sheets.
pub(super) fn callouts(d: &Drawing, s: &mut Sheet, unit: UiUnit, zh: bool) {
    let scale = fit(&d.views);
    let mut row = 0usize;
    let mut entries = Vec::new();
    let tag = |s: &mut Sheet, view: usize, anchor: Vec2, text: String, lane: usize| {
        let v = &d.views[view];
        let anchor = sheet_point(anchor, view, v, scale);
        let label = anchor + Vec2::new(5., -8. - lane as f32 * 6.);
        s.leaders.insert(s.texts.len(), s.lines.len());
        s.lines.push(([anchor, label], true));
        s.texts.push((label, text, 0.));
    };
    for c in &d.components {
        for (index, t) in c.cylinders.iter().enumerate() {
            let Some((view, rim)) = best_rim(t) else {
                continue;
            };
            let center = rim.center;
            let code = format!(
                "#{} {}{}",
                c.id,
                if t.internal { "H" } else { "C" },
                index + 1
            );
            let v = &d.views[view];
            let axes = [("X", "Y"), ("X", "-Z"), ("-Z", "Y")][view];
            entries.push((
                code.clone(),
                format!(
                    "{code} Ø{}  L {}",
                    unit.format_mm(t.radius * 2.),
                    unit.format_mm(t.length)
                ),
                format!(
                    "{} {}  {} {}",
                    axes.0,
                    unit.format_mm((center.x - v.min.x) as f64),
                    axes.1,
                    unit.format_mm((center.y - v.min.y) as f64)
                ),
                view,
                center,
                rim.minor_ratio < 0.99999,
            ));
        }
        for (index, a) in c.thread_annotations.iter().enumerate() {
            let t = &a.target;
            let p = &a.params;
            let base = Vec3::from_array(t.origin.map(|v| v as f32));
            let axis = Vec3::from_array(t.axis.map(|v| v as f32));
            let view = (0..3)
                .max_by(|&a, &b| {
                    projected(axis, a)
                        .truncate()
                        .length_squared()
                        .total_cmp(&projected(axis, b).truncate().length_squared())
                })
                .unwrap();
            let v = &d.views[view];
            let start = projected(base + axis * p.offset as f32, view).truncate();
            let end = projected(base + axis * (p.offset + p.length) as f32, view).truncate();
            let n = (end - start).normalize_or_zero().perp() * p.diameter as f32 * 0.5;
            for sign in [-1., 1.] {
                s.dashed.push(
                    [start + n * sign, end + n * sign].map(|q| sheet_point(q, view, v, scale)),
                );
            }
            let code = format!("#{} T{}", c.id, index + 1);
            entries.push((
                code.clone(),
                format!(
                    "{code} Ø{} × P{}",
                    unit.format_mm(p.diameter),
                    unit.format_mm(p.pitch)
                ),
                format!(
                    "L {}  {} {} {}",
                    unit.format_mm(p.length),
                    if zh { "偏移" } else { "OFFSET" },
                    unit.format_mm(p.offset),
                    if p.left_hand { "LH" } else { "RH" }
                ),
                view,
                (start + end) * 0.5,
                false,
            ));
        }
    }
    if !entries.is_empty() {
        s.texts.push((
            Vec2::new(222., 17.),
            if zh {
                "特征表：H 孔 / C 柱 / T 螺纹"
            } else {
                "FEATURES: H BORE / C CYLINDER / T THREAD"
            }
            .into(),
            0.,
        ));
        s.texts.push((
            Vec2::new(222., 22.),
            if zh {
                "L 为深度/轴向长度；坐标从视图左下基准量取。"
            } else {
                "L = DEPTH / AXIAL LENGTH; POSITION FROM VIEW DATUM"
            }
            .into(),
            0.,
        ));
    }
    let mut lanes = [0; 3];
    for (code, first, second, view, anchor, inclined) in entries.iter().take(10) {
        tag(
            s,
            *view,
            *anchor,
            code.clone(),
            if *inclined { lanes[*view] } else { 0 },
        );
        if *inclined {
            lanes[*view] += 1;
        }
        let y = 28. + row as f32 * 7.;
        s.texts.push((Vec2::new(222., y), first.clone(), 0.));
        s.texts.push((Vec2::new(222., y + 3.2), second.clone(), 0.));
        row += 1;
    }
    if entries.len() > 10 {
        s.texts.push((
            Vec2::new(222., 101.),
            if zh {
                "其余特征见附加尺寸页。"
            } else {
                "ADDITIONAL FEATURES: SEE DIMENSION SHEETS"
            }
            .into(),
            0.,
        ));
    }
}

fn wrap(out: &mut Vec<String>, s: String) {
    let mut line = String::new();
    let mut width = 0;
    for ch in s.chars() {
        let size = if ch.is_ascii() { 1 } else { 2 };
        if width + size > 106 {
            out.push(std::mem::take(&mut line));
            width = 0;
        }
        line.push(ch);
        width += size;
    }
    if !line.is_empty() {
        out.push(line);
    }
}
pub(super) fn rows(d: &Drawing, unit: UiUnit, zh: bool) -> Vec<String> {
    let mut rows = Vec::new();
    let f = |v: f64| unit.format_mm(v);
    for c in &d.components {
        wrap(&mut rows, format!("#{}  {}", c.id, c.name));
        let size = c.max - c.min;
        wrap(
            &mut rows,
            format!(
                "{}: X {}   Y {}   Z {}",
                if zh { "包围盒尺寸" } else { "Bounds size" },
                f(size.x as f64),
                f(size.y as f64),
                f(size.z as f64)
            ),
        );
        wrap(
            &mut rows,
            format!(
                "{}: X {}   Y {}   Z {}",
                if zh {
                    "世界坐标最小点"
                } else {
                    "World minimum"
                },
                f(c.min.x as f64),
                f(c.min.y as f64),
                f(c.min.z as f64)
            ),
        );
        for (i, t) in c.cylinders.iter().enumerate() {
            wrap(
                &mut rows,
                format!(
                    "  {} {}: Ø{}   {} {}",
                    if zh {
                        if t.internal {
                            "孔"
                        } else {
                            "圆柱"
                        }
                    } else if t.internal {
                        "Bore"
                    } else {
                        "Cylinder"
                    },
                    i + 1,
                    f(t.radius * 2.),
                    if zh { "长度" } else { "Length" },
                    f(t.length)
                ),
            );
            wrap(
                &mut rows,
                format!(
                    "    {}: X {}   Y {}   Z {}",
                    if zh { "轴起点" } else { "Axis base" },
                    f(t.origin[0]),
                    f(t.origin[1]),
                    f(t.origin[2])
                ),
            );
            wrap(
                &mut rows,
                format!(
                    "    {}: ({:.4}, {:.4}, {:.4})",
                    if zh { "轴方向" } else { "Axis direction" },
                    t.axis[0],
                    t.axis[1],
                    t.axis[2]
                ),
            );
        }
        for (en, cn, value) in &c.notes {
            wrap(
                &mut rows,
                format!("  {}: {}", if zh { cn } else { en }, f(*value)),
            );
        }
        if let Some(p) = c.thread {
            wrap(
                &mut rows,
                format!(
                    "  {}: Ø{}   P {}   L {}",
                    if zh { "螺纹" } else { "Thread" },
                    f(p[0]),
                    f(p[1]),
                    f(p[2])
                ),
            );
            wrap(
                &mut rows,
                format!(
                    "    {} {}   {} {}",
                    if zh { "起点偏移" } else { "Start offset" },
                    f(p[3]),
                    if zh { "间隙" } else { "Clearance" },
                    f(p[4])
                ),
            );
        }
    }
    for (view, v) in d.views.iter().enumerate() {
        wrap(
            &mut rows,
            format!(
                "{} {} — {}",
                if zh { "视图" } else { "VIEW" },
                view + 1,
                if zh {
                    "隐藏腔体 / 隐藏轮廓坐标与尺寸"
                } else {
                    "Hidden cavity / contour stations and sizes"
                }
            ),
        );
        let mut seen = std::collections::HashSet::new();
        for &[a, b] in &v.hidden {
            let delta = (b - a).abs();
            if delta.min_element() > 1e-4 || delta.max_element() < 1e-3 {
                continue;
            }
            let key = (a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits());
            if !seen.insert(key) {
                continue;
            }
            wrap(
                &mut rows,
                format!(
                    "  ({}, {}) → ({}, {})   L {}",
                    f(a.x as f64),
                    f(a.y as f64),
                    f(b.x as f64),
                    f(b.y as f64),
                    f(a.distance(b) as f64)
                ),
            );
        }
    }
    // Outline stations expose steps and noncircular positions without guessing
    // their design intent. Values are in each view's displayed coordinates.
    for (view, v) in d.views.iter().enumerate() {
        wrap(
            &mut rows,
            format!(
                "{} — {}",
                if zh {
                    ["前视图", "俯视图", "右视图"][view]
                } else {
                    ["FRONT", "TOP", "RIGHT"][view]
                },
                if zh {
                    "直线轮廓关键坐标 / 长度"
                } else {
                    "Straight outline stations / lengths"
                }
            ),
        );
        let mut seen = std::collections::HashSet::new();
        for &[a, b] in &v.lines {
            let delta = (b - a).abs();
            if delta.min_element() > 1e-4 || delta.max_element() < 1e-3 {
                continue;
            }
            let (axis, station, lo, hi) = if delta.y < 1e-4 {
                ("H", a.y, a.x.min(b.x), a.x.max(b.x))
            } else {
                ("V", a.x, a.y.min(b.y), a.y.max(b.y))
            };
            let key = (
                axis,
                (station * 1000.).round() as i64,
                (lo * 1000.).round() as i64,
                (hi * 1000.).round() as i64,
            );
            if seen.insert(key) {
                wrap(
                    &mut rows,
                    format!(
                        "  {axis}: {} | {} → {} | L {}",
                        f(station as f64),
                        f(lo as f64),
                        f(hi as f64),
                        f((hi - lo) as f64)
                    ),
                );
            }
        }
    }
    rows
}
fn with_rows<T>(d: &Drawing, unit: UiUnit, zh: bool, f: impl FnOnce(&[String]) -> T) -> T {
    let mut cache = d.schedule_cache.borrow_mut();
    if cache
        .as_ref()
        .is_none_or(|(u, z, _)| *u != unit || *z != zh)
    {
        *cache = Some((unit, zh, rows(d, unit, zh)));
    }
    f(&cache.as_ref().unwrap().2)
}
const ROWS: usize = 33;
pub(super) fn pages(d: &Drawing, unit: UiUnit, zh: bool) -> usize {
    if d.views.len() != 3 {
        return 1;
    }
    1 + with_rows(d, unit, zh, |rows| rows.len().div_ceil(ROWS))
}
pub(super) fn schedule(d: &Drawing, unit: UiUnit, zh: bool, page: usize) -> Sheet {
    let mut s = Sheet::default();
    s.texts.push((
        Vec2::new(148., 15.),
        format!(
            "{} — {}",
            d.title,
            if zh {
                "关键尺寸明细"
            } else {
                "Key dimensions"
            }
        ),
        0.,
    ));
    s.texts.push((Vec2::new(148.,23.),if zh {"几何测量值；未推断公差。H / V 为视图水平 / 垂直轮廓。"}else{"Geometric measurements; no inferred tolerances. H / V: view horizontal / vertical outlines."}.into(),0.));
    with_rows(d, unit, zh, |rows| {
        for (i, row) in rows.iter().skip((page - 1) * ROWS).take(ROWS).enumerate() {
            s.texts
                .push((Vec2::new(148., 32. + i as f32 * 4.2), row.clone(), 0.));
        }
    });
    format::title_block(d, &mut s, unit, zh, page);
    s.texts.push((
        Vec2::new(148., 175.),
        format!("{} / {} | {}", page + 1, pages(d, unit, zh), unit.symbol()),
        0.,
    ));
    s
}
