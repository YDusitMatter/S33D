//! Preview bounds and recessed-face selection through retained pocket sketches.
use super::*;

fn app() -> S33DApp {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    app.settings.show_tool_exit_confirmation = false;
    app
}

fn fixture(normal: [f64; 3], round: bool, linked: bool) -> (S33DApp, BodyId, BodyId) {
    let mut app = app();
    let plane = PlaneDef::from_normal(normal);
    let region = Region2D::new(vec![[-15., -15.], [15., -15.], [15., 15.], [-15., 15.]]);
    let shape = app.kernel.extrude_region(&region, &plane, 10.).unwrap();
    let solid = app.doc.alloc_id();
    app.doc.add_body(Body {
        id: solid,
        name: "Base".into(),
        kind: BodyKind::Solid,
        feature: Feature::Derived {
            source: "Regression base".into(),
            operation: None,
        },
        shape: Some(shape),
        material: Default::default(),
        visible: true,
    });
    let sketch = app.doc.alloc_id();
    let mut top = plane;
    top.origin = (plane.origin() + plane.normal() * 10.).to_array();
    app.doc.add_body(Body {
        id: sketch,
        name: "Pocket sketch".into(),
        kind: BodyKind::Sketch,
        feature: Feature::Sketch(SketchFeature {
            vertex_locks: Default::default(),
            plane: top,
            entities: vec![if round {
                SketchEntity::Circle { c: [0., 0.], r: 4. }
            } else {
                SketchEntity::Rect {
                    c1: [-4., -3.],
                    c2: [4., 3.],
                }
            }],
            on_body: linked.then_some(solid),
            world_xyz_coordinates: false,
        }),
        shape: None,
        material: Default::default(),
        visible: true,
    });
    app.selected = vec![sketch];
    assert!(app.set_region_selection(sketch, vec![0]));
    app.start_pocket();
    assert_eq!(app.extrude_ctx.as_ref().unwrap().target_body, Some(solid));
    (app, solid, sketch)
}

#[test]
fn unrelated_or_ambiguous_visible_solids_do_not_get_a_pocket_automatically() {
    let (mut app, solid, _) = fixture([0., 1., 0.], true, false);
    app.cancel_extrude();
    let mut other = app.body(solid).unwrap().clone();
    other.id = app.doc.alloc_id();
    let overlap = other.id;
    app.doc.add_body(other);
    app.start_pocket();
    assert_eq!(app.extrude_ctx.as_ref().unwrap().target_body, None);
    app.cancel_extrude();
    app.doc.body_mut(solid).unwrap().visible = false;
    let plane = PlaneDef::from_normal([0., 1., 0.]);
    let far = Region2D::new(vec![[50., 50.], [55., 50.], [55., 55.], [50., 55.]]);
    app.doc.body_mut(overlap).unwrap().shape =
        Some(app.kernel.extrude_region(&far, &plane, 10.).unwrap());
    app.start_pocket();
    assert_eq!(app.extrude_ctx.as_ref().unwrap().target_body, None);
}

#[test]
#[ignore = "Requires a copy of the local display holder project"]
fn display_holder_sketch_one_circles_infer_chamfer_14() {
    let path = std::env::var("S33D_DISPLAY_HOLDER_PROJECT").unwrap();
    let mut app = app();
    app.load_s33d_project(&std::fs::read_to_string(path).unwrap())
        .unwrap();
    let sketch = app
        .doc
        .bodies
        .iter()
        .find(|b| b.name == "Sketch 1")
        .unwrap()
        .id;
    let target = app
        .doc
        .bodies
        .iter()
        .find(|b| b.name == "Chamfer 14")
        .unwrap()
        .id;
    app.doc.body_mut(sketch).unwrap().visible = true;
    let circles: Vec<_> = app
        .body(sketch)
        .unwrap()
        .sketch()
        .unwrap()
        .entities
        .iter()
        .filter_map(|entity| match entity {
            SketchEntity::Circle { c, .. } => Some(*c),
            _ => None,
        })
        .collect();
    let regions = app.regions_of(sketch);
    assert_eq!(circles.len(), 6);
    for center in circles {
        let region = regions
            .iter()
            .position(|region| s33d_model::point_in_region(center, region))
            .unwrap();
        app.selected = vec![sketch];
        assert!(app.set_region_selection(sketch, vec![region]));
        app.start_pocket();
        assert_eq!(
            app.extrude_ctx.as_ref().unwrap().target_body,
            Some(target),
            "{center:?}"
        );
        app.cancel_extrude();
    }
}

fn preview_range(app: &S33DApp) -> (f64, f64) {
    let ctx = app.extrude_ctx.as_ref().unwrap();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for shape in &ctx.ghost_shapes {
        for point in app.kernel.tessellate(*shape).unwrap().positions {
            let along = (glam::DVec3::from_array(point.map(f64::from)) - ctx.sketch.plane.origin())
                .dot(ctx.sketch.plane.normal());
            min = min.min(along);
            max = max.max(along);
        }
    }
    (min, max)
}

#[test]
fn shallow_pocket_preview_starts_on_sketch_plane_and_commit_retains_exact_depth() {
    for normal in [[0., 1., 0.], [0., 0., 1.], [1., 2., 3.]] {
        let (mut app, solid, sketch) = fixture(normal, true, true);
        let original = app.body(solid).unwrap().shape;
        let plane = app.profile_sketch(sketch).unwrap().plane;
        app.set_extrude_depth(0.1);
        let (lo, hi) = preview_range(&app);
        assert!(
            hi.abs() < 1e-4,
            "preview must start on plane, got {lo}..{hi}"
        );
        assert!((lo + 0.1).abs() < 1e-4);
        assert_eq!(
            app.body(solid).unwrap().shape,
            original,
            "preview cannot commit"
        );
        app.commit_extrude();
        let result = app.body(solid).unwrap().shape.unwrap();
        let mesh = app.kernel.tessellate(result).unwrap();
        let origin = plane.origin() + plane.normal() * 5.;
        let distance =
            ray_mesh_intersect(origin.to_array(), (-plane.normal()).to_array(), &mesh).unwrap();
        assert!(
            (distance - 5.1).abs() < 1e-4,
            "committed pocket depth: {distance}"
        );
        assert!(app.undo());
        assert_eq!(app.body(solid).unwrap().shape, original);
        assert!(app.redo());
        assert_eq!(app.body(solid).unwrap().shape, Some(result));
    }
}

#[test]
fn through_preview_spans_target_without_boolean_padding() {
    let (mut app, _, _) = fixture([0., 1., 0.], false, true);
    app.extrude_ctx.as_mut().unwrap().through = true;
    app.set_extrude_depth(0.1);
    let (lo, hi) = preview_range(&app);
    assert!((lo + 10.).abs() < 1e-4 && hi.abs() < 1e-4, "{lo}..{hi}");
}

#[test]
fn native_planar_face_pocket_uses_unpadded_preview_and_robust_commit() {
    let mut app = app();
    let solid = app
        .execute_agent_request(AgentRequest::new(
            json!(1),
            "primitive.create",
            json!({"kind":"box", "width":20., "depth":20., "height":20.}),
        ))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let raw_shape = app.body(solid).unwrap().shape.unwrap();
    let mesh = app.kernel.tessellate(raw_shape).unwrap();
    let exact = s33d_kernel::occt::ExactShape::from_faceted_mesh(&mesh).unwrap();
    let shape = app.kernel.insert_exact(exact);
    app.doc.body_mut(solid).unwrap().shape = Some(shape);
    let (min, max) = mesh.bounds().unwrap();
    let mut plane = DatumPlane::Top.def();
    plane.origin = [
        f64::from((min.x + max.x) * 0.5),
        f64::from(max.y),
        f64::from((min.z + max.z) * 0.5),
    ];
    let (pos, rect) = aim(&mut app, plane, 0.);
    app.face_pick = app.pick_face(pos, rect).map(|(face, _)| face);
    app.selected = vec![solid];
    app.start_pocket();
    assert!(app.extrude_ctx.as_ref().unwrap().exact_face.is_some());
    let plane = app.extrude_ctx.as_ref().unwrap().sketch.plane;
    app.set_extrude_depth(0.1);
    let (lo, hi) = preview_range(&app);
    assert!(hi.abs() < 1e-4 && (lo + 0.1).abs() < 1e-4, "{lo}..{hi}");
    app.commit_extrude();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while app.face_extrude_job.is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "native commit timed out"
        );
        app.poll_exact_face_commit();
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = app.body(solid).unwrap().shape.unwrap();
    assert_ne!(shape, result);
    assert!(app.kernel.exact_shape(result).is_some());
    let mesh = app.kernel.tessellate(result).unwrap();
    let distance = ray_mesh_intersect(
        (plane.origin() + plane.normal() * 5.).to_array(),
        (-plane.normal()).to_array(),
        &mesh,
    )
    .unwrap();
    assert!((distance - 5.1).abs() < 1e-4, "native depth: {distance}");
}

fn aim(app: &mut S33DApp, plane: PlaneDef, depth: f64) -> (egui::Pos2, egui::Rect) {
    let center = plane.origin() - plane.normal() * depth;
    app.camera.eye =
        (center + plane.normal() * 50. + glam::DVec3::from_array(plane.x_axis) * 2.).as_vec3();
    app.camera.target = center.as_vec3();
    app.camera.up = glam::DVec3::from_array(plane.y_axis).as_vec3();
    (
        egui::pos2(400., 300.),
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.)),
    )
}

fn pointer(ctx: &egui::Context, app: &mut S33DApp, pos: egui::Pos2, pressed: Option<bool>) {
    let mut events = vec![egui::Event::PointerMoved(pos)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
    }
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
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

fn select_bottom(app: &mut S33DApp, solid: BodyId, sketch: BodyId, depth: f64) {
    let plane = app.profile_sketch(sketch).unwrap().plane;
    let region = app.regions_of(sketch).remove(0);
    let centroid = region.outer.iter().fold(glam::DVec2::ZERO, |sum, p| {
        sum + glam::DVec2::from_array(*p)
    }) / region.outer.len() as f64;
    let mut centered = plane;
    centered.origin = plane.to_world(centroid.x, centroid.y);
    let (pos, rect) = aim(app, centered, depth);
    let face = app.pick_face(pos, rect);
    let region = app.pick_sketch_region(pos, rect);
    assert_eq!(region.as_ref().unwrap().0, sketch);
    let hit = &face.as_ref().expect("bottom exists").0;
    assert!(
        (glam::DVec3::from_array(hit.center.map(f64::from)) - plane.origin()).dot(plane.normal())
            < -0.01
    );
    assert!(
        app.face_precedes_sketch_region(&face, &region),
        "bottom must win over consumed sketch"
    );
    let ctx = egui::Context::default();
    pointer(&ctx, app, pos, None);
    app.update_hover(pos, rect);
    assert_eq!(app.hover_face.as_ref().unwrap().body_id, solid);
    for _ in 0..2 {
        pointer(&ctx, app, pos, Some(true));
        pointer(&ctx, app, pos, Some(false));
    }
    assert_eq!(app.selected, vec![solid]);
    assert!(app.selected_regions.is_empty());
    let selected = app.face_pick.as_ref().unwrap();
    assert!(
        (glam::DVec3::from_array(selected.center.map(f64::from)) - plane.origin())
            .dot(plane.normal())
            < -0.01
    );
    assert!(app.body(sketch).unwrap().visible);
}

#[test]
fn blind_pocket_bottom_is_reachable_with_visible_circle_or_rectangle_after_roundtrip() {
    for (round, normal) in [(true, [0., 1., 0.]), (false, [1., 2., 3.])] {
        for linked in [true, false] {
            let (mut app, solid, sketch) = fixture(normal, round, linked);
            app.set_extrude_depth(8.);
            app.commit_extrude();
            let saved = app.project_to_s33d().unwrap();
            app.load_s33d_project(&saved).unwrap();
            select_bottom(&mut app, solid, sketch, 8.);
        }
    }
}

#[test]
fn unrelated_or_changed_sketches_keep_region_priority_above_bottom() {
    for case in 0..4 {
        let (mut app, solid, sketch) = fixture([0., 1., 0.], true, true);
        app.set_extrude_depth(8.);
        app.commit_extrude();
        let plane = app.profile_sketch(sketch).unwrap().plane;
        if case == 0 {
            if let Feature::Extrude {
                pocket_source_sketches,
                ..
            } = &mut app.doc.body_mut(solid).unwrap().feature
            {
                pocket_source_sketches.clear();
            }
        } else {
            let Feature::Sketch(profile) = &mut app.doc.body_mut(sketch).unwrap().feature else {
                unreachable!()
            };
            match case {
                1 => profile.plane.origin[1] += 1.,
                2 => profile.entities = vec![SketchEntity::Circle { c: [0., 0.], r: 5. }],
                3 => {
                    profile.entities = vec![SketchEntity::Rect {
                        c1: [-4., -4.],
                        c2: [4., 4.],
                    }]
                }
                _ => unreachable!(),
            }
        }
        let (pos, rect) = aim(&mut app, plane, 8.);
        let face = app.pick_face(pos, rect);
        let region = app.pick_sketch_region(pos, rect);
        assert!(face.is_some() && region.is_some());
        assert!(
            !app.face_precedes_sketch_region(&face, &region),
            "unrelated or changed overlay {case}"
        );
    }
}

#[test]
#[ignore = "requires S33D_POCKET_FIXTURE pointing to an independent project 5 copy"]
fn copied_project5_preview_and_bottom_selection() {
    let path = std::env::var_os("S33D_POCKET_FIXTURE").expect("S33D_POCKET_FIXTURE");
    let before = std::fs::read(&path).unwrap();
    let mut app = app();
    app.load_s33d_project(std::str::from_utf8(&before).unwrap())
        .unwrap();
    // First use the user's saved oblique camera, before the controlled view.
    let plane = app.profile_sketch(4).unwrap().plane;
    let center = glam::DVec3::from_array(plane.to_world(13., 16.)) - plane.normal() * 8.;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    app.viewport_rect = rect;
    let pos = app.world_to_viewport(center.as_vec3(), rect).unwrap();
    let face = app.pick_face(pos, rect);
    let region = app.pick_sketch_region(pos, rect);
    assert_eq!(region.as_ref().unwrap().0, 4);
    assert!(app.face_precedes_sketch_region(&face, &region));
    assert!((f64::from(face.unwrap().0.center[1]) - 2.).abs() < 1e-4);
    select_bottom(&mut app, 2, 4, 8.);
    // Return to the pre-pocket state without changing the source file.
    assert!(app.undo());
    app.selected = vec![4];
    assert!(app.set_region_selection(4, vec![0]));
    app.start_pocket();
    app.set_extrude_depth(0.1);
    let (lo, hi) = preview_range(&app);
    assert!(hi.abs() < 1e-4 && (lo + 0.1).abs() < 1e-4, "{lo}..{hi}");
    assert_eq!(std::fs::read(path).unwrap(), before);
}
