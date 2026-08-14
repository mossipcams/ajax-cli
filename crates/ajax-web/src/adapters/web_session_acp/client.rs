//! Minimal newline-delimited JSON-RPC stdio client for Cursor ACP.
//!
//! We implement this locally instead of pulling `agent-client-protocol` because
//! the published SDK targets a different async runtime shape than ajax-web's
//! tokio WebSocket bridge. This module covers initialize, session/new,
//! session/prompt, session/cancel, session/update notifications, and permission
//! requests from the agent.

use ajax_core::{
    adapters::{acp_args_for_candidate, acp_launch_for_agent, AcpLaunch, AcpModelSelection},
    models::AgentClient,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
use std::{cell::RefCell, path::PathBuf};

#[cfg(test)]
thread_local! {
    static TEST_ACP_PROGRAM: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_ACP_EXTRA_ARGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` with ACP spawn redirected to a test program (typically a Node fake agent).
#[cfg(test)]
pub(crate) fn with_test_acp_program<F, R>(path: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_ACP_PROGRAM.with(|slot| {
        *slot.borrow_mut() = Some(path.to_path_buf());
        let result = f();
        *slot.borrow_mut() = None;
        result
    })
}

/// Add argv tokens for the next test ACP spawns inside `f` (e.g. `--load-fail`).
#[cfg(test)]
pub(crate) fn with_test_acp_extra_args<F, R>(args: &[&str], f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_ACP_EXTRA_ARGS.with(|slot| {
        let saved = slot.borrow().clone();
        *slot.borrow_mut() = args.iter().map(|s| (*s).to_string()).collect();
        let result = f();
        *slot.borrow_mut() = saved;
        result
    })
}

enum PendingResponse {
    Blocking(Sender<Result<Value, String>>),
    Streaming { method: &'static str },
}

type PendingResponses = Arc<Mutex<HashMap<u64, PendingResponse>>>;

#[derive(Debug, Clone)]
pub enum AcpClientEvent {
    SessionUpdate(Value),
    ClientRequest {
        id: Value,
        method: String,
        params: Value,
    },
    RequestFinished {
        id: u64,
        method: &'static str,
        result: Result<Value, String>,
    },
    Error(String),
    Exited,
}

pub struct SpawnReport {
    pub load_session_advertised: bool,
    pub resumed: bool,
}

pub struct AcpStdioClient {
    stdin: ChildStdin,
    events: Receiver<AcpClientEvent>,
    next_id: u64,
    pending: PendingResponses,
    session_id: String,
    /// Kept because each harness advertises its model catalog here.
    session_new_result: Value,
    child: Child,
    _reader: thread::JoinHandle<()>,
}

impl AcpStdioClient {
    /// Spawn the harness's ACP process. `model` is only placed on the argv for
    /// harnesses that pin at spawn (Cursor); the bridges select in-band.
    pub fn spawn(
        agent: AgentClient,
        worktree_path: &Path,
        model: Option<&str>,
        resume_session_id: Option<&str>,
    ) -> Result<(Self, SpawnReport), String> {
        let mut child = spawn_acp_process(agent, worktree_path, model)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "acp process missing stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "acp process missing stdout".to_string())?;
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel();
        let pending_for_reader = Arc::clone(&pending);
        let reader = thread::spawn(move || read_loop(stdout, pending_for_reader, event_tx));

        let mut client = Self {
            stdin,
            events: event_rx,
            next_id: 1,
            pending,
            session_id: String::new(),
            session_new_result: Value::Null,
            child,
            _reader: reader,
        };
        let init_result = client.initialize()?;
        let load_session_advertised = load_session_advertised(&init_result);
        let mut resumed = false;
        if let Some(resume_id) = resume_session_id.filter(|_| load_session_advertised) {
            if client.session_load(worktree_path, resume_id).is_ok() {
                client.session_id = resume_id.to_string();
                client.drain_pending_events();
                resumed = true;
            } else {
                client.drain_pending_events();
                client.session_id = client.session_new(worktree_path)?;
            }
        } else {
            client.session_id = client.session_new(worktree_path)?;
        }
        client.apply_model_in_band(agent, model);
        let report = SpawnReport {
            load_session_advertised,
            resumed,
        };
        Ok((client, report))
    }

    /// Tell a bridge harness which model to run. Cursor is already pinned on its
    /// argv; Codex takes `session/set_model`, Claude and Pi take a `model`
    /// config option. A refusal is not fatal — the harness keeps its own default.
    fn apply_model_in_band(&mut self, agent: AgentClient, model: Option<&str>) {
        let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
            return;
        };
        if model_uses_cli_default(Some(model)) {
            return;
        }
        let Some(launch) = acp_launch_for_agent(agent) else {
            return;
        };
        let session_id = self.session_id.clone();
        let (method, params) = match launch.model_selection {
            AcpModelSelection::SpawnArg => return,
            AcpModelSelection::SetModel => (
                "session/set_model",
                json!({ "sessionId": session_id, "modelId": model }),
            ),
            AcpModelSelection::ConfigOption => (
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": model }),
            ),
        };
        if let Err(error) = self.call(method, params) {
            tracing::warn!(
                target: "ajax_web",
                agent = ?agent,
                model = %model,
                error = %error,
                "acp model selection refused"
            );
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// True when the ACP OS process has exited. Reconnect must respawn, not
    /// reattach to a dead stdio pipe.
    pub fn host_exited(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => true,
        }
    }

    #[cfg(test)]
    pub(crate) fn kill_host_for_test(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    #[cfg(test)]
    pub(crate) fn child_id(&self) -> u32 {
        self.child.id()
    }

    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.events.try_recv().ok()
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn begin_prompt(&mut self, text: &str) -> Result<u64, String> {
        if self.prompt_in_flight() {
            return Err("prompt already in flight".to_string());
        }
        self.begin_request(
            "session/prompt",
            json!({
                "sessionId": self.session_id,
                "prompt": [{ "type": "text", "text": text }],
            }),
        )
    }

    pub fn begin_cancel(&mut self) -> Result<u64, String> {
        self.begin_request("session/cancel", json!({ "sessionId": self.session_id }))
    }

    pub(crate) fn prompt_in_flight(&self) -> bool {
        self.pending.lock().unwrap().values().any(|entry| {
            matches!(
                entry,
                PendingResponse::Streaming {
                    method: "session/prompt"
                }
            )
        })
    }

    pub fn respond_client_request(&mut self, id: &Value, result: Value) -> Result<(), String> {
        self.write_response(id, result)
    }

    fn initialize(&mut self) -> Result<Value, String> {
        let response = self.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": { "name": "ajax-web", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        self.write_notification("notifications/initialized", json!({}))?;
        Ok(response)
    }

    fn session_load(&mut self, worktree_path: &Path, session_id: &str) -> Result<(), String> {
        let mut params = session_new_params(worktree_path);
        if let Value::Object(ref mut map) = params {
            map.insert("sessionId".to_string(), json!(session_id));
        }
        self.call("session/load", params)?;
        Ok(())
    }

    fn drain_pending_events(&self) {
        while self.poll_event().is_some() {}
    }

    fn session_new(&mut self, worktree_path: &Path) -> Result<String, String> {
        let response = self.call("session/new", session_new_params(worktree_path))?;
        let session_id = response
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "session/new missing sessionId".to_string())?;
        self.session_new_result = response;
        Ok(session_id)
    }

    /// Raw `session/new` result, which is where each harness advertises the
    /// models it can run. Empty until a session has been created.
    pub fn session_new_result(&self) -> &Value {
        &self.session_new_result
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(id, PendingResponse::Blocking(tx));
        self.write_request(method, params, id)?;
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(message)) => Err(message),
            Err(_) => Err(format!("acp request timed out: {method}")),
        }
    }

    fn begin_request(&mut self, method: &'static str, params: Value) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.pending
            .lock()
            .unwrap()
            .insert(id, PendingResponse::Streaming { method });
        if let Err(error) = self.write_request(method, params, id) {
            self.pending.lock().unwrap().remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    fn write_request(&mut self, method: &str, params: Value, id: u64) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        write_line(&mut self.stdin, &payload)
    }

    fn write_notification(&mut self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        write_line(&mut self.stdin, &payload)
    }

    fn write_response(&mut self, id: &Value, result: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        write_line(&mut self.stdin, &payload)
    }
}

impl Drop for AcpStdioClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_line(stdin: &mut ChildStdin, payload: &Value) -> Result<(), String> {
    let mut line = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .map_err(|error| format!("acp stdin write failed: {error}"))
}

/// `mcpServers` is required and must be an array. Omitting it fails Cursor's
/// schema validation, which it surfaces only as JSON-RPC "Internal error" —
/// the orchestration session could never start without this key.
pub(crate) fn session_new_params(worktree_path: &Path) -> Value {
    json!({
        "cwd": worktree_path.display().to_string(),
        "mcpServers": [],
    })
}

pub(crate) fn load_session_advertised(value: &Value) -> bool {
    value
        .get("agentCapabilities")
        .and_then(|caps| caps.get("loadSession"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// True when `--model` should be omitted (Cursor default / Auto).
pub(crate) fn model_uses_cli_default(model: Option<&str>) -> bool {
    match model.map(str::trim) {
        None | Some("") | Some("auto") => true,
        Some(_) => false,
    }
}

/// Build argv for one candidate program of this harness's ACP launch.
pub(crate) fn acp_args_for_program(
    launch: AcpLaunch,
    base_args: &[&str],
    model: Option<&str>,
) -> Vec<String> {
    let model = (!model_uses_cli_default(model)).then_some(model).flatten();
    acp_args_for_candidate(launch, base_args, model)
}

/// Candidate list for this harness, native endpoint first.
///
/// A harness that grows its own `acp` subcommand should be used directly rather
/// than through its packaged adapter, so the CLI is asked (once per TTL) whether
/// it advertises one. Asking beats trying: an unknown argument is a prompt to
/// some CLIs, which would start a real session.
fn acp_candidates(launch: AcpLaunch) -> Vec<(String, Vec<String>)> {
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(program) = launch.native_program {
        if native_acp_advertised(program) {
            candidates.push((program.to_string(), vec!["acp".to_string()]));
        }
    }
    for (program, base_args) in launch.candidates {
        candidates.push((
            (*program).to_string(),
            base_args.iter().map(|arg| (*arg).to_string()).collect(),
        ));
    }
    candidates
}

/// True when `<program> --help` lists an `acp` subcommand. Cached: this runs on
/// every session acquire and the answer only changes when the CLI is upgraded.
fn native_acp_advertised(program: &str) -> bool {
    static CACHE: Mutex<Option<HashMap<String, (Instant, bool)>>> = Mutex::new(None);
    const TTL: Duration = Duration::from_secs(300);

    if let Ok(guard) = CACHE.lock() {
        if let Some((checked_at, advertised)) =
            guard.as_ref().and_then(|entries| entries.get(program))
        {
            if checked_at.elapsed() < TTL {
                return *advertised;
            }
        }
    }

    let advertised = Command::new(program)
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("acp ") || line == "acp"
            })
        });

    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(program.to_string(), (Instant::now(), advertised));
    }
    advertised
}

fn spawn_acp_process(
    agent: AgentClient,
    worktree_path: &Path,
    model: Option<&str>,
) -> Result<Child, String> {
    #[cfg(test)]
    if let Some(program) = TEST_ACP_PROGRAM.with(|slot| slot.borrow().clone()) {
        let extra_args = TEST_ACP_EXTRA_ARGS.with(|slot| slot.borrow().clone());
        let mut command = Command::new("node");
        command.arg(program);
        command.args(extra_args);
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        return command
            .spawn()
            .map_err(|error| format!("failed to spawn test acp program: {error}"));
    }

    let Some(launch) = acp_launch_for_agent(agent) else {
        return Err(format!("no ACP mapping for agent {agent:?}"));
    };
    for (program, base_args) in acp_candidates(launch) {
        let base_args: Vec<&str> = base_args.iter().map(String::as_str).collect();
        let args = acp_args_for_program(launch, &base_args, model);
        let mut command = Command::new(&program);
        command.args(&args);
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Ok(child) = command.spawn() {
            return Ok(child);
        }
    }
    // Every candidate is missing: an install problem, not a runtime failure.
    Err(format!(
        "{agent:?} ACP agent is not installed — {}",
        launch.install_hint
    ))
}

fn read_loop(
    stdout: impl std::io::Read + Send + 'static,
    pending: PendingResponses,
    event_tx: Sender<AcpClientEvent>,
) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let Ok(line) = line else {
            let _ = event_tx.send(AcpClientEvent::Exited);
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            if method == "session/update" {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                let _ = event_tx.send(AcpClientEvent::SessionUpdate(params));
                continue;
            }
            if let Some(id) = value.get("id") {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                let _ = event_tx.send(AcpClientEvent::ClientRequest {
                    id: id.clone(),
                    method: method.to_string(),
                    params,
                });
                continue;
            }
        }
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            let pending_entry = pending.lock().unwrap().remove(&id);
            let Some(pending_entry) = pending_entry else {
                continue;
            };
            let result = if let Some(error) = value.get("error") {
                Err(error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("acp error")
                    .to_string())
            } else {
                Ok(value.get("result").cloned().unwrap_or(Value::Null))
            };
            match pending_entry {
                PendingResponse::Blocking(tx) => {
                    let _ = tx.send(result);
                }
                PendingResponse::Streaming { method } => {
                    let _ = event_tx.send(AcpClientEvent::RequestFinished { id, method, result });
                }
            }
        }
    }
    let _ = event_tx.send(AcpClientEvent::Exited);
}
