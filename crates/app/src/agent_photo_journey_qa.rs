//! Real stdio bridge -> loopback -> desktop ownership -> native image round trip.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "Requires built MCP bridge and native rendering adapter; run for packaged photo acceptance"]
fn packaged_mcp_photo_journey_returns_images_corrects_saves_and_reopens() {
    let bridge = std::env::var_os("S33D_PHOTO_QA_BRIDGE").expect("Set built bridge path");
    let dir = PathBuf::from(
        std::env::var_os("S33D_PHOTO_QA_DIR").expect("Set private QA output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let photo = dir.join("dimensioned-product-reference.png");
    // Publicly reproducible reference, 40 x 20 silhouette at 8 px/mm.
    // This fixture asserts transport/geometry, not photographic likeness.
    image::RgbImage::from_fn(400, 240, |x, y| {
        if (40..360).contains(&x) && (40..200).contains(&y) {
            image::Rgb([50, 120, 205])
        } else {
            image::Rgb([245, 245, 245])
        }
    })
    .save(&photo)
    .unwrap();
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    app.background_modeling = true;
    app.agent_server = AgentServer::start("127.0.0.1:0").unwrap();
    let address = app.agent_server.address().unwrap().to_string();
    let done = std::thread::spawn(move || {
        let mut child = Command::new(bridge)
            .arg("mcp")
            .arg("--address")
            .arg(address)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut n = 0;
        let mut rpc = |method: &str, params: Value| -> Value {
            n += 1;
            serde_json::to_writer(
                &mut stdin,
                &json!({"jsonrpc":"2.0","id":n,"method":method,"params":params}),
            )
            .unwrap();
            stdin.write_all(b"\n").unwrap();
            stdin.flush().unwrap();
            let mut line = String::new();
            assert!(stdout.read_line(&mut line).unwrap() > 0);
            let value: Value = serde_json::from_str(&line).unwrap();
            assert!(value.get("error").is_none(), "{value}");
            value["result"].clone()
        };
        let init = rpc(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"S33D release acceptance","version":"1"}}),
        );
        assert_eq!(init["protocolVersion"], "2025-11-25");
        let list = rpc("tools/list", json!({}));
        assert_eq!(list["tools"].as_array().unwrap().len(), 6);
        let mut tool = |name: &str, args: Value| -> Value {
            let r = rpc("tools/call", json!({"name":name,"arguments":args}));
            assert_ne!(r["isError"], true, "{}", r["content"][0]["text"]);
            r
        };
        let guide = tool("s33d_workflow", json!({"mode":"photo"}));
        assert_eq!(guide["isError"], false);
        tool("s33d_describe", json!({}));
        let import = tool(
            "s33d_execute",
            json!({"method":"reference.import","params":{"paths":[photo]},"idempotency_key":"journey-import"}),
        );
        let imported: Value =
            serde_json::from_str(import["content"][0]["text"].as_str().unwrap()).unwrap();
        let asset = imported["result"]["imported"][0]["asset_id"].clone();
        let reference = tool("s33d_reference", json!({"asset_id":asset}));
        assert_eq!(reference["content"][1]["type"], "image");
        let bytes = STANDARD
            .decode(reference["content"][1]["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 400);
        std::fs::write(dir.join("reference-returned.png"), bytes).unwrap();
        let state = tool("s33d_execute", json!({"method":"document.summary"}));
        let summary: Value =
            serde_json::from_str(state["content"][0]["text"].as_str().unwrap()).unwrap();
        let create = json!({"method":"primitive.create","params":{"kind":"box","width":40,"depth":20,"height":8,"name":"Photo product","_expected":{"document_id":summary["result"]["document_id"],"revision":summary["result"]["revision"]}},"idempotency_key":"journey-product"});
        let first = tool("s33d_execute", create.clone());
        let first: Value =
            serde_json::from_str(first["content"][0]["text"].as_str().unwrap()).unwrap();
        let id = first["result"]["id"].clone();
        let replay = tool("s33d_execute", create);
        let replay: Value =
            serde_json::from_str(replay["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(first["result"], replay["result"]);
        for (name, color) in [
            ("before", [0.8, 0.1, 0.1]),
            ("corrected", [0.196, 0.471, 0.804]),
        ] {
            tool(
                "s33d_execute",
                json!({"method":"body.material.set","params":{"body_ids":[id],"base_color_srgb":color},"idempotency_key":format!("journey-{name}")}),
            );
            let image = tool(
                "s33d_render",
                json!({"width":400,"height":240,"camera":{"direction":"top","projection":"orthographic"}}),
            );
            assert_eq!(image["content"][1]["type"], "image");
            assert!(!image["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("base64"));
            let bytes = STANDARD
                .decode(image["content"][1]["data"].as_str().unwrap())
                .unwrap();
            assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 400);
            std::fs::write(dir.join(format!("model-{name}.png")), bytes).unwrap();
        }
        let saved = dir.join("photo-journey.s33d");
        tool(
            "s33d_execute",
            json!({"method":"project.save","params":{"path":saved},"idempotency_key":"journey-save"}),
        );
        std::fs::remove_file(photo).unwrap();
        tool(
            "s33d_execute",
            json!({"method":"project.open","params":{"path":saved},"idempotency_key":"journey-reopen"}),
        );
        tool("s33d_reference", json!({"asset_id":asset}));
        let summary = tool("s33d_execute", json!({"method":"document.summary"}));
        let summary: Value =
            serde_json::from_str(summary["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(summary["result"]["body_count"], 1);
        assert_eq!(summary["result"]["unsaved_changes"], false);
        assert_eq!(
            summary["result"]["bodies"][0]["bounds"]["size"],
            json!([40., 8., 20.])
        );
        std::fs::write(dir.join("mcp-journey-result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"tools":6,"reference_image":true,"native_model_images":2,"correction":"RGB color corrected to reference","save_reopen":true,"original_photo_removed":true,"idempotent_expected_revision_replay":true,"body_count":1,"production_screenshot_dependency":false})).unwrap()).unwrap();
        drop(tool);
        drop(rpc);
        drop(stdin);
        assert!(child.wait().unwrap().success());
    });
    let start = Instant::now();
    while !done.is_finished() && start.elapsed() < Duration::from_secs(180) {
        app.poll_model_job();
        app.process_agent_requests();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done.is_finished(), "MCP photo acceptance timed out");
    done.join().unwrap();
}

#[test]
#[ignore = "Local complex surface regression: requires user project path and native adapter"]
fn complex_user_surface_model_renders_without_mutating_original_project() {
    let path = PathBuf::from(
        std::env::var_os("S33D_COMPLEX_PHOTO_PROJECT")
            .expect("Set a local complex surface project"),
    );
    let dir = PathBuf::from(std::env::var_os("S33D_PHOTO_QA_DIR").expect("Set private QA output"));
    let mut app = S33DApp::from_ui_state(UiStateFile::default(), false);
    app.open_project_from_path(&path).unwrap();
    let original = app.project_to_s33d().unwrap();
    for direction in ["top", "front", "iso"] {
        let result = app
            .execute_agent_request(AgentRequest::new(
                json!(1),
                "view.render",
                json!({"width":1024,"height":768,"camera":{"direction":direction}}),
            ))
            .unwrap();
        let bytes = STANDARD
            .decode(result["image"]["data"].as_str().unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let first = image.get_pixel(0, 0);
        assert!(
            image.pixels().filter(|p| p != &first).count() > 1000,
            "Empty complex model render"
        );
        std::fs::write(dir.join(format!("complex-{direction}.png")), bytes).unwrap();
    }
    assert_eq!(app.project_to_s33d().unwrap(), original);
}
