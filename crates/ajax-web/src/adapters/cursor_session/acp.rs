//! Cursor `agent … acp` stdio spawn and owner-driven JSON-RPC IO.

use serde_json::{json, Value};
use std::{
    path::Path,
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};

static NEXT_RPC_ID: AtomicU64 = AtomicU64::new(10_000);

pub struct AcpSession {
    pub session_id: String,
    // Held so `kill_on_drop` tears the ACP child down with the idle slot.
    _child: Child,
    cmd_tx: mpsc::UnboundedSender<AcpCommand>,
}

pub enum AcpCommand {
    Prompt { id: u64, text: String },
    Cancel { id: u64 },
    JsonRpcResponse { id: u64, result: Value },
}

pub enum AcpEvent {
    AgentMessage(String),
    WireEvent(Value),
    PermissionRequest {
        jsonrpc_id: u64,
        request_id: String,
        title: Option<String>,
        detail: Option<String>,
    },
    PromptFinished {
        _id: u64,
        stop_reason: String,
    },
    Exited,
}

pub struct SpawnResult {
    pub session: AcpSession,
    pub loaded: bool,
}

pub async fn spawn_acp_session(
    worktree: &Path,
    model: &str,
    resume_session_id: Option<&str>,
    event_tx: mpsc::UnboundedSender<AcpEvent>,
) -> Result<SpawnResult, String> {
    let mut child = Command::new("agent")
        .args(["--model", model, "acp"])
        .current_dir(worktree)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("agent acp spawn failed: {error}"))?;

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
    }

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "agent acp missing stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent acp missing stdout".to_string())?;
    let mut reader = BufReader::new(stdout);

    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {},
        },
    });
    write_line(&mut stdin, &init).await?;
    let init_resp = read_response_line(&mut reader, 1).await?;
    let load_session = init_resp
        .pointer("/result/agentCapabilities/loadSession")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let (session_id, loaded) = match resume_session_id {
        Some(resume_id) if load_session => {
            let load_req = json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "session/load",
                "params": {
                    "sessionId": resume_id,
                    "mcpServers": [],
                },
            });
            write_line(&mut stdin, &load_req).await?;
            match read_response_line(&mut reader, 2).await {
                Ok(_) => (resume_id.to_string(), true),
                Err(_) => {
                    let sid = session_new(&mut stdin, &mut reader, 3, worktree).await?;
                    (sid, false)
                }
            }
        }
        Some(_) | None => {
            let sid = session_new(&mut stdin, &mut reader, 2, worktree).await?;
            (sid, false)
        }
    };

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(acp_io_loop(
        stdin,
        reader,
        session_id.clone(),
        cmd_rx,
        event_tx,
    ));

    Ok(SpawnResult {
        session: AcpSession {
            session_id,
            _child: child,
            cmd_tx,
        },
        loaded,
    })
}

impl AcpSession {
    pub fn send(&self, command: AcpCommand) -> Result<(), String> {
        self.cmd_tx
            .send(command)
            .map_err(|_| "acp command channel closed".to_string())
    }
}

pub fn next_rpc_id() -> u64 {
    NEXT_RPC_ID.fetch_add(1, Ordering::Relaxed)
}

async fn acp_io_loop(
    mut stdin: tokio::process::ChildStdin,
    mut reader: BufReader<tokio::process::ChildStdout>,
    session_id: String,
    mut cmd_rx: mpsc::UnboundedReceiver<AcpCommand>,
    event_tx: mpsc::UnboundedSender<AcpEvent>,
) {
    let mut line = String::new();
    let mut pending_prompt: Option<u64> = None;

    loop {
        tokio::select! {
            command = cmd_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                match command {
                    AcpCommand::Prompt { id, text } => {
                        pending_prompt = Some(id);
                        let payload = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": "session/prompt",
                            "params": {
                                "sessionId": session_id,
                                "prompt": [{ "type": "text", "text": text }],
                            },
                        });
                        if write_line(&mut stdin, &payload).await.is_err() {
                            break;
                        }
                    }
                    AcpCommand::Cancel { id } => {
                        let payload = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": "session/cancel",
                            "params": {},
                        });
                        if write_line(&mut stdin, &payload).await.is_err() {
                            break;
                        }
                    }
                    AcpCommand::JsonRpcResponse { id, result } => {
                        let payload = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result,
                        });
                        if write_line(&mut stdin, &payload).await.is_err() {
                            break;
                        }
                    }
                }
            }
            read = reader.read_line(&mut line) => {
                match read {
                    Ok(0) | Err(_) => {
                        let _ = event_tx.send(AcpEvent::Exited);
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            line.clear();
                            continue;
                        }
                        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                            line.clear();
                            continue;
                        };
                        if let Some(event) = map_acp_to_event(&msg, pending_prompt) {
                            if matches!(event, AcpEvent::PromptFinished { .. }) {
                                pending_prompt = None;
                            }
                            if event_tx.send(event).is_err() {
                                break;
                            }
                        }
                        line.clear();
                    }
                }
            }
        }
    }
}

fn map_acp_to_event(msg: &Value, pending_prompt: Option<u64>) -> Option<AcpEvent> {
    if msg.get("method") == Some(&json!("session/update")) {
        let update = msg.pointer("/params/update")?;
        if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
            return Some(AcpEvent::AgentMessage(text.to_string()));
        }
        let session_update = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if session_update == "tool_call" {
            let mut frame = json!({ "type": "tool_call" });
            if let Some(obj) = frame.as_object_mut() {
                if let Some(update_obj) = update.as_object() {
                    for (key, value) in update_obj {
                        if key != "sessionUpdate" {
                            obj.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            return Some(AcpEvent::WireEvent(frame));
        }
        return Some(AcpEvent::WireEvent(json!({
            "type": "artifact",
            "kind": session_update,
        })));
    }
    if msg.get("method") == Some(&json!("session/request_permission")) {
        let jsonrpc_id = msg.get("id").and_then(Value::as_u64)?;
        let params = msg.get("params")?;
        let request_id = params
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Some(AcpEvent::PermissionRequest {
            jsonrpc_id,
            request_id,
            title: params
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string),
            detail: params
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    if msg.get("result").is_some() {
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            if pending_prompt == Some(id) {
                let stop_reason = msg
                    .pointer("/result/stopReason")
                    .and_then(Value::as_str)
                    .unwrap_or("end_turn")
                    .to_string();
                return Some(AcpEvent::PromptFinished {
                    _id: id,
                    stop_reason,
                });
            }
        }
    }
    None
}

async fn session_new(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    id: u64,
    worktree: &Path,
) -> Result<String, String> {
    let session_new = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/new",
        "params": {
            "mcpServers": [],
            "cwd": worktree.display().to_string(),
        },
    });
    write_line(stdin, &session_new).await?;
    let session_resp = read_response_line(reader, id).await?;
    session_resp
        .get("result")
        .and_then(|result| result.get("sessionId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "session/new missing sessionId".to_string())
}

async fn write_line(stdin: &mut tokio::process::ChildStdin, value: &Value) -> Result<(), String> {
    let mut payload = serde_json::to_string(value).map_err(|error| error.to_string())?;
    payload.push('\n');
    stdin
        .write_all(payload.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stdin.flush().await.map_err(|error| error.to_string())
}

async fn read_response_line(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    expected_id: u64,
) -> Result<Value, String> {
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("acp stdout eof during handshake".to_string());
        }
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = serde_json::from_str(line.trim()).map_err(|error| error.to_string())?;
        if msg.get("id") == Some(&json!(expected_id)) {
            if msg.get("error").is_some() {
                return Err(format!("json-rpc error: {msg}"));
            }
            return Ok(msg);
        }
    }
}
