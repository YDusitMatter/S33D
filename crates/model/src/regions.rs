//! 平面分割 polygonize：把所有线段在交点处切开，然后沿半边图行走，
//! 提取全部最小封闭区域。纯几何，不访问文档或渲染状态。
//!
//! 移植自 Super3D 原型 `js/regions.js`。
//!
//! 输入：线段 `[[x1, y1, x2, y2], ...]`
//! 输出：区域列表，`outer` 逆时针、`holes` 顺时针、`area` 为正。

pub use crate::region_mesh::triangulate_region;
use s33d_kernel::Region2D;

#[derive(Debug, Clone)]
pub struct Face {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
    pub area: f64,
}

fn cross(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ax * by - ay * bx
}

fn point_in_poly(pt: [f64; 2], poly: &[[f64; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let (x, y) = (pt[0], pt[1]);
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if (yi > y) != (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub(crate) fn signed_area(pts: &[[f64; 2]]) -> f64 {
    s33d_kernel::region::signed_area(pts)
}

/// Welding follows each incident segment's scale and representable coordinate
/// precision. An unrelated entity never changes another component's tolerance.
fn segment_tolerance(s: &[f64; 4]) -> f64 {
    let length = (s[2] - s[0]).hypot(s[3] - s[1]);
    let precision = s.iter().map(|v| v.abs()).fold(0.0, f64::max) * f64::EPSILON * 2.0;
    (length * 1e-9).max(precision).max(1e-10)
}

#[derive(Debug, Clone, Copy)]
struct SegEvent {
    t: f64,
    u: f64,
    x: f64,
    y: f64,
    tolerance: f64,
}

/// 线段 ab 与 cd 的接触事件。
fn seg_events(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], tol_p: f64) -> Vec<SegEvent> {
    let rx = b[0] - a[0];
    let ry = b[1] - a[1];
    let sx = d[0] - c[0];
    let sy = d[1] - c[1];
    let qx = c[0] - a[0];
    let qy = c[1] - a[1];
    let len_r = (rx * rx + ry * ry).sqrt();
    let len_s = (sx * sx + sy * sy).sqrt();
    if len_r < 1e-12 || len_s < 1e-12 {
        return Vec::new();
    }
    let den = cross(rx, ry, sx, sy);
    let mut out = Vec::new();
    if den.abs() <= 1e-10 * len_r * len_s {
        // 平行：仅当 cd 两端点都在 ab 直线上时视为共线
        let d_c = cross(rx, ry, qx, qy).abs() / len_r;
        let d_d = cross(rx, ry, d[0] - a[0], d[1] - a[1]).abs() / len_r;
        if d_c > tol_p || d_d > tol_p {
            return out;
        }
        let rr = rx * rx + ry * ry;
        let t_c = (qx * rx + qy * ry) / rr;
        let t_d = t_c + (sx * rx + sy * ry) / rr;
        let lo = (0.0f64).max(t_c.min(t_d));
        let hi = (1.0f64).min(t_c.max(t_d));
        if lo > hi + 1e-9 {
            return out;
        }
        let ss = sx * sx + sy * sy;
        let push = |t: f64, out: &mut Vec<SegEvent>| {
            let tc = (0.0f64).max((1.0f64).min(t));
            let x = a[0] + rx * tc;
            let y = a[1] + ry * tc;
            let u = (0.0f64).max((1.0f64).min(((x - c[0]) * sx + (y - c[1]) * sy) / ss));
            out.push(SegEvent {
                t: tc,
                u,
                x,
                y,
                tolerance: tol_p,
            });
        };
        push(lo, &mut out);
        if hi > lo + 1e-9 {
            push(hi, &mut out);
        }
        return out;
    }
    let t = cross(qx, qy, sx, sy) / den;
    let u = cross(qx, qy, rx, ry) / den;
    let eps = 1e-9;
    if t < -eps || t > 1.0 + eps || u < -eps || u > 1.0 + eps {
        return out;
    }
    let tc = (0.0f64).max((1.0f64).min(t));
    let uc = (0.0f64).max((1.0f64).min(u));
    let point = if len_r <= len_s {
        [a[0] + rx * tc, a[1] + ry * tc]
    } else {
        [c[0] + sx * uc, c[1] + sy * uc]
    };
    out.push(SegEvent {
        t: tc,
        u: uc,
        x: point[0],
        y: point[1],
        tolerance: tol_p,
    });
    out
}

/// An undirected bridge has the same planar face on both sides. Including it
/// in a face walk creates an out-and-back slit, not a material boundary. Remove
/// all such edges from this derived graph, including links between closed rings
/// whose endpoints both have degree >= 3. A leaf-only cleanup misses those.
///
/// Iterative Tarjan traversal avoids recursion proportional to sketch size.
/// Genuine dividers belong to a cycle and are retained. The sketch entities and
/// all remaining boundary subdivision vertices are left untouched.
fn remove_graph_bridges(vertex_count: usize, edges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    let mut adjacent = vec![Vec::<(usize, usize)>::new(); vertex_count];
    for (edge, &(a, b)) in edges.iter().enumerate() {
        adjacent[a].push((b, edge));
        adjacent[b].push((a, edge));
    }
    let mut discovered = vec![0usize; vertex_count];
    let mut low = vec![0usize; vertex_count];
    let mut bridges = vec![false; edges.len()];
    let mut clock = 0;
    for root in 0..vertex_count {
        if discovered[root] != 0 || adjacent[root].is_empty() {
            continue;
        }
        clock += 1;
        discovered[root] = clock;
        low[root] = clock;
        let mut stack = vec![(root, usize::MAX, 0usize)];
        while let Some((vertex, parent_edge, next)) = stack.last_mut() {
            let (v, parent_edge) = (*vertex, *parent_edge);
            if *next < adjacent[v].len() {
                let (neighbor, edge) = adjacent[v][*next];
                *next += 1;
                if edge == parent_edge {
                    continue;
                }
                if discovered[neighbor] == 0 {
                    clock += 1;
                    discovered[neighbor] = clock;
                    low[neighbor] = clock;
                    stack.push((neighbor, edge, 0));
                } else {
                    low[v] = low[v].min(discovered[neighbor]);
                }
            } else {
                stack.pop();
                if parent_edge != usize::MAX {
                    let (a, b) = edges[parent_edge];
                    let parent = if a == v { b } else { a };
                    if low[v] > discovered[parent] {
                        bridges[parent_edge] = true;
                    }
                    low[parent] = low[parent].min(low[v]);
                }
            }
        }
    }
    edges
        .into_iter()
        .enumerate()
        .filter_map(|(i, edge)| (!bridges[i]).then_some(edge))
        .collect()
}

/// 平面分割：输入线段，输出最小封闭区域（outer CCW + holes CW）。
pub fn polygonize(segments: &[[f64; 4]]) -> Vec<Face> {
    // 1. Filter zero-length segments. Reject non-finite input as a whole;
    // retaining just its finite subset would silently manufacture a region.
    let mut segs: Vec<[f64; 4]> = Vec::new();
    for s in segments {
        if s.iter().any(|v| !v.is_finite()) {
            return Vec::new();
        }
        let length = (s[2] - s[0]).hypot(s[3] - s[1]);
        if !length.is_finite() {
            return Vec::new();
        }
        if length < 1e-12 {
            continue;
        }
        segs.push(*s);
    }
    if segs.is_empty() {
        return Vec::new();
    }
    let tolerances: Vec<f64> = segs.iter().map(segment_tolerance).collect();

    // 2. 每条线段在接触点处切开
    let mut cuts: Vec<Vec<SegEvent>> = segs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            vec![
                SegEvent {
                    t: 0.0,
                    u: 0.0,
                    x: s[0],
                    y: s[1],
                    tolerance: tolerances[i],
                },
                SegEvent {
                    t: 1.0,
                    u: 0.0,
                    x: s[2],
                    y: s[3],
                    tolerance: tolerances[i],
                },
            ]
        })
        .collect();
    for i in 0..segs.len() {
        let a = [segs[i][0], segs[i][1]];
        let b = [segs[i][2], segs[i][3]];
        for j in (i + 1)..segs.len() {
            let tol_p = tolerances[i].min(tolerances[j]);
            if a[0].max(b[0]) < segs[j][0].min(segs[j][2]) - tol_p
                || a[0].min(b[0]) > segs[j][0].max(segs[j][2]) + tol_p
                || a[1].max(b[1]) < segs[j][1].min(segs[j][3]) - tol_p
                || a[1].min(b[1]) > segs[j][1].max(segs[j][3]) + tol_p
            {
                continue;
            }
            let c = [segs[j][0], segs[j][1]];
            let d = [segs[j][2], segs[j][3]];
            for ev in seg_events(a, b, c, d, tol_p) {
                cuts[i].push(SegEvent {
                    t: ev.t,
                    x: ev.x,
                    y: ev.y,
                    u: 0.0,
                    tolerance: ev.tolerance,
                });
                cuts[j].push(SegEvent {
                    t: ev.u,
                    x: ev.x,
                    y: ev.y,
                    u: 0.0,
                    tolerance: ev.tolerance,
                });
            }
        }
    }

    // 3. 邻近切割点聚类为共享顶点（网格哈希）
    let mut pts: Vec<[f64; 2]> = Vec::new();
    // Separate power-of-two grid levels keep the local tolerance policy
    // efficient without making small geometry use the largest sketch scale.
    let mut grids: std::collections::BTreeMap<
        i32,
        std::collections::HashMap<(i64, i64), Vec<usize>>,
    > = Default::default();
    let mut point_tolerances: Vec<f64> = Vec::new();
    let cluster = |x: f64,
                   y: f64,
                   tolerance: f64,
                   pts: &mut Vec<[f64; 2]>,
                   point_tolerances: &mut Vec<f64>,
                   grids: &mut std::collections::BTreeMap<
        i32,
        std::collections::HashMap<(i64, i64), Vec<usize>>,
    >|
     -> usize {
        for (&level, grid) in grids.iter() {
            let cell = 2f64.powi(level) * 2.0;
            let cx = (x / cell).floor() as i64;
            let cy = (y / cell).floor() as i64;
            for ix in cx.saturating_sub(1)..=cx.saturating_add(1) {
                for iy in cy.saturating_sub(1)..=cy.saturating_add(1) {
                    if let Some(lst) = grid.get(&(ix, iy)) {
                        for &id in lst {
                            if (pts[id][0] - x).hypot(pts[id][1] - y)
                                <= tolerance.min(point_tolerances[id])
                            {
                                point_tolerances[id] = point_tolerances[id].min(tolerance);
                                return id;
                            }
                        }
                    }
                }
            }
        }
        let id = pts.len();
        pts.push([x, y]);
        point_tolerances.push(tolerance);
        let level = tolerance.log2().ceil() as i32;
        let cell = 2f64.powi(level) * 2.0;
        grids
            .entry(level)
            .or_default()
            .entry(((x / cell).floor() as i64, (y / cell).floor() as i64))
            .or_default()
            .push(id);
        id
    };

    // 4. 相邻切割点之间的原子边
    let mut edge_set: std::collections::HashMap<(usize, usize), (usize, usize)> =
        Default::default();
    for i in 0..segs.len() {
        cuts[i].sort_by(|p, q| p.t.total_cmp(&q.t));
        let mut prev: i64 = -1;
        for c in &cuts[i] {
            let id = cluster(
                c.x,
                c.y,
                c.tolerance,
                &mut pts,
                &mut point_tolerances,
                &mut grids,
            );
            if prev >= 0 && id as i64 != prev {
                let a = prev.min(id as i64) as usize;
                let b = prev.max(id as i64) as usize;
                if !edge_set.contains_key(&(a, b))
                    && ((pts[a][0] - pts[b][0]).powi(2) + (pts[a][1] - pts[b][1]).powi(2)).sqrt()
                        > point_tolerances[a].min(point_tolerances[b]) * 2.0
                {
                    edge_set.insert((a, b), (a, b));
                }
            }
            prev = id as i64;
        }
    }
    let edges = remove_graph_bridges(pts.len(), edge_set.values().copied().collect());
    if edges.is_empty() {
        return Vec::new();
    }

    // 5. 邻接表按极角逆时针排序
    let mut adj: std::collections::HashMap<usize, Vec<(usize, f64)>> = Default::default();
    for &(a, b) in &edges {
        adj.entry(a)
            .or_default()
            .push((b, (pts[b][1] - pts[a][1]).atan2(pts[b][0] - pts[a][0])));
        adj.entry(b)
            .or_default()
            .push((a, (pts[a][1] - pts[b][1]).atan2(pts[a][0] - pts[b][0])));
    }
    for lst in adj.values_mut() {
        lst.sort_by(|p, q| p.1.partial_cmp(&q.1).unwrap());
    }

    // 6. 半边行走：每个顶点取到达边的角前驱（最右转），遍历每条有向边恰好一次
    let mut visited: std::collections::HashSet<(usize, usize)> = Default::default();
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    for &(a, b) in &edges {
        for start in [(a, b), (b, a)] {
            if visited.contains(&start) {
                continue;
            }
            let mut path: Vec<usize> = Vec::new();
            let mut cur = start;
            let mut guard = edges.len() * 2 + 10;
            while !visited.contains(&cur) && guard > 0 {
                guard -= 1;
                visited.insert(cur);
                path.push(cur.0);
                let lst = &adj[&cur.1];
                let idx = lst.iter().position(|x| x.0 == cur.0).unwrap();
                let nxt = lst[(idx + lst.len() - 1) % lst.len()];
                cur = (cur.1, nxt.0);
            }
            if cur == start && path.len() >= 3 {
                cycles.push(path);
            }
        }
    }

    // 7. 有界区域（CCW）+ 洞（落在区域内的 CW 环）
    let mut faces: Vec<Face> = Vec::new();
    let mut holes: Vec<(Vec<[f64; 2]>, f64)> = Vec::new();
    for c in &cycles {
        let mut p: Vec<[f64; 2]> = c.iter().map(|&id| pts[id]).collect();
        // Half-edge traversal starts from a HashMap. Fix the reference point
        // before measuring area, not just before returning the ring.
        canonicalize_ring(&mut p);
        let tolerance = c.iter().map(|&id| point_tolerances[id]).fold(0.0, f64::max);
        let min_area = tolerance * tolerance * 4.0;
        let a = signed_area(&p);
        if a.abs() < min_area {
            continue;
        }
        if a > 0.0 {
            faces.push(Face {
                outer: p,
                holes: Vec::new(),
                area: a,
            });
        } else {
            holes.push((p, a));
        }
    }
    for (h, ha) in &holes {
        let mut best: Option<usize> = None;
        for (i, f) in faces.iter().enumerate() {
            // Every closed ring produces one CCW face and the same boundary
            // in CW direction. Their areas can differ by a few ULPs, so a
            // plain `f.area > -ha` occasionally classifies a ring as its own
            // hole. Require a scale-aware separation before containment.
            let hole_area = -*ha;
            let area_tolerance = f.area.abs().max(hole_area.abs()) * f64::EPSILON * 64.0;
            if f.area > hole_area + area_tolerance && point_in_poly(h[0], &f.outer) {
                if best.map(|bi| f.area < faces[bi].area).unwrap_or(true) {
                    best = Some(i);
                }
            }
        }
        if let Some(bi) = best {
            faces[bi].holes.push(h.clone());
        }
    }

    // Hole discovery also starts from randomized HashMap traversal. Keep hole
    // order canonical because it feeds both highlight triangulation and the
    // kernel wire builder.
    for face in &mut faces {
        canonicalize_ring(&mut face.outer);
        for hole in &mut face.holes {
            canonicalize_ring(hole);
        }
        face.holes
            .sort_by(|left, right| canonical_ring_cmp(left, right));
    }

    // 8. Fully deterministic ordering. Area + centroid is insufficient for
    // equal concentric/symmetric profiles: HashMap iteration can then assign a
    // different Region index to Pick, highlight and Extrude. Bounds and a
    // rotation-independent boundary comparison provide a total geometry key.
    faces.sort_by(|f1, f2| {
        let c1 = centroid(&f1.outer);
        let c2 = centroid(&f2.outer);
        let b1 = ring_bounds(&f1.outer);
        let b2 = ring_bounds(&f2.outer);
        f1.area
            .total_cmp(&f2.area)
            .then_with(|| c1[0].total_cmp(&c2[0]))
            .then_with(|| c1[1].total_cmp(&c2[1]))
            .then_with(|| b1[0].total_cmp(&b2[0]))
            .then_with(|| b1[1].total_cmp(&b2[1]))
            .then_with(|| b1[2].total_cmp(&b2[2]))
            .then_with(|| b1[3].total_cmp(&b2[3]))
            .then_with(|| canonical_ring_cmp(&f1.outer, &f2.outer))
            .then_with(|| f1.holes.len().cmp(&f2.holes.len()))
    });
    faces
}

fn canonicalize_ring(ring: &mut Vec<[f64; 2]>) {
    let Some(start) = (0..ring.len()).min_by(|&a, &b| {
        ring[a][0]
            .total_cmp(&ring[b][0])
            .then_with(|| ring[a][1].total_cmp(&ring[b][1]))
    }) else {
        return;
    };
    ring.rotate_left(start);
}

fn canonical_ring_cmp(left: &[[f64; 2]], right: &[[f64; 2]]) -> std::cmp::Ordering {
    let start = |ring: &[[f64; 2]]| {
        (0..ring.len())
            .min_by(|&a, &b| {
                ring[a][0]
                    .total_cmp(&ring[b][0])
                    .then_with(|| ring[a][1].total_cmp(&ring[b][1]))
            })
            .unwrap_or(0)
    };
    left.len().cmp(&right.len()).then_with(|| {
        let left_start = start(left);
        let right_start = start(right);
        (0..left.len())
            .map(|offset| {
                let a = left[(left_start + offset) % left.len()];
                let b = right[(right_start + offset) % right.len()];
                a[0].total_cmp(&b[0]).then_with(|| a[1].total_cmp(&b[1]))
            })
            .find(|ordering| !ordering.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn ring_bounds(ring: &[[f64; 2]]) -> [f64; 4] {
    ring.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |mut bounds, point| {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
            bounds
        },
    )
}

fn centroid(pts: &[[f64; 2]]) -> [f64; 2] {
    let Some(&origin) = pts.first() else {
        return [0.0, 0.0];
    };
    let mut x = 0.0;
    let mut y = 0.0;
    for p in pts {
        x += p[0] - origin[0];
        y += p[1] - origin[1];
    }
    [
        origin[0] + x / pts.len() as f64,
        origin[1] + y / pts.len() as f64,
    ]
}

/// 实体的折线链 → 线段列表（与 `sketch.js computeRegions` 一致）。
pub fn entity_segments(entities: &[crate::sketch::SketchEntity]) -> Vec<[f64; 4]> {
    let mut segs = Vec::new();
    for (pts, closed) in entities.iter().flat_map(|e| e.to_chains()) {
        let cnt = if closed {
            pts.len()
        } else {
            pts.len().saturating_sub(1)
        };
        for i in 0..cnt {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            segs.push([a[0], a[1], b[0], b[1]]);
        }
    }
    segs
}

/// 实体列表的内容指纹（f64 → to_bits 哈希，确定性）。
/// 用于区域缓存失效判断：实体未变时复用平面分割结果。
pub fn entities_fingerprint(entities: &[crate::sketch::SketchEntity]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    for e in entities {
        match e {
            crate::sketch::SketchEntity::Path { .. } => {
                6u8.hash(&mut h);
                let chains = e.to_chains();
                chains.len().hash(&mut h);
                for (points, closed) in chains {
                    closed.hash(&mut h);
                    points.len().hash(&mut h);
                    for p in points {
                        p[0].to_bits().hash(&mut h);
                        p[1].to_bits().hash(&mut h);
                    }
                }
            }
            crate::sketch::SketchEntity::Line { a, b } => {
                0u8.hash(&mut h);
                a[0].to_bits().hash(&mut h);
                a[1].to_bits().hash(&mut h);
                b[0].to_bits().hash(&mut h);
                b[1].to_bits().hash(&mut h);
            }
            crate::sketch::SketchEntity::Arc { c, r, a0, a1, ccw } => {
                1u8.hash(&mut h);
                c[0].to_bits().hash(&mut h);
                c[1].to_bits().hash(&mut h);
                r.to_bits().hash(&mut h);
                a0.to_bits().hash(&mut h);
                a1.to_bits().hash(&mut h);
                ccw.hash(&mut h);
            }
            crate::sketch::SketchEntity::Circle { c, r } => {
                2u8.hash(&mut h);
                c[0].to_bits().hash(&mut h);
                c[1].to_bits().hash(&mut h);
                r.to_bits().hash(&mut h);
            }
            crate::sketch::SketchEntity::Rect { c1, c2 } => {
                3u8.hash(&mut h);
                c1[0].to_bits().hash(&mut h);
                c1[1].to_bits().hash(&mut h);
                c2[0].to_bits().hash(&mut h);
                c2[1].to_bits().hash(&mut h);
            }
            crate::sketch::SketchEntity::RotatedRect {
                c,
                width,
                height,
                rot,
            } => {
                5u8.hash(&mut h);
                c[0].to_bits().hash(&mut h);
                c[1].to_bits().hash(&mut h);
                width.to_bits().hash(&mut h);
                height.to_bits().hash(&mut h);
                rot.to_bits().hash(&mut h);
            }
            crate::sketch::SketchEntity::Polygon { c, r, n, rot } => {
                4u8.hash(&mut h);
                c[0].to_bits().hash(&mut h);
                c[1].to_bits().hash(&mut h);
                r.to_bits().hash(&mut h);
                n.hash(&mut h);
                rot.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// 由草图实体计算全部最小封闭区域（与 `sketch.js regionShapes` 一致）。
pub fn compute_regions(entities: &[crate::sketch::SketchEntity]) -> Vec<Region2D> {
    let segs = entity_segments(entities);
    polygonize(&segs)
        .into_iter()
        .map(|f| Region2D {
            outer: f.outer,
            holes: f.holes,
        })
        .collect()
}

/// 点在多边形内的测试（与 `sketch.js pointInPoly` 一致）。
pub fn point_in_region(pt: [f64; 2], region: &Region2D) -> bool {
    if !point_in_poly(pt, &region.outer) {
        return false;
    }
    !region.holes.iter().any(|h| point_in_poly(pt, h))
}

#[cfg(test)]
#[path = "region_boundary_tests.rs"]
mod boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sketch::SketchEntity;

    #[test]
    fn review_shared_bridge_two_holes_complete_area() {
        let regions = compute_regions(&[
            SketchEntity::Rect {
                c1: [0., 0.],
                c2: [100., 100.],
            },
            SketchEntity::Circle {
                c: [65., 95.],
                r: 1.,
            },
            SketchEntity::Circle {
                c: [85., 52.],
                r: 5.,
            },
        ]);
        let region = regions.iter().find(|r| r.holes.len() == 2).unwrap();
        let tris = triangulate_region(&region.outer, &region.holes)
            .unwrap()
            .triangles;
        let expected = signed_area(&region.outer).abs()
            - region
                .holes
                .iter()
                .map(|h| signed_area(h).abs())
                .sum::<f64>();
        let actual = tris
            .iter()
            .map(|t| tri_sign(t[0], t[1], t[2]).abs() * 0.5)
            .sum::<f64>();
        assert!(
            (actual - expected).abs() < 1e-7,
            "actual={actual}, expected={expected}"
        );
    }

    #[test]
    fn review_far_open_segment_does_not_remove_hole() {
        let mut entities = vec![
            SketchEntity::Rect {
                c1: [0., 0.],
                c2: [10., 10.],
            },
            SketchEntity::Circle { c: [5., 5.], r: 1. },
        ];
        let before = compute_regions(&entities);
        entities.push(SketchEntity::Line {
            a: [1_000_000., 20.],
            b: [1_000_001., 20.],
        });
        assert_eq!(compute_regions(&entities), before);
    }

    #[test]
    fn local_welding_and_hole_classification_preserve_small_features() {
        for scale in [0.001, 1., 1000.] {
            let mut entities = vec![
                SketchEntity::Rect {
                    c1: [0., 0.],
                    c2: [10. * scale, 10. * scale],
                },
                SketchEntity::Circle {
                    c: [5. * scale, 5. * scale],
                    r: scale,
                },
            ];
            let before = compute_regions(&entities);
            for distance in [1e6, 1e9, 1e12] {
                entities.push(SketchEntity::Line {
                    a: [distance, 20.],
                    b: [distance + 1., 20.],
                });
                assert_eq!(
                    compute_regions(&entities),
                    before,
                    "scale={scale}, distance={distance}"
                );
            }
        }
        let regions = compute_regions(&[
            SketchEntity::Rect {
                c1: [0., 0.],
                c2: [0.001, 0.001],
            },
            SketchEntity::Rect {
                c1: [1e-8, 1e-8],
                c2: [0.001 - 1e-8, 0.001 - 1e-8],
            },
        ]);
        assert_eq!(regions.len(), 2);
        let annulus = regions
            .iter()
            .find(|r| r.holes.len() == 1)
            .expect("narrow small-scale annulus retained");
        triangulate_region(&annulus.outer, &annulus.holes).unwrap();
    }

    #[test]
    fn very_long_cutting_line_uses_local_intersection_precision() {
        let regions = compute_regions(&[
            SketchEntity::Line {
                a: [-1e12, 5.],
                b: [1e12, 5.],
            },
            SketchEntity::Rect {
                c1: [0., 0.],
                c2: [10., 10.],
            },
        ]);
        assert_eq!(regions.len(), 2);
        assert!(regions.iter().all(|r| (r.signed_area() - 50.).abs() < 1e-9));
    }

    #[test]
    fn review_large_translation_preserves_region_topology_and_area() {
        let origin = 1_000_000_000.;
        let regions = compute_regions(&[
            SketchEntity::Rect {
                c1: [origin, origin],
                c2: [origin + 10., origin + 10.],
            },
            SketchEntity::Rect {
                c1: [origin + 4., origin + 4.],
                c2: [origin + 6., origin + 6.],
            },
        ]);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions.iter().map(|r| r.holes.len()).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            regions
                .iter()
                .map(Region2D::signed_area)
                .collect::<Vec<_>>(),
            vec![4., 100.]
        );
    }

    #[test]
    fn equal_area_regions_have_deterministic_order_for_entity_permutations() {
        let mut entities = (0..4)
            .map(|i| SketchEntity::Rect {
                c1: [1e9 + i as f64 * 10., 1e9],
                c2: [1e9 + i as f64 * 10. + 1.3, 1e9 + 1.7],
            })
            .collect::<Vec<_>>();
        let expected = compute_regions(&entities);
        assert_eq!(expected.len(), 4);
        for i in 0..100 {
            entities.rotate_left(1);
            if i % 2 == 0 {
                entities.reverse();
            }
            assert_eq!(compute_regions(&entities), expected);
        }
    }

    /// 移植自 `tests/unit-regions.mjs` 的核心用例。
    #[test]
    fn two_overlapping_rects_three_faces() {
        let r1 = SketchEntity::Rect {
            c1: [0.0, 0.0],
            c2: [10.0, 10.0],
        };
        let r2 = SketchEntity::Rect {
            c1: [5.0, 0.0],
            c2: [15.0, 10.0],
        };
        let regions = compute_regions(&[r1, r2]);
        assert_eq!(regions.len(), 3, "two overlapping rects -> 3 faces");
        // 总面积 = 100 + 100 - 50 = 150
        let total: f64 = regions.iter().map(|r| r.signed_area()).sum();
        assert!((total - 150.0).abs() < 1e-6, "total={total}");
    }

    #[test]
    fn rect_with_hole() {
        let outer = SketchEntity::Rect {
            c1: [0.0, 0.0],
            c2: [10.0, 10.0],
        };
        let inner = SketchEntity::Rect {
            c1: [3.0, 3.0],
            c2: [7.0, 7.0],
        };
        let regions = compute_regions(&[outer, inner]);
        assert_eq!(
            regions.len(),
            2,
            "outer + hole -> outer face with hole + inner face"
        );
        // 最大的区域包含洞
        let big = regions
            .iter()
            .max_by_key(|r| (r.signed_area() * 1e6) as i64)
            .unwrap();
        assert_eq!(big.holes.len(), 1);
        assert!((big.signed_area() - 100.0).abs() < 1e-6);
        let small = regions
            .iter()
            .min_by_key(|r| (r.signed_area() * 1e6) as i64)
            .unwrap();
        assert_eq!(small.holes.len(), 0);
        assert!((small.signed_area() - 16.0).abs() < 1e-6);
    }

    #[test]
    fn concentric_mixed_profiles_keep_stable_indices_and_triangulate() {
        let entities = vec![
            SketchEntity::Circle {
                c: [0.0, 0.0],
                r: 40.0,
            },
            SketchEntity::Polygon {
                c: [0.0, 0.0],
                r: 53.0,
                n: 6,
                rot: 0.0,
            },
            SketchEntity::Rect {
                c1: [-70.0, -80.0],
                c2: [70.0, 80.0],
            },
        ];
        let expected = compute_regions(&entities);
        assert_eq!(expected.len(), 3);
        for _ in 0..100 {
            assert_eq!(compute_regions(&entities), expected);
        }
        for region in expected {
            let triangles = triangulate_region(&region.outer, &region.holes)
                .unwrap()
                .triangles;
            assert!(!triangles.is_empty());
            let triangulated_area: f64 = triangles
                .iter()
                .map(|triangle| (tri_sign(triangle[0], triangle[1], triangle[2]) * 0.5).abs())
                .sum();
            let hole_area: f64 = region
                .holes
                .iter()
                .map(|hole| signed_area(hole).abs())
                .sum();
            let expected_area = signed_area(&region.outer).abs() - hole_area;
            assert!(
                (triangulated_area - expected_area).abs() < 1e-5,
                "triangulated={triangulated_area}, expected={expected_area}"
            );
        }
    }

    #[test]
    fn shared_edge_two_rects_two_faces() {
        let r1 = SketchEntity::Rect {
            c1: [0.0, 0.0],
            c2: [10.0, 10.0],
        };
        let r2 = SketchEntity::Rect {
            c1: [10.0, 0.0],
            c2: [20.0, 10.0],
        };
        let regions = compute_regions(&[r1, r2]);
        assert_eq!(regions.len(), 2);
        let total: f64 = regions.iter().map(|r| r.signed_area()).sum();
        assert!((total - 200.0).abs() < 1e-6);
    }

    #[test]
    fn triangulate_rect_covers_area() {
        // 矩形（CCW）：三角化后三角形总面积 = 矩形面积
        let rect: Vec<[f64; 2]> = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        let tris = triangulate_region(&rect, &[]).unwrap().triangles;
        assert!(!tris.is_empty());
        let total: f64 = tris
            .iter()
            .map(|t| (tri_sign(t[0], t[1], t[2]) / 2.0).abs())
            .sum();
        assert!((total - 50.0).abs() < 1e-6, "area={total}");
    }

    #[test]
    fn triangulate_region_with_hole_no_panic() {
        let outer: Vec<[f64; 2]> = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        // hole 为 CW（region 约定：outer CCW、holes CW）
        let hole: Vec<[f64; 2]> = vec![[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0]];
        let tris = triangulate_region(&outer, &[hole]).unwrap().triangles;
        assert!(!tris.is_empty());
        let total: f64 = tris
            .iter()
            .map(|t| (tri_sign(t[0], t[1], t[2]) / 2.0).abs())
            .sum();
        assert!((total - 84.0).abs() < 1e-6, "area={total}");
    }

    #[test]
    fn triangulate_concave_region_with_two_holes_covers_only_selected_area() {
        let outer = vec![
            [0.0, 0.0],
            [18.0, 0.0],
            [18.0, 14.0],
            [11.0, 14.0],
            [11.0, 8.0],
            [7.0, 8.0],
            [7.0, 14.0],
            [0.0, 14.0],
        ];
        let holes = vec![
            vec![[2.0, 3.0], [2.0, 6.0], [5.0, 6.0], [5.0, 3.0]],
            vec![[13.0, 3.0], [13.0, 6.0], [16.0, 6.0], [16.0, 3.0]],
        ];
        let tris = triangulate_region(&outer, &holes).unwrap().triangles;
        assert!(!tris.is_empty());
        let total: f64 = tris
            .iter()
            .map(|t| (tri_sign(t[0], t[1], t[2]) / 2.0).abs())
            .sum();
        assert!((total - 210.0).abs() < 1e-6, "area={total}");
    }
    #[test]
    fn capsule_with_two_round_holes_fills_without_missing_wedges() {
        // Exact sketch dimensions from the reported rendering error1 project.
        let entities = vec![
            SketchEntity::Line {
                a: [-50., 25.],
                b: [50., 25.],
            },
            SketchEntity::Line {
                a: [50., -25.],
                b: [-50., -25.],
            },
            SketchEntity::Arc {
                c: [50., 0.],
                r: 25.,
                a0: 1.5 * std::f64::consts::PI,
                a1: 2.5 * std::f64::consts::PI,
                ccw: true,
            },
            SketchEntity::Arc {
                c: [-50., 0.],
                r: 25.,
                a0: 0.5 * std::f64::consts::PI,
                a1: 1.5 * std::f64::consts::PI,
                ccw: true,
            },
            SketchEntity::Circle {
                c: [-50., 0.],
                r: 10.,
            },
            SketchEntity::Circle {
                c: [50., 0.],
                r: 10.,
            },
        ];
        let regions = compute_regions(&entities);
        let region = regions.iter().find(|r| r.holes.len() == 2).unwrap();
        let expected = signed_area(&region.outer).abs()
            - region
                .holes
                .iter()
                .map(|h| signed_area(h).abs())
                .sum::<f64>();
        for holes in [
            region.holes.clone(),
            region.holes.iter().rev().cloned().collect(),
        ] {
            let tris = triangulate_region(&region.outer, &holes).unwrap().triangles;
            let area: f64 = tris
                .iter()
                .map(|t| tri_sign(t[0], t[1], t[2]).abs() * 0.5)
                .sum();
            assert!(
                (area - expected).abs() < 1e-6,
                "area={area}, expected={expected}"
            );
            // Test coverage as well as area: overlapping triangles must not
            // compensate for a missing wedge or paint over a hole.
            for x in -74..75 {
                for y in -24..25 {
                    let p = [x as f64 + 0.137, y as f64 + 0.319];
                    let covered = tris
                        .iter()
                        .filter(|t| point_strictly_in_tri(p, t[0], t[1], t[2]))
                        .count();
                    assert_eq!(covered, usize::from(point_in_region(p, region)), "at {p:?}");
                }
            }
        }
    }

    #[test]
    fn point_in_region_with_hole() {
        let region = Region2D {
            outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            holes: vec![vec![[3.0, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0]]],
        };
        assert!(point_in_region([5.0, 9.0], &region));
        assert!(!point_in_region([5.0, 5.0], &region));
        assert!(!point_in_region([20.0, 20.0], &region));
    }
}

// ==================== 多边形三角化（区域填充渲染用） ====================

#[cfg(test)]
fn tri_sign(pa: [f64; 2], pb: [f64; 2], pc: [f64; 2]) -> f64 {
    (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0])
}

#[cfg(test)]
fn point_strictly_in_tri(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let d1 = tri_sign(p, a, b);
    let d2 = tri_sign(p, b, c);
    let d3 = tri_sign(p, c, a);
    let eps = 1e-12;
    (d1 > eps && d2 > eps && d3 > eps) || (d1 < -eps && d2 < -eps && d3 < -eps)
}
