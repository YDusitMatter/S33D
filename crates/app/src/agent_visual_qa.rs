use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};

fn app() -> S33DApp {
    S33DApp::from_ui_state(UiStateFile::default(), false)
}
fn call(app: &mut S33DApp, method: &str, params: Value) -> Value {
    app.execute_agent_request(AgentRequest::new(json!(1), method, params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}
fn product(app: &mut S33DApp) -> BodyId {
    call(
        app,
        "primitive.create",
        json!({"kind":"box","width":40,"depth":20,"height":10,"name":"Reference product"}),
    )["id"]
        .as_u64()
        .unwrap()
}
fn temp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("s33d-photo-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn legacy_workflow_get_matches_embedded_guide_without_editing_document() {
    let mut app = app();
    product(&mut app);
    let before = call(&mut app, "document.summary", json!({}));
    let project = app.project_to_s33d().unwrap();
    for mode in ["auto", "photo", "model", "edit", "geometry"] {
        assert_eq!(
            call(&mut app, "workflow.get", json!({"mode":mode})),
            crate::workflow_guide::guide(mode).unwrap()
        );
    }
    assert_eq!(
        call(&mut app, "workflow.get", json!({})),
        crate::workflow_guide::guide("auto").unwrap()
    );
    for params in [
        json!({"mode":"invalid"}),
        json!({"unexpected":true}),
        json!({"mode":12}),
    ] {
        assert!(app
            .execute_agent_request(AgentRequest::new(json!(1), "workflow.get", params))
            .is_err());
    }
    assert!(crate::agent_protocol::is_read_only_method("workflow.get"));
    assert_eq!(call(&mut app, "document.summary", json!({})), before);
    assert_eq!(app.project_to_s33d().unwrap(), project);
}

#[test]
fn summary_is_lightweight_and_revision_survives_undo_redo_without_aba() {
    let mut app = app();
    let initial = call(&mut app, "document.summary", json!({}));
    let id = product(&mut app);
    let a = call(&mut app, "document.summary", json!({}));
    let legacy = call(&mut app, "document.get", json!({}));
    assert!(legacy["bodies"][0]["feature"].is_object());
    assert!(a["bodies"][0].get("feature").is_none());
    assert_eq!(a["bodies"][0]["bounds"]["size"], json!([40., 10., 20.]));
    assert!(a["revision"].as_u64() > initial["revision"].as_u64());
    call(&mut app, "history.undo", json!({}));
    call(&mut app, "history.redo", json!({}));
    let b = call(&mut app, "document.summary", json!({}));
    assert!(b["revision"].as_u64() > a["revision"].as_u64());
    let before = app.doc.bodies.clone();
    let stale=app.execute_agent_request(AgentRequest::new(json!(2),"body.material.set",json!({"body_ids":[id],"base_color_srgb":[0.,0.,0.],"_expected":{"document_id":a["document_id"],"revision":a["revision"]}})));
    assert_eq!(stale.unwrap_err().0, "stale");
    assert_eq!(before, app.doc.bodies);
    let rev = b["revision"].clone();
    call(
        &mut app,
        "view.set",
        json!({"direction":"left","projection":"orthographic"}),
    );
    assert_eq!(
        call(&mut app, "document.summary", json!({}))["revision"],
        rev
    );
    app.set_body_visible(id, false);
    assert!(call(&mut app, "document.summary", json!({}))["revision"].as_u64() > rev.as_u64());
    let text = app.project_to_s33d().unwrap();
    let doc_id = app.agent_clock.document_id.clone();
    app.load_s33d_project(&text).unwrap();
    assert_ne!(app.agent_clock.document_id, doc_id);
}

#[test]
fn material_batch_is_atomic_preserves_fields_and_roundtrips() {
    let mut app = app();
    let id = product(&mut app);
    app.doc.body_mut(id).unwrap().material.color[3] = 0.7;
    app.doc.body_mut(id).unwrap().material.roughness = 0.83;
    let old = app.doc.bodies.clone();
    let bad = app.execute_agent_request(AgentRequest::new(
        json!(1),
        "body.material.set",
        json!({"body_ids":[id,9999],"base_color_srgb":[1.,0.,0.]}),
    ));
    assert!(bad.is_err());
    assert_eq!(app.doc.bodies, old);
    for color in [json!([-0.1, 0., 0.]), json!([0., 2., 0.]), json!([0., 0.])] {
        assert!(app
            .execute_agent_request(AgentRequest::new(
                json!(1),
                "body.material.set",
                json!({"body_ids":[id],"base_color_srgb":color})
            ))
            .is_err());
        assert_eq!(app.doc.bodies, old);
    }
    call(
        &mut app,
        "body.material.set",
        json!({"body_ids":[id],"base_color_srgb":[0.2,0.6,0.9]}),
    );
    let material = app.doc.body(id).unwrap().material.clone();
    assert_eq!(material.color, [0.2, 0.6, 0.9, 0.7]);
    assert_eq!(material.roughness, 0.83);
    let text = app.project_to_s33d().unwrap();
    app.load_s33d_project(&text).unwrap();
    assert_eq!(app.doc.body(id).unwrap().material, material);
    assert!(app.undo());
    assert_eq!(app.doc.bodies[0].material.color, old[0].material.color);
}

#[test]
fn reference_assets_are_portable_deduplicated_bounded_and_atomic() {
    let dir = temp("assets");
    let image = image::RgbImage::from_fn(80, 40, |x, y| image::Rgb([x as u8, y as u8, 100]));
    image.save(dir.join("photo.png")).unwrap();
    image.save(dir.join("photo.jpg")).unwrap();
    std::fs::write(dir.join("bad.png"), b"not an image").unwrap();
    let mut app = app();
    call(
        &mut app,
        "reference.import",
        json!({"paths":[dir.join("photo.png"),dir.join("photo.jpg")]}),
    );
    assert!(app.project_has_changes());
    assert_eq!(app.references.assets.len(), 2);
    let previous = app.references.clone();
    call(
        &mut app,
        "reference.import",
        json!({"paths":[dir.join("photo.png")]}),
    );
    assert_eq!(app.references, previous);
    assert!(app
        .execute_agent_request(AgentRequest::new(
            json!(1),
            "reference.import",
            json!({"paths":[dir.join("photo.png"),dir.join("bad.png")]})
        ))
        .is_err());
    assert_eq!(app.references, previous);
    let asset_id = app.reference_ids()[0].clone();
    let image = call(&mut app, "reference.get", json!({"asset_id":asset_id}));
    assert!(STANDARD
        .decode(image["image"]["data"].as_str().unwrap())
        .unwrap()
        .starts_with(b"\x89PNG"));
    assert!(app
        .execute_agent_request(AgentRequest::new(
            json!(1),
            "reference.get",
            json!({"asset_id":dir.join("photo.png")})
        ))
        .is_err());
    let saved = dir.join("portable.s33d");
    app.save_project_to_path(&saved).unwrap();
    assert!(!app.project_has_changes());
    std::fs::remove_file(dir.join("photo.png")).unwrap();
    std::fs::remove_file(dir.join("photo.jpg")).unwrap();
    let mut reopened = S33DApp::from_ui_state(UiStateFile::default(), false);
    reopened.open_project_from_path(&saved).unwrap();
    assert_eq!(reopened.references, previous);
    assert!(!reopened.project_has_changes());
    let mut corrupted: Value = serde_json::from_str(&app.project_to_s33d().unwrap()).unwrap();
    corrupted["references"]["schema_version"] = json!(99);
    let before = reopened.doc.bodies.clone();
    let identity = reopened.agent_clock.document_id.clone();
    assert!(reopened.load_s33d_project(&corrupted.to_string()).is_err());
    assert_eq!(reopened.doc.bodies, before);
    assert_eq!(reopened.agent_clock.document_id, identity);
    reopened.references.assets[0].png_data = "a".into();
    assert!(reopened.references.validate().is_err());
    let mut oversized = vec![0u8; crate::reference_assets::MAX_FILE_BYTES as usize + 1];
    oversized[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    std::fs::write(dir.join("large.png"), oversized).unwrap();
    assert!(crate::reference_assets::ReferenceAsset::from_path(&dir.join("large.png")).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_camera_and_render_requests_preserve_the_document_and_view() {
    let mut app = app();
    product(&mut app);
    let camera = agent_visual::camera_value(&app.camera);
    let bodies = app.doc.bodies.clone();
    for params in [
        json!({"eye":[0.,0.,0.],"target":[0.,0.,0.]}),
        json!({"up":[0.,0.,0.]}),
        json!({"projection":"fish"}),
        json!({"fov_y_deg":180}),
        json!({"near":100.,"far":1.}),
    ] {
        assert!(app
            .execute_agent_request(AgentRequest::new(json!(1), "view.set", params))
            .is_err());
        assert_eq!(agent_visual::camera_value(&app.camera), camera);
    }
    for params in [
        json!({"width":99999}),
        json!({"width":2048,"height":2048}),
        json!({"body_ids":[9999]}),
        json!({"body_ids":[]}),
    ] {
        assert!(app
            .execute_agent_request(AgentRequest::new(json!(1), "view.render", params))
            .is_err());
    }
    assert_eq!(app.doc.bodies, bodies);
    assert_eq!(agent_visual::camera_value(&app.camera), camera);
    for direction in ["front", "back", "top", "bottom", "left", "right", "iso"] {
        call(&mut app, "view.set", json!({"direction":direction}));
        assert!(app.camera.proj_matrix(1.).is_finite());
    }
}

#[test]
fn fit_output_keeps_projection_and_all_corners_inside_wide_and_tall_images() {
    for ortho in [true, false] {
        for aspect in [0.125, 1., 8.] {
            let mut camera = Camera::default();
            camera.ortho = ortho;
            let min = glam::Vec3::new(-100., -5., -2.);
            let max = -min;
            camera.fit_output(min, max, aspect);
            assert_eq!(camera.ortho, ortho);
            let vp = camera.proj_matrix(aspect) * camera.view_matrix();
            for x in [min.x, max.x] {
                for y in [min.y, max.y] {
                    for z in [min.z, max.z] {
                        let p = vp.project_point3(glam::Vec3::new(x, y, z));
                        assert!(
                            p.x.abs() < 0.99 && p.y.abs() < 0.99 && p.z > 0. && p.z < 1.,
                            "{p:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn jpeg_exif_orientation_is_applied_and_metadata_is_not_redistributed() {
    let dir = temp("orientation");
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(60, 20)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let jpeg = jpeg.into_inner();
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut rotated = vec![0xff, 0xd8, 0xff, 0xe1];
    rotated.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    rotated.extend_from_slice(exif);
    rotated.extend_from_slice(&jpeg[2..]);
    let path = dir.join("rotated.jpg");
    std::fs::write(&path, rotated).unwrap();
    let asset = crate::reference_assets::ReferenceAsset::from_path(&path).unwrap();
    assert!(asset.orientation_applied);
    assert_eq!((asset.width, asset.height), (20, 60));
    let png = STANDARD.decode(&asset.png_data).unwrap();
    assert!(!png.windows(4).any(|w| w == b"eXIf"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn curve_color_is_used_by_native_scene_and_survives_save_reopen() {
    let mut app = app();
    let curve=call(&mut app,"curve.equation.apply",json!({"params":{
        "expressions":["10*cos(t)","10*sin(t)","t"],"domain":[0.,6.28318530718],"unit_mm":1.,"degrees":false,"spherical":false,
        "transform":[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.]
    }}))["id"].as_u64().unwrap();
    call(
        &mut app,
        "body.material.set",
        json!({"body_ids":[curve],"base_color_srgb":[0.1,0.7,0.2]}),
    );
    app.selected.clear();
    let mut scene = SceneData::default();
    app.curve_scene(&mut scene);
    assert_eq!(scene.body_edge_lines[0].color, [0.1, 0.7, 0.2]);
    let text = app.project_to_s33d().unwrap();
    app.load_s33d_project(&text).unwrap();
    assert_eq!(
        &app.doc.body(curve).unwrap().material.color[..3],
        &[0.1, 0.7, 0.2]
    );
}

#[test]
#[ignore = "Requires a native wgpu adapter; run explicitly for photo/model image acceptance"]
fn native_model_images_preserve_state_and_show_color_geometry_and_hidden_subset() {
    let mut app = app();
    let id = product(&mut app);
    call(
        &mut app,
        "body.material.set",
        json!({"body_ids":[id],"base_color_srgb":[0.8,0.1,0.1]}),
    );
    let state = app.project_to_s33d().unwrap();
    let revision = app.agent_clock.revision;
    let scene = app.scene_version;
    let image = call(
        &mut app,
        "view.render",
        json!({"width":385,"height":256,"camera":{"direction":"iso"},"background":[0.,0.,0.]}),
    );
    let bytes = STANDARD
        .decode(image["image"]["data"].as_str().unwrap())
        .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (385, 256));
    assert!(
        decoded
            .pixels()
            .filter(|p| u16::from(p[0]) > u16::from(p[1]) * 2 && p[0] > 80)
            .count()
            > 500
    );
    assert_eq!(app.project_to_s33d().unwrap(), state);
    assert_eq!(app.agent_clock.revision, revision);
    assert_eq!(app.scene_version, scene);
    assert_eq!(image["body_ids"], json!([id]));
    if let Some(dir) = std::env::var_os("S33D_PHOTO_QA_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(PathBuf::from(dir).join("native-product.png"), bytes).unwrap();
    }
    app.set_body_visible(id, false);
    assert!(app
        .execute_agent_request(AgentRequest::new(json!(1), "view.render", json!({})))
        .is_err());
    let hidden = call(
        &mut app,
        "view.render",
        json!({"body_ids":[id],"width":256,"height":385,"camera":{"direction":"front","projection":"orthographic"}}),
    );
    assert_eq!(hidden["body_ids"], json!([id]));
    assert!(!app.doc.body(id).unwrap().visible);
}
