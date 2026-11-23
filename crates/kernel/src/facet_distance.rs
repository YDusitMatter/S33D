//! Bound numerical conditioning against the original boundary in both directions.
use crate::faceted::Facets;
use glam::DVec3;

struct Node {
    lo: DVec3,
    hi: DVec3,
    children: Option<(Box<Node>, Box<Node>)>,
    triangles: Vec<[DVec3; 3]>,
}
impl Node {
    fn new(mut triangles: Vec<[DVec3; 3]>) -> Self {
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for tri in &triangles {
            for &p in tri {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        let children = if triangles.len() > 8 {
            let d = hi - lo;
            let axis = if d.x >= d.y && d.x >= d.z {
                0
            } else if d.y >= d.z {
                1
            } else {
                2
            };
            let middle = triangles.len() / 2;
            triangles.select_nth_unstable_by(middle, |a, b| {
                let center = |t: &[DVec3; 3]| t[0][axis] + t[1][axis] + t[2][axis];
                center(a).total_cmp(&center(b))
            });
            let right = triangles.split_off(middle);
            Some((
                Box::new(Self::new(std::mem::take(&mut triangles))),
                Box::new(Self::new(right)),
            ))
        } else {
            None
        };
        Self {
            lo,
            hi,
            children,
            triangles,
        }
    }
    fn near(&self, p: DVec3, distance2: f64) -> bool {
        self.near_matching(p, distance2, &|_| true)
    }

    fn near_matching(
        &self,
        p: DVec3,
        distance2: f64,
        matches: &impl Fn([DVec3; 3]) -> bool,
    ) -> bool {
        if (p - p.clamp(self.lo, self.hi)).length_squared() > distance2 {
            return false;
        }
        if let Some((left, right)) = &self.children {
            return left.near_matching(p, distance2, matches)
                || right.near_matching(p, distance2, matches);
        }
        self.triangles
            .iter()
            .any(|&t| matches(t) && point_triangle_distance2(p, t) <= distance2)
    }
}
fn segment_distance2(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let edge = b - a;
    let len = edge.length_squared();
    let t = if len > 0. {
        ((p - a).dot(edge) / len).clamp(0., 1.)
    } else {
        0.
    };
    (p - a - edge * t).length_squared()
}
fn point_triangle_distance2(p: DVec3, [a, b, c]: [DVec3; 3]) -> f64 {
    let ab = b - a;
    let ac = c - a;
    let normal = ab.cross(ac);
    let area2 = normal.length_squared();
    if area2 > 0. {
        let projected = p - normal * ((p - a).dot(normal) / area2);
        let u = (b - projected).cross(c - projected).dot(normal);
        let v = (c - projected).cross(a - projected).dot(normal);
        let w = (a - projected).cross(b - projected).dot(normal);
        if u >= 0. && v >= 0. && w >= 0. {
            return (p - projected).length_squared();
        }
    }
    segment_distance2(p, a, b)
        .min(segment_distance2(p, b, c))
        .min(segment_distance2(p, c, a))
}
fn triangles(facets: &Facets) -> Vec<[DVec3; 3]> {
    facets
        .indices
        .chunks_exact(3)
        .map(|t| std::array::from_fn(|j| DVec3::from_array(facets.positions[t[j] as usize])))
        .collect()
}
fn direction(
    source: &[[DVec3; 3]],
    target: Vec<[DVec3; 3]>,
    distance: f64,
    folded_source: Option<(&Node, f64)>,
) -> bool {
    let tree = Node::new(target);
    source.iter().all(|&[a, b, c]| {
        let cross = (b - a).cross(c - a);
        let normal = cross.normalize_or_zero();
        let longest = (b - a).length_squared().max((c - b).length_squared()).max((a - c).length_squared());
        [
            a,
            b,
            c,
            (a + b) * 0.5,
            (b + c) * 0.5,
            (c + a) * 0.5,
            (a + b + c) / 3.,
        ]
        .iter()
        .all(|&p| {
            let near = tree.near(p, distance * distance)
                || folded_source.is_some_and(|(source, epsilon)| {
                    let epsilon2 = epsilon * epsilon;
                    let faces_against = |normal: DVec3| {
                        source.near_matching(p, epsilon2, &|[d, e, f]| {
                            normal.dot((e - d).cross(f - d).normalize_or_zero()) < -0.9999
                        })
                    };
                    // A collapsed closure across a folded wall can have an
                    // unrelated normal. Even then require both wall sides.
                    faces_against(normal) || (cross.length_squared() <= longest * epsilon2
                        && source.near_matching(p, epsilon2, &|[d, e, f]| {
                            faces_against((e - d).cross(f - d).normalize_or_zero())
                        }))
                });
            #[cfg(test)]
            if !near && std::env::var_os("S33D_GEOMETRY_TIMINGS").is_some() {
                let mut limit = distance;
                while limit < 100. && !tree.near(p, limit * limit) {
                    limit *= 2.;
                }
                eprintln!(
                    "boundary sample {p:?}, triangle {a:?} {b:?} {c:?}, distance bracket [{},{}] mm",
                    limit / 2.,
                    limit
                );
            }
            near
        })
    })
}
pub(crate) fn within(
    source: &Facets,
    target: &Facets,
    distance: f64,
    folded_tolerance: Option<f64>,
) -> bool {
    let a = triangles(source);
    let b = triangles(target);
    let folded = folded_tolerance.map(|epsilon| (Node::new(a.clone()), epsilon));
    direction(
        &a,
        b.clone(),
        distance,
        folded.as_ref().map(|(node, epsilon)| (node, *epsilon)),
    ) && direction(&b, a, distance, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_opposite_near_coincident_source_faces_may_disappear() {
        let mut source = Facets {
            positions: vec![
                [0., 0., 0.],
                [1., 0., 0.],
                [0., 1., 0.],
                [0., 0., 1.],
                [1., 0., 1.],
                [0., 1., 1.],
                [0., 0., 1.000001],
                [1., 0., 1.000001],
                [0., 1., 1.000001],
            ],
            indices: vec![0, 1, 2, 3, 4, 5, 6, 8, 7],
        };
        let target = Facets {
            positions: source.positions[0..3].to_vec(),
            indices: vec![0, 1, 2],
        };
        assert!(!within(&source, &target, 8e-5, None));
        assert!(within(&source, &target, 8e-5, Some(1e-5)));
        assert!(!within(&target, &source, 8e-5, Some(1e-5)));
        source.indices.swap(7, 8);
        assert!(!within(&source, &target, 8e-5, Some(1e-5)));
        source.indices.swap(7, 8);
        for position in &mut source.positions[6..] {
            position[2] += 0.0001;
        }
        assert!(!within(&source, &target, 8e-5, Some(1e-5)));
    }
}
