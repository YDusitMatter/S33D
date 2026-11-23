//! Regressions for continuous edge treatment and selection gestures.
use super::*;

fn box_app(ctx: &egui::Context) -> (S33DApp, BodyId) {
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.settings.show_tool_exit_confirmation = false;
    let result = app
        .execute_agent_request(AgentRequest::new(
            serde_json::json!(1),
            "primitive.create",
            serde_json::json!({"kind":"box", "width":20., "depth":20., "height":20.}),
        ))
        .unwrap();
    (app, result["id"].as_u64().unwrap())
}

fn volume(app: &mut S33DApp, id: BodyId) -> f64 {
    let shape = app.doc.body(id).unwrap().shape.unwrap();
    app.shape_measurements(shape).unwrap().3
}

#[test]
fn sketch_reference_lines_skip_split_triangle_seams() {
    let mesh = MeshData::new(
        vec![
            [0., 0., 0.],
            [0., 0., 1.],
            [1., 0., 1.],
            [0., 0., 0.],
            [1., 0., 1.],
            [1., 0., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    let mut reference = SketchReferenceGeometry::new(mesh, 22.);
    let eye = glam::Vec3::new(0.5, 10., 0.5);
    let (visible, hidden) = reference.lines(eye);
    assert_eq!(visible.len(), 8);
    assert!(hidden.is_empty());
    let visible_ptr = visible.as_ptr();
    assert_eq!(reference.lines(eye).0.as_ptr(), visible_ptr);
}

#[test]
#[ignore = "Run with S33D_PROJECT6 pointing to the user's regression project"]
fn project6_surface_sketch_entry_exit() {
    use glam::Vec3;
    use std::time::Instant;
    let timed = |label: &str, run: &mut dyn FnMut()| {
        let start = Instant::now();
        run();
        eprintln!("{label}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.);
    };
    let path = std::env::var_os("S33D_PROJECT6").expect("Set S33D_PROJECT6");
    let input = std::fs::read_to_string(path).unwrap();
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx));
    app.load_s33d_project(&input).unwrap();
    let original_bodies = app.doc.bodies.len();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
    app.viewport_rect = rect;
    app.camera.eye = Vec3::new(-7.57, 160., 31.13);
    app.camera.target = Vec3::new(-7.57, 20., 31.13);
    app.camera.up = -Vec3::Z;
    app.camera.ortho = true;
    app.camera.ortho_half = 85.;
    timed("First Idle scene (includes display weld)", &mut || {
        app.refresh_scene()
    });

    for draw in [false, true] {
        let mut face = None;
        timed("Select forward planar face", &mut || {
            face = app.pick_face(rect.center(), rect).map(|(face, _)| face)
        });
        let face = face.expect("pick Project 6 cap");
        assert!(face.normal[1] > 0.99 && face.area_mm2 > 100.);
        app.face_pick = Some(face.clone());
        timed("Surface menu profile check", &mut || {
            eprintln!("Bounded profile: {}", app.has_bounded_planar_selection());
        });
        timed("Sketch On", &mut || app.sketch_on_selected_profile());
        assert_eq!(app.mode, Mode::Sketch);
        assert_eq!(app.sketch_on_body, Some(face.body_id));
        timed("First Sketch scene", &mut || app.refresh_scene());
        let (line_vertices, marker_vertices) = {
            let scene = app.scene.lock().unwrap();
            (
                scene
                    .body_edge_lines
                    .iter()
                    .map(|group| group.positions.len())
                    .sum::<usize>(),
                scene.sketch_point_lines.positions.len(),
            )
        };
        eprintln!("Sketch reference line vertices: {line_vertices}");
        eprintln!("Sketch marker line vertices: {marker_vertices}");
        assert!(line_vertices < 300_000);
        assert!(marker_vertices > 0 && marker_vertices < 410_000);
        assert!(!app.sketch.as_ref().unwrap().external_key_points.is_empty());
        app.bump_scene();
        timed("Next Sketch scene", &mut || app.refresh_scene());
        if draw {
            app.sketch
                .as_mut()
                .unwrap()
                .entities
                .push(SketchEntity::Circle { c: [0.; 2], r: 1. });
        }
        timed("Exit Sketch", &mut || app.exit_sketch());
        assert_eq!(app.mode, Mode::Idle);
        assert_eq!(app.doc.bodies.len(), original_bodies + usize::from(draw));
        timed("Shaded scene after exit", &mut || app.refresh_scene());
        timed("Face hover after exit", &mut || {
            app.update_hover(rect.center(), rect)
        });
    }
}

#[test]
fn polygon_hole_rims_join_across_side_counts_and_corner_angles() {
    use glam::{DVec2, Vec3};
    let regular = |count: usize, phase: f64, radius: f64| {
        (0..count)
            .map(|i| {
                let angle = phase + std::f64::consts::TAU * i as f64 / count as f64;
                [radius * angle.cos(), radius * angle.sin()]
            })
            .collect::<Vec<_>>()
    };
    let profiles = [
        regular(3, 0.19, 30.),
        regular(4, 0.43, 30.),
        regular(5, 0., 30.),
        regular(5, 0.23, 30.),
        regular(7, 0.17, 30.),
        regular(15, 0.37, 30.),
        regular(37, 0.41, 30.),
        vec![
            [-25., -17.],
            [16., -24.],
            [29., 3.],
            [11., 25.],
            [-27., 16.],
        ],
        vec![[-30., -10.], [15., -6.], [45., 0.], [15., 6.], [-30., 10.]],
        vec![
            [-22., -20.],
            [28., -18.],
            [12., 1.],
            [27., 25.],
            [-19., 24.],
        ],
    ];
    for (case, profile) in profiles.into_iter().enumerate() {
        let ctx = egui::Context::default();
        let n = profile.len();
        let points = profile
            .iter()
            .copied()
            .map(DVec2::from_array)
            .collect::<Vec<_>>();
        let mut perimeter = 0.0;
        let mut corner_sum = 0.0;
        for i in 0..n {
            let incoming = points[i] - points[(i + n - 1) % n];
            let outgoing = points[(i + 1) % n] - points[i];
            perimeter += outgoing.length();
            corner_sum += (incoming.perp_dot(outgoing).atan2(incoming.dot(outgoing)) / 2.0).tan();
        }
        let distance: f64 = 2.0;
        // Integrate the area added by an offset w of the hole from w=0 to w=d.
        let removed = perimeter * distance.powi(2) / 2.0 + corner_sum * distance.powi(3) / 3.0;
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        let region = Region2D {
            outer: regular(96, 0.13, 65.),
            holes: vec![profile.into_iter().rev().collect()],
        };
        let source = app
            .kernel
            .extrude_region(&region, &DatumPlane::Top.def(), 20.)
            .unwrap();
        let root = app.doc.alloc_id();
        app.doc.add_body(Body {
            id: root,
            name: format!("Polygon hole {case}"),
            kind: BodyKind::Solid,
            feature: Feature::Imported {
                path: String::new(),
            },
            shape: Some(source),
            material: Material::default(),
            visible: true,
        });
        let original_mesh = app.kernel.tessellate(source).unwrap();
        let geometry = PickGeometry::new(original_mesh.clone());
        let seed = geometry
            .chains
            .iter()
            .flatten()
            .find(|edge| edge.a[1] > 19.9 && edge.b[1] > 19.9 && edge.a[0].hypot(edge.a[2]) < 55.)
            .unwrap();
        let (rim, _) = find_polygon_chamfer_rim(&geometry, std::slice::from_ref(seed), distance)
            .unwrap_or_else(|| panic!("case {case}, {n} sides: cannot identify the rim"));
        assert_eq!(rim.len(), n);
        app.camera.eye = Vec3::new(0., 150., 0.);
        app.camera.target = Vec3::ZERO;
        app.camera.up = -Vec3::Z;
        app.camera.ortho = true;
        app.camera.ortho_half = 75.;
        app.chamfer_active = true;
        app.chamfer_fillet = false;
        app.chamfer_param = distance as f32;
        pointer_frame(&ctx, &mut app, egui::Pos2::ZERO, None);
        let rect = app.viewport_rect;
        let project =
            app.camera.proj_matrix(rect.width() / rect.height()) * app.camera.view_matrix();
        let mut latest = root;
        let mut recovered_end_cap = false;
        let original_volume = volume(&mut app, root);
        for (i, edge) in rim.iter().enumerate() {
            let mid = (Vec3::from(edge.a) + Vec3::from(edge.b)) * 0.5;
            let wall_normal = if edge.n1[1].abs() < 0.1 {
                edge.n1
            } else {
                edge.n2
            };
            let ndc = project.project_point3(mid - Vec3::from(wall_normal) * 0.35);
            let pos = egui::pos2(
                rect.left() + (1. + ndc.x) * rect.width() / 2.,
                rect.top() + (1. - ndc.y) * rect.height() / 2.,
            );
            pointer_frame(&ctx, &mut app, pos, Some(true));
            pointer_frame(&ctx, &mut app, pos, Some(false));
            let picked = app
                .selected_edge
                .as_ref()
                .unwrap_or_else(|| panic!("case {case}: cannot pick side {i}"));
            assert_eq!(picked.body_id, latest);
            let sides = polygon_chamfer_rim_indices(&rim, &picked.segments).unwrap_or_else(|| {
                panic!("case {case}: pick did not return the original rim at side {i}")
            });
            assert_eq!(sides, vec![i], "case {case}: each click selects one side");
            if case == 2 && latest != root {
                let shape = app.doc.body(latest).unwrap().shape.unwrap();
                let geometry = app.picking_geometry(shape);
                let (eye, direction) = app.ray_from_ndc([ndc.x, ndc.y]);
                let (triangle, _) = ray_mesh_tri(eye, direction, &geometry.mesh).unwrap();
                let (chain, _, _) =
                    closest_edge_on_hit_face(&geometry, triangle, &project, rect, pos).unwrap();
                let raw = &geometry.chains[chain];
                if polygon_chamfer_rim_indices(&rim, raw).is_none() {
                    assert_eq!(
                        recover_polygon_chamfer_sides(&rim, raw, distance),
                        Some(sides.clone())
                    );
                    recovered_end_cap = true;
                }
            }
            app.apply_chamfer();
            assert_ne!(
                app.selected,
                vec![latest],
                "case {case} at side {i}: {:?}",
                app.history
            );
            latest = app.selected[0];
        }
        if case == 2 {
            assert!(
                recovered_end_cap,
                "five-sided hole must exercise a merged end cap"
            );
        }
        let actual = original_volume - volume(&mut app, latest);
        assert!(
            (actual - removed).abs() < 0.03,
            "case {case}: removed {actual} instead of {removed}"
        );
        let result = app.doc.body(latest).unwrap().shape.unwrap();
        assert_eq!(
            app.kernel.tessellate(result).unwrap().bounds(),
            original_mesh.bounds()
        );
    }
}

#[test]
fn chamfer_picks_the_complete_rim_of_a_sketch_circle() {
    use glam::Vec3;
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let plane = DatumPlane::Top.def();
    let outer = SketchEntity::Circle { c: [0.; 2], r: 50. };
    let hole = SketchEntity::Circle { c: [0.; 2], r: 22. };
    let (inner_ring, _) = hole.to_chain();
    let count = inner_ring.len();
    let region = Region2D {
        outer: outer.to_chain().0,
        holes: vec![inner_ring.into_iter().rev().collect()],
    };
    let source = app.kernel.extrude_region(&region, &plane, 14.).unwrap();
    let root = app.doc.alloc_id();
    app.doc.add_body(Body {
        id: root,
        name: "Circular hole".into(),
        kind: BodyKind::Solid,
        feature: Feature::Extrude {
            sketch: SketchFeature {
                plane,
                entities: vec![outer, hole],
                vertex_locks: Default::default(),
                world_xyz_coordinates: false,
                on_body: None,
            },
            params: ExtrudeParams {
                depth: 14.,
                symmetric: false,
                bevel: 0.,
            },
            source_sketch: None,
            pocket_source_sketches: Vec::new(),
            region_indices: Vec::new(),
        },
        shape: Some(source),
        material: Material::default(),
        visible: true,
    });
    app.chamfer_active = true;
    app.camera.eye = Vec3::new(0., 150., 0.);
    app.camera.target = Vec3::ZERO;
    app.camera.up = -Vec3::Z;
    app.camera.ortho = true;
    app.camera.ortho_half = 60.;
    pointer_frame(&ctx, &mut app, egui::Pos2::ZERO, None);
    let rect = app.viewport_rect;
    let pos = rect.center() + egui::vec2(22.3 * rect.height() / 120., 0.);
    pointer_frame(&ctx, &mut app, pos, Some(true));
    pointer_frame(&ctx, &mut app, pos, Some(false));
    assert_eq!(app.selected_edge.as_ref().unwrap().segments.len(), count);
    app.apply_chamfer();
    assert_ne!(app.selected, vec![root]);
}

#[test]
fn successive_hexagonal_hole_chamfers_share_the_six_miter_joints() {
    use glam::Vec3;
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    let polygon = |count, radius| {
        (0..count).map(move |i| {
            let angle = std::f64::consts::TAU * f64::from(i) / f64::from(count) + 0.23;
            [radius * angle.cos(), radius * angle.sin()]
        })
    };
    let region = Region2D {
        outer: polygon(96, 63.65).collect(),
        holes: vec![polygon(6, 30.0).rev().collect()],
    };
    let source = app
        .kernel
        .extrude_region(&region, &DatumPlane::Top.def(), 20.0)
        .unwrap();
    let root = app.doc.alloc_id();
    app.doc.add_body(Body {
        id: root,
        name: "Hexagonal hole".into(),
        kind: BodyKind::Solid,
        feature: Feature::Imported {
            path: String::new(),
        },
        shape: Some(source),
        material: Material::default(),
        visible: true,
    });
    let original = app.kernel.tessellate(source).unwrap();
    let geometry = PickGeometry::new(original.clone());
    let seed = geometry
        .chains
        .iter()
        .flatten()
        .find(|edge| edge.a[1] > 19.9 && edge.b[1] > 19.9 && Vec3::from(edge.a).length() < 40.0)
        .unwrap();
    let rim = geometry.inner_rim(seed).unwrap();
    assert_eq!(rim.len(), 6);
    app.camera.eye = Vec3::new(0., 150., 0.);
    app.camera.target = Vec3::ZERO;
    app.camera.up = -Vec3::Z;
    app.camera.ortho = true;
    app.camera.ortho_half = 75.0;
    app.chamfer_active = true;
    app.chamfer_fillet = false;
    app.chamfer_param = 2.;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 900.));
    app.viewport_rect = rect;
    let project = app.camera.proj_matrix(1.0) * app.camera.view_matrix();
    let mut latest = root;
    let mut operations = Vec::new();
    let mut prior_volume = volume(&mut app, root);
    // Pick from the visible result. Alternate across the opening to leave
    // disconnected partial runs before joining them and closing the last seam.
    for i in [0, 1, 4, 3, 2, 5] {
        let edge = &rim[i];
        let middle = (Vec3::from(edge.a) + Vec3::from(edge.b)) * 0.5;
        let toward_material = Vec3::new(middle.x, 0., middle.z).normalize();
        let ndc = project.project_point3(middle + toward_material * 0.2);
        let pos = egui::pos2((1. + ndc.x) * 450.0, (1. - ndc.y) * 450.0);
        let picked = app
            .pick_edge(pos, rect)
            .unwrap_or_else(|| panic!("cannot pick exposed upper edge {i} after {operations:?}"));
        assert_eq!(picked.body_id, latest);
        assert_eq!(
            polygon_chamfer_rim_indices(&rim, &picked.segments),
            Some(vec![i])
        );
        app.selected = vec![picked.body_id];
        app.selected_edge = Some(picked);
        app.apply_chamfer();
        assert_ne!(app.selected, vec![latest], "side {i}: {:?}", app.history);
        latest = app.selected[0];
        operations.push(latest);
        let result_volume = volume(&mut app, latest);
        assert!(result_volume < prior_volume - 45.0);
        prior_volume = result_volume;
    }
    let (whole, _) = app
        .evaluate_chamfer_feature(source, &rim, Some(2.), None)
        .unwrap();
    let whole_volume = app.shape_measurements(whole).unwrap().3;
    assert!((prior_volume - whole_volume).abs() < 0.02);
    let shape = app.doc.body(latest).unwrap().shape.unwrap();
    let mesh = app.kernel.tessellate(shape).unwrap();
    assert_eq!(mesh.bounds(), original.bounds());
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.positions[t[i] as usize]));
        let normal = (b - a).cross(c - a).normalize_or_zero();
        assert!(
            normal.y.abs() < 0.005
                || (normal.y.abs() - 1.0).abs() < 0.005
                || (normal.y - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "unexpected corner face normal {normal:?}"
        );
    }

    // Recreate the independent cutters used by older projects, including
    // shortened edge selectors left by an adjacent operation.
    let mut legacy_base = source;
    for (&id, i) in operations.iter().zip([0, 1, 4, 3, 2, 5]) {
        let mut edge = rim[i].clone();
        let a = Vec3::from(edge.a);
        let b = Vec3::from(edge.b);
        edge.a = a.lerp(b, 0.02).to_array();
        edge.b = a.lerp(b, 0.98).to_array();
        let (shape, _) = app
            .evaluate_chamfer_feature(legacy_base, std::slice::from_ref(&edge), Some(2.), None)
            .unwrap();
        let body = app.doc.body_mut(id).unwrap();
        body.shape = Some(shape);
        if let Feature::Chamfer { edges, .. } = &mut body.feature {
            *edges = model_chamfer_segments(&[edge]);
        }
        legacy_base = shape;
    }
    assert!((volume(&mut app, latest) - whole_volume).abs() > 0.1);
    let file = app.project_to_s33d().unwrap();
    app.load_s33d_project(&file).unwrap();
    let last = app.doc.body(latest).unwrap().clone();
    app.selected = vec![latest];
    app.restore_chamfer_from_feature(&last);
    app.apply_chamfer();
    assert_eq!(app.selected, vec![latest]);
    assert!((volume(&mut app, latest) - whole_volume).abs() < 0.02);
    app.delete_chamfer_operation(operations[2]).unwrap();
    assert!(volume(&mut app, latest) > whole_volume + 45.0);
}

#[test]
fn apply_updates_one_location_then_accumulates_the_next_without_done() {
    for fillet in [false, true] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        let mesh = app
            .kernel
            .tessellate(app.doc.body(root).unwrap().shape.unwrap())
            .unwrap();
        let chains = mesh_sharp_edge_segments(&mesh);
        let first_edge = chains.iter().find(|c| c.len() == 1).unwrap().clone();
        let center = |e: &[EdgeSeg]| (glam::Vec3::from(e[0].a) + glam::Vec3::from(e[0].b)) * 0.5;
        let opposite_edge = chains
            .iter()
            .filter(|c| c.len() == 1)
            .max_by(|a, b| {
                center(a)
                    .distance_squared(center(&first_edge))
                    .total_cmp(&center(b).distance_squared(center(&first_edge)))
            })
            .unwrap()
            .clone();
        app.chamfer_active = true;
        app.chamfer_fillet = fillet;
        app.chamfer_param = 1.;
        app.selected_edge = Some(EdgeRef {
            body_id: root,
            segments: first_edge,
        });
        app.selected = vec![root];
        let original = volume(&mut app, root);
        app.apply_chamfer();
        assert_eq!(app.doc.bodies.len(), 2, "first Apply: {:?}", app.history);
        let first = app.selected[0];
        let first_volume = volume(&mut app, first);
        assert!(first_volume < original - 1.);

        app.chamfer_param = 2.;
        app.apply_chamfer();
        assert_eq!(app.doc.bodies.len(), 2, "same location updates in place");
        assert_eq!(app.selected, vec![first]);
        let updated = volume(&mut app, first);
        assert!(updated < first_volume - 1.);

        app.selected_edge = Some(EdgeRef {
            body_id: first,
            segments: opposite_edge,
        });
        app.apply_chamfer();
        assert_eq!(
            app.doc.bodies.len(),
            3,
            "new location must keep the first feature"
        );
        let second = app.selected[0];
        assert_ne!(first, second);
        assert!(
            matches!(app.doc.body(second).unwrap().feature, Feature::Chamfer { base: Some(b), .. } if b == first)
        );
        assert!(volume(&mut app, second) < updated - 1.);
        assert_eq!(
            app.doc
                .bodies
                .iter()
                .filter(|b| b.visible)
                .map(|b| b.id)
                .collect::<Vec<_>>(),
            vec![second]
        );
        assert_eq!(
            volume(&mut app, first),
            updated,
            "previous feature is retained"
        );

        let before_update = volume(&mut app, second);
        app.chamfer_param = 0.7;
        app.apply_chamfer();
        assert_eq!(app.doc.bodies.len(), 3);
        assert_eq!(app.selected, vec![second]);
        assert!(volume(&mut app, second) > before_update + 1.);
        let before_exit = app.doc.bodies.clone();
        app.chamfer_param = 1.; // Exit accepts the latest pending edit.
        app.exit_chamfer_tool();
        assert!(!app.chamfer_active && app.selected_edge.is_none());
        let final_doc = app.doc.bodies.clone();
        assert_ne!(final_doc, before_exit);
        assert!(matches!(
            app.doc.body(second).unwrap().feature,
            Feature::Chamfer {
                chamfer: Some(1.),
                ..
            } | Feature::Chamfer {
                fillet: Some(1.),
                ..
            }
        ));
        assert!(app.undo());
        assert_eq!(app.doc.bodies, before_exit);
        assert!(app.redo());
        assert_eq!(app.doc.bodies, final_doc);
    }
}

fn pointer_frame(ctx: &egui::Context, app: &mut S33DApp, pos: egui::Pos2, pressed: Option<bool>) {
    pointer_frame_modified(ctx, app, pos, pressed, egui::Modifiers::NONE);
}

fn pointer_frame_modified(
    ctx: &egui::Context,
    app: &mut S33DApp,
    pos: egui::Pos2,
    pressed: Option<bool>,
    modifiers: egui::Modifiers,
) {
    let mut events = vec![egui::Event::PointerMoved(pos)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            app.tool_exit_prompt_ui(ctx);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let response = ui.allocate_rect(ui.max_rect(), egui::Sense::click_and_drag());
                    app.viewport_rect = response.rect;
                    app.viewport_pointer(&response);
                });
        },
    );
}

#[test]
fn double_and_triple_click_exit_edge_tool_and_select_the_actual_face_and_object() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    let mesh = app
        .kernel
        .tessellate(app.doc.body(root).unwrap().shape.unwrap())
        .unwrap();
    let edge = mesh_sharp_edge_segments(&mesh)
        .into_iter()
        .find(|c| c.len() == 1)
        .unwrap();
    app.selected_edge = Some(EdgeRef {
        body_id: root,
        segments: edge,
    });
    app.selected = vec![root];
    app.chamfer_active = true;
    app.chamfer_param = 1.;
    app.apply_chamfer();
    let result = app.selected[0];
    let doc = app.doc.bodies.clone();
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    app.camera.up = glam::Vec3::Y;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for click in 1..=3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
        match click {
            1 => assert!(app.chamfer_active),
            2 => {
                assert!(!app.chamfer_active);
                assert_eq!(app.face_pick.as_ref().map(|f| f.body_id), Some(result));
                assert!(app.selected_edge.is_none());
            }
            _ => {
                assert!(!app.chamfer_active);
                assert_eq!(app.selected, vec![result]);
                assert!(app.face_pick.is_none() && app.selected_edge.is_none());
            }
        }
        assert_eq!(app.doc.bodies, doc, "gestures must retain applied geometry");
    }
}

/// Input must be an independent copy. Optional PNG output visualizes the
/// actual scene highlight mesh; this check never writes the input project.
#[test]
#[ignore = "requires S33D_REGION_FIXTURE pointing to a copied user project"]
fn copied_region_fixture_has_complete_scene_highlight() {
    let input = std::path::PathBuf::from(
        std::env::var_os("S33D_REGION_FIXTURE").expect("Set S33D_REGION_FIXTURE"),
    );
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx));
    let original = std::fs::read(&input).unwrap();
    app.load_s33d_project(std::str::from_utf8(&original).unwrap())
        .unwrap();
    let id = app.doc.bodies.iter().find(|b| b.is_sketch()).unwrap().id;
    let sketch = app.doc.body(id).unwrap().sketch().unwrap().clone();
    let regions = app.regions_of(id);
    let index = regions.iter().position(|r| r.holes.len() == 2).unwrap();
    let region = &regions[index];
    let area = |ring: &[[f64; 2]]| {
        (0..ring.len())
            .map(|i| {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                a[0] * b[1] - a[1] * b[0]
            })
            .sum::<f64>()
            .abs()
            * 0.5
    };
    let expected = area(&region.outer) - region.holes.iter().map(|h| area(h)).sum::<f64>();
    app.selected = vec![id];
    assert!(app.set_region_selection(app.selected[0], vec![index]));
    app.refresh_scene();
    let scene = app.scene.lock().unwrap();
    let triangles: Vec<_> = scene
        .region_surfaces
        .iter()
        .filter(|p| (p.opacity - 0.45).abs() < 1e-6)
        .flat_map(|p| {
            p.indices.chunks_exact(3).map(move |t| {
                [
                    p.positions[t[0] as usize],
                    p.positions[t[1] as usize],
                    p.positions[t[2] as usize],
                ]
            })
        })
        .collect();
    let actual: f64 = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|p| glam::DVec3::from_array(p.map(f64::from)));
            (b - a).cross(c - a).length() * 0.5
        })
        .sum();
    assert!(
        (actual - expected).abs() < 0.001,
        "scene area={actual}; expected={expected}"
    );
    if let Some(output) = std::env::var_os("S33D_REGION_PNG") {
        let output = std::path::PathBuf::from(output);
        assert_ne!(input, output);
        assert_eq!(output.extension().unwrap(), "png");
        let mut image = image::RgbImage::from_pixel(1240, 480, image::Rgb([43, 47, 53]));
        let origin = glam::DVec3::from_array(sketch.plane.origin);
        let u = sketch.plane.x_axis();
        let v = sketch.plane.y_axis();
        let project = |p: [f64; 3]| {
            let p = glam::DVec3::from_array(p.map(f64::from)) - origin;
            [620. + p.dot(u) * 8., 240. - p.dot(v) * 8.]
        };
        let cross = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        };
        for t in &triangles {
            let [a, b, c] = t.map(project);
            let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.) as u32;
            let max_x = a[0].max(b[0]).max(c[0]).ceil().min(1239.) as u32;
            let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.) as u32;
            let max_y = a[1].max(b[1]).max(c[1]).ceil().min(479.) as u32;
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let p = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                    let d = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
                    if d.iter().all(|d| *d >= -1e-8) || d.iter().all(|d| *d <= 1e-8) {
                        image.put_pixel(x, y, image::Rgb([187, 128, 55]));
                    }
                }
            }
        }
        image.save(output).unwrap();
    }
    assert_eq!(std::fs::read(input).unwrap(), original);
}

fn add_profile(app: &mut S33DApp) -> BodyId {
    let id = app.doc.alloc_id();
    app.doc.add_body(Body {
        id,
        name: "Profile".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane: PlaneDef {
                origin: [100., 0., 0.],
                ..PlaneDef::from_normal([0., 1., 0.])
            },
            entities: vec![SketchEntity::Rect {
                c1: [1., 0.],
                c2: [3., 3.],
            }],
            world_xyz_coordinates: false,
            on_body: None,
        }),
        shape: None,
        material: Default::default(),
        visible: true,
    });
    id
}

#[test]
fn multiclick_exits_all_nonconflicting_workspace_tools_before_face_and_object_selection() {
    for tool in [
        "move",
        "rotate",
        "scale",
        "rotate-axis",
        "align",
        "align-axis",
        "array",
        "draw3d",
        "boolean",
        "split",
        "section",
        "mirror",
        "offset",
        "extrude",
        "revolve-axis",
        "revolve",
        "equation",
        "pick-plane",
    ] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        app.selected = vec![root];
        match tool {
            "move" => app.toggle_gizmo_tool(GizmoMode::Translate),
            "rotate" | "rotate-axis" => {
                app.toggle_gizmo_tool(GizmoMode::Rotate);
                app.rotate_axis_pick_active = tool == "rotate-axis";
            }
            "scale" => app.toggle_gizmo_tool(GizmoMode::Scale),
            "align" | "align-axis" => {
                app.toggle_align_tool();
                app.axis_align.picking = tool == "align-axis";
            }
            "array" => app.toggle_array_tool(),
            "draw3d" => app.start_insert_3d(),
            "boolean" => {
                let other = app.execute_agent_request(AgentRequest::new(serde_json::json!(2), "primitive.create",
                    serde_json::json!({"kind":"box","width":20.,"depth":20.,"height":20.,"position":[5.,0.,0.]}))).unwrap()["id"].as_u64().unwrap();
                app.selected = vec![root, other];
                app.start_boolean(BoolOp::Union);
            }
            "split" => {
                app.start_split();
                app.set_split_datum_plane(MirrorPlaneSource::YZ);
            }
            "section" => app.start_section_view(),
            "mirror" => {
                app.start_mirror();
                app.set_mirror_datum_plane(MirrorPlaneSource::YZ);
            }
            "offset" => {
                let mesh = app
                    .kernel
                    .tessellate(app.doc.body(root).unwrap().shape.unwrap())
                    .unwrap();
                let exact = s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh).unwrap();
                let shape = app.kernel.insert_exact(exact);
                app.doc.body_mut(root).unwrap().shape = Some(shape);
                app.start_offset_shell();
                assert_eq!(app.mode, Mode::OffsetShell);
                app.offset_ctx.as_mut().unwrap().operation =
                    s33d_kernel::occt::OffsetMode::SurfaceOffset;
                app.offset_ctx.as_mut().unwrap().distance = 1.;
            }
            "extrude" | "revolve-axis" | "revolve" => {
                let id = add_profile(&mut app);
                app.selected = vec![id];
                assert!(app.set_region_selection(app.selected[0], vec![0]));
                if tool == "extrude" {
                    app.start_extrude();
                } else {
                    app.start_revolve();
                    if tool == "revolve" {
                        app.revolve_with_axis([0., 0.], [0., 1.]);
                    }
                }
                assert!(
                    matches!(app.mode, Mode::Extrude | Mode::RevolveAxis | Mode::Revolve),
                    "{tool}"
                );
            }
            "equation" => {
                app.start_equation_curve(None);
                app.preview_equation_curve();
                assert!(app.curve_ctx.as_ref().unwrap().preview.is_some());
            }
            "pick-plane" => app.mode = Mode::PickPlane,
            _ => unreachable!(),
        }
        let bodies = app.doc.bodies.clone();
        app.camera.eye = glam::Vec3::new(40., 40., 40.);
        app.camera.target = glam::Vec3::ZERO;
        app.camera.up = glam::Vec3::Y;
        let pos = egui::pos2(400., 300.);
        pointer_frame(&ctx, &mut app, pos, None);
        for count in 1..=3 {
            pointer_frame(&ctx, &mut app, pos, Some(true));
            pointer_frame(&ctx, &mut app, pos, Some(false));
            if count >= 2 {
                if tool == "revolve-axis" {
                    assert_eq!(
                        app.mode,
                        Mode::RevolveAxis,
                        "An unfinished axis must remain editable"
                    );
                    assert_eq!(app.doc.bodies, bodies);
                    continue;
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while app.pending_tool_exit.is_some() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "{tool} exit timed out"
                    );
                    app.poll_exact_face_commit();
                    app.poll_offset_preview();
                    app.poll_tool_exit();
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert_eq!(
                    app.mode,
                    Mode::Idle,
                    "{tool}, click {count}, {:?}",
                    app.history
                );
                assert!(
                    app.gizmo.is_none()
                        && !app.align_active
                        && !app.array_active
                        && !app.chamfer_active,
                    "{tool}"
                );
                assert!(
                    !app.rotate_axis_pick_active && !app.axis_align.picking,
                    "{tool}"
                );
                assert!(
                    app.extrude_ctx.is_none()
                        && app.revolve_ctx.is_none()
                        && app.boolean_ctx.is_none(),
                    "{tool}"
                );
                assert!(
                    app.split_ctx.is_none()
                        && app.section_view_ctx.is_none()
                        && app.mirror_ctx.is_none()
                        && app.offset_ctx.is_none(),
                    "{tool}"
                );
                if count == 2 {
                    assert!(
                        app.face_pick.is_some(),
                        "{tool} must select a visible result face"
                    );
                } else {
                    assert_eq!(app.selected.len(), 1, "{tool}");
                    assert!(app.doc.body(app.selected[0]).unwrap().visible, "{tool}");
                    assert!(app.face_pick.is_none());
                }
                assert!(app.selected_edge.is_none(), "{tool}");
            }
        }
        if [
            "array", "boolean", "split", "mirror", "offset", "extrude", "revolve", "equation",
            "draw3d",
        ]
        .contains(&tool)
        {
            assert_ne!(
                app.doc.bodies, bodies,
                "Exiting {tool} must apply its current state"
            );
        } else {
            assert_eq!(
                app.doc.bodies, bodies,
                "{tool} has no pending geometry changes"
            );
        }
        if tool != "revolve-axis" {
            assert!(!app.sketch_tools_open && !app.draw3d_tools_open, "{tool}");
        }
    }
}

#[test]
fn multiclick_respects_point_construction_and_draw3d_placement() {
    let ctx = egui::Context::default();
    let (mut app, _) = box_app(&ctx);
    app.mode = Mode::Sketch;
    let mut sketch =
        crate::sketch_session::SketchSession::new(PlaneDef::from_normal([0., 1., 0.]), vec![]);
    sketch.set_tool(SketchTool::Line);
    sketch.pending = vec![[12., 12.]];
    app.sketch = Some(sketch);
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
        assert_eq!(
            app.mode,
            Mode::Sketch,
            "Line-chain points must not exit Sketch"
        );
    }
    app.cancel_sketch();
    app.start_insert_3d();
    app.choose_insert_3d_primitive(Primitive3DKind::Box);
    for _ in 0..3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
        assert_eq!(
            app.mode,
            Mode::Insert3D,
            "Base and sizing clicks must not become an exit gesture"
        );
    }
}

#[test]
fn multiclick_sketch_exit_picks_in_the_clicked_view_and_restores_the_3d_camera() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    let profile = add_profile(&mut app);
    app.edit_sketch(profile);
    app.sketch.as_mut().unwrap().set_tool(SketchTool::Pick);
    let mut final_camera = app.camera.clone();
    final_camera.eye = glam::Vec3::new(50., 30., 60.);
    final_camera.target = glam::Vec3::ZERO;
    app.saved_camera = Some(final_camera.clone());
    app.camera.eye = glam::Vec3::new(0., 60., 0.);
    app.camera.target = glam::Vec3::ZERO;
    app.camera.up = glam::Vec3::Z;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for count in 1..=3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
        if count == 2 {
            let face = app.face_pick.as_ref().unwrap();
            assert_eq!(face.body_id, root);
            assert!(
                face.normal[1] > 0.99,
                "The clicked top face must survive the camera restore"
            );
        }
    }
    assert_eq!(app.mode, Mode::Idle);
    assert_eq!(app.selected, vec![root]);
    assert!(app.face_pick.is_none());
    assert_eq!(app.camera.eye, final_camera.eye);
    assert_eq!(app.camera.target, final_camera.target);
}

#[test]
fn multiclick_draw3d_exit_applies_and_commits_latest_parameters() {
    for applied in [false, true] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        app.start_insert_3d();
        app.edit_draw_3d_body(root);
        assert_eq!(app.insert_3d_ctx.as_ref().unwrap().editing_body, Some(root));
        let mut params = app.insert_3d_ctx.as_ref().unwrap().params.unwrap();
        params.width = 24.;
        app.update_insert_3d_params(params);
        if applied {
            assert!(app.apply_insert_3d_params());
        }
        let old = app.doc.body(root).unwrap().clone();
        app.camera.eye = glam::Vec3::new(40., 40., 40.);
        app.camera.target = glam::Vec3::ZERO;
        app.camera.up = glam::Vec3::Y;
        let pos = egui::pos2(400., 300.);
        pointer_frame(&ctx, &mut app, pos, None);
        for _ in 0..2 {
            pointer_frame(&ctx, &mut app, pos, Some(true));
            pointer_frame(&ctx, &mut app, pos, Some(false));
        }
        assert_eq!(app.mode, Mode::Idle, "applied={applied}");
        assert_eq!(app.face_pick.as_ref().map(|f| f.body_id), Some(root));
        assert!(
            matches!(app.doc.body(root).unwrap().feature,Feature::Primitive3D {params} if params.width == 24.)
        );
        assert!(!app.draw3d_tools_open && !app.sketch_tools_open);
        assert!(app.undo());
        assert_eq!(app.doc.body(root).unwrap(), &old);
    }
}

#[test]
fn multiclick_invalid_equation_editor_stays_open_without_selection_side_effects() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.selected = vec![root];
    app.start_equation_curve(None);
    app.curve_ctx.as_mut().unwrap().preview = None;
    app.curve_ctx.as_mut().unwrap().params.expressions[0] = "invalid(".into();
    let bodies = app.doc.bodies.clone();
    let selected = app.selected.clone();
    let pos = egui::pos2(400., 300.);
    for _ in 0..3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert_eq!(app.mode, Mode::EquationCurve);
    assert_eq!(app.selected, selected);
    assert_eq!(app.doc.bodies, bodies);
    assert!(app.face_pick.is_none());
}

#[test]
fn multiclick_ctrl_selection_survives_a_single_object_tool_conflict_on_the_first_click() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    let second=app.execute_agent_request(AgentRequest::new(serde_json::json!(2),"primitive.create",
        serde_json::json!({"kind":"box","width":20.,"depth":20.,"height":20.,"position":[40.,0.,0.]}))).unwrap()["id"].as_u64().unwrap();
    app.selected = vec![root];
    app.toggle_array_tool();
    app.camera.eye = glam::Vec3::new(80., 40., 40.);
    app.camera.target = glam::Vec3::new(40., 0., 0.);
    app.camera.up = glam::Vec3::Y;
    let pos = egui::pos2(400., 300.);
    pointer_frame_modified(&ctx, &mut app, pos, None, egui::Modifiers::CTRL);
    for count in 1..=3 {
        pointer_frame_modified(&ctx, &mut app, pos, Some(true), egui::Modifiers::CTRL);
        pointer_frame_modified(&ctx, &mut app, pos, Some(false), egui::Modifiers::CTRL);
        if count >= 2 {
            assert_eq!(app.selected, vec![root, second], "click {count}");
            assert!(!app.array_active);
        }
    }
    assert!(app.face_pick.is_none());
}

#[test]
fn exit_applies_array_once_and_collapses_all_tool_menus() {
    for already_applied in [false, true] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        app.selected = vec![root];
        app.toggle_array_tool();
        app.array_spacing = [35.; 3];
        app.array_copies = [3, 1, 1];
        if already_applied {
            app.array_selected();
        }
        app.sketch_tools_open = true;
        app.draw3d_tools_open = true;
        app.thread_ui.expanded = true;
        app.thread_ui.print_open = true;
        app.thread_ui.save_open = true;
        assert!(app.exit_tool_for_workspace_selection());
        assert_eq!(app.doc.bodies.len(), 3, "already_applied={already_applied}");
        assert!(!app.array_active && !app.sketch_tools_open && !app.draw3d_tools_open);
        assert!(!app.thread_ui.expanded && !app.thread_ui.print_open && !app.thread_ui.save_open);
        let completed = app.doc.bodies.clone();
        assert!(app.exit_tool_for_workspace_selection());
        assert_eq!(
            app.doc.bodies, completed,
            "A later exit must not repeat Array"
        );
        assert!(app.undo());
        assert_eq!(app.doc.bodies.len(), 1);
    }
}

#[test]
fn exit_applies_pending_transform_fields_without_repeating_applied_values() {
    for operation in ["move", "move-to", "rotate", "scale", "size"] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        app.selected = vec![root];
        let original = app.doc.bodies.clone();
        match operation {
            "move" | "move-to" => {
                app.toggle_gizmo_tool(GizmoMode::Translate);
                if operation == "move" {
                    app.move_input = [7., 0., 0.];
                } else {
                    app.move_to_input = [7., 10., 0.];
                }
            }
            "rotate" => {
                app.toggle_gizmo_tool(GizmoMode::Rotate);
                app.rotate_input.1 = 30.;
            }
            "scale" | "size" => {
                app.toggle_gizmo_tool(GizmoMode::Scale);
                if operation == "scale" {
                    app.scale_input = [200., 100., 100.];
                } else {
                    app.scale_size_input = [40., 20., 20.];
                }
            }
            _ => unreachable!(),
        }
        assert!(app.exit_tool_for_workspace_selection(), "{operation}");
        assert_ne!(app.doc.bodies, original, "{operation}");
        if operation.starts_with("move") {
            assert!((app.body_center(root).x - 7.).abs() < 1e-4);
        }
        if ["scale", "size"].contains(&operation) {
            assert!((volume(&mut app, root) - 16000.).abs() < 1e-2);
        }
        let completed = app.doc.bodies.clone();
        assert!(app.exit_tool_for_workspace_selection());
        assert_eq!(app.doc.bodies, completed);
        assert!(app.undo());
        assert_eq!(app.doc.bodies, original, "{operation}");
    }
}

#[test]
fn failed_exit_keeps_operation_parameters_and_menus_open() {
    for operation in ["extrude", "revolve", "boolean", "split", "mirror", "scale"] {
        let ctx = egui::Context::default();
        let (mut app, root) = box_app(&ctx);
        app.selected = vec![root];
        match operation {
            "extrude" | "revolve" => {
                let id = add_profile(&mut app);
                app.selected = vec![id];
                assert!(app.set_region_selection(app.selected[0], vec![0]));
                if operation == "extrude" {
                    app.start_extrude();
                    app.extrude_ctx.as_mut().unwrap().depth = f64::NAN;
                } else {
                    app.start_revolve();
                    app.revolve_with_axis([0., 0.], [0., 1.]);
                    app.revolve_ctx.as_mut().unwrap().angle_deg = f64::NAN;
                }
            }
            "boolean" => {
                app.mode = Mode::Boolean;
                app.boolean_ctx = Some(BooleanCtx {
                    op: BoolOp::Subtract,
                    target: Some(root),
                    tool: None,
                    picking: Some(BooleanOperandSlot::Tool),
                    keep_originals: false,
                });
            }
            "split" => {
                app.start_split();
                app.split_ctx.as_mut().unwrap().previews.clear();
            }
            "mirror" => {
                app.start_mirror();
                app.mirror_ctx.as_mut().unwrap().plane_def = None;
            }
            "scale" => {
                app.toggle_gizmo_tool(GizmoMode::Scale);
                app.scale_input = [-1.; 3];
            }
            _ => unreachable!(),
        }
        app.sketch_tools_open = true;
        app.draw3d_tools_open = true;
        let before = app.doc.bodies.clone();
        let mode = app.mode;
        assert!(!app.exit_tool_for_workspace_selection(), "{operation}");
        assert_eq!(app.mode, mode, "{operation}");
        assert_eq!(app.doc.bodies, before, "{operation}");
        assert!(
            app.sketch_tools_open && app.draw3d_tools_open,
            "{operation}"
        );
    }
}

#[test]
fn multiclick_array_triple_click_does_not_apply_again_after_double_click() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.selected = vec![root];
    app.toggle_array_tool();
    app.array_spacing = [50.; 3];
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for count in 1..=3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
        if count >= 2 {
            assert_eq!(app.doc.bodies.len(), 3, "click {count}");
        }
    }
    assert_eq!(app.selected, vec![root]);
}

#[test]
fn multiclick_exact_pocket_waits_for_apply_then_exits_and_selects() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.settings.show_tool_exit_confirmation = true;
    let mesh = app
        .kernel
        .tessellate(app.doc.body(root).unwrap().shape.unwrap())
        .unwrap();
    let shape = app
        .kernel
        .insert_exact(s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh).unwrap());
    app.doc.body_mut(root).unwrap().shape = Some(shape);
    app.camera.eye = glam::Vec3::new(0., 70., 0.);
    app.camera.target = glam::Vec3::new(0., 10., 0.);
    app.camera.up = glam::Vec3::Z;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    app.selected = vec![root];
    app.face_pick = Some(app.pick_face(pos, app.viewport_rect).unwrap().0);
    app.start_extrude();
    app.set_extrude_depth(3.);
    assert!(app.extrude_ctx.as_ref().unwrap().exact_face.is_some());
    app.sketch_tools_open = true;
    app.draw3d_tools_open = true;
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(app.tool_exit_prompt_open());
    assert_eq!(app.doc.body(root).unwrap().shape, Some(shape));
    app.resolve_tool_exit_prompt(true);
    assert!(app.pending_tool_exit.is_some());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.pending_tool_exit.is_some() {
        assert!(std::time::Instant::now() < deadline);
        app.poll_exact_face_commit();
        app.poll_tool_exit();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.mode, Mode::Idle);
    assert!(!app.sketch_tools_open && !app.draw3d_tools_open);
    assert_eq!(app.face_pick.as_ref().map(|f| f.body_id), Some(root));
    assert_eq!(app.doc.bodies.len(), 1);
    assert_ne!(app.doc.body(root).unwrap().shape, Some(shape));
    assert!((volume(&mut app, root) - 9200.).abs() < 1e-2);
    assert!(app.undo());
    assert_eq!(app.doc.body(root).unwrap().shape, Some(shape));
}

#[test]
fn exit_prompt_cancel_keeps_the_tool_draft_and_confirm_applies_once() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.settings.show_tool_exit_confirmation = true;
    app.selected = vec![root];
    app.toggle_array_tool();
    app.array_spacing = [50.; 3];
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    let pos = egui::pos2(400., 300.);
    let before = app.doc.bodies.clone();
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(app.tool_exit_prompt_open() && app.array_active);
    assert_eq!(
        app.tool_exit_prompt.as_ref().unwrap().tool,
        ("Array", "阵列")
    );
    assert_eq!(
        app.doc.bodies, before,
        "Nothing applies before confirmation"
    );
    let denied = app
        .execute_agent_request(AgentRequest::new(
            serde_json::json!(9),
            "primitive.create",
            serde_json::json!({"kind":"box"}),
        ))
        .unwrap_err();
    assert_eq!(denied.0, "busy");
    assert_eq!(app.doc.bodies, before);
    app.tool_exit_prompt.as_mut().unwrap().dont_show_again = true;
    app.resolve_tool_exit_prompt(false);
    assert!(app.array_active && app.settings.show_tool_exit_confirmation);
    assert_eq!(app.array_spacing, [50.; 3]);
    assert_eq!(app.doc.bodies, before);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(app.tool_exit_prompt_open());
    app.resolve_tool_exit_prompt(true);
    assert!(!app.tool_exit_prompt_open() && !app.array_active);
    assert_eq!(app.doc.bodies.len(), 3);
    assert_eq!(app.face_pick.as_ref().map(|f| f.body_id), Some(root));
    assert!(
        app.settings.show_tool_exit_confirmation,
        "Unchecked prompts remain enabled"
    );
    assert!(app.undo());
    assert_eq!(app.doc.bodies, before);
}

#[test]
fn exit_prompt_remembers_triple_click_and_restores_suppressed_tips() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.settings.show_tool_exit_confirmation = true;
    app.settings.show_first_use_tips = false;
    app.selected = vec![root];
    app.toggle_array_tool();
    app.array_spacing = [50.; 3];
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..3 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert_eq!(app.doc.bodies.len(), 1);
    assert_eq!(app.tool_exit_prompt.as_ref().unwrap().click.count, 3);
    assert!(
        !app.tool_exit_prompt.as_ref().unwrap().dont_show_again,
        "The third click must not toggle the dialog checkbox"
    );
    app.tool_exit_prompt.as_mut().unwrap().dont_show_again = true;
    app.resolve_tool_exit_prompt(true);
    assert_eq!(app.doc.bodies.len(), 3);
    assert_eq!(app.selected, vec![root]);
    assert!(
        app.face_pick.is_none(),
        "Confirmed triple-click selects the whole object"
    );
    assert!(!app.settings.show_tool_exit_confirmation);
    let restored: AppSettings =
        serde_json::from_str(&serde_json::to_string(&app.settings).unwrap()).unwrap();
    assert!(!restored.show_tool_exit_confirmation);
    assert!(
        serde_json::from_str::<AppSettings>("{}")
            .unwrap()
            .show_tool_exit_confirmation
    );
    app.toggle_gizmo_tool(GizmoMode::Scale);
    ctx.input_mut(|i| i.pointer = Default::default());
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(
        !app.tool_exit_prompt_open() && app.gizmo.is_none(),
        "Suppression skips later confirmations"
    );
    let document = app.doc.bodies.clone();
    app.reenable_all_hidden_tips();
    assert!(app.settings.show_tool_exit_confirmation && app.settings.show_first_use_tips);
    assert_eq!(app.doc.bodies, document);
    app.toggle_array_tool();
    // A fresh native input sequence must ask again after re-enabling.
    ctx.input_mut(|i| i.pointer = Default::default());
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(app.tool_exit_prompt_open());
}

#[test]
fn exit_prompt_preserves_ctrl_intent_and_does_not_prompt_idle_selection() {
    let ctx = egui::Context::default();
    let (mut app, root) = box_app(&ctx);
    app.settings.show_tool_exit_confirmation = true;
    app.camera.eye = glam::Vec3::new(40., 40., 40.);
    app.camera.target = glam::Vec3::ZERO;
    let pos = egui::pos2(400., 300.);
    pointer_frame(&ctx, &mut app, pos, None);
    for _ in 0..2 {
        pointer_frame(&ctx, &mut app, pos, Some(true));
        pointer_frame(&ctx, &mut app, pos, Some(false));
    }
    assert!(
        !app.tool_exit_prompt_open(),
        "Ordinary selection has no active tool to exit"
    );
    app.face_pick = None;
    app.selected = vec![root];
    app.toggle_gizmo_tool(GizmoMode::Translate);
    app.move_input = [0., 1., 0.];
    ctx.input_mut(|i| i.pointer = Default::default());
    app.last_click = None;
    for _ in 0..2 {
        pointer_frame_modified(&ctx, &mut app, pos, Some(true), egui::Modifiers::CTRL);
        pointer_frame_modified(&ctx, &mut app, pos, Some(false), egui::Modifiers::CTRL);
    }
    assert!(app.tool_exit_prompt_open());
    // The user has released Ctrl while reading the dialog.
    pointer_frame(&ctx, &mut app, pos, None);
    app.resolve_tool_exit_prompt(true);
    assert!(
        app.selected.is_empty(),
        "Ctrl-click removes the original object from selection"
    );
    assert!(app.gizmo.is_none());
    assert!((app.body_center(root).y - 11.).abs() < 1e-4);
}

fn pocket_bore_app(ctx: &egui::Context) -> (S33DApp, BodyId, BodyId) {
    pocket_bore_app_with_owner(ctx, true)
}

fn pocket_bore_app_with_owner(ctx: &egui::Context, linked: bool) -> (S33DApp, BodyId, BodyId) {
    let (mut app, solid) = box_app(ctx);
    let mesh = app
        .kernel
        .tessellate(app.doc.body(solid).unwrap().shape.unwrap())
        .unwrap();
    let (min, max) = mesh.bounds().unwrap();
    let mut plane = DatumPlane::Top.def();
    plane.origin = [
        f64::from((min.x + max.x) * 0.5),
        f64::from(max.y),
        f64::from((min.z + max.z) * 0.5),
    ];
    let sketch = app.doc.alloc_id();
    app.doc.add_body(Body {
        id: sketch,
        name: "Visible pocket sketch".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane,
            entities: vec![SketchEntity::Circle { c: [0., 0.], r: 3. }],
            on_body: linked.then_some(solid),
            world_xyz_coordinates: false,
        }),
        shape: None,
        material: Material::default(),
        visible: true,
    });
    app.selected = vec![sketch];
    assert!(app.set_region_selection(app.selected[0], vec![0]));
    app.start_pocket();
    app.extrude_ctx.as_mut().unwrap().target_body = Some(solid);
    app.extrude_ctx.as_mut().unwrap().through = true;
    app.set_extrude_depth(f64::from(max.y - min.y));
    app.commit_extrude();
    assert_eq!(app.mode, Mode::Idle);
    assert!(app.doc.body(sketch).unwrap().visible);
    (app, solid, sketch)
}

fn look_through_bore(app: &mut S33DApp, target: &crate::threads::Target) {
    let axis = glam::DVec3::from_array(target.axis);
    let radial = axis.any_orthonormal_vector();
    let origin = glam::DVec3::from_array(target.origin);
    app.camera.eye =
        (origin + axis * (target.length + target.radius) + radial * target.radius).as_vec3();
    app.camera.target =
        (origin + axis * target.length * 0.5 - radial * target.radius * 0.5).as_vec3();
    app.camera.up = axis.as_vec3();
}

fn double_click_bore_with_visible_sketch(
    ctx: &egui::Context,
    app: &mut S33DApp,
    solid: BodyId,
) -> crate::threads::Target {
    let target = app
        .thread_targets(&[solid])
        .unwrap()
        .into_iter()
        .find(|t| t.internal)
        .expect("Pocket must expose a complete cylindrical bore");
    look_through_bore(app, &target);
    let pos = egui::pos2(400., 300.);
    pointer_frame(ctx, app, pos, None);
    let (face, face_distance) = app
        .pick_face(pos, app.viewport_rect)
        .expect("Visible inner wall");
    assert_eq!(face.body_id, solid);
    assert!(app.thread_target_for_face(&face).unwrap().internal);
    let (_, _, region_distance) = app
        .pick_sketch_region(pos, app.viewport_rect)
        .expect("Mouth sketch overlaps this ray");
    assert!(
        region_distance < face_distance,
        "Reproduce the closer sketch intercepting the bore"
    );
    app.update_hover(pos, app.viewport_rect);
    assert_eq!(
        app.hover_face.as_ref().map(|face| face.body_id),
        Some(solid),
        "Hover must identify the same inner wall as double-click"
    );
    for _ in 0..2 {
        pointer_frame(ctx, app, pos, Some(true));
        pointer_frame(ctx, app, pos, Some(false));
    }
    assert_eq!(
        app.selected,
        vec![solid],
        "A mouth sketch must not steal the inner-wall double-click"
    );
    let face = app
        .face_pick
        .clone()
        .expect("Select the complete bore wall");
    assert!(app.selected_regions.is_empty());
    let selected = app
        .thread_target_for_face(&face)
        .expect("Show the thread generator");
    assert!(selected.internal);
    selected
}

fn thread_sidebar_frame(
    ctx: &egui::Context,
    app: &mut S33DApp,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn collect(shape: &egui::Shape, output: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(text) => output.push((
                text.galley.job.text.clone(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, output)),
            _ => {}
        }
    }
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520., 1900.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| crate::threads::panel(app, ui));
        },
    );
    let mut labels = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut labels);
    }
    labels
}

fn click_thread_sidebar_label(
    ctx: &egui::Context,
    app: &mut S33DApp,
    labels: &[(String, egui::Rect)],
    label: &str,
) -> Vec<(String, egui::Rect)> {
    let pos = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Missing sidebar label {label}"))
        .1
        .center();
    for pressed in [true, false] {
        thread_sidebar_frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    thread_sidebar_frame(ctx, app, vec![])
}

fn generate_bore_from_sidebar(ctx: &egui::Context, app: &mut S33DApp, solid: BodyId) {
    ctx.style_mut(|style| style.animation_time = 0.);
    app.settings.language = UiLanguage::English;
    let before = app.doc.body(solid).unwrap().shape;
    let labels = thread_sidebar_frame(ctx, app, vec![]);
    assert!(
        !labels.iter().any(|(text, _)| text == "Nominal diameter"),
        "Initially collapsed"
    );
    let labels = click_thread_sidebar_label(ctx, app, &labels, "Thread generator");
    assert!(labels
        .iter()
        .any(|(text, _)| text.contains("Internal / nut")));
    assert!(labels.iter().any(|(text, _)| text == "Nominal diameter"));
    assert!(
        !labels.iter().any(|(text, _)| text.starts_with("warning:")),
        "Valid initial bore draft: {labels:?}"
    );
    click_thread_sidebar_label(ctx, app, &labels, "Generate thread");
    assert_ne!(
        app.doc.body(solid).unwrap().shape,
        before,
        "Sidebar Generate must cut the actual bore"
    );
    assert!(!app.thread_ui.expanded);
}

#[test]
fn pocket_bore_thread_entry_is_reachable_with_its_sketch_visible() {
    let ctx = egui::Context::default();
    let (mut app, solid, sketch) = pocket_bore_app(&ctx);
    double_click_bore_with_visible_sketch(&ctx, &mut app, solid);
    assert!(
        app.doc.body(sketch).unwrap().visible,
        "Do not hide the user's sketch"
    );
    let before = volume(&mut app, solid);
    let original = app.doc.body(solid).unwrap().shape;
    generate_bore_from_sidebar(&ctx, &mut app, solid);
    assert!(volume(&mut app, solid) < before - 1.);
    let threaded = app.doc.body(solid).unwrap().shape;
    assert!(app.undo());
    assert_eq!(app.doc.body(solid).unwrap().shape, original);
    assert!(app.redo());
    assert_eq!(app.doc.body(solid).unwrap().shape, threaded);
}

#[test]
fn datum_plane_pocket_source_selects_bore_and_generates_after_project_roundtrip() {
    let ctx = egui::Context::default();
    let (mut app, solid, sketch) = pocket_bore_app_with_owner(&ctx, false);
    assert!(app.profile_sketch(sketch).unwrap().on_body.is_none());
    assert!(matches!(&app.doc.body(solid).unwrap().feature,
        Feature::Extrude { source_sketch: None, pocket_source_sketches, .. } if pocket_source_sketches == &vec![sketch]));
    let saved = app.project_to_s33d().unwrap();
    app.load_s33d_project(&saved).unwrap();
    double_click_bore_with_visible_sketch(&ctx, &mut app, solid);
    assert!(app.doc.body(sketch).unwrap().visible);
    let before = app.doc.body(solid).unwrap().shape;
    generate_bore_from_sidebar(&ctx, &mut app, solid);
    assert!(app.undo());
    assert_eq!(app.doc.body(solid).unwrap().shape, before);
    assert!(app.redo());
    assert_ne!(app.doc.body(solid).unwrap().shape, before);
}

#[test]
fn bore_selection_preserves_unrelated_and_coplanar_sketch_region_priority() {
    for case in 0..6 {
        let ctx = egui::Context::default();
        let (mut app, solid, sketch_id) = pocket_bore_app(&ctx);
        let target = app
            .thread_targets(&[solid])
            .unwrap()
            .into_iter()
            .find(|t| t.internal)
            .unwrap();
        look_through_bore(&mut app, &target);
        let Feature::Sketch(sketch) = &mut app.doc.body_mut(sketch_id).unwrap().feature else {
            unreachable!()
        };
        match case {
            0 => sketch.on_body = None,
            1 => sketch.on_body = Some(9999),
            2 => sketch.plane.origin[1] += 1.,
            3 => sketch.entities = vec![SketchEntity::Circle { c: [0., 0.], r: 4. }],
            4 => {
                sketch.entities = vec![SketchEntity::Rect {
                    c1: [-6., -6.],
                    c2: [6., 6.],
                }]
            }
            5 => {
                sketch.entities = vec![SketchEntity::Circle { c: [6., 6.], r: 2. }];
                let point = glam::DVec3::from_array(sketch.plane.to_world(6., 6.)).as_vec3();
                app.camera.eye = point + glam::Vec3::Y * 20.;
                app.camera.target = point;
                app.camera.up = glam::Vec3::Z;
            }
            _ => unreachable!(),
        }
        if case <= 1 {
            // These are deliberately unrelated overlays, with no consumed
            // Pocket source link. A real datum-plane Pocket is tested separately.
            if let Feature::Extrude {
                pocket_source_sketches,
                ..
            } = &mut app.doc.body_mut(solid).unwrap().feature
            {
                pocket_source_sketches.clear();
            }
        }
        let pos = egui::pos2(400., 300.);
        pointer_frame(&ctx, &mut app, pos, None);
        for _ in 0..2 {
            pointer_frame(&ctx, &mut app, pos, Some(true));
            pointer_frame(&ctx, &mut app, pos, Some(false));
        }
        assert_eq!(
            app.selected,
            vec![sketch_id],
            "Unrelated/planar region {case}"
        );
        assert_eq!(app.selected_regions, vec![0], "case {case}");
        assert!(app.face_pick.is_none(), "case {case}");
    }
}

#[test]
#[ignore = "requires S33D_BORE_FIXTURE pointing to an independent user-project copy"]
fn copied_project2_bore_can_be_selected_and_threaded() {
    let path = std::path::PathBuf::from(
        std::env::var_os("S33D_BORE_FIXTURE").expect("Set S33D_BORE_FIXTURE"),
    );
    let original = std::fs::read(&path).unwrap();
    let ctx = egui::Context::default();
    let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
    app.load_s33d_project(std::str::from_utf8(&original).unwrap())
        .unwrap();
    let solid = app
        .doc
        .bodies
        .iter()
        .find(|b| b.name == "Box 2")
        .unwrap()
        .id;
    double_click_bore_with_visible_sketch(&ctx, &mut app, solid);
    let before = app.doc.body(solid).unwrap().shape.unwrap();
    let screw_before = app
        .doc
        .bodies
        .iter()
        .find(|b| b.name == "Cylinder 1")
        .unwrap()
        .clone();
    generate_bore_from_sidebar(&ctx, &mut app, solid);
    assert_ne!(app.doc.body(solid).unwrap().shape, Some(before));
    assert_eq!(app.doc.body(screw_before.id).unwrap(), &screw_before);
    if let Some(output) = std::env::var_os("S33D_BORE_OUTPUT") {
        let output = std::path::PathBuf::from(output);
        assert!(!output.exists() && output != path);
        app.save_project_to_path(&output).unwrap();
    }
    assert!(app.undo());
    assert_eq!(app.doc.body(solid).unwrap().shape, Some(before));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "Fixture stays unchanged"
    );
}
