//! Local automation protocol shared by the S33D desktop host and the
//! `s33d-agent` MCP/CLI bridge.
//!
//! The desktop process owns all document and kernel state.  This module only
//! moves newline-delimited JSON requests between a loopback TCP socket and the
//! eframe UI thread; modeling commands are executed by `S33DApp`.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "agent_lifetime.rs"]
mod agent_lifetime;
#[allow(unused_imports)] // The desktop hosts the watch and the bridge connects to it.
pub use agent_lifetime::connect_desktop_watch;
pub use agent_lifetime::DesktopLifetime;

pub const PROTOCOL_NAME: &str = "s33d-agent";
pub const PROTOCOL_VERSION: u32 = 2;
pub const DEFAULT_AGENT_PORT: u16 = 53_433;
pub const DEFAULT_AGENT_ADDRESS: &str = "127.0.0.1:53433";
pub const UI_STATE_FILE_NAME: &str = "ui_state.json";
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONNECTIONS: usize = 8;
const MAX_PENDING_REQUESTS: usize = 16;
const MAX_REPLAY_RECORDS: usize = 256;
const MAX_REPLAY_INPUT_BYTES: usize = 8 * MAX_REQUEST_BYTES;
const MAX_REPLAY_RESPONSE_BYTES: usize = 64 * 1024;
const REPLAY_RETENTION: Duration = Duration::from_secs(10 * 60);

/// Stable per-user UI/settings location shared by the desktop application and
/// the Agent bridge. A fixed location prevents different launch working
/// directories from silently creating independent settings files.
pub fn user_ui_state_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("S33D").join(UI_STATE_FILE_NAME);
    }
    #[cfg(target_os = "macos")]
    if let Some(root) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root)
            .join("Library")
            .join("Application Support")
            .join("S33D")
            .join(UI_STATE_FILE_NAME);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(root) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(root).join("s33d").join(UI_STATE_FILE_NAME);
        }
        if let Some(root) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(root)
                .join(".config")
                .join("s33d")
                .join(UI_STATE_FILE_NAME);
        }
    }
    PathBuf::from(UI_STATE_FILE_NAME)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRequest {
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    /// Explicit identity for one logical write. JSON-RPC `id` is only correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

impl AgentRequest {
    #[allow(dead_code)]
    pub fn new(id: Value, method: impl Into<String>, params: Value) -> Self {
        Self {
            id,
            method: method.into(),
            params,
            idempotency_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentResponse {
    pub id: Value,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

impl AgentResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
            idempotency_key: None,
        }
    }

    pub fn failure(id: Value, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(AgentError {
                code: code.into(),
                message: message.into(),
            }),
            idempotency_key: None,
        }
    }
}

pub struct PendingAgentRequest {
    pub request: AgentRequest,
    response_tx: Sender<AgentResponse>,
    control: Arc<RequestControl>,
}

impl PendingAgentRequest {
    /// The UI and timeout thread arbitrate under the same lock. A timeout that
    /// wins while queued makes execution impossible, including deadline races.
    pub fn try_begin(&self) -> bool {
        self.control.try_begin()
    }

    pub fn respond(self, response: AgentResponse) {
        self.control.complete(&response);
        let _ = self.response_tx.send(response);
    }
}

enum RequestState {
    Queued,
    Running,
    Finished(AgentResponse, Instant),
}

struct RequestControl {
    deadline: Instant,
    stop: Arc<AtomicBool>,
    state: Mutex<RequestState>,
}

impl RequestControl {
    fn new(timeout: Duration, stop: Arc<AtomicBool>) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            stop,
            state: Mutex::new(RequestState::Queued),
        }
    }

    fn try_begin(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if !matches!(*state, RequestState::Queued) {
            return false;
        }
        if Instant::now() >= self.deadline || self.stop.load(Ordering::Acquire) {
            *state = RequestState::Finished(Self::expired(Value::Null), Instant::now());
            return false;
        }
        *state = RequestState::Running;
        true
    }

    fn expired(id: Value) -> AgentResponse {
        AgentResponse::failure(
            id,
            "expired",
            "Request expired while queued and was not executed",
        )
    }

    fn poll(&self, id: Value, cancel_queued: bool) -> AgentResponse {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, RequestState::Queued)
            && (cancel_queued
                || Instant::now() >= self.deadline
                || self.stop.load(Ordering::Acquire))
        {
            *state = RequestState::Finished(Self::expired(id.clone()), Instant::now());
        }
        match &*state {
            RequestState::Finished(response, _) => {
                let mut response = response.clone();
                response.id = id;
                response
            }
            RequestState::Queued => AgentResponse::failure(id, "queued", "Request is queued; do not submit it with a different key"),
            RequestState::Running => AgentResponse::failure(id, "in_progress", "Request has started and cannot be cancelled here; retry the SAME idempotency key to retrieve its result, never a new key"),
        }
    }

    fn complete(&self, response: &AgentResponse) {
        // Keep replay memory bounded even when history.undo returns a whole model.
        let cached = if serde_json::to_writer(BudgetWriter(MAX_REPLAY_RESPONSE_BYTES), response)
            .is_err()
        {
            if response.ok {
                AgentResponse::success(
                    response.id.clone(),
                    json!({"request_completed":true,"result_omitted":true,"note":"Large replay result omitted; query document.get. Do not repeat with a new key."}),
                )
            } else {
                AgentResponse::failure(response.id.clone(), "completed_error", "Request completed with an oversized error; inspect document.get before another operation")
            }
        } else {
            response.clone()
        };
        *self.state.lock().unwrap() = RequestState::Finished(cached, Instant::now());
    }
}

struct BudgetWriter(usize);
impl Write for BudgetWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() > self.0 {
            return Err(io::ErrorKind::OutOfMemory.into());
        }
        self.0 -= data.len();
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ReplayRecord {
    payload: String,
    control: Arc<RequestControl>,
}

#[derive(Default)]
struct ReplayLedger {
    records: HashMap<String, ReplayRecord>,
    input_bytes: usize,
}

impl ReplayLedger {
    fn prune(&mut self) {
        self.records.retain(|_, record| {
            !matches!(&*record.control.state.lock().unwrap(), RequestState::Finished(_, at) if at.elapsed() >= REPLAY_RETENTION)
        });
        self.input_bytes = self
            .records
            .iter()
            .map(|(key, r)| key.len() + r.payload.len())
            .sum();
    }
}

pub fn is_read_only_method(method: &str) -> bool {
    matches!(
        method,
        "system.ping"
            | "system.describe"
            | "workflow.get"
            | "document.get"
            | "document.summary"
            | "reference.list"
            | "reference.get"
            | "view.get"
            | "view.render"
            | "body.get"
            | "thread.presets"
            | "thread.targets"
            | "feature.offset.status"
            | "curve.equation.preview"
            | "surface.equation.preview"
            | "sketch.trim.preview"
    )
}

/// Background loopback listener.  It never touches the model directly: each
/// accepted command is forwarded to the eframe thread and waits for its reply.
pub struct AgentServer {
    address: Option<SocketAddr>,
    receiver: Receiver<PendingAgentRequest>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    startup_error: Option<String>,
}

impl AgentServer {
    pub fn start_loopback(port: u16, lifetime: Arc<DesktopLifetime>) -> Self {
        if std::env::var("S33D_AGENT_DISABLE").is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        }) {
            return Self::disabled("disabled by S33D_AGENT_DISABLE");
        }
        let address = format!("127.0.0.1:{port}");
        Self::start_watching(&address, lifetime).unwrap_or_else(Self::disabled)
    }

    pub fn disabled(reason: impl Into<String>) -> Self {
        let (_tx, receiver) = mpsc::sync_channel(MAX_PENDING_REQUESTS);
        Self {
            address: None,
            receiver,
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
            startup_error: Some(reason.into()),
        }
    }

    #[cfg(test)]
    pub fn start(address: &str) -> Result<Self, String> {
        Self::start_watching(address, Arc::new(DesktopLifetime::default()))
    }

    pub fn start_watching(address: &str, lifetime: Arc<DesktopLifetime>) -> Result<Self, String> {
        let listener = TcpListener::bind(address)
            .map_err(|error| format!("cannot bind Agent Control at {address}: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure Agent Control: {error}"))?;
        let local_address = listener
            .local_addr()
            .map_err(|error| format!("cannot read Agent Control address: {error}"))?;
        let (request_tx, receiver) = mpsc::sync_channel(MAX_PENDING_REQUESTS);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("s33d-agent-listener".to_string())
            .spawn(move || listener_loop(listener, request_tx, thread_stop, lifetime))
            .map_err(|error| format!("cannot start Agent Control: {error}"))?;
        Ok(Self {
            address: Some(local_address),
            receiver,
            stop,
            thread: Some(thread),
            startup_error: None,
        })
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    pub fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub fn try_recv(&self) -> Option<PendingAgentRequest> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for AgentServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn listener_loop(
    listener: TcpListener,
    request_tx: SyncSender<PendingAgentRequest>,
    stop: Arc<AtomicBool>,
    lifetime: Arc<DesktopLifetime>,
) {
    let ledger = Arc::new(Mutex::new(ReplayLedger::default()));
    let mut clients: Vec<JoinHandle<()>> = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let mut i = 0;
        while i < clients.len() {
            if clients[i].is_finished() {
                let _ = clients.swap_remove(i).join();
            } else {
                i += 1;
            }
        }
        match listener.accept() {
            Ok((stream, _)) => {
                // Reject before allocating a worker or a request buffer. Never
                // block the accept loop writing a rejection to an untrusted peer.
                if clients.len() >= MAX_CONNECTIONS {
                    drop(stream);
                    continue;
                }
                let request_tx = request_tx.clone();
                let stop = Arc::clone(&stop);
                let ledger = Arc::clone(&ledger);
                let lifetime = Arc::clone(&lifetime);
                if let Ok(client) = thread::Builder::new()
                    .name("s33d-agent-client".to_string())
                    .spawn(move || handle_client(stream, request_tx, stop, ledger, lifetime))
                {
                    clients.push(client);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    // Readers and waiters poll stop; no detached workers survive server restart.
    for client in clients {
        let _ = client.join();
    }
}

/// Read at most limit bytes before deciding whether a frame is oversized. A
/// streaming peer cannot grow this buffer by withholding its newline.
fn read_bounded_line(reader: &mut impl BufRead, limit: usize) -> io::Result<Vec<u8>> {
    let mut line = Vec::with_capacity(limit.min(8192));
    loop {
        let data = reader.fill_buf()?;
        if data.is_empty() {
            return Ok(line);
        }
        let count = data
            .iter()
            .position(|v| *v == b'\n')
            .map_or(data.len(), |n| n + 1);
        if count > limit - line.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request exceeds byte limit",
            ));
        }
        let done = data[count - 1] == b'\n';
        if line.capacity() < line.len() + count {
            line.reserve_exact(count);
        }
        line.extend_from_slice(&data[..count]);
        reader.consume(count);
        if done {
            return Ok(line);
        }
    }
}

struct DeadlineReader {
    stream: TcpStream,
    deadline: Instant,
    stop: Arc<AtomicBool>,
}

impl Read for DeadlineReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.stop.load(Ordering::Acquire) {
                // BufReader retries Interrupted; shutdown must terminate its read.
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.stream
                .set_read_timeout(Some(remaining.min(Duration::from_millis(100))))?;
            match self.stream.read(buffer) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                result => return result,
            }
        }
    }
}

/// Large image/geometry replies must survive partial socket writes, while a
/// stopped desktop or a peer that stops reading must still release the worker.
struct DeadlineWriter {
    stream: TcpStream,
    deadline: Instant,
    stop: Arc<AtomicBool>,
}

impl Write for DeadlineWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            if self.stop.load(Ordering::Acquire) {
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            if Instant::now() >= self.deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            // Nonblocking sends report actual byte progress. A timed blocking
            // send must not be retried as if zero bytes had moved.
            match self.stream.write(&buffer[..buffer.len().min(64 * 1024)]) {
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    thread::sleep(Duration::from_millis(1));
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn enqueue_request(
    request: AgentRequest,
    request_tx: &SyncSender<PendingAgentRequest>,
    stop: &Arc<AtomicBool>,
    ledger: &Mutex<ReplayLedger>,
) -> Result<(Arc<RequestControl>, Receiver<AgentResponse>), AgentResponse> {
    let id = request.id.clone();
    if !(id.is_null() || id.is_number() || id.as_str().is_some_and(|s| s.len() <= 128)) {
        return Err(AgentResponse::failure(
            Value::Null,
            "invalid_id",
            "id must be null, a number or a string of at most 128 bytes",
        ));
    }
    let control = Arc::new(RequestControl::new(RESPONSE_TIMEOUT, Arc::clone(stop)));
    let mut ledger = ledger.lock().unwrap();
    ledger.prune();
    // Only writes are cached. State/mesh inspection replies may be large and
    // must remain fresh. Every write requires an explicit cross-connection key.
    let replay = if is_read_only_method(&request.method) {
        None
    } else {
        let Some(key) = request
            .idempotency_key
            .as_ref()
            .filter(|k| !k.is_empty() && k.len() <= 128)
        else {
            return Err(AgentResponse::failure(id, "idempotency_key_required", "Writes require idempotency_key (1..128 bytes); keep it unchanged when retrying this logical operation"));
        };
        let payload = serde_json::to_string(&(&request.method, &request.params)).unwrap();
        if let Some(record) = ledger.records.get(key) {
            return Err(if record.payload == payload {
                record.control.poll(id, false)
            } else {
                AgentResponse::failure(
                    id,
                    "idempotency_conflict",
                    "This key already identifies different method/params",
                )
            });
        }
        if ledger.records.len() >= MAX_REPLAY_RECORDS
            || ledger.input_bytes + key.len() + payload.len() > MAX_REPLAY_INPUT_BYTES
        {
            return Err(AgentResponse::failure(id, "server_busy", "Idempotency window is full; no command was queued. Wait for completed records to expire."));
        }
        Some((key.clone(), payload))
    };
    let (response_tx, response_rx) = mpsc::channel();
    match request_tx.try_send(PendingAgentRequest {
        request,
        response_tx,
        control: Arc::clone(&control),
    }) {
        Ok(()) => {
            if let Some((key, payload)) = replay {
                ledger.input_bytes += key.len() + payload.len();
                ledger.records.insert(
                    key,
                    ReplayRecord {
                        payload,
                        control: Arc::clone(&control),
                    },
                );
            }
            Ok((control, response_rx))
        }
        Err(mpsc::TrySendError::Full(_)) => Err(AgentResponse::failure(
            id,
            "server_busy",
            "Request queue is full; no command was queued",
        )),
        Err(mpsc::TrySendError::Disconnected(_)) => Err(AgentResponse::failure(
            id,
            "host_unavailable",
            "S33D closed before the command could run",
        )),
    }
}

fn handle_client(
    stream: TcpStream,
    request_tx: SyncSender<PendingAgentRequest>,
    stop: Arc<AtomicBool>,
    ledger: Arc<Mutex<ReplayLedger>>,
    lifetime: Arc<DesktopLifetime>,
) {
    // Use explicit nonblocking progress on every platform, preserving partial
    // writes across WouldBlock instead of relying on ambiguous SO_SNDTIMEO.
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(DeadlineReader {
        stream,
        deadline: Instant::now() + RESPONSE_TIMEOUT,
        stop: Arc::clone(&stop),
    });
    let mut writer = DeadlineWriter {
        stream: writer,
        deadline: Instant::now() + RESPONSE_TIMEOUT,
        stop: Arc::clone(&stop),
    };
    loop {
        reader.get_mut().deadline = Instant::now() + RESPONSE_TIMEOUT;
        let line = match read_bounded_line(&mut reader, MAX_REQUEST_BYTES) {
            Ok(line) => line,
            Err(error) => {
                if error.kind() == io::ErrorKind::InvalidData {
                    let _ = write_json_line(
                        &mut writer,
                        &AgentResponse::failure(
                            Value::Null,
                            "request_too_large",
                            "request exceeds 1 MiB; connection closed",
                        ),
                    );
                }
                return;
            }
        };
        if line.is_empty() || stop.load(Ordering::Acquire) {
            return;
        }
        if line == agent_lifetime::WATCH_REQUEST {
            // Transfer socket ownership to the desktop lifetime, outside the
            // request worker and independent of future listener replacements.
            drop(reader);
            lifetime.watch(writer.stream);
            return;
        }
        let mut response_key = None;
        let mut response = {
            match serde_json::from_slice::<AgentRequest>(&line) {
                Ok(request) => {
                    response_key = request.idempotency_key.clone();
                    let request_id = request.id.clone();
                    if request.method == "request.cancel" {
                        let key = request
                            .params
                            .get("idempotency_key")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let ledger = ledger.lock().unwrap();
                        ledger.records.get(key).map_or_else(
                            || {
                                AgentResponse::failure(
                                    request_id.clone(),
                                    "not_found",
                                    "Key is unknown or outside the replay retention window",
                                )
                            },
                            |r| r.control.poll(request_id.clone(), true),
                        )
                    } else {
                        match enqueue_request(request, &request_tx, &stop, &ledger) {
                            Err(response) => response,
                            Ok((control, response_rx)) => loop {
                                match response_rx.recv_timeout(Duration::from_millis(25)) {
                                    Ok(response) => break response,
                                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                                        break control.poll(request_id.clone(), true)
                                    }
                                    Err(mpsc::RecvTimeoutError::Timeout) => {
                                        if Instant::now() >= control.deadline
                                            || stop.load(Ordering::Acquire)
                                        {
                                            break control.poll(request_id.clone(), true);
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
                Err(error) => AgentResponse::failure(
                    Value::Null,
                    "invalid_json",
                    format!("invalid Agent Control request: {error}"),
                ),
            }
        };
        response.idempotency_key = response_key;
        writer.deadline = Instant::now() + RESPONSE_TIMEOUT;
        if write_json_line(&mut writer, &response).is_err() {
            return;
        }
    }
}

fn write_json_line(writer: &mut impl Write, value: &impl Serialize) -> std::io::Result<()> {
    // JSON punctuation and escaped strings otherwise cause many tiny writes.
    let mut buffered = BufWriter::with_capacity(64 * 1024, writer);
    serde_json::to_writer(&mut buffered, value).map_err(std::io::Error::other)?;
    buffered.write_all(b"\n")?;
    buffered.flush()
}

#[allow(dead_code)]
pub fn send_request(address: &str, request: &AgentRequest) -> Result<AgentResponse, String> {
    let address: SocketAddr = address
        .parse()
        .map_err(|error| format!("invalid S33D Agent Control address: {error}"))?;
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|error| {
            format!(
                "cannot connect to S33D at {address}: {error}; start the S33D desktop app first"
            )
        })?;
    stream
        .set_read_timeout(Some(RESPONSE_TIMEOUT + Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(|error| error.to_string())?;
    write_json_line(&mut stream, request).map_err(|error| error.to_string())?;
    let line = read_bounded_line(&mut BufReader::new(stream), 64 * MAX_REQUEST_BYTES)
        .map_err(|error| error.to_string())?;
    if line.is_empty() {
        return Err("S33D closed the Agent Control connection without a response".to_string());
    }
    serde_json::from_slice(&line).map_err(|error| format!("invalid response from S33D: {error}"))
}

pub fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, String> {
    serde_json::from_value(params).map_err(|error| format!("invalid parameters: {error}"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionSetParams {
    pub body_ids: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyIdParams {
    pub body_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyIdsParams {
    pub body_ids: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyRenameParams {
    pub body_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyVisibilityParams {
    pub body_id: u64,
    pub visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrimitiveCreateParams {
    pub kind: String,
    pub name: Option<String>,
    pub position: [f64; 3],
    pub width: Option<f64>,
    pub depth: Option<f64>,
    pub height: Option<f64>,
    pub radius: Option<f64>,
    pub top_radius: Option<f64>,
    pub major_radius: Option<f64>,
    pub minor_radius: Option<f64>,
}

impl Default for PrimitiveCreateParams {
    fn default() -> Self {
        Self {
            kind: String::new(),
            name: None,
            position: [0.0; 3],
            width: None,
            depth: None,
            height: None,
            radius: None,
            top_radius: None,
            major_radius: None,
            minor_radius: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformParams {
    pub body_ids: Vec<u64>,
    pub translation: [f64; 3],
    pub rotation_deg: [f64; 3],
    pub scale: [f64; 3],
    pub pivot: Option<[f64; 3]>,
}

impl Default for TransformParams {
    fn default() -> Self {
        Self {
            body_ids: Vec::new(),
            translation: [0.0; 3],
            rotation_deg: [0.0; 3],
            scale: [1.0; 3],
            pivot: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BooleanApplyParams {
    pub operation: String,
    pub body_ids: Vec<u64>,
    pub keep_originals: bool,
    pub name: Option<String>,
}

impl Default for BooleanApplyParams {
    fn default() -> Self {
        Self {
            operation: String::new(),
            body_ids: Vec::new(),
            keep_originals: false,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SketchCreateParams {
    /// `xy`, `yz`, `zx`/`xz`, or `custom`.
    pub plane: String,
    pub origin: [f64; 3],
    /// Required for `custom`; ignored for datum planes.
    pub normal: Option<[f64; 3]>,
    pub entities: Vec<AgentSketchEntity>,
    pub name: Option<String>,
}

impl Default for SketchCreateParams {
    fn default() -> Self {
        Self {
            plane: "xy".to_string(),
            origin: [0.0; 3],
            normal: None,
            entities: Vec::new(),
            name: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentSketchEntity {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_deg: f64,
        end_deg: f64,
        #[serde(default = "default_true")]
        counterclockwise: bool,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Rectangle {
        corner1: [f64; 2],
        corner2: [f64; 2],
    },
    RotatedRectangle {
        center: [f64; 2],
        width: f64,
        height: f64,
        /// Positive is clockwise, matching the S33D UI.
        rotation_deg: f64,
    },
    Polygon {
        center: [f64; 2],
        radius: f64,
        sides: u32,
        #[serde(default)]
        rotation_deg: f64,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExtrudeFeatureParams {
    pub sketch_id: u64,
    pub region_indices: Vec<usize>,
    pub depth: f64,
    pub symmetric: bool,
    pub name: Option<String>,
}

impl Default for ExtrudeFeatureParams {
    fn default() -> Self {
        Self {
            sketch_id: 0,
            region_indices: vec![0],
            depth: 20.0,
            symmetric: false,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RevolveFeatureParams {
    pub sketch_id: u64,
    pub region_index: usize,
    pub axis_point: [f64; 2],
    pub axis_direction: [f64; 2],
    pub angle_deg: f64,
    pub name: Option<String>,
}

impl Default for RevolveFeatureParams {
    fn default() -> Self {
        Self {
            sketch_id: 0,
            region_index: 0,
            axis_point: [0.0, 0.0],
            axis_direction: [0.0, 1.0],
            angle_deg: 360.0,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathParams {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    pub body_id: u64,
    pub path: String,
    #[serde(default)]
    pub format: Option<String>,
}

/// Coordinates are local sketch-plane UV, in millimeters (not screen pixels).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchTrimParams {
    pub sketch_id: u64,
    pub point: [f64; 2],
    pub tolerance: f64,
    pub preview_token: Option<String>,
}

pub fn protocol_description() -> Value {
    json!({
        "protocol": PROTOCOL_NAME,
        "version": PROTOCOL_VERSION,
        "transport": {
            "desktop": "newline-delimited JSON over loopback TCP",
            "default_address": DEFAULT_AGENT_ADDRESS,
            "bridge": "MCP stdio or CLI through s33d-agent"
        },
        "units": "All coordinates, lengths, radii, and sizes are millimeters; angles are degrees.",
        "desktop_view_guides": {
            "agent_commands":false,
            "notes":"The top-right workspace switch and View menu show/hide auxiliary grid and RGB axes together. Settings > Re-enable all hidden tips also restores guides. Display only; snapping, geometry and orientation gizmo are unchanged. File > Open Recent persists up to ten successful project open/save paths in local preferences, uses normal unsaved-changes confirmation and background loading, and preserves the current document on failure. Agents use published project.open with a real path, not recent.* or guide-toggle methods. Native model images omit guides."
        },
        "desktop_drawing": {
            "name":"3-view Drawing", "agent_commands":false,
            "project_persistence":true,
            "export_formats":["svg","dxf","pdf","eps"], "dwg_available":false,
            "notes":"Desktop UI only: generate from shared Objects selection, edit and drag dimensions, text, feature labels and title-block text, then Export. Model projection lines remain fixed. Verified inclined cylinder/bore rims support true-diameter measurements. project.save persists drawing snapshots, dimensions, text overrides and offsets, title blocks and component notes in .s33d; project.open restores them. Wait for generation before saving. Legacy projects without drawing data restore an empty drawing. Transient interactions and drawing Undo/Redo history are not persisted. SVG/DXF/EPS use one file per sheet; EPS embeds Type 42 text and PDF includes all sheets with an embedded Chinese font. DXF coordinates are paper-space mm at the stated scale, not 1:1 model coordinates; dimensions are non-associative. Projection is tessellation-based, not exact HLR. body.export exports 3D geometry only; do not invent drawing.* commands."
        },
        "desktop_thread_editor": {
            "blank_correction":{"agent_commands":false,"label":"Correct according to nominal diameter","action":"Resize actual bore or blank; retain nominal diameter and all thread/print parameters","bore_diameter":"diameter - 2*depth","external_blank_diameter":"diameter + length*taper","result":"Verified resized wall stays selected for Generate; correction and generation are separate Undo steps","limits":"Parametric cylinder, exact solid or certified Sketch Extrude/Pocket wall required. Faceted sketch solids are validated and promoted to exact geometry (100000-triangle input limit); shared source sketches remain unchanged and consumed Pocket sources are retained. Raw imported meshes are read-only; invalid parameters, topology, interference or dependent features can reject correction without document changes. thread.apply does not resize geometry automatically."},
            "selection":"Recognized complete cylinder or bore wall; click the entry to expand geometry, compatible presets and editable parameters.",
            "printing_defaults":{"process":"fdm","nozzle_mm":0.4,"layer_mm":0.2,"resin_layer_mm":0.05},
            "printing_settings":{"entry":"... beside suggest for 3D printing","apply_trigger":"suggest for 3D printing after closing settings","settings_alone_change_parameters":false},
            "printing_profiles":{"fdm":{"included_angle_deg":90,"depth_over_pitch":0.25,"crest_and_root_flat_over_pitch":0.25,"minimum_pitch":"max(4*nozzle_mm,6*layer_mm)"},"resin":{"included_angle_deg":60,"depth_over_pitch":0.433013,"crest_and_root_flat_over_pitch":0.25,"minimum_pitch":"max(8*layer_mm,0.5 mm)"}},
            "notes":"Desktop print suggestions allow nozzle/layer changes or resin and use complementary print-specific screw/nut profiles. Changing settings alone does not alter the draft; close settings and click suggest. Editing parameters switches the preset to Custom. Personal presets save all parameters, wall role and print settings in the user configuration directory. Print suggestions and personal-preset management are desktop-only; thread.presets returns built-ins. Final pitch also considers nominal diameter/7 and rounds up to supported pitch candidates. Pair the same nominal diameter, pitch, print process, hand and radial_fit; print a short calibration pair. Heuristic suggestions do not certify production tolerances or physical fit."
        },
        "desktop_tool_exit": {
            "agent_commands":false,
            "selection":{"single_click":"edge","double_click":"face_or_closed_region","triple_click":"whole_object","draw3d_single_click":"select_and_edit_body"},
            "semantics":"apply_and_done_then_collapse_menus",
            "confirmation":{"default_enabled":true,"choices":["confirm","cancel"],"dont_show_again":"Saved only on confirm","reenable":"Settings > Re-enable all hidden tips; also restores Getting Started tips"},
            "notes":"Nonconflicting workspace double/triple-click exits ask before applying the current tool state. Confirm preserves the state with Apply + Done, collapses menus and continues the original face/object selection; Cancel keeps the tool and draft. Already applied arrays and relative transforms are not repeated. Invalid geometry stays editable; background work finishes before exit. Point-by-point sketch construction and Draw 3D placement retain their clicks. Explicit Exit/Cancel buttons and idle selection do not add this prompt. No new MCP methods or preference-management commands."
        },
        "request_safety": {
            "max_request_bytes":MAX_REQUEST_BYTES,"max_connections":MAX_CONNECTIONS,"max_queued_requests":MAX_PENDING_REQUESTS,
            "queue_deadline_seconds":RESPONSE_TIMEOUT.as_secs(),"write_idempotency_key_required":true,
            "replay_retention_seconds_after_completion":REPLAY_RETENTION.as_secs(),"max_replay_records":MAX_REPLAY_RECORDS,
            "max_replay_input_bytes":MAX_REPLAY_INPUT_BYTES,"max_replay_response_bytes_per_record":MAX_REPLAY_RESPONSE_BYTES,
            "desktop_exit_confirmation":{"blocked_error":"busy","allowed_methods":["system.ping","system.describe","workflow.get"],"resolution":"Confirm or cancel in the desktop, then retry","notes":"Applies to desktop-dispatched requests; transport-level request.cancel retains its existing behavior."},
            "desktop_modeling_progress":{"blocked_error":"busy","allowed_methods":["system.ping","system.describe","workflow.get"],"notes":"Heavy modeling requests run in the background and return their response after validation and document commit. The desktop shows progress. Desktop Cancel discards this calculation without changing the model; transport request.cancel still cancels only queued requests and returns in_progress for running work."},
            "desktop_export_progress":{"blocked_error":"busy","allowed_methods":["system.ping","system.describe","workflow.get"],"notes":"body.export runs in the background. Success is returned only after the complete file is published. Failure, desktop Cancel/Esc, reset or stale work preserves the destination. Model and Undo history are unchanged. Transport request.cancel still only cancels queued requests."},
            "notes":"Queued expiry/cancel never executes; running work returns in_progress and cannot be interrupted. Reuse the SAME key and payload to poll/replay. Keys are scoped to this server lifetime and retention window; outside it inspect document state instead of retrying. Oversized cached results return a completion receipt."
        },
        "workflow": [
            "Read workflow.get (or the MCP s33d_workflow tool), the live catalog and document.summary; retain stable IDs.",
            "Maintain evidence, shared constraints, current gate, largest deviation, decision and next recheck in the Agent checkpoint.",
            "Choose construction from the target shape; verify summary after writes and native images for appearance. Major shape failures block detail.",
            "Reconcile ambiguous state before retrying. Undo only this task operation after checking no user work intervened; change causal level when corrections do not improve."
        ],
        "methods": [
            {"name":"system.ping", "params":{}},
            {"name":"system.describe", "params":{}},
            {"name":"workflow.get", "params":{"mode":"auto | photo | model | edit | geometry (optional, default auto)"}, "notes":"Read-only version-matched guide, source hashes and Agent-maintained execution-state template; compatible with older three-tool hosts. Does not judge likeness or persist an execution plan."},
            {"name":"document.summary","params":{},"notes":"Lightweight document identity/revision, save/busy state and body bounds/material/classification. No B-Rep, triangles, curve samples or definitions. Use body.get only for necessary detail. Mutations accept optional _expected:{document_id,revision}; checked on desktop before execution. Undo/Redo advances revision; opening a document changes identity. Idempotent completed retries replay before revision checks. Camera is separate view_revision."},
            {"name":"reference.import","params":{"paths":["<USER_SELECTED_LOCAL_PHOTO_PATH>"]},"notes":"PNG/JPEG only. Atomic bounded batch; 24 MiB source, 32 megapixels, 16 photos / 32 MiB encoded library. Applies EXIF orientation, downsamples to 2048 pixels, strips metadata. Photos embedded in project; no original file required after saving. Chat attachments need a real host transfer or desktop once-only import. Returns stable SHA-256 asset IDs. No model geometry changed; marks project unsaved. Reference import/remove are not geometry Undo operations."},
            {"name":"reference.list","params":{}},
            {"name":"reference.get","params":{"asset_id":"<REGISTERED_ID>","max_edge":1600},"notes":"Read only a registered asset, max_edge 128..2048. MCP bridge returns native image content; never an arbitrary filesystem read."},
            {"name":"reference.remove","params":{"asset_id":"<REGISTERED_ID>"},"notes":"Explicit removal; save required. Does not remove original user file."},
            {"name":"view.get","params":{},"notes":"World standard directions, camera and independent view revision. Front is world +Z, not necessarily semantic object front."},
            {"name":"view.set","params":{"direction":"front","projection":"orthographic"},"notes":"Partial camera update: eye,target,up,projection (perspective/orthographic),fov_y_deg 5..150,ortho_half,near,far or world direction front/back/top/bottom/left/right/iso. Finite distinct camera and nonparallel up required. Explicitly changes GUI camera only."},
            {"name":"view.render","params":{"width":1024,"height":768,"fit":true,"camera":{"direction":"iso"}},"notes":"Read-only immutable model snapshot and native MCP image. Optional explicit body_ids (including hidden bodies), RGB background and camera as view.set. Default visible committed objects; no grid, selection, section clipping or drafts. Dimensions 128..2048, at most 3 megapixels. fit accounts for output aspect; fit:false preserves supplied camera exactly. Returns document_id/revision/camera/size/body_ids. Does not change GUI view, selection, history or saved state. Native wgpu adapter required; no screenshot runtime. No persistent image cache."},
            {"name":"body.material.set","params":{"body_ids":[1],"base_color_srgb":[0.2,0.4,0.8]},"notes":"Atomic RGB update; validates every stable ID and finite 0..1 channels, preserves alpha/metalness/roughness, Undo/Redo and save/reopen. Same RGB convention as native viewport; not a physical material renderer. Curves/sketches supported in model images."},
            {"name":"document.get", "params":{}, "notes":"Includes read-only logical groups {id,name,members}; members are stable body IDs, not fused geometry. Grouping, center-axis alignment and fastener recipe generation currently use the desktop UI; do not infer unlisted write methods."},
            {"name":"request.cancel","params":{"idempotency_key":"<ORIGINAL_WRITE_KEY>"},"notes":"Transport-level cancellation, even while UI is blocked. Cancels queued work only; returns in_progress for running work, original result for completed work, not_found outside retention."},
            {"name":"thread.presets","params":{},"notes":"Returns built-in metric, UNC/UNF, BSPP and BSPT presets, each with external/internal parameters. The old print preset was removed. Desktop print suggestions and user presets are not returned by this method. Defaults are starting models, not tolerance or gauge certification."},
            {"name":"thread.targets","params":{"body_ids":[1]},"notes":"Returns full analytic cylinders and holes, certified complete faceted-import bands, and complete uncut bands remaining after earlier thread cuts (body_id,face_id,origin,axis,radius,length,internal). Faceted imports require closed end rings and full-wall coverage with radial error at most 1% radius and 0.02 mm. Provenance-backed thread remnants require matching mesh signatures, full-wall coverage and wall orientation; radial chord error may be up to 1% radius. Mesh targets use synthetic face IDs. Each complete uncut axial band is an independent target. Incomplete/interrupted bands are rejected. Requery after every cut or geometry change; retain the complete returned target."},
            {"name":"thread.apply","params":{"targets":"array returned by thread.targets","params":{"diameter":6,"pitch":1,"length":8,"offset":0,"depth":0.61343,"angle_deg":60,"clearance":0,"radial_fit":0,"left_hand":false,"taper":0,"profile":"truncated_v","flat_fraction":0.16666666666666666}},"notes":"Sampled mesh cutting; replaces analytic source on success. Atomic batch 1..32 surfaces, preserves body IDs, Undo/Redo. All lengths mm. radial_fit defaults to zero and shifts the whole profile inward for screws or outward for nuts; two mating parts each using c give approximately 2c combined radial allowance. Use matching nominal diameter, pitch and hand. radial_fit must be 0..pitch/4. clearance remains additional groove depth, not whole-profile allowance. Internal blank radius=diameter/2-depth; external blank radius=diameter/2+length*taper/2. V-profile preset root-flat/pitch defaults: external 1/6, internal 1/8. Use the complete external/internal parameters from thread.presets before customizing; an omitted flat_fraction uses the general external default. BSP uses rounded_55; taper=0 (BSPP) or 0.0625 (BSPT). BSPT diameter at start offset, not gauge plane. Length must exceed pitch and fit target. Requested length/pitch is at most 125 turns, sampled at 96 steps per turn. The cutter extends one turn at each end then clips to offset..offset+length; these two extra turns do not count against the request limit. Grooves open through physical end faces; offset/length still permit partial threads. After a partial cut requery thread.targets: each complete uncut band has its own origin and length, and offset is local to that band. Targets from before the cut are stale and rejected; remaining bands persist through save/reopen and Undo/Redo. No automatic production runout or entry chamfer. No preview or cosmetic threads."},
            {"name":"body.get", "params":{"body_id":1}},
            {"name":"selection.set", "params":{"body_ids":[1,2]}},
            {"name":"primitive.create", "params":{"kind":"box|cylinder|sphere|cone|torus", "name":"optional", "position":[0,0,0], "width":20, "depth":20, "height":20, "radius":10, "top_radius":0, "major_radius":15, "minor_radius":4}},
            {"name":"feature.offset.start", "params":{"body_id":1,"mode":"SurfaceOffset|HollowSolid|ThickenSurface|SymmetricThickenSurface","distance":-1.0,"tolerance":0.000001,"faces":[0],"hide_original":false},"notes":"Exact B-Rep only. SymmetricThickenSurface uses positive total thickness, half along each normal direction; use faces:[] for thickening. Zero-based face IDs for HollowSolid. Returns an asynchronous job_id; no mutation until commit."},
            {"name":"curve.equation.preview","params":{"params":{"expressions":["20*cos(t)","20*sin(t)","2*t"],"spherical":false,"degrees":false,"domain":[0,12.5663706144],"unit_mm":1,"transform":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}},"notes":"Read-only sampled spatial curve preview. t is the parameter; XYZ world axes. No planar profile implied."},
            {"name":"curve.equation.apply","params":{"params":"same as curve.equation.preview","body_id":"optional existing curve id to edit"},"notes":"Validate and commit curve definition atomically, with undo. Omitting body_id creates a curve."},
            {"name":"surface.equation.preview","params":{"params":{"expressions":["u","v","0.02*(u^2-v^2)"],"spherical":false,"degrees":false,"u_domain":[-10,10],"v_domain":[-10,10],"unit_mm":1,"transform":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],"resolution":24,"fill":false}},"notes":"Read-only smooth B-spline interpolation of a bounded u/v grid, not an exact symbolic surface. resolution:4..64; Fill requires an already closed surface and never adds caps."},
            {"name":"surface.equation.apply","params":{"params":"same as surface.equation.preview","body_id":"optional existing equation surface id"},"notes":"Commit validated surface/solid atomically, preserving ID on edit. For normal extrusion use feature.offset.start with SymmetricThickenSurface and positive total thickness; linked offset results are frozen snapshots."},
            {"name":"feature.offset.status", "params":{"job_id":"returned job_id"}},
            {"name":"feature.offset.commit", "params":{"job_id":"returned job_id"}},
            {"name":"feature.offset.cancel", "params":{"job_id":"returned job_id"}},
            {"name":"sketch.create", "params":{"plane":"xy|yz|zx|custom", "origin":[0,0,0], "normal":[0,0,1], "name":"optional", "entities":[{"type":"rectangle", "corner1":[-10,-5], "corner2":[10,5]}]}},
            {"name":"sketch.trim.preview", "params":{"sketch_id":1,"point":[10,0],"tolerance":0.1},"notes":"Read-only Trim plan for a committed Sketch, in its local plane UV coordinates (mm). Returns removed geometry, replacements, resulting regions and preview_token. Does not validate dependent solids. Equation Curves are not supported."},
            {"name":"sketch.trim.apply", "params":{"sketch_id":1,"point":[10,0],"tolerance":0.1,"preview_token":"token returned by preview"},"notes":"Requires Idle mode and a fresh preview for the same sketch, point and tolerance. Atomically trims and rebuilds supported dependents with Undo/Redo, retaining body IDs. Failure preserves the document. Query fresh regions afterward; composite outlines become independent segments."},
            {"name":"feature.extrude", "description":"Extrude a Sketch or planar equation Curve. Use the current body's sketch.regions indices; sketch_id accepts either body kind.", "params":{"sketch_id":1, "region_indices":[0], "depth":20, "symmetric":false, "name":"optional"}},
            {"name":"feature.revolve", "params":{"sketch_id":1, "region_index":0, "axis_point":[0,0], "axis_direction":[0,1], "angle_deg":360, "name":"optional"}},
            {"name":"body.transform", "params":{"body_ids":[1], "translation":[0,0,0], "rotation_deg":[0,0,0], "scale":[1,1,1], "pivot":[0,0,0]}},
            {"name":"boolean.apply", "params":{"operation":"union|subtract|intersect", "body_ids":[1,2], "keep_originals":false, "name":"optional"}},
            {"name":"body.rename", "params":{"body_id":1, "name":"Bracket"}},
            {"name":"body.set_visibility", "params":{"body_id":1, "visible":true}},
            {"name":"body.delete", "params":{"body_ids":[1]}},
            {"name":"file.import", "params":{"path":"<MODEL_PATH>"}, "notes":"STEP, IGES, STL, OBJ, PLY, GLB/glTF and 3MF build meshes. 3MF units/components/transforms are preserved; printer settings are not imported; unsupported required extensions reject the import without partial objects."},
            {"name":"body.export", "params":{"body_id":1, "path":"<OUTPUT_PATH>", "format":"step|stl|obj|ply|glb"},"notes":"Background export with staged publication; success means the file exists completely. Failure/cancel preserves the previous destination. STEP meshes require a validated closed solid; exact surfaces retain B-rep."},
            {"name":"project.save", "params":{"path":"<PROJECT_PATH>"}},
            {"name":"project.open", "params":{"path":"<PROJECT_PATH>"}},
            {"name":"history.undo", "params":{}},
            {"name":"history.redo", "params":{}},
            {"name":"view.fit", "params":{}},
            {"name":"view.recenter", "params":{},"notes":"Idle mode only. Center visible document geometry, switch to perspective and fit its size. Ignores hidden, deleted and preview geometry."}
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_request(key: &str) -> AgentRequest {
        let mut request = AgentRequest::new(json!(1), "primitive.create", json!({"kind":"box"}));
        request.idempotency_key = Some(key.into());
        request
    }

    #[test]
    fn frame_limit_is_enforced_before_eof_or_newline() {
        struct Endless {
            consumed: usize,
        }
        impl BufRead for Endless {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                Ok(&[b'x'; 8192])
            }
            fn consume(&mut self, count: usize) {
                self.consumed += count;
            }
        }
        impl Read for Endless {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                unreachable!()
            }
        }
        let mut reader = Endless { consumed: 0 };
        assert_eq!(
            read_bounded_line(&mut reader, MAX_REQUEST_BYTES)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(reader.consumed, MAX_REQUEST_BYTES);
        let mut reader = io::Cursor::new(b"abc\ndef\n");
        assert_eq!(read_bounded_line(&mut reader, 4).unwrap(), b"abc\n");
        assert_eq!(read_bounded_line(&mut reader, 4).unwrap(), b"def\n");
        assert!(read_bounded_line(&mut io::Cursor::new(b"abcde\n"), 4).is_err());
    }

    #[test]
    fn queue_full_and_missing_keys_never_admit_writes() {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ledger = Mutex::new(ReplayLedger::default());
        let mut missing = write_request("missing");
        missing.idempotency_key = None;
        assert!(
            matches!(enqueue_request(missing,&tx,&stop,&ledger),Err(r) if r.error.as_ref().unwrap().code=="idempotency_key_required")
        );
        assert!(rx.try_recv().is_err());
        assert!(enqueue_request(write_request("first"), &tx, &stop, &ledger).is_ok());
        assert!(
            matches!(enqueue_request(write_request("second"),&tx,&stop,&ledger),Err(r) if r.error.as_ref().unwrap().code=="server_busy")
        );
        assert_eq!(ledger.lock().unwrap().records.len(), 1);
    }

    #[test]
    fn queued_deadline_cancel_and_shutdown_are_terminal() {
        let stop = Arc::new(AtomicBool::new(false));
        let expired = RequestControl::new(Duration::ZERO, Arc::clone(&stop));
        assert!(!expired.try_begin());
        let cancelled = RequestControl::new(RESPONSE_TIMEOUT, Arc::clone(&stop));
        assert_eq!(
            cancelled.poll(json!(1), true).error.unwrap().code,
            "expired"
        );
        assert!(!cancelled.try_begin());
        let stopped = RequestControl::new(RESPONSE_TIMEOUT, Arc::clone(&stop));
        stop.store(true, Ordering::Release);
        assert!(!stopped.try_begin());
    }

    #[test]
    fn running_timeout_is_not_cancellation_and_replay_is_at_most_once() {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ledger = Mutex::new(ReplayLedger::default());
        let request = write_request("logical-operation");
        let (control, response_rx) = enqueue_request(request.clone(), &tx, &stop, &ledger)
            .ok()
            .unwrap();
        let pending = rx.recv().unwrap();
        assert!(pending.try_begin());
        assert!(!pending.try_begin());
        assert_eq!(
            control.poll(json!(2), true).error.unwrap().code,
            "in_progress"
        );
        assert!(
            matches!(enqueue_request(request.clone(),&tx,&stop,&ledger),Err(r) if r.error.as_ref().unwrap().code=="in_progress")
        );
        assert!(rx.try_recv().is_err());
        pending.respond(AgentResponse::success(json!(1), json!({"body_id":7})));
        assert!(response_rx.recv().unwrap().ok);
        let mut retry = request.clone();
        retry.id = json!(99);
        let replay = enqueue_request(retry, &tx, &stop, &ledger).err().unwrap();
        assert_eq!(replay.id, json!(99));
        assert_eq!(replay.result, Some(json!({"body_id":7})));
        let mut conflict = request;
        conflict.params = json!({"kind":"sphere"});
        assert!(
            matches!(enqueue_request(conflict,&tx,&stop,&ledger),Err(r) if r.error.as_ref().unwrap().code=="idempotency_conflict")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn replay_ledger_refuses_to_evict_live_or_recent_writes() {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ledger = Mutex::new(ReplayLedger::default());
        for i in 0..MAX_REPLAY_RECORDS {
            assert!(
                enqueue_request(write_request(&format!("key-{i}")), &tx, &stop, &ledger).is_ok()
            );
            rx.recv()
                .unwrap()
                .respond(AgentResponse::success(json!(1), json!({})));
        }
        assert!(
            matches!(enqueue_request(write_request("overflow"),&tx,&stop,&ledger),Err(r) if r.error.as_ref().unwrap().code=="server_busy")
        );
        assert_eq!(ledger.lock().unwrap().records.len(), MAX_REPLAY_RECORDS);
        let replay = enqueue_request(write_request("key-0"), &tx, &stop, &ledger)
            .err()
            .unwrap();
        assert!(replay.ok);
        let huge = RequestControl::new(RESPONSE_TIMEOUT, stop);
        huge.complete(&AgentResponse::success(
            json!(1),
            json!({"blob":"x".repeat(MAX_REPLAY_RESPONSE_BYTES+1)}),
        ));
        assert_eq!(
            huge.poll(json!(2), false).result.unwrap()["result_omitted"],
            true
        );
    }

    #[test]
    fn socket_without_newline_is_rejected_at_limit() {
        let server = AgentServer::start("127.0.0.1:0").unwrap();
        let mut socket = TcpStream::connect(server.address().unwrap()).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .write_all(&vec![b'x'; MAX_REQUEST_BYTES + 1])
            .unwrap();
        let response: AgentResponse =
            serde_json::from_slice(&read_bounded_line(&mut BufReader::new(socket), 4096).unwrap())
                .unwrap();
        assert_eq!(response.error.unwrap().code, "request_too_large");
        assert!(server.try_recv().is_none());
    }

    #[test]
    fn partial_frames_have_an_absolute_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(DeadlineReader {
            stream,
            deadline: Instant::now() + Duration::from_millis(60),
            stop: Arc::new(AtomicBool::new(false)),
        });
        let started = Instant::now();
        let error = read_bounded_line(&mut reader, MAX_REQUEST_BYTES).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(peer);
    }

    #[test]
    fn multi_megabyte_reply_reaches_a_slow_reader_without_truncation() {
        let server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = server.address().unwrap();
        let reader = thread::spawn(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            socket.set_read_timeout(Some(RESPONSE_TIMEOUT)).unwrap();
            write_json_line(
                &mut socket,
                &AgentRequest::new(json!(1), "reference.get", json!({})),
            )
            .unwrap();
            let mut data = Vec::new();
            let mut block = [0; 8192];
            loop {
                let count = socket.read(&mut block).unwrap();
                assert_ne!(count, 0, "response ended before the JSON newline");
                data.extend_from_slice(&block[..count]);
                if data.last() == Some(&b'\n') {
                    break;
                }
                thread::sleep(Duration::from_millis(2));
            }
            let response: AgentResponse = serde_json::from_slice(&data).unwrap();
            let result = response.result.unwrap();
            assert_eq!(
                result["image"].as_str().unwrap().len(),
                8 * MAX_REQUEST_BYTES
            );
        });
        let until = Instant::now() + Duration::from_secs(5);
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        };
        assert!(pending.try_begin());
        pending.respond(AgentResponse::success(
            json!(1),
            json!({"image":"a".repeat(8 * MAX_REQUEST_BYTES)}),
        ));
        reader.join().unwrap();
    }

    #[test]
    fn reply_to_peer_that_stops_reading_obeys_deadline_and_shutdown() {
        for shutdown in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let signal = Arc::clone(&stop);
            let started = Instant::now();
            let worker = thread::spawn(move || {
                let mut writer = DeadlineWriter {
                    stream,
                    deadline: Instant::now()
                        + if shutdown {
                            RESPONSE_TIMEOUT
                        } else {
                            Duration::from_millis(80)
                        },
                    stop,
                };
                // Loopback stacks may buffer whole frames without a reader.
                // Keep producing until the absolute deadline or stop fires.
                loop {
                    if let Err(error) = writer.write_all(&[b'x'; 16384]) {
                        break error.kind();
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            });
            if shutdown {
                thread::sleep(Duration::from_millis(80));
                signal.store(true, Ordering::Release);
            }
            let error = worker.join().unwrap();
            assert_eq!(
                error,
                if shutdown {
                    io::ErrorKind::ConnectionAborted
                } else {
                    io::ErrorKind::TimedOut
                }
            );
            assert!(started.elapsed() < Duration::from_secs(3));
            drop(peer);
        }
    }

    #[test]
    fn replay_budgets_and_expiry_are_enforced_without_evicting_running_work() {
        let (tx, rx) = mpsc::sync_channel(MAX_PENDING_REQUESTS);
        let stop = Arc::new(AtomicBool::new(false));
        let ledger = Mutex::new(ReplayLedger::default());
        // Large write payloads hit the aggregate byte budget before record count.
        for i in 0..9 {
            let mut request = write_request(&format!("large-{i}"));
            request.params = json!({"text":"x".repeat(MAX_REQUEST_BYTES - 1024)});
            let result = enqueue_request(request, &tx, &stop, &ledger);
            if i < 8 {
                assert!(result.is_ok());
                let pending = rx.recv().unwrap();
                assert!(pending.try_begin());
                pending.respond(AgentResponse::success(json!(1), json!({})));
            } else {
                assert!(
                    matches!(result, Err(r) if r.error.as_ref().unwrap().code == "server_busy")
                );
            }
        }
        {
            let mut ledger = ledger.lock().unwrap();
            let old = Instant::now() - REPLAY_RETENTION;
            for record in ledger.records.values() {
                *record.control.state.lock().unwrap() =
                    RequestState::Finished(AgentResponse::success(Value::Null, json!({})), old);
            }
            // A running entry never expires, regardless of its original deadline.
            *ledger.records["large-0"].control.state.lock().unwrap() = RequestState::Running;
            ledger.prune();
            assert_eq!(ledger.records.len(), 1);
            assert!(ledger.input_bytes < MAX_REPLAY_INPUT_BYTES);
        }
        assert!(enqueue_request(write_request("small"), &tx, &stop, &ledger).is_ok());
    }

    #[test]
    fn desktop_shutdown_ends_all_watches_after_listener_replacement() {
        let lifetime = Arc::new(DesktopLifetime::default());
        let first = AgentServer::start_watching("127.0.0.1:0", Arc::clone(&lifetime)).unwrap();
        let first_address = first.address().unwrap();
        let mut bridges = [
            connect_desktop_watch(first_address).unwrap(),
            connect_desktop_watch(first_address).unwrap(),
        ];
        assert!(
            first.try_recv().is_none(),
            "watches must not use the UI request queue"
        );
        let second = AgentServer::start_watching("127.0.0.1:0", Arc::clone(&lifetime)).unwrap();
        drop(first);
        for bridge in &mut bridges {
            bridge
                .set_read_timeout(Some(Duration::from_millis(50)))
                .unwrap();
            let error = bridge.read(&mut [0]).unwrap_err();
            assert!(matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ));
        }
        let second_address = second.address().unwrap();
        let mut third = connect_desktop_watch(second_address).unwrap();
        lifetime.close();
        for bridge in bridges.iter_mut().chain([&mut third]) {
            bridge
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            assert_eq!(bridge.read(&mut [0]).unwrap(), 0);
        }
        assert!(connect_desktop_watch(second_address).is_err());
    }

    #[test]
    fn connection_cap_and_shutdown_do_not_leave_detached_clients() {
        let server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = server.address().unwrap();
        let mut sockets = Vec::new();
        for id in 0..MAX_CONNECTIONS {
            let mut socket = TcpStream::connect(address).unwrap();
            write_json_line(
                &mut socket,
                &AgentRequest::new(json!(id), "system.ping", json!({})),
            )
            .unwrap();
            sockets.push(socket);
        }
        let until = Instant::now() + Duration::from_secs(3);
        let mut pending = Vec::new();
        while pending.len() < MAX_CONNECTIONS && Instant::now() < until {
            if let Some(p) = server.try_recv() {
                pending.push(p);
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(pending.len(), MAX_CONNECTIONS);
        let mut extra = TcpStream::connect(address).unwrap();
        extra
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut byte = [0];
        match extra.read(&mut byte) {
            Ok(0) => (),
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => (),
            other => panic!("excess connection was not closed: {other:?}"),
        }
        assert!(server.try_recv().is_none());
        drop(server);
        assert!(pending.iter().all(|p| !p.try_begin()));
    }

    #[test]
    fn response_roundtrip_keeps_structured_error() {
        let response = AgentResponse::failure(json!(7), "bad_request", "missing body");
        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: AgentResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, response);
    }

    #[test]
    fn primitive_params_reject_unknown_fields() {
        let error = parse_params::<PrimitiveCreateParams>(json!({
            "kind": "box",
            "mystery": 3
        }))
        .unwrap_err();
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn loopback_server_forwards_request_and_response() {
        let server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = server.address().unwrap().to_string();
        let client = thread::spawn(move || {
            send_request(
                &address,
                &AgentRequest::new(json!(1), "system.ping", json!({})),
            )
            .unwrap()
        });
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let id = pending.request.id.clone();
        assert!(pending.try_begin());
        pending.respond(AgentResponse::success(id, json!({"pong": true})));
        let response = client.join().unwrap();
        assert_eq!(response.result, Some(json!({"pong": true})));
    }

    #[test]
    fn occupied_port_is_reported_without_disturbing_existing_listener() {
        let server = AgentServer::start("127.0.0.1:0").unwrap();
        let address = server.address().unwrap().to_string();
        let error = match AgentServer::start(&address) {
            Ok(_) => panic!("the occupied port unexpectedly accepted a second listener"),
            Err(error) => error,
        };
        assert!(error.contains("cannot bind Agent Control"));
        assert_eq!(server.address().unwrap().to_string(), address);
    }
}
