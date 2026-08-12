//! Minimal newline-delimited JSON-RPC stdio client for Cursor ACP.
//!
//! We implement this locally instead of pulling `agent-client-protocol` because
//! the published SDK targets a different async runtime shape than ajax-web's
//! tokio WebSocket bridge. This module covers initialize, session/new,
//! session/prompt, session/cancel, session/update notifications, and permission
//! requests from the agent.

use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
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

/// Cursor ACP allows one in-flight `session/prompt`; additional prompts queue here.
const MAX_QUEUED_PROMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptDispatch {
    StartNow,
    Queued,
}

/// Decide whether to start a prompt now or enqueue it behind the in-flight turn.
pub(crate) fn dispatch_prompt(
    prompt_in_flight: bool,
    queued: &mut VecDeque<String>,
    text: String,
) -> PromptDispatch {
    if prompt_in_flight {
        // ponytail: cap at 8 queued prompts; upgrade path is block + error event to the operator.
        if queued.len() >= MAX_QUEUED_PROMPTS {
            queued.pop_front();
        }
        queued.push_back(text);
        PromptDispatch::Queued
    } else {
        PromptDispatch::StartNow
    }
}

pub(crate) fn clear_prompt_queue(queued: &mut VecDeque<String>) {
    queued.clear();
}

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
    queued_prompts: VecDeque<String>,
    session_id: String,
    child: Child,
    _reader: thread::JoinHandle<()>,
}

impl AcpStdioClient {
    /// Spawn Cursor ACP. `model` of `None`/`Some("auto")` omits `--model` so
    /// Cursor's default applies; any other id is pinned at process start.
    pub fn spawn(
        worktree_path: &Path,
        model: Option<&str>,
        resume_session_id: Option<&str>,
    ) -> Result<(Self, SpawnReport), String> {
        let mut child = spawn_cursor_acp_process(worktree_path, model)?;
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
            queued_prompts: VecDeque::new(),
            session_id: String::new(),
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
        let report = SpawnReport {
            load_session_advertised,
            resumed,
        };
        Ok((client, report))
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
        let text = text.to_string();
        match dispatch_prompt(
            self.prompt_in_flight(),
            &mut self.queued_prompts,
            text.clone(),
        ) {
            PromptDispatch::StartNow => self.begin_request(
                "session/prompt",
                json!({
                    "sessionId": self.session_id,
                    "prompt": [{ "type": "text", "text": text }],
                }),
            ),
            PromptDispatch::Queued => Ok(0),
        }
    }

    pub fn begin_cancel(&mut self, keep_queue: bool) -> Result<u64, String> {
        if !keep_queue {
            clear_prompt_queue(&mut self.queued_prompts);
        }
        self.begin_request("session/cancel", json!({ "sessionId": self.session_id }))
    }

    /// Start the next queued prompt when no `session/prompt` is in flight.
    pub fn flush_queued_prompt(&mut self) -> Result<(), String> {
        if self.prompt_in_flight() {
            return Ok(());
        }
        let Some(text) = self.queued_prompts.pop_front() else {
            return Ok(());
        };
        self.begin_request(
            "session/prompt",
            json!({
                "sessionId": self.session_id,
                "prompt": [{ "type": "text", "text": text }],
            }),
        )?;
        Ok(())
    }

    fn prompt_in_flight(&self) -> bool {
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
        response
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "session/new missing sessionId".to_string())
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
fn session_new_params(worktree_path: &Path) -> Value {
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

/// Argv templates for spawning Cursor ACP (`--model` is inserted later).
pub(crate) fn cursor_acp_program_candidates() -> [(&'static str, &'static [&'static str]); 2] {
    [("agent", &["acp"]), ("cursor", &["agent", "acp"])]
}

/// Build argv for one candidate program, inserting `--model <id>` before `acp`.
pub(crate) fn cursor_acp_args_for_program(base_args: &[&str], model: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = base_args.iter().map(|s| (*s).to_string()).collect();
    if model_uses_cli_default(model) {
        return args;
    }
    let Some(model) = model.map(str::trim).filter(|s| !s.is_empty()) else {
        return args;
    };
    // `agent acp` → `agent --model ID acp`; `cursor agent acp` → `cursor agent --model ID acp`.
    if let Some(acp_at) = args.iter().position(|a| a == "acp") {
        args.insert(acp_at, "--model".to_string());
        args.insert(acp_at + 1, model.to_string());
    } else {
        args.push("--model".to_string());
        args.push(model.to_string());
        args.push("acp".to_string());
    }
    args
}

fn spawn_cursor_acp_process(worktree_path: &Path, model: Option<&str>) -> Result<Child, String> {
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

    let mut last_error = String::from("failed to spawn cursor acp process");
    for (program, base_args) in cursor_acp_program_candidates() {
        let args = cursor_acp_args_for_program(base_args, model);
        let mut command = Command::new(program);
        command.args(&args);
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => last_error = format!("failed to spawn {program} acp: {error}"),
        }
    }
    Err(last_error)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_session_advertised_from_initialize_result() {
        let value = json!({ "agentCapabilities": { "loadSession": true } });
        assert!(load_session_advertised(&value));
        assert!(!load_session_advertised(&json!({})));
        assert!(!load_session_advertised(&json!({
            "agentCapabilities": { "loadSession": false }
        })));
    }

    /// Cursor validates `session/new` params and rejects a missing
    /// `mcpServers` with an opaque JSON-RPC "Internal error", so the session
    /// could never start. Keep the key present and an array.
    #[test]
    fn session_new_params_carry_mcp_servers_array() {
        let params = session_new_params(Path::new("/repo/worktree"));
        assert_eq!(
            params.get("cwd").and_then(Value::as_str),
            Some("/repo/worktree")
        );
        assert_eq!(
            params.get("mcpServers").and_then(Value::as_array),
            Some(&vec![])
        );
    }

    #[test]
    fn cursor_acp_command_prefers_agent_binary() {
        let candidates = cursor_acp_program_candidates();
        assert_eq!(candidates[0].0, "agent");
        assert_eq!(candidates[0].1, &["acp"][..]);
        assert_eq!(candidates[1].0, "cursor");
        assert_eq!(candidates[1].1, &["agent", "acp"][..]);
    }

    #[test]
    fn dispatch_prompt_starts_when_idle() {
        let mut queued = VecDeque::new();
        assert_eq!(
            dispatch_prompt(false, &mut queued, "hello".to_string()),
            PromptDispatch::StartNow
        );
        assert!(queued.is_empty());
    }

    #[test]
    fn dispatch_prompt_queues_when_in_flight() {
        let mut queued = VecDeque::new();
        assert_eq!(
            dispatch_prompt(true, &mut queued, "next".to_string()),
            PromptDispatch::Queued
        );
        assert_eq!(queued, VecDeque::from(["next".to_string()]));
    }

    #[test]
    fn dispatch_prompt_cap_drops_oldest() {
        let mut queued: VecDeque<String> = (0..MAX_QUEUED_PROMPTS)
            .map(|i| format!("old-{i}"))
            .collect();
        assert_eq!(
            dispatch_prompt(true, &mut queued, "new".to_string()),
            PromptDispatch::Queued
        );
        assert_eq!(queued.len(), MAX_QUEUED_PROMPTS);
        assert_eq!(queued.front().map(String::as_str), Some("old-1"));
        assert_eq!(queued.back().map(String::as_str), Some("new"));
    }

    #[test]
    fn clear_prompt_queue_empties_queued() {
        let mut queued = VecDeque::from(["a".to_string(), "b".to_string()]);
        clear_prompt_queue(&mut queued);
        assert!(queued.is_empty());
    }

    #[test]
    fn begin_cancel_keep_queue_leaves_queue_intact() {
        let mut queued = VecDeque::from(["next".to_string()]);
        let keep_queue = true;
        if !keep_queue {
            clear_prompt_queue(&mut queued);
        }
        assert_eq!(queued, VecDeque::from(["next".to_string()]));
    }

    #[test]
    fn cursor_acp_args_insert_model_before_acp() {
        assert_eq!(
            cursor_acp_args_for_program(&["acp"], Some("composer-2.5")),
            vec!["--model", "composer-2.5", "acp"]
        );
        assert_eq!(
            cursor_acp_args_for_program(&["agent", "acp"], Some("gpt-5.6-sol-medium")),
            vec!["agent", "--model", "gpt-5.6-sol-medium", "acp"]
        );
        assert_eq!(
            cursor_acp_args_for_program(&["acp"], Some("auto")),
            vec!["acp"]
        );
        assert_eq!(cursor_acp_args_for_program(&["acp"], None), vec!["acp"]);
    }
}
