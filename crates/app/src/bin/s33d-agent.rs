//! MCP stdio and command-line bridge for a running S33D desktop process.
#![recursion_limit = "256"]

#[allow(dead_code)]
#[path = "../agent_protocol.rs"]
mod agent_protocol;
#[path = "../workflow_guide.rs"]
mod workflow_guide;

use agent_protocol::{
    connect_desktop_watch, send_request, user_ui_state_path, AgentRequest, AgentResponse,
    DEFAULT_AGENT_ADDRESS, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let address_override =
        take_option(&mut args, "--address").or_else(|| std::env::var("S33D_AGENT_ADDRESS").ok());
    match args.first().map(String::as_str) {
        None | Some("mcp") => run_mcp(address_override.as_deref()),
        Some("call") => run_call(address_override.as_deref(), &args[1..]),
        Some("status") => run_status(address_override.as_deref()),
        Some("describe") => run_single(address_override.as_deref(), "system.describe", json!({})),
        Some("help") | Some("--help") | Some("-h") => print_help(),
        Some("--version") | Some("-V") => println!("s33d-agent {}", env!("CARGO_PKG_VERSION")),
        Some(other) => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn take_option(args: &mut Vec<String>, option: &str) -> Option<String> {
    let index = args.iter().position(|value| value == option)?;
    if index + 1 >= args.len() {
        eprintln!("{option} requires a value");
        std::process::exit(2);
    }
    let value = args.remove(index + 1);
    args.remove(index);
    Some(value)
}

fn print_help() {
    println!(
        "S33D Agent bridge v{PROTOCOL_VERSION}\n\
         \n\
         Usage:\n\
           s33d-agent --version\n\
           s33d-agent mcp [--address 127.0.0.1:53433]\n\
           s33d-agent status [--address ...]\n\
           s33d-agent describe [--address ...]\n\
           s33d-agent call <method> [params-json] [--request-key <unique-write-key>] [--address ...]\n\
         \n\
         The S33D desktop app must be running. Examples:\n\
           s33d-agent call document.get\n\
           s33d-agent call primitive.create \"{{\\\"kind\\\":\\\"box\\\",\\\"width\\\":40,\\\"depth\\\":20,\\\"height\\\":8}}\"\n\
         \n\
         Without --address or S33D_AGENT_ADDRESS, the bridge reads the port saved by S33D Settings from the per-user S33D configuration."
    );
}

fn run_status(address_override: Option<&str>) {
    run_single(address_override, "system.ping", json!({}));
}

fn run_call(address_override: Option<&str>, args: &[String]) {
    let mut args = args.to_vec();
    let key = take_option(&mut args, "--request-key");
    let Some(method) = args.first() else {
        eprintln!("call requires a method; use `s33d-agent describe` to list methods");
        std::process::exit(2);
    };
    let params = match args.get(1) {
        Some(json_text) => serde_json::from_str(json_text).unwrap_or_else(|error| {
            eprintln!("invalid params JSON: {error}");
            std::process::exit(2);
        }),
        None => json!({}),
    };
    run_single_with_key(address_override, method, params, key.as_deref());
}

fn run_single(address_override: Option<&str>, method: &str, params: Value) {
    run_single_with_key(address_override, method, params, None);
}

fn run_single_with_key(
    address_override: Option<&str>,
    method: &str,
    params: Value,
    key: Option<&str>,
) {
    match call_s33d(address_override, method, params, key) {
        Ok(response) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string())
            );
            if !response.ok {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn call_s33d(
    address_override: Option<&str>,
    method: &str,
    params: Value,
    key: Option<&str>,
) -> Result<AgentResponse, String> {
    let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let address = configured_address(address_override);
    let mut request = AgentRequest::new(json!(id), method, params);
    if !agent_protocol::is_read_only_method(method) || key.is_some() {
        request.idempotency_key = Some(key.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "bridge-{}-{}-{id}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            )
        }));
    }
    if key.is_none() && !agent_protocol::is_read_only_method(method) {
        // Preserve the CLI identity even if an outer process stops waiting.
        eprintln!(
            "S33D request key: {}",
            request.idempotency_key.as_deref().unwrap()
        );
    }
    send_request(&address, &request).map_err(|error| format!(
        "{error}; idempotency_key={:?}. Execution may have started. Retry the SAME key and payload within 10 minutes on the same running server; do not blindly retry with a new key.", request.idempotency_key
    ))
}

fn configured_address(address_override: Option<&str>) -> String {
    address_override
        .map(str::to_owned)
        .or_else(saved_settings_address)
        .unwrap_or_else(|| DEFAULT_AGENT_ADDRESS.to_string())
}

fn saved_settings_address() -> Option<String> {
    std::fs::read_to_string(user_ui_state_path())
        .ok()
        .and_then(|text| address_from_ui_state(&text))
}

fn address_from_ui_state(text: &str) -> Option<String> {
    let state: Value = serde_json::from_str(text).ok()?;
    let port = state.pointer("/settings/agent_control_port")?.as_u64()?;
    (1..=u16::MAX as u64)
        .contains(&port)
        .then(|| format!("127.0.0.1:{port}"))
}

fn run_mcp(address_override: Option<&str>) {
    let watch_address = address_override.map(str::to_owned);
    thread::spawn(move || watch_desktop_exit(watch_address.as_deref()));
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("MCP input error: {error}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                write_mcp(
                    &mut stdout,
                    json!({
                        "jsonrpc":"2.0",
                        "id": Value::Null,
                        "error":{"code":-32700,"message":format!("Parse error: {error}")}
                    }),
                );
                continue;
            }
        };
        if let Some(response) = handle_mcp_message(address_override, &message) {
            write_mcp(&mut stdout, response);
        }
    }
}

fn watch_desktop_exit(address_override: Option<&str>) {
    loop {
        let Ok(address) = configured_address(address_override).parse() else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        match connect_desktop_watch(address) {
            Ok(mut desktop) => {
                // The main thread may be blocked waiting for the MCP host's
                // next input. This bridge owns no desktop state to flush.
                loop {
                    match desktop.read(&mut [0]) {
                        Ok(0) => std::process::exit(0),
                        Err(error) if error.kind() != io::ErrorKind::Interrupted => {
                            std::process::exit(0)
                        }
                        _ => continue,
                    }
                }
            }
            Err(error) => {
                // Before the first handshake, keep serving offline tools and
                // allow either process to start first. Older desks can still
                // process requests without participating in the handshake.
                let delay = if error.kind() == io::ErrorKind::Unsupported {
                    3
                } else {
                    1
                };
                thread::sleep(Duration::from_secs(delay));
            }
        }
    }
}

fn write_mcp(writer: &mut impl Write, response: Value) {
    if serde_json::to_writer(&mut *writer, &response).is_ok() {
        let _ = writer.write_all(b"\n");
        let _ = writer.flush();
    }
}

fn handle_mcp_message(address_override: Option<&str>, message: &Value) -> Option<Value> {
    let method = message.get("method")?.as_str()?;
    let id = message.get("id").cloned();
    if method.starts_with("notifications/") {
        return None;
    }
    let Some(id) = id else {
        return None;
    };
    let result = match method {
        "initialize" => {
            let requested_version = message
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "initialize requires protocolVersion".to_owned()));
            let requested_version = match requested_version {
                Ok(v) => v,
                Err((code, message)) => {
                    return Some(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
                    )
                }
            };
            let negotiated = if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"]
                .contains(&requested_version)
            {
                requested_version
            } else {
                "2025-11-25"
            };
            Ok(json!({
                "protocolVersion": negotiated,
                "capabilities": {"tools": {"listChanged": false}, "resources":{"subscribe":false,"listChanged":false}},
                "serverInfo": {"name":"s33d-agent", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "S33D controls the open desktop CAD document. For text, photo or continued edits, get s33d_workflow, s33d_describe and document.summary first. Follow the shared modeling strategy; maintain actual stage, largest deviation, causal decision and required recheck. Major shape failures block detailing; guide hashes are provenance, not proof of compliance. The visual Agent interprets photos visible in its chat and calls S33D to model; importing them into S33D is optional, not a prerequisite. Do not require asset IDs, server-readable photo paths or a second upload for images the Agent can already see. Use s33d_render for native model comparisons and s33d_reference only for optional imported references; no desktop capture dependency. Use millimeters/degrees, stable IDs, and verify summary after writes. Choose idempotency_key BEFORE each write; retry the SAME payload/key only within ten minutes of completion on the same listener. Queued cancellation never executes; in_progress cannot be interrupted. After restart/expiry reconcile state before repeating. Optional _expected document identity/revision prevents stale commands; it is separate from view revision. Resolve active desktop tools/dialogs before edits. Preserve user work and reuse already authorized dimensions and decisions."
            }))
        }
        "ping" => Ok(json!({})),
        "resources/list" => Ok(workflow_guide::resources()),
        "resources/read" => workflow_guide::resource(
            message
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .map_err(|e| (-32602, e)),
        "tools/list" => Ok(json!({"tools": mcp_tools()})),
        "tools/call" => call_mcp_tool(
            address_override,
            message.get("params").unwrap_or(&Value::Null),
        ),
        _ => Err((-32601, format!("Method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
        }
    })
}

fn mcp_tools() -> Vec<Value> {
    vec![
        json!({"name":"s33d_workflow","description":"Get the shipped version-matched workflow for text modeling, photo reconstruction or continued edits. Call at the start of modeling; no separately installed skill needed.","inputSchema":{"type":"object","properties":{"mode":{"type":"string","enum":["auto","model","photo","edit","geometry"]}},"additionalProperties":false}}),
        json!({"name":"s33d_reference","description":"List optional project reference photos, or read one registered asset as a native MCP image. The visual Agent can model directly from photos visible in its chat without importing them or calling this tool. For optional project storage or alternate delivery, use reference.import with an actual available local path, or Settings > Agent Connect > Import reference photos. Do not guess attachment paths.","inputSchema":{"type":"object","properties":{"asset_id":{"type":"string"},"max_edge":{"type":"integer","minimum":128,"maximum":2048}},"additionalProperties":false}}),
        json!({"name":"s33d_render","description":"Return a native S33D model image and immutable document snapshot metadata for visual comparison. Defaults to fitted visible committed geometry, does not change GUI camera/selection/history. Native GPU or software wgpu adapter required, no screen capture.","inputSchema":{"type":"object","properties":{"width":{"type":"integer","minimum":128,"maximum":2048},"height":{"type":"integer","minimum":128,"maximum":2048},"body_ids":{"type":"array","items":{"type":"integer"},"minItems":1,"maxItems":1024},"fit":{"type":"boolean"},"background":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":3,"maxItems":3},"camera":{"type":"object"},"_expected":{"type":"object","properties":{"document_id":{"type":"string"},"revision":{"type":"integer","minimum":0}},"required":["document_id","revision"],"additionalProperties":false}},"additionalProperties":false}}),
        json!({
            "name":"s33d_describe",
            "description":"Return the complete S33D Agent Control method catalog and parameter examples. Call this before the first modeling command.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}
        }),
        json!({
            "name":"s33d_state",
            "description":"Inspect the running S33D document, stable body IDs, feature types, sketch regions, visibility, selection, and world-space bounds.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}
        }),
        json!({
            "name":"s33d_execute",
            "description":"For a modeling request get s33d_workflow, catalog and document.summary. Execute one S33D modeling method. Use s33d_describe for the live method catalog and parameter examples, then s33d_state to verify. Sketch Trim requires preview followed by apply with its token; solid threads use thread.presets, thread.targets and atomic thread.apply (sampled mesh output). view.recenter centers visible models in perspective.",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "method":{"type":"string","description":"A method returned by s33d_describe, such as primitive.create, sketch.create, feature.extrude, body.transform, or boolean.apply."},
                    "params":{"type":"object","description":"The exact parameter object for the selected method."},
                    "idempotency_key":{"type":"string","minLength":1,"maxLength":128,"description":"REQUIRED for writes: choose a unique key BEFORE submitting. Reuse EXACTLY this key and method/params on retries within 10 minutes of completion on the same running server. Never use a new key after an ambiguous timeout. Read-only methods and request.cancel do not require this field."}
                },
                "required":["method"],
                "additionalProperties":false
            }
        }),
    ]
}

fn call_mcp_tool(address_override: Option<&str>, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (-32602, "tools/call requires a tool name".to_string()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err((-32602, "Tool arguments must be an object".into()));
    }
    if name == "s33d_workflow" {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            mode: Option<String>,
        }
        let input: Input =
            serde_json::from_value(arguments).map_err(|e| (-32602, e.to_string()))?;
        let guide = workflow_guide::guide(input.mode.as_deref().unwrap_or("auto"))
            .map_err(|e| (-32602, e))?;
        return Ok(
            json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&guide).unwrap()}],"isError":false}),
        );
    }
    let (method, command_params) = match name {
        "s33d_render" => ("view.render", arguments.clone()),
        "s33d_reference" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                asset_id: Option<String>,
                max_edge: Option<u32>,
            }
            let input: Input =
                serde_json::from_value(arguments.clone()).map_err(|e| (-32602, e.to_string()))?;
            if let Some(id) = input.asset_id {
                (
                    "reference.get",
                    json!({"asset_id":id,"max_edge":input.max_edge.unwrap_or(1600)}),
                )
            } else {
                if input.max_edge.is_some() {
                    return Err((-32602, "max_edge requires asset_id".into()));
                }
                ("reference.list", json!({}))
            }
        }
        "s33d_describe" => ("system.describe", json!({})),
        "s33d_state" => ("document.get", json!({})),
        "s33d_execute" => {
            let method = arguments
                .get("method")
                .and_then(Value::as_str)
                .ok_or_else(|| (-32602, "s33d_execute requires `method`".to_string()))?;
            let command_params = arguments
                .get("params")
                .cloned()
                .unwrap_or_else(|| json!({}));
            (method, command_params)
        }
        _ => return Err((-32602, format!("Unknown tool: {name}"))),
    };
    let key = match arguments.get("idempotency_key") {
        Some(Value::String(key)) if !key.is_empty() && key.len() <= 128 => Some(key.as_str()),
        Some(_) => {
            return Err((
                -32602,
                "idempotency_key must be a string of 1..128 bytes".into(),
            ))
        }
        None => None,
    };
    if !agent_protocol::is_read_only_method(method) && method != "request.cancel" && key.is_none() {
        return Err((-32602, "Writes require idempotency_key: choose it before submitting, and reuse the same key and payload on retry".into()));
    }
    match call_s33d(address_override, method, command_params, key) {
        Ok(mut response) => {
            let image = response
                .result
                .as_mut()
                .and_then(Value::as_object_mut)
                .and_then(|r| r.remove("image"));
            let is_error = !response.ok;
            let text = serde_json::to_string_pretty(&response)
                .unwrap_or_else(|_| "S33D returned an unreadable response".to_string());
            let mut content = vec![json!({"type":"text","text":text})];
            if !is_error {
                if let Some(image) = image {
                    if image["mime_type"] != "image/png" || !image["data"].is_string() {
                        return Err((-32603, "Invalid S33D image result".into()));
                    }
                    content
                        .push(json!({"type":"image","mimeType":"image/png","data":image["data"]}));
                }
            }
            Ok(json!({"content":content,"isError":is_error}))
        }
        Err(error) => Ok(json!({
            "content":[{"type":"text","text":error}],
            "isError":true
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_negotiation_does_not_echo_an_unsupported_version() {
        for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
            let response = handle_mcp_message(
                None,
                &json!({"id":1,"method":"initialize","params":{"protocolVersion":version}}),
            )
            .unwrap();
            assert_eq!(response["result"]["protocolVersion"], version);
            assert!(response["result"]["capabilities"]["resources"].is_object());
            assert!(response["result"]["capabilities"].get("prompts").is_none());
        }
        for version in ["2099-01-01", "bad-version", "2020-01-01"] {
            let response = handle_mcp_message(
                None,
                &json!({"id":1,"method":"initialize","params":{"protocolVersion":version}}),
            )
            .unwrap();
            assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
        }
        assert_eq!(
            handle_mcp_message(None, &json!({"id":1,"method":"initialize","params":{}})).unwrap()
                ["error"]["code"],
            -32602
        );
        assert_eq!(
            handle_mcp_message(None, &json!({"id":2,"method":"no-such-method"})).unwrap()["error"]
                ["code"],
            -32601
        );
    }

    #[test]
    fn shipped_workflow_and_resources_work_without_any_desktop_or_personal_skill() {
        for mode in ["auto", "photo", "model", "edit"] {
            let response = call_mcp_tool(
                Some("invalid-address"),
                &json!({"name":"s33d_workflow","arguments":{"mode":mode}}),
            )
            .unwrap();
            assert_eq!(response["isError"], false);
            let guide: Value =
                serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(guide["application_version"], env!("CARGO_PKG_VERSION"));
            assert_eq!(guide["content_sha256"].as_str().unwrap().len(), 64);
            assert!(guide["entry"]
                .as_str()
                .unwrap()
                .contains("document.summary"));
        }
        assert!(call_mcp_tool(
            None,
            &json!({"name":"s33d_workflow","arguments":{"mode":"invented"}})
        )
        .is_err());
        let resources =
            handle_mcp_message(None, &json!({"id":3,"method":"resources/list"})).unwrap();
        let uri = &resources["result"]["resources"][0]["uri"];
        let resource = handle_mcp_message(
            None,
            &json!({"id":4,"method":"resources/read","params":{"uri":uri}}),
        )
        .unwrap();
        assert!(
            resource["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .len()
                > 1000
        );
        assert!(handle_mcp_message(
            None,
            &json!({"id":5,"method":"resources/read","params":{"uri":"file:///arbitrary"}})
        )
        .unwrap()
        .get("error")
        .is_some());
    }

    #[test]
    fn image_results_are_mcp_image_blocks_and_metadata_contains_no_base64() {
        let server = agent_protocol::AgentServer::start("127.0.0.1:0").unwrap();
        let address = server.address().unwrap().to_string();
        let worker = std::thread::spawn(move || {
            call_mcp_tool(
                Some(&address),
                &json!({"name":"s33d_render","arguments":{"width":256,"height":128}}),
            )
            .unwrap()
        });
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        assert_eq!(pending.request.method, "view.render");
        assert!(pending.try_begin());
        pending.respond(AgentResponse::success(json!(1),json!({"document_id":"fixture","revision":7,"image":{"mime_type":"image/png","data":"controlled-image-data"}})));
        let result = worker.join().unwrap();
        assert_eq!(result["isError"], false);
        assert_eq!(
            result["content"][1],
            json!({"type":"image","mimeType":"image/png","data":"controlled-image-data"})
        );
        assert!(!result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("controlled-image-data"));
    }

    #[test]
    fn initialize_returns_tools_capability_and_instructions() {
        let response = handle_mcp_message(
            Some(DEFAULT_AGENT_ADDRESS),
            &json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"initialize",
                "params":{"protocolVersion":"2025-03-26"}
            }),
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert!(response["result"]["capabilities"]["tools"].is_object());
        assert!(response["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("millimeters"));
        let description = agent_protocol::protocol_description();
        assert_eq!(description["desktop_drawing"]["agent_commands"], false);
        assert_eq!(
            description["desktop_drawing"]["export_formats"],
            json!(["svg", "dxf", "pdf", "eps"])
        );
        assert_eq!(
            description["desktop_thread_editor"]["printing_settings"]
                ["settings_alone_change_parameters"],
            false
        );
        assert_eq!(
            description["desktop_thread_editor"]["printing_profiles"]["fdm"]["included_angle_deg"],
            90
        );
        assert_eq!(
            description["desktop_tool_exit"]["semantics"],
            "apply_and_done_then_collapse_menus"
        );
        assert_eq!(
            description["desktop_tool_exit"]["confirmation"]["default_enabled"],
            true
        );
        assert_eq!(
            description["request_safety"]["desktop_exit_confirmation"]["allowed_methods"],
            json!(["system.ping", "system.describe", "workflow.get"])
        );
        assert!(!description["methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"].as_str().unwrap().starts_with("drawing.")));
    }

    #[test]
    fn tools_list_exposes_describe_state_and_execute() {
        let names: Vec<String> = mcp_tools()
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect();
        assert_eq!(
            names,
            [
                "s33d_workflow",
                "s33d_reference",
                "s33d_render",
                "s33d_describe",
                "s33d_state",
                "s33d_execute"
            ]
        );
    }

    #[test]
    fn bridge_reads_the_port_saved_by_s33d_settings() {
        let state = r#"{"settings":{"agent_control_port":54321}}"#;
        assert_eq!(
            address_from_ui_state(state).as_deref(),
            Some("127.0.0.1:54321")
        );
        assert!(address_from_ui_state(r#"{"settings":{"agent_control_port":0}}"#).is_none());
        assert_eq!(
            configured_address(Some("127.0.0.1:60000")),
            "127.0.0.1:60000"
        );
    }

    #[test]
    fn mcp_write_requires_identity_before_any_network_submission() {
        let missing = call_mcp_tool(
            Some("invalid-address"),
            &json!({
                "name":"s33d_execute", "arguments":{"method":"primitive.create","params":{"kind":"box"}}
            }),
        );
        assert!(matches!(missing, Err((-32602, message)) if message.contains("idempotency_key")));
        for key in [json!(""), json!(7), json!("x".repeat(129))] {
            assert!(matches!(
                call_mcp_tool(
                    Some("invalid-address"),
                    &json!({
                        "name":"s33d_execute", "arguments":{"method":"primitive.create","idempotency_key":key}
                    })
                ),
                Err((-32602, _))
            ));
        }
        let valid = call_mcp_tool(Some("invalid-address"), &json!({
            "name":"s33d_execute", "arguments":{"method":"primitive.create","idempotency_key":"unique-box"}
        })).unwrap();
        assert_eq!(valid["isError"], true);
        assert!(valid["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unique-box"));
        assert!(call_mcp_tool(Some("invalid-address"), &json!({"name":"s33d_state"})).is_ok());
    }
}
