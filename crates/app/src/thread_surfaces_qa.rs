//! Partial threads must leave independently pickable, persistent cylinder bands.
use super::*;
use crate::threads::{Params, Saved, Target};

fn fixture(internal: bool) -> (S33DApp, BodyId, Target) {
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    let id = if internal {
        let radius = 3. - Params::default().depth;
        let shape = app
            .kernel
            .revolve_region(
                &Region2D::new(vec![[radius, 0.], [6., 0.], [6., 12.], [radius, 12.]]),
                &DatumPlane::Front.def(),
                &Axis2D {
                    point: [0., 0.],
                    dir: [0., 1.],
                },
                360.,
            )
            .unwrap();
        let id = app.doc.alloc_id();
        app.doc.add_body(Body {
            id,
            name: "Partial thread tube".into(),
            kind: BodyKind::Solid,
            feature: Feature::Derived {
                source: "test tube".into(),
                operation: None,
            },
            shape: Some(shape),
            material: Material::default(),
            visible: true,
        });
        id
    } else {
        app.execute_agent_request(AgentRequest::new(
            json!(1),
            "primitive.create",
            json!({
                "kind":"cylinder", "radius":3., "height":12.
            }),
        ))
        .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let target = wall_targets(&app, id, internal).pop().unwrap();
    (app, id, target)
}

fn wall_targets(app: &S33DApp, id: BodyId, internal: bool) -> Vec<Target> {
    app.thread_targets(&[id])
        .unwrap()
        .into_iter()
        .filter(|t| t.internal == internal)
        .collect()
}

fn params(target: &Target, offset: f64, length: f64) -> Params {
    Params {
        diameter: 2.
            * (target.radius
                + if target.internal {
                    Params::default().depth
                } else {
                    0.
                }),
        length,
        offset,
        ..Default::default()
    }
}

fn pick_band(app: &mut S33DApp, target: &Target) -> FaceHit {
    let shape = app.doc.body(target.body_id).unwrap().shape.unwrap();
    let original = app.kernel.tessellate(shape).unwrap();
    let signature = crate::threads::mesh_signature(&original);
    let axis = glam::DVec3::from_array(target.axis);
    let center = glam::DVec3::from_array(target.origin) + axis * (target.length * 0.5);
    let radial = axis.any_orthonormal_vector();
    app.camera.eye = (center + radial * if target.internal { 0. } else { 25. }).as_vec3();
    app.camera.target = (center + radial * if target.internal { 1. } else { 0. }).as_vec3();
    app.camera.up = axis.as_vec3();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    let face = app
        .pick_face(rect.center(), rect)
        .expect("ray must hit the remaining cylinder")
        .0;
    let picked = app
        .thread_target_for_face(&face)
        .expect("remaining band must be cylindrical");
    assert!(
        picked.same_wall(target),
        "picked {picked:?}, expected {target:?}"
    );
    assert!(
        face.triangles.len() > 12,
        "select the whole band, not one facet"
    );
    // FaceHit indices refer to the rendered/welded mesh, whose vertex order
    // can differ from the kernel's stored Boolean result.
    let mesh = app.mesh_for(shape);
    let (original_area, original_volume) = crate::ui::mesh_surface_area_and_volume(&original);
    let (area, volume) = crate::ui::mesh_surface_area_and_volume(&mesh);
    assert!(
        (area - original_area).abs() < original_area.max(1.) * 1e-5,
        "display added or lost faces"
    );
    assert!(
        (volume - original_volume).abs() < original_volume.abs().max(1.) * 1e-5,
        "display changed volume"
    );
    for &i in &face.triangles {
        let delta = glam::DVec3::from_array(mesh.positions[i as usize].map(f64::from))
            - glam::DVec3::from_array(target.origin);
        let z = delta.dot(axis);
        assert!(
            z >= -0.002 && z <= target.length + 0.002,
            "highlight crossed a thread boundary: {z}"
        );
    }
    assert!(app.begin_thread_on_face(&face));
    assert!(app.thread_choices[0].same_wall(target));
    assert_eq!(
        signature,
        crate::threads::mesh_signature(&app.kernel.tessellate(shape).unwrap()),
        "selection must not change the stored solid"
    );
    face
}

#[test]
fn partial_thread_keeps_upper_band_pickable_in_old_saves_and_consecutive_apply() {
    for internal in [false, true] {
        let (mut app, id, original) = fixture(internal);
        // Warm the picker cache before replacing the shape.
        pick_band(&mut app, &original);
        app.apply_threads(vec![original.clone()], params(&original, 0., 6.))
            .unwrap();
        let remaining = wall_targets(&app, id, internal);
        assert_eq!(remaining.len(), 1, "uncut half was lost: {remaining:?}");
        let upper = remaining[0].clone();
        assert!((upper.length - 6.).abs() < 1e-6);
        assert!(
            glam::DVec3::from_array(upper.origin).distance(
                glam::DVec3::from_array(original.origin)
                    + glam::DVec3::from_array(original.axis) * 6.
            ) < 1e-6
        );
        pick_band(&mut app, &upper);

        // Reproduce the old writer: only unrelated walls were kept. Existing
        // projects must recover the missing half from certified provenance.
        let Feature::Derived { source, .. } = &mut app.doc.body_mut(id).unwrap().feature else {
            panic!()
        };
        let mut saved: Saved =
            serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
        saved.remaining.retain(|t| t.internal != internal);
        *source = format!("Thread v1: {}", serde_json::to_string(&saved).unwrap());
        let project = app.project_to_s33d().unwrap();
        app.load_s33d_project(&project).unwrap();
        let restored = wall_targets(&app, id, internal);
        assert_eq!(restored.len(), 1);
        assert!(restored[0].same_wall(&upper));
        pick_band(&mut app, &restored[0]);
        app.apply_threads(restored.clone(), params(&restored[0], 0., 6.))
            .unwrap();
        assert!(wall_targets(&app, id, internal).is_empty());
        assert!(app.undo());
        assert_eq!(wall_targets(&app, id, internal).len(), 1);
        assert!(app.redo());
        assert!(wall_targets(&app, id, internal).is_empty());
    }
}

#[test]
fn partial_thread_middle_leaves_two_distinct_bands_for_selection_and_batch_apply() {
    for internal in [false, true] {
        let (mut app, id, original) = fixture(internal);
        app.apply_threads(vec![original.clone()], params(&original, 4., 4.))
            .unwrap();
        let bands = wall_targets(&app, id, internal);
        assert_eq!(bands.len(), 2, "both uncut ends must survive: {bands:?}");
        assert_ne!(bands[0].face_id, bands[1].face_id);
        assert!(bands.iter().all(|t| (t.length - 4.).abs() < 1e-6));
        assert_eq!(
            serde_json::to_value(&bands).unwrap(),
            serde_json::to_value(wall_targets(&app, id, internal)).unwrap()
        );
        for band in &bands {
            pick_band(&mut app, band);
        }
        let before = app.doc.bodies.clone();
        assert!(app
            .apply_threads(vec![original.clone()], params(&original, 0., 12.))
            .is_err());
        assert_eq!(
            app.doc.bodies, before,
            "stale full-wall selection must not replace earlier threads"
        );
        app.apply_threads(bands.clone(), params(&bands[0], 0., 4.))
            .unwrap();
        assert!(wall_targets(&app, id, internal).is_empty());
        let Feature::Derived { source, .. } = &app.doc.body(id).unwrap().feature else {
            panic!()
        };
        let saved: Saved =
            serde_json::from_str(source.strip_prefix("Thread v1: ").unwrap()).unwrap();
        assert_eq!(saved.annotations.len(), 3);
        assert!(app.undo());
        assert_eq!(wall_targets(&app, id, internal).len(), 2);
    }
}

#[test]
fn partial_thread_reflection_keeps_remaining_band_selectable() {
    let (mut app, id, original) = fixture(false);
    app.apply_threads(vec![original.clone()], params(&original, 0., 6.))
        .unwrap();
    app.selected = vec![id];
    app.apply_transform(
        &glam::Mat4::from_scale(glam::vec3(-1., 1., 1.)),
        "reflect threaded cylinder",
    );
    let bands = wall_targets(&app, id, false);
    assert_eq!(bands.len(), 1);
    assert!((bands[0].length - 6.).abs() < 1e-6);
    pick_band(&mut app, &bands[0]);
}
