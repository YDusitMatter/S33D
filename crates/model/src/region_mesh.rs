//! Checked region triangulation. Backends are allowed to fail; partial meshes
//! are never returned. Coordinates and boundary vertices remain in f64.
use std::collections::BTreeMap;

type Point = [f64; 2];
type Triangle = [usize; 3];

pub const REGION_TRIANGULATION_BACKEND: &str = "earcut 0.4.11";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriangulationErrorKind {
    NoRegion,
    InvalidInput,
    BackendFailure,
    ValidationFailure,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "{kind:?}: {detail} (backend={backend}, rings={ring_count}, expected={expected_area:?}, actual={actual_area:?})"
)]
pub struct TriangulationError {
    pub backend: &'static str,
    pub kind: TriangulationErrorKind,
    pub detail: String,
    pub ring_count: usize,
    pub expected_area: Option<f64>,
    pub actual_area: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ValidatedRegionMesh {
    /// Exact input coordinates, outer first, then holes in input order.
    pub vertices: Vec<Point>,
    /// Counterclockwise triangles referring to `vertices`.
    pub indices: Vec<Triangle>,
    pub triangles: Vec<[Point; 3]>,
    pub expected_area: f64,
    pub actual_area: f64,
    pub backend: &'static str,
}

impl ValidatedRegionMesh {
    /// A representable point strictly inside an original-coordinate triangle.
    /// Prefer the largest usable triangle, so text classification and picking
    /// never depend on a rounded centroid of an almost collinear cap sliver.
    pub fn representable_interior_point(&self) -> Option<Point> {
        self.triangles
            .iter()
            .filter_map(|&t| triangle_interior_point(t).map(|p| (orient(t[0], t[1], t[2]), p)))
            .max_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, p)| p)
    }
}

fn orient(a: Point, b: Point, c: Point) -> f64 {
    let coord = |p: Point| robust::Coord { x: p[0], y: p[1] };
    robust::orient2d(coord(a), coord(b), coord(c))
}

fn on_segment(p: Point, a: Point, b: Point) -> bool {
    orient(a, b, p) == 0.0
        && p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

fn opposite(a: f64, b: f64) -> bool {
    (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0)
}

fn boxes_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    a[0].min(b[0]) <= c[0].max(d[0])
        && c[0].min(d[0]) <= a[0].max(b[0])
        && a[1].min(b[1]) <= c[1].max(d[1])
        && c[1].min(d[1]) <= a[1].max(b[1])
}

fn proper_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    boxes_intersect(a, b, c, d)
        && opposite(orient(a, b, c), orient(a, b, d))
        && opposite(orient(c, d, a), orient(c, d, b))
}

fn contact(a: Point, b: Point, c: Point, d: Point) -> bool {
    boxes_intersect(a, b, c, d)
        && (proper_cross(a, b, c, d)
            || on_segment(a, c, d)
            || on_segment(b, c, d)
            || on_segment(c, a, b)
            || on_segment(d, a, b))
}

fn inside(p: Point, ring: &[Point]) -> bool {
    // Winding-number test uses adaptive orientation, without ray division or
    // adding a small displacement to large absolute coordinates.
    let mut winding = 0i32;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        if a[1] <= p[1] {
            if b[1] > p[1] && orient(a, b, p) > 0.0 {
                winding += 1;
            }
        } else if b[1] <= p[1] && orient(a, b, p) < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

fn strict_in_triangle(p: Point, t: [Point; 3]) -> bool {
    orient(t[0], t[1], p) > 0.0 && orient(t[1], t[2], p) > 0.0 && orient(t[2], t[0], p) > 0.0
}

fn triangle_interior_point(t: [Point; 3]) -> Option<Point> {
    let p = [
        t[0][0] + ((t[1][0] - t[0][0]) + (t[2][0] - t[0][0])) / 3.0,
        t[0][1] + ((t[1][1] - t[0][1]) + (t[2][1] - t[0][1])) / 3.0,
    ];
    strict_in_triangle(p, t).then_some(p)
}

fn compensated_sum(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.;
    let mut correction = 0.;
    for value in values {
        let adjusted = value - correction;
        let next = sum + adjusted;
        correction = (next - sum) - adjusted;
        sum = next;
    }
    sum
}

fn ring_area(ring: &[Point]) -> f64 {
    compensated_sum((1..ring.len() - 1).map(|i| orient(ring[0], ring[i], ring[i + 1]) * 0.5))
}

fn determinant_products(t: [Point; 3]) -> f64 {
    ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1])).abs()
        + ((t[1][1] - t[0][1]) * (t[2][0] - t[0][0])).abs()
}

struct Input {
    vertices: Vec<Point>,
    normalized: Vec<Point>,
    rings: Vec<std::ops::Range<usize>>,
    expected: f64,
}

impl Input {
    fn error(
        &self,
        kind: TriangulationErrorKind,
        detail: impl Into<String>,
        actual: Option<f64>,
    ) -> TriangulationError {
        TriangulationError {
            backend: REGION_TRIANGULATION_BACKEND,
            kind,
            detail: detail.into(),
            ring_count: self.rings.len(),
            expected_area: (self.expected > 0.0).then_some(self.expected),
            actual_area: actual,
        }
    }
    fn material(&self, p: Point) -> bool {
        inside(p, &self.vertices[self.rings[0].clone()])
            && !self.rings[1..]
                .iter()
                .any(|r| inside(p, &self.vertices[r.clone()]))
    }
    fn boundary(&self) -> Vec<(usize, usize)> {
        self.rings
            .iter()
            .flat_map(|r| {
                r.clone()
                    .map(|i| (i, if i + 1 == r.end { r.start } else { i + 1 }))
            })
            .collect()
    }
}

fn prepare(outer: &[Point], holes: &[Vec<Point>]) -> Result<Input, TriangulationError> {
    let error = |kind, detail: &str| TriangulationError {
        backend: REGION_TRIANGULATION_BACKEND,
        kind,
        detail: detail.into(),
        ring_count: 1 + holes.len(),
        expected_area: None,
        actual_area: None,
    };
    if outer.is_empty() {
        return Err(error(
            TriangulationErrorKind::NoRegion,
            "empty outer boundary",
        ));
    }
    let mut vertices = Vec::new();
    let mut rings = Vec::new();
    for ring in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
        if ring.len() < 3 {
            return Err(error(
                TriangulationErrorKind::InvalidInput,
                "a boundary has fewer than three vertices",
            ));
        }
        if ring.iter().flatten().any(|x| !x.is_finite()) {
            return Err(error(
                TriangulationErrorKind::InvalidInput,
                "non-finite coordinate",
            ));
        }
        let start = vertices.len();
        vertices.extend_from_slice(ring);
        rings.push(start..vertices.len());
    }
    // The backend addresses nodes through u32 byte offsets. Refuse its panic
    // domain explicitly, before allocating its internal node storage.
    if vertices.len() > u32::MAX as usize / 64 {
        return Err(error(
            TriangulationErrorKind::InvalidInput,
            "too many boundary vertices",
        ));
    }
    let origin = outer[0];
    let extent = vertices
        .iter()
        .map(|p| (p[0] - origin[0]).abs().max((p[1] - origin[1]).abs()))
        .fold(0., f64::max);
    if extent <= 0. || !extent.is_finite() {
        return Err(error(
            TriangulationErrorKind::InvalidInput,
            "zero or unsupported coordinate extent",
        ));
    }
    // A local backend frame improves its conditioning, but translation may
    // round near-collinear points. Only original coordinates are certified.
    // The original vertices also remain untouched for cap sidewalls.
    let scale = extent.log2().ceil().exp2();
    if !scale.is_finite() || scale <= 0. || !(scale * scale).is_finite() || scale * scale == 0. {
        return Err(error(
            TriangulationErrorKind::InvalidInput,
            "coordinate extent exceeds supported finite-area range",
        ));
    }
    let normalized = vertices
        .iter()
        .map(|p| [(p[0] - origin[0]) / scale, (p[1] - origin[1]) / scale])
        .collect::<Vec<_>>();
    let mut input = Input {
        vertices,
        normalized,
        rings,
        expected: 0.,
    };
    for (ri, r) in input.rings.iter().enumerate() {
        let ring = &input.vertices[r.clone()];
        if !ring_area(ring).is_finite() || ring_area(ring) == 0. {
            return Err(input.error(
                TriangulationErrorKind::InvalidInput,
                format!("boundary {ri} has zero area"),
                None,
            ));
        }
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            if a == b {
                return Err(input.error(
                    TriangulationErrorKind::InvalidInput,
                    format!("boundary {ri} has duplicate consecutive vertices"),
                    None,
                ));
            }
            for j in i + 1..ring.len() {
                if j == i + 1 || (i == 0 && j == ring.len() - 1) {
                    continue;
                }
                if contact(a, b, ring[j], ring[(j + 1) % ring.len()]) {
                    return Err(input.error(
                        TriangulationErrorKind::InvalidInput,
                        format!("boundary {ri} self-intersects or touches itself"),
                        None,
                    ));
                }
            }
        }
    }
    for i in 0..input.rings.len() {
        for j in i + 1..input.rings.len() {
            let a = &input.vertices[input.rings[i].clone()];
            let b = &input.vertices[input.rings[j].clone()];
            if (0..a.len()).any(|ai| {
                (0..b.len())
                    .any(|bi| contact(a[ai], a[(ai + 1) % a.len()], b[bi], b[(bi + 1) % b.len()]))
            }) {
                return Err(input.error(
                    TriangulationErrorKind::InvalidInput,
                    format!("boundaries {i} and {j} intersect or touch"),
                    None,
                ));
            }
            if i == 0 && !inside(b[0], a) {
                return Err(input.error(
                    TriangulationErrorKind::InvalidInput,
                    format!("hole {j} is outside the outer boundary"),
                    None,
                ));
            }
            if i > 0 && (inside(a[0], b) || inside(b[0], a)) {
                return Err(input.error(
                    TriangulationErrorKind::InvalidInput,
                    "holes overlap or contain each other",
                    None,
                ));
            }
        }
    }
    input.expected = ring_area(&input.vertices[input.rings[0].clone()]).abs()
        - compensated_sum(
            input.rings[1..]
                .iter()
                .map(|r| ring_area(&input.vertices[r.clone()]).abs()),
        );
    if input.expected <= 0. || !input.expected.is_finite() {
        return Err(input.error(
            TriangulationErrorKind::InvalidInput,
            "non-positive material area",
            None,
        ));
    }
    Ok(input)
}

fn restore_boundary_vertices(points: &[Point], triangles: &mut Vec<Triangle>) {
    // Earcut may omit collinear boundary nodes. Split every incident edge at
    // those exact nodes, so caps still pair with the unmodified sidewall ring.
    for p in 0..points.len() {
        let count = triangles.len();
        for ti in 0..count {
            let t = triangles[ti];
            if t.contains(&p) {
                continue;
            }
            for edge in 0..3 {
                let a = t[edge];
                let b = t[(edge + 1) % 3];
                let c = t[(edge + 2) % 3];
                if on_segment(points[p], points[a], points[b]) {
                    triangles[ti] = [a, p, c];
                    triangles.push([p, b, c]);
                    break;
                }
            }
        }
    }
}

fn validate(input: &Input, triangles: &[Triangle]) -> Result<f64, TriangulationError> {
    let fail = |detail: String, actual| {
        input.error(TriangulationErrorKind::ValidationFailure, detail, actual)
    };
    if triangles.is_empty() {
        return Err(fail("backend returned no triangles".into(), Some(0.)));
    }
    let p = &input.vertices;
    let mut edges: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    let boundary = input.boundary();
    let mut expected_edges = BTreeMap::new();
    for (ri, ring) in input.rings.iter().enumerate() {
        let forward = (ri == 0) == (ring_area(&p[ring.clone()]) > 0.0);
        for a in ring.clone() {
            let b = if a + 1 == ring.end { ring.start } else { a + 1 };
            expected_edges.insert((a.min(b), a.max(b)), if forward { (a, b) } else { (b, a) });
        }
    }
    let mut areas = Vec::with_capacity(triangles.len());
    for (ti, t) in triangles.iter().enumerate() {
        if t.iter().any(|&i| i >= p.len()) {
            return Err(fail(
                format!("triangle {ti} has an out-of-range index"),
                None,
            ));
        }
        let twice = orient(p[t[0]], p[t[1]], p[t[2]]);
        if twice <= 0. || !twice.is_finite() {
            return Err(fail(
                format!("triangle {ti} is degenerate or has inconsistent winding"),
                None,
            ));
        }
        areas.push(twice * 0.5);
        // A rounded centroid of a tiny valid triangle can leave that same
        // triangle. Such a value is not a geometric witness. Coverage is
        // certified independently by the oriented boundary chain below.
        if triangle_interior_point(t.map(|i| p[i])).is_some_and(|center| !input.material(center)) {
            return Err(fail(
                format!("triangle {ti} is outside the selected material"),
                None,
            ));
        }
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push((a, b));
            for &(c, d) in &boundary {
                if proper_cross(p[a], p[b], p[c], p[d]) {
                    return Err(fail(
                        format!("triangle {ti} crosses a material boundary"),
                        None,
                    ));
                }
            }
        }
    }
    // A positive, nonoverlapping planar triangle chain whose full directed
    // boundary equals outer-CCW + holes-CW has exactly the material's winding
    // everywhere off the boundary. Opposite internal edge pairs cancel. This
    // certifies coverage even where no f64 interior sample is representable;
    // an undirected boundary count or matching total area would not suffice.
    for (edge, direction) in &expected_edges {
        if edges.get(edge).map(Vec::as_slice) != Some(std::slice::from_ref(direction)) {
            return Err(fail(
                format!("missing, multiply covered, or incorrectly directed material boundary edge {edge:?}"),
                None,
            ));
        }
    }
    for (key, uses) in &edges {
        if expected_edges.contains_key(key) {
            continue;
        }
        if uses.len() != 2 || uses[0] != (uses[1].1, uses[1].0) {
            return Err(fail(
                format!("unpaired or overlapping internal edge {key:?}"),
                None,
            ));
        }
    }
    // Area equality alone can hide compensating overlaps and gaps. Check
    // positive-area overlap independently, including nested/identical faces.
    for i in 0..triangles.len() {
        for j in i + 1..triangles.len() {
            let a = triangles[i].map(|k| p[k]);
            let b = triangles[j].map(|k| p[k]);
            let bounds = |t: [Point; 3]| {
                [
                    t[0][0].min(t[1][0]).min(t[2][0]),
                    t[0][1].min(t[1][1]).min(t[2][1]),
                    t[0][0].max(t[1][0]).max(t[2][0]),
                    t[0][1].max(t[1][1]).max(t[2][1]),
                ]
            };
            let ab = bounds(a);
            let bb = bounds(b);
            if ab[2] <= bb[0] || bb[2] <= ab[0] || ab[3] <= bb[1] || bb[3] <= ab[1] {
                continue;
            }
            if a.iter().any(|&q| strict_in_triangle(q, b))
                || b.iter().any(|&q| strict_in_triangle(q, a))
                || triangle_interior_point(a).is_some_and(|center| strict_in_triangle(center, b))
                || triangle_interior_point(b).is_some_and(|center| strict_in_triangle(center, a))
                || (0..3).any(|ai| {
                    (0..3).any(|bi| proper_cross(a[ai], a[(ai + 1) % 3], b[bi], b[(bi + 1) % 3]))
                })
            {
                return Err(fail(format!("triangles {i} and {j} overlap"), None));
            }
        }
    }
    let actual = compensated_sum(areas.into_iter());
    // A very thin annulus subtracts almost equal ring areas. Bound subtraction
    // roundoff by the gross boundary area, while topology/coverage checks above
    // remain mandatory even when the net material area is tiny.
    // Adaptive orientation guarantees the determinant's sign, not a uniformly
    // small relative error in its magnitude. Bound cancellation by the absolute
    // original-coordinate products in both boundary fans and mesh triangles.
    let gross_products = compensated_sum(
        input
            .rings
            .iter()
            .flat_map(|r| {
                (r.start + 1..r.end - 1).map(|i| determinant_products([p[r.start], p[i], p[i + 1]]))
            })
            .chain(
                triangles
                    .iter()
                    .map(|t| determinant_products(t.map(|i| p[i]))),
            ),
    );
    let area_tolerance = input.expected * 1e-10 + gross_products * f64::EPSILON * 64.0;
    if !actual.is_finite()
        || !area_tolerance.is_finite()
        || (actual - input.expected).abs() > area_tolerance
    {
        return Err(fail(
            "triangle area does not equal outer area minus all holes".into(),
            Some(actual),
        ));
    }
    Ok(actual)
}

/// Triangulate an outer boundary with all its holes, or reject the whole mesh.
/// Accepts either winding, but rejects open/degenerate/self-touching rings and
/// holes that touch, overlap, nest, or leave the outer boundary.
pub fn triangulate_region(
    outer: &[Point],
    holes: &[Vec<Point>],
) -> Result<ValidatedRegionMesh, TriangulationError> {
    let input = prepare(outer, holes)?;
    let (indices, actual) = match backend_candidate(&input, &input.normalized) {
        Ok(mesh) => mesh,
        Err(first) => {
            // Translation can change a nearly collinear original predicate.
            // Try an exact power-of-two scale without subtraction; each frame
            // still has to pass the complete certificate on original vertices.
            let magnitude = input
                .vertices
                .iter()
                .flatten()
                .map(|v| v.abs())
                .fold(0.0, f64::max);
            let scale = magnitude.log2().floor().exp2();
            let scaled = input
                .vertices
                .iter()
                .map(|p| [p[0] / scale, p[1] / scale])
                .collect::<Vec<_>>();
            match backend_candidate(&input, &scaled) {
                Ok(mesh) => mesh,
                Err(mut last) => {
                    last.detail = format!(
                        "translated frame: {}; scaled-original frame: {}",
                        first.detail, last.detail
                    );
                    return Err(last);
                }
            }
        }
    };
    let triangles = indices
        .iter()
        .map(|t| t.map(|i| input.vertices[i]))
        .collect();
    Ok(ValidatedRegionMesh {
        vertices: input.vertices,
        indices,
        triangles,
        expected_area: input.expected,
        actual_area: actual,
        backend: REGION_TRIANGULATION_BACKEND,
    })
}

fn backend_candidate(
    input: &Input,
    frame: &[Point],
) -> Result<(Vec<Triangle>, f64), TriangulationError> {
    let mut flat = Vec::<usize>::new();
    let starts = input.rings[1..].iter().map(|r| r.start).collect::<Vec<_>>();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        earcut::Earcut::new().earcut(frame.iter().copied(), &starts, &mut flat);
    }));
    if result.is_err() {
        return Err(input.error(
            TriangulationErrorKind::BackendFailure,
            "earcut panicked",
            None,
        ));
    }
    if flat.len() % 3 != 0 || flat.iter().any(|&i| i >= input.vertices.len()) {
        return Err(input.error(
            TriangulationErrorKind::BackendFailure,
            "earcut returned malformed indices",
            None,
        ));
    }
    let mut indices = flat
        .chunks_exact(3)
        .map(|t| [t[0], t[1], t[2]])
        .collect::<Vec<_>>();
    for t in &mut indices {
        if orient(
            input.vertices[t[0]],
            input.vertices[t[1]],
            input.vertices[t[2]],
        ) < 0.
        {
            t.swap(1, 2);
        }
    }
    restore_boundary_vertices(&input.vertices, &mut indices);
    let actual = validate(input, &indices)?;
    Ok((indices, actual))
}

/// Convenience for cap consumers that need coordinates rather than indices.
pub fn checked_triangles(
    outer: &[Point],
    holes: &[Vec<Point>],
) -> Result<Vec<[Point; 3]>, TriangulationError> {
    triangulate_region(outer, holes).map(|mesh| mesh.triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    }
    fn circle(x: f64, y: f64, r: f64, n: usize) -> Vec<Point> {
        (0..n)
            .map(|i| {
                let angle = std::f64::consts::TAU * i as f64 / n as f64;
                [x + r * angle.cos(), y + r * angle.sin()]
            })
            .collect()
    }

    #[test]
    fn rejects_invalid_boundaries_instead_of_partial_success() {
        let outer = rect(0., 0., 10., 10.);
        let cases = vec![
            (Vec::new(), vec![], TriangulationErrorKind::NoRegion),
            (
                vec![[0., 0.], [1., 1.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                vec![[0., 0.], [1., 0.], [f64::NAN, 1.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                vec![[0., 0.], [1., 0.], [f64::INFINITY, 1.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                vec![[0., 0.], [1., 0.], [2., 0.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                vec![[0., 0.], [2., 2.], [0., 2.], [2., 0.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                vec![[0., 0.], [2., 0.], [2., 0.], [0., 2.]],
                vec![],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![vec![]],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![rect(9., 9., 2., 2.)],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![rect(0., 2., 2., 2.)],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![rect(3., 3., 2., 2.), rect(5., 3., 2., 2.)],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![rect(3., 3., 3., 3.), rect(4., 4., 3., 3.)],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer.clone(),
                vec![rect(2., 2., 6., 6.), rect(3., 3., 2., 2.)],
                TriangulationErrorKind::InvalidInput,
            ),
            (
                outer,
                vec![rect(20., 20., 2., 2.)],
                TriangulationErrorKind::InvalidInput,
            ),
        ];
        for (outer, holes, kind) in cases {
            let error = triangulate_region(&outer, &holes).unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.backend, REGION_TRIANGULATION_BACKEND);
            assert!(error.to_string().contains(REGION_TRIANGULATION_BACKEND));
        }
    }

    #[test]
    fn cap_keeps_all_collinear_sidewall_edges() {
        let outer = vec![
            [0., 0.],
            [1., 0.],
            [2., 0.],
            [3., 0.],
            [3., 3.],
            [0., 3.],
            [0., 2.],
            [0., 1.],
        ];
        let hole = vec![[1., 1.], [1.5, 1.], [2., 1.], [2., 2.], [1., 2.]];
        let mesh = triangulate_region(&outer, &[hole.clone()]).unwrap();
        assert_eq!(mesh.vertices, [outer, hole].concat());
        assert_eq!(mesh.indices.len(), mesh.vertices.len() + 2 - 2);
        assert_eq!(mesh.actual_area, 8.);
    }

    #[test]
    fn thin_positive_material_is_not_rejected_by_subtracted_area_roundoff() {
        let outer = rect(0., 0., 100., 100.);
        let hole = rect(1e-8, 1e-8, 100. - 2e-8, 100. - 2e-8);
        let mesh = triangulate_region(&outer, &[hole]).unwrap();
        assert!((mesh.actual_area - 4e-6).abs() < 1e-11);
    }

    #[test]
    fn validator_rejects_missing_hole_partial_reversed_duplicate_and_overlapping_meshes() {
        let outer = rect(0., 0., 10., 10.);
        let input = prepare(&outer, &[rect(3., 3., 2., 2.)]).unwrap();
        assert!(
            validate(&input, &[[0, 1, 2], [0, 2, 3]]).is_err(),
            "hole was ignored"
        );
        let mesh = triangulate_region(&outer, &[rect(3., 3., 2., 2.)]).unwrap();
        let mut bad = mesh.indices.clone();
        bad.pop();
        assert!(validate(&input, &bad).is_err(), "partial mesh accepted");
        let mut bad = mesh.indices.clone();
        bad[0].swap(0, 1);
        assert!(
            validate(&input, &bad).is_err(),
            "reversed triangle accepted"
        );
        let mut bad = mesh.indices.clone();
        bad.push(bad[0]);
        assert!(validate(&input, &bad).is_err(), "overlap accepted");
        let mut bad = mesh.indices;
        bad[0][0] = usize::MAX;
        assert!(
            validate(&input, &bad).is_err(),
            "out-of-range index accepted"
        );
    }

    #[test]
    fn normalized_backend_frame_cannot_reverse_returned_original_triangles() {
        let outer = vec![
            [-59.16729559300637, 0.5192378864668827],
            [30., 0.5192378864668827],
            [30., 60.],
            [-59.16729559300637, 60.],
        ];
        let hole = vec![
            [-11.079150514707639, 27.61271222864038],
            [-10.910697959923635, 27.425672478801495],
            [-9.506777199033598, 25.866841933030365],
            [-14., 24.],
            [-14., 29.],
        ];
        // Translating these first three hole vertices to the backend frame
        // reverses their exact orientation. Certify the untouched source frame.
        let mesh = triangulate_region(&outer, &[hole]).unwrap();
        assert!(mesh.triangles.iter().all(|t| orient(t[0], t[1], t[2]) > 0.));
    }

    #[test]
    fn equal_area_gap_overlap_compensation_and_filled_hole_are_rejected() {
        let outer = rect(0., 0., 10., 10.);
        let hole = rect(4., 4., 2., 2.);
        let input = prepare(&outer, &[hole.clone()]).unwrap();
        let mesh = triangulate_region(&outer, &[hole]).unwrap();
        let mut corrupted = None;
        for i in 0..mesh.indices.len() {
            for j in i + 1..mesh.indices.len() {
                let a = mesh.indices[i].map(|k| input.vertices[k]);
                let b = mesh.indices[j].map(|k| input.vertices[k]);
                if orient(a[0], a[1], a[2]) == orient(b[0], b[1], b[2]) {
                    let mut bad = mesh.indices.clone();
                    bad[i] = bad[j];
                    corrupted = Some(bad);
                    break;
                }
            }
            if corrupted.is_some() {
                break;
            }
        }
        let bad = corrupted.expect("symmetric mesh has equal-area triangles");
        let actual = compensated_sum(bad.iter().map(|t| {
            orient(
                input.vertices[t[0]],
                input.vertices[t[1]],
                input.vertices[t[2]],
            ) * 0.5
        }));
        assert_eq!(
            actual, input.expected,
            "attack must pass an area-only validator"
        );
        assert!(validate(&input, &bad).is_err());
        // CCW hole-fill triangles oppose the required material-left CW chain.
        assert!(validate(&input, &[[4, 5, 6], [4, 6, 7]]).is_err());
    }

    #[test]
    fn unrepresentable_sliver_centroid_never_becomes_an_interior_witness() {
        let triangle = [
            [-11.292294326408348, 26.90279884683966],
            [-11.449248627204332, 26.698252019665784],
            [-11.856835870610151, 26.167073982412916],
        ];
        assert!(orient(triangle[0], triangle[1], triangle[2]) > 0.);
        let certified = triangulate_region(&triangle, &[]).unwrap();
        assert_eq!(certified.vertices, triangle);
        assert!(certified.actual_area > 0.0);
        let mesh = ValidatedRegionMesh {
            vertices: triangle.to_vec(),
            indices: vec![[0, 1, 2]],
            triangles: vec![triangle],
            expected_area: 0.,
            actual_area: 0.,
            backend: REGION_TRIANGULATION_BACKEND,
        };
        if let Some(p) = mesh.representable_interior_point() {
            assert!(strict_in_triangle(p, triangle));
        }
        let large = [[0., 0.], [10., 0.], [0., 10.]];
        let mut mesh = mesh;
        mesh.triangles.push(large);
        let p = mesh.representable_interior_point().unwrap();
        assert!(
            strict_in_triangle(p, large),
            "largest reliable original triangle must supply the witness"
        );
    }

    #[test]
    fn complete_coverage_survives_winding_rotation_hole_order_scale_and_translation() {
        let outer = rect(0., 0., 100., 100.);
        let holes = vec![
            circle(65., 95., 1., 72),
            circle(85., 52., 5., 72),
            rect(15., 20., 10., 30.),
        ];
        for scale in [0.001, 1., 1000.] {
            for offset in [0., 1e6, 1e9] {
                for variant in 0..4 {
                    let transform = |p: Point| [p[0] * scale + offset, p[1] * scale + offset];
                    let mut o = outer.iter().copied().map(transform).collect::<Vec<_>>();
                    let mut h = holes
                        .iter()
                        .map(|r| r.iter().copied().map(transform).collect::<Vec<_>>())
                        .collect::<Vec<_>>();
                    o.rotate_left(variant % outer.len());
                    if variant & 1 != 0 {
                        o.reverse();
                        h.reverse();
                    }
                    for ring in &mut h {
                        ring.rotate_left(variant);
                        if variant & 2 != 0 {
                            ring.reverse();
                        }
                    }
                    let mesh = triangulate_region(&o, &h).unwrap_or_else(|e| {
                        panic!("scale={scale}, offset={offset}, variant={variant}: {e}")
                    });
                    assert!(
                        (mesh.actual_area - mesh.expected_area).abs() <= mesh.expected_area * 1e-10
                    );
                    // Independent sample coverage: missing and overlapping regions
                    // cannot cancel each other through area equality.
                    let normalized = mesh
                        .triangles
                        .iter()
                        .map(|t| t.map(|p| [(p[0] - offset) / scale, (p[1] - offset) / scale]))
                        .collect::<Vec<_>>();
                    for x in 0..20 {
                        for y in 0..20 {
                            let p = [x as f64 * 5. + 0.137, y as f64 * 5. + 0.319];
                            let expected = inside(p, &outer) && !holes.iter().any(|h| inside(p, h));
                            let coverage = normalized
                                .iter()
                                .filter(|&&t| strict_in_triangle(p, t))
                                .count();
                            assert_eq!(
                                coverage,
                                usize::from(expected),
                                "at {p:?} scale={scale} offset={offset}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fixed_seed_many_holes_and_narrow_concave_bridge_remain_complete() {
        let outer = vec![
            [0., 0.],
            [100., 0.],
            [100., 100.],
            [50.00001, 100.],
            [50.00001, 40.],
            [50., 40.],
            [50., 100.],
            [0., 100.],
        ];
        let mut state = 0x213ac54u64;
        for _ in 0..12 {
            let mut holes = Vec::new();
            for row in 0..3 {
                for col in 0..4 {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let radius = 1. + ((state >> 32) % 1000) as f64 / 1000.;
                    holes.push(circle(
                        10. + col as f64 * 25.,
                        10. + row as f64 * 30.,
                        radius,
                        16,
                    ));
                }
            }
            let mesh = triangulate_region(&outer, &holes).unwrap();
            assert!((mesh.actual_area - mesh.expected_area).abs() < 1e-7);
        }
    }
}
