//! Regression cases for open sketch edges entering closed material regions.
use super::*;
use crate::sketch::SketchEntity;

fn rect(x: f64, y: f64, w: f64, h: f64) -> SketchEntity {
    SketchEntity::Rect {
        c1: [x, y],
        c2: [x + w, y + h],
    }
}
fn line(a: [f64; 2], b: [f64; 2]) -> SketchEntity {
    SketchEntity::Line { a, b }
}
fn checked_material_areas(entities: &[SketchEntity]) -> Vec<f64> {
    let regions = compute_regions(entities);
    regions
        .iter()
        .enumerate()
        .map(|(i, r)| {
            triangulate_region(&r.outer, &r.holes)
                .unwrap_or_else(|e| {
                    panic!(
                        "region {i}, outer={}, holes={}: {e}",
                        r.outer.len(),
                        r.holes.len()
                    )
                })
                .actual_area
        })
        .collect()
}

#[test]
fn dangling_trees_do_not_become_material_boundaries() {
    let variants = vec![
        vec![line([0., 5.], [4., 5.])],
        vec![line([0., 5.], [-4., 5.])],
        vec![line([3., 5.], [7., 5.])],
        vec![
            line([0., 5.], [4., 5.]),
            line([4., 5.], [7., 7.]),
            line([4., 5.], [7., 3.]),
        ],
    ];
    for branches in variants {
        let mut entities = vec![rect(0., 0., 10., 10.)];
        entities.extend(branches);
        assert_eq!(checked_material_areas(&entities), vec![100.]);
    }
}

#[test]
fn bridges_between_rings_preserve_separate_outer_and_holes() {
    let cases = vec![
        (
            vec![
                rect(0., 0., 10., 10.),
                rect(3., 3., 4., 4.),
                line([0., 5.], [3., 5.]),
            ],
            1,
            vec![16., 84.],
        ),
        (
            vec![
                rect(0., 0., 10., 10.),
                rect(2., 3., 2., 4.),
                rect(6., 3., 2., 4.),
                line([4., 5.], [6., 5.]),
            ],
            2,
            vec![8., 8., 84.],
        ),
    ];
    for (entities, holes, expected) in cases {
        let regions = compute_regions(&entities);
        assert_eq!(regions.iter().map(|r| r.holes.len()).max(), Some(holes));
        assert_eq!(checked_material_areas(&entities), expected);
    }
}

#[test]
fn actual_two_ended_dividers_remain_in_the_arrangement() {
    let mut entities = vec![rect(0., 0., 10., 10.), line([-4., 5.], [14., 5.])];
    assert_eq!(checked_material_areas(&entities), vec![50., 50.]);
    entities.insert(1, rect(3., 3., 4., 4.));
    assert_eq!(checked_material_areas(&entities), vec![8., 8., 42., 42.]);
    let two_bridges = vec![
        rect(0., 0., 10., 10.),
        rect(3., 3., 4., 4.),
        line([0., 5.], [3., 5.]),
        line([7., 5.], [10., 5.]),
    ];
    assert_eq!(checked_material_areas(&two_bridges), vec![16., 42., 42.]);
}

#[test]
fn bridge_pruning_handles_deep_graphs_without_recursion() {
    let count = 20_000;
    let mut edges = (0..count).map(|i| (i, i + 1)).collect::<Vec<_>>();
    edges.extend([
        (count, count + 1),
        (count + 1, count + 2),
        (count + 2, count),
    ]);
    assert_eq!(
        remove_graph_bridges(count + 3, edges),
        vec![
            (count, count + 1),
            (count + 1, count + 2),
            (count + 2, count)
        ]
    );
}

#[test]
fn intersecting_curves_with_dangling_edges_keep_all_faces_checked() {
    let mut entities = vec![
        rect(0., 0., 10., 10.),
        SketchEntity::Circle { c: [8., 5.], r: 4. },
    ];
    let expected = checked_material_areas(&entities);
    entities.push(line([0., 5.], [2., 5.]));
    let actual = checked_material_areas(&entities);
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 1e-10);
    }
}

#[test]
fn real_selection_error_project_all_seventeen_regions_are_complete() {
    let entities: Vec<SketchEntity> = serde_json::from_str(include_str!(
        "../tests/fixtures/selection-error-entities.json"
    ))
    .unwrap();
    assert_eq!(entities.len(), 10);
    let regions = compute_regions(&entities);
    assert_eq!(regions.len(), 17);
    let meshes = regions
        .iter()
        .enumerate()
        .map(|(i, r)| {
            triangulate_region(&r.outer, &r.holes)
                .unwrap_or_else(|e| panic!("real project region {i}: {e}"))
        })
        .collect::<Vec<_>>();
    assert!(meshes
        .iter()
        .any(|m| (m.actual_area - 2217.879327149846).abs() < 1e-8));
    let chains = entities
        .iter()
        .flat_map(SketchEntity::to_chains)
        .filter_map(|(points, closed)| closed.then_some(points))
        .collect::<Vec<_>>();
    for x in -25..26 {
        for y in -23..24 {
            let p = [x as f64 * 2.57 + 0.139, y as f64 * 2.53 + 0.317];
            let expected = chains.iter().any(|ring| point_in_poly(p, ring));
            let count = meshes
                .iter()
                .flat_map(|m| &m.triangles)
                .filter(|t| point_strictly_in_tri(p, t[0], t[1], t[2]))
                .count();
            assert_eq!(
                count,
                usize::from(expected),
                "real project coverage at {p:?}"
            );
        }
    }
    // The last isolated open segment must not cause the selected face failure.
    let without_isolated = compute_regions(&entities[..9]);
    assert_eq!(without_isolated, regions);
}
