//! Host `agent acp` JSON-RPC bridge for Ajax Web Session.
//! ponytail: module path `web_session_rpc` is historical; backend is ACP.

mod bridge;

use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

pub(crate) use bridge::{bridge_task_web_session_socket, AcpLiveUpdater};

const EVENT_QUEUE_BOUND: usize = 64;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const DEFAULT_ACP_ARGS: &[&str] = &["acp"];

type PendingRpcMap = HashMap<u64, SyncSender<Result<Value, AgentAcpError>>>;
type SharedPendingRpcs = Arc<Mutex<PendingRpcMap>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentAcpError {
    StartupFailed(String),
    SessionClosed,
    Protocol(String),
    HandshakeFailed(String),
}

impl std::fmt::Display for AgentAcpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartupFailed(message) => {
                write!(formatter, "agent acp startup failed: {message}")
            }
            Self::SessionClosed => write!(formatter, "agent acp session closed"),
            Self::Protocol(message) => write!(formatter, "agent acp protocol error: {message}"),
            Self::HandshakeFailed(message) => {
                write!(formatter, "agent acp handshake failed: {message}")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentAcpEvent {
    PromptStarted,
    AssistantDelta {
        text: String,
    },
    ToolCall {
        tool_name: String,
        status: String,
        summary: String,
        path: Option<String>,
    },
    ToolCallUpdate {
        tool_name: String,
        status: String,
        summary: String,
        path: Option<String>,
    },
    FileUpdate {
        path: String,
        status: String,
        summary: String,
    },
    AgentSettled,
    Error {
        message: String,
    },
    OperatorRequest {
        request_id: String,
        json_rpc_id: Value,
        method: String,
        summary: String,
        kind: OperatorRequestKind,
    },
    Exited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorRequestKind {
    Permission,
    Question,
}

#[derive(Debug, Deserialize)]
struct JsonRpcLine {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
}

pub fn encode_acp_request(id: u64, method: &str, params: Value) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
    )
}

pub fn encode_acp_response(id: &Value, result: Value) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        })
    )
}

pub fn encode_acp_notification(method: &str, params: Value) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        })
    )
}

pub fn encode_acp_error_response(id: &Value, code: i32, message: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            },
        })
    )
}

pub fn parse_acp_event_line(line: &[u8]) -> Result<Option<AgentAcpEvent>, AgentAcpError> {
    let trimmed = trim_line(line);
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed: JsonRpcLine = serde_json::from_slice(trimmed)
        .map_err(|error| AgentAcpError::Protocol(format!("invalid acp JSON: {error}")))?;

    if parsed.method.as_deref() == Some("session/update") {
        return Ok(event_from_update(parsed.params.as_ref()));
    }

    if parsed.id.is_some() {
        if let Some(error) = parsed.error {
            return Ok(Some(AgentAcpEvent::Error {
                message: extract_error_message(&error),
            }));
        }
        if parsed.result.is_some() {
            return Ok(Some(AgentAcpEvent::AgentSettled));
        }
    }

    Ok(None)
}

fn trim_line(line: &[u8]) -> &[u8] {
    let mut trimmed = line;
    while trimmed
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        trimmed = &trimmed[1..];
    }
    while trimmed
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    trimmed
}

fn event_from_update(params: Option<&Value>) -> Option<AgentAcpEvent> {
    let update = params?.get("update")?;
    match update.get("sessionUpdate").and_then(Value::as_str)? {
        "agent_message_chunk" => {
            let text = update
                .get("content")
                .and_then(|content| content.get("text"))
                .and_then(Value::as_str)?;
            (!text.is_empty()).then(|| AgentAcpEvent::AssistantDelta {
                text: text.to_string(),
            })
        }
        "tool_call" | "tool_call_update" => {
            let tool_name = first_string(update, &["title", "toolName", "name"])
                .unwrap_or_else(|| "Tool call".to_string());
            let status =
                first_string(update, &["status", "state"]).unwrap_or_else(|| "updated".to_string());
            let summary = first_string(update, &["summary", "title", "name"])
                .unwrap_or_else(|| tool_name.clone());
            let path = update
                .pointer("/rawInput/path")
                .or_else(|| update.pointer("/input/path"))
                .or_else(|| update.get("path"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if update.get("sessionUpdate").and_then(Value::as_str) == Some("tool_call") {
                Some(AgentAcpEvent::ToolCall {
                    tool_name,
                    status,
                    summary,
                    path,
                })
            } else {
                Some(AgentAcpEvent::ToolCallUpdate {
                    tool_name,
                    status,
                    summary,
                    path,
                })
            }
        }
        "file_update" | "diff" | "diff_update" => {
            let path = update.get("path").and_then(Value::as_str)?.to_string();
            let status =
                first_string(update, &["status", "kind"]).unwrap_or_else(|| "changed".to_string());
            let summary = first_string(update, &["summary", "title"])
                .unwrap_or_else(|| format!("{status} {path}"));
            Some(AgentAcpEvent::FileUpdate {
                path,
                status,
                summary,
            })
        }
        _ => None,
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown agent acp error")
        .to_string()
}

fn resolve_agent_program() -> String {
    for candidate in ["agent", "cursor-agent"] {
        if Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {candidate}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return candidate.to_string();
        }
    }
    "agent".to_string()
}

pub(crate) struct AgentAcpProcess {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    events: Receiver<AgentAcpEvent>,
    reader: Option<JoinHandle<()>>,
    next_id: AtomicU64,
    pending: SharedPendingRpcs,
    prompt_ids: Arc<Mutex<HashSet<u64>>>,
    session_id: Option<String>,
}

impl AgentAcpProcess {
    pub(crate) fn spawn(
        worktree: &Path,
        program: &str,
        args: &[&str],
    ) -> Result<Self, AgentAcpError> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(worktree)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                AgentAcpError::StartupFailed(format!("failed to start `{program}`: {error}"))
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AgentAcpError::StartupFailed("agent acp stdin unavailable".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentAcpError::StartupFailed("agent acp stdout unavailable".to_string())
        })?;
        let stdin = Arc::new(Mutex::new(stdin));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let prompt_ids = Arc::new(Mutex::new(HashSet::new()));
        let (tx, rx) = sync_channel(EVENT_QUEUE_BOUND);
        let reader = spawn_event_reader(
            stdout,
            tx,
            Arc::clone(&stdin),
            Arc::clone(&pending),
            Arc::clone(&prompt_ids),
        );
        Ok(Self {
            child,
            stdin,
            events: rx,
            reader: Some(reader),
            next_id: AtomicU64::new(1),
            pending,
            prompt_ids,
            session_id: None,
        })
    }

    pub(crate) fn handshake(&mut self, worktree: &Path) -> Result<String, AgentAcpError> {
        self.handshake_with_session(worktree, None)
    }

    pub(crate) fn handshake_with_session(
        &mut self,
        worktree: &Path,
        previous_session_id: Option<&str>,
    ) -> Result<String, AgentAcpError> {
        self.rpc(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false
                },
                "clientInfo": { "name": "ajax-web-session", "version": "0.1.0" }
            }),
        )?;
        self.rpc("authenticate", json!({ "methodId": "cursor_login" }))?;
        let (result, loaded_session_id) = if let Some(session_id) = previous_session_id {
            match self.rpc(
                "session/load",
                json!({ "sessionId": session_id, "cwd": worktree }),
            ) {
                Ok(result) => (result, Some(session_id.to_string())),
                Err(_) => (
                    self.rpc("session/new", json!({ "cwd": worktree, "mcpServers": [] }))?,
                    None,
                ),
            }
        } else {
            (
                self.rpc("session/new", json!({ "cwd": worktree, "mcpServers": [] }))?,
                None,
            )
        };
        let session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .or(loaded_session_id.as_deref())
            .ok_or_else(|| {
                AgentAcpError::HandshakeFailed("session handshake missing sessionId".into())
            })?
            .to_string();
        self.session_id = Some(session_id.clone());
        Ok(session_id)
    }

    fn rpc(&mut self, method: &str, params: Value) -> Result<Value, AgentAcpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = sync_channel(1);
        self.pending.lock().expect("pending lock").insert(id, tx);
        self.write_line(&encode_acp_request(id, method, params))?;
        match rx.recv_timeout(RPC_TIMEOUT) {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                self.pending.lock().expect("pending lock").remove(&id);
                Err(AgentAcpError::Protocol(format!(
                    "timed out waiting for acp response to {method}"
                )))
            }
        }
    }

    pub(crate) fn send_prompt(&mut self, message: &str) -> Result<(), AgentAcpError> {
        let session_id = self
            .session_id
            .as_deref()
            .ok_or_else(|| AgentAcpError::Protocol("session not initialized".into()))?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.prompt_ids.lock().expect("prompt ids lock").insert(id);
        self.write_line(&encode_acp_request(
            id,
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": message }]
            }),
        ))?;
        Ok(())
    }

    pub(crate) fn send_cancel(&mut self) -> Result<(), AgentAcpError> {
        let Some(session_id) = self.session_id.as_deref() else {
            return Ok(());
        };
        self.write_line(&encode_acp_notification(
            "session/cancel",
            json!({ "sessionId": session_id }),
        ))
    }

    pub(crate) fn respond_json_rpc(
        &mut self,
        json_rpc_id: &Value,
        result: Value,
    ) -> Result<(), AgentAcpError> {
        self.write_line(&encode_acp_response(json_rpc_id, result))
    }

    pub(crate) fn poll_event(&mut self) -> Option<AgentAcpEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    Some(AgentAcpEvent::Exited)
                } else {
                    None
                }
            }
            Err(TryRecvError::Disconnected) => Some(AgentAcpEvent::Exited),
        }
    }

    fn write_line(&self, line: &str) -> Result<(), AgentAcpError> {
        let mut stdin = self.stdin.lock().expect("stdin lock");
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|error| {
                AgentAcpError::Protocol(format!("failed to write acp command: {error}"))
            })
    }
}

impl Drop for AgentAcpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.reader.take();
    }
}

fn spawn_event_reader(
    stdout: impl Read + Send + 'static,
    tx: SyncSender<AgentAcpEvent>,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: SharedPendingRpcs,
    prompt_ids: Arc<Mutex<HashSet<u64>>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = trim_line(&line);
                    if trimmed.is_empty() {
                        continue;
                    }
                    let parsed: JsonRpcLine = match serde_json::from_slice(trimmed) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            let _ = tx.try_send(AgentAcpEvent::Error {
                                message: format!("invalid acp JSON: {error}"),
                            });
                            continue;
                        }
                    };

                    if let Some(method) = parsed.method.as_deref() {
                        if let Some(id) = parsed.id.as_ref() {
                            if let Some(parked) =
                                park_operator_request(method, id, parsed.params.as_ref())
                            {
                                let _ = tx.try_send(parked);
                            } else if let Some(response) = auto_response_for_method(method) {
                                write_stdin_line(&stdin, &encode_acp_response(id, response));
                            } else {
                                write_stdin_line(
                                    &stdin,
                                    &encode_acp_error_response(id, -32601, "Method not found"),
                                );
                            }
                            continue;
                        }
                        if method == "session/update" {
                            if let Some(event) = event_from_update(parsed.params.as_ref()) {
                                let _ = tx.try_send(event);
                            }
                            continue;
                        }
                        continue;
                    }

                    if let Some(id_value) = parsed.id {
                        if let Some(id) = json_id_as_u64(&id_value) {
                            let is_prompt = prompt_ids.lock().expect("prompt ids lock").remove(&id);
                            if is_prompt {
                                let event = if let Some(error) = parsed.error {
                                    AgentAcpEvent::Error {
                                        message: extract_error_message(&error),
                                    }
                                } else {
                                    AgentAcpEvent::AgentSettled
                                };
                                let _ = tx.try_send(event);
                                continue;
                            }
                            if let Some(waiter) = pending.lock().expect("pending lock").remove(&id)
                            {
                                let payload = if let Some(error) = parsed.error {
                                    Err(AgentAcpError::Protocol(extract_error_message(&error)))
                                } else {
                                    Ok(parsed.result.unwrap_or(Value::Null))
                                };
                                let _ = waiter.send(payload);
                                continue;
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.try_send(AgentAcpEvent::Exited);
    })
}

fn json_id_as_u64(id: &Value) -> Option<u64> {
    id.as_u64()
        .or_else(|| id.as_str().and_then(|s| s.parse().ok()))
}

fn auto_response_for_method(method: &str) -> Option<Value> {
    match method {
        // permission + ask_question are parked for the operator (see park_operator_request).
        "cursor/create_plan" => Some(json!({ "outcome": { "outcome": "cancelled" } })),
        _ => None,
    }
}

fn park_operator_request(
    method: &str,
    json_rpc_id: &Value,
    params: Option<&Value>,
) -> Option<AgentAcpEvent> {
    let kind = match method {
        "session/request_permission" => OperatorRequestKind::Permission,
        "cursor/ask_question" => OperatorRequestKind::Question,
        _ => return None,
    };
    let request_id = json_id_as_string(json_rpc_id);
    let summary = operator_request_summary(kind, params);
    Some(AgentAcpEvent::OperatorRequest {
        request_id,
        json_rpc_id: json_rpc_id.clone(),
        method: method.to_string(),
        summary,
        kind,
    })
}

fn json_id_as_string(id: &Value) -> String {
    match id {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn operator_request_summary(kind: OperatorRequestKind, params: Option<&Value>) -> String {
    match kind {
        OperatorRequestKind::Permission => {
            let tool = params
                .and_then(|value| {
                    value
                        .pointer("/toolCall/title")
                        .or_else(|| value.get("title"))
                })
                .and_then(Value::as_str);
            match tool {
                Some(title) if !title.is_empty() => format!("Permission: {title}"),
                _ => "Permission required".to_string(),
            }
        }
        OperatorRequestKind::Question => {
            let question = params
                .and_then(|value| {
                    value
                        .pointer("/question")
                        .or_else(|| value.get("text"))
                        .or_else(|| value.pointer("/questions/0/prompt"))
                })
                .and_then(Value::as_str);
            match question {
                Some(text) if !text.is_empty() => text.to_string(),
                _ => "Agent question".to_string(),
            }
        }
    }
}

fn write_stdin_line(stdin: &Arc<Mutex<ChildStdin>>, line: &str) {
    if let Ok(mut guard) = stdin.lock() {
        let _ = guard.write_all(line.as_bytes());
        let _ = guard.flush();
    }
}

pub(crate) fn spawn_default_agent_acp(worktree: &Path) -> Result<AgentAcpProcess, AgentAcpError> {
    let program = resolve_agent_program();
    AgentAcpProcess::spawn(worktree, &program, DEFAULT_ACP_ARGS)
}

#[cfg(test)]
mod tests;
