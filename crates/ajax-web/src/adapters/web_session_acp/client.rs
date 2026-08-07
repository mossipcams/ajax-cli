//! Minimal newline-delimited JSON-RPC stdio client for Cursor ACP.
//!
//! We implement this locally instead of pulling `agent-client-protocol` because
//! the published SDK targets a different async runtime shape than ajax-web's
//! tokio WebSocket bridge. This module covers initialize, session/new,
//! session/prompt, session/cancel, session/update notifications, and permission
//! requests from the agent.

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
    time::Duration,
};

type PendingResponses = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

#[derive(Debug, Clone)]
pub enum AcpClientEvent {
    SessionUpdate(Value),
    ClientRequest {
        id: Value,
        method: String,
        params: Value,
    },
    Error(String),
    Exited,
}

pub struct AcpStdioClient {
    stdin: ChildStdin,
    events: Receiver<AcpClientEvent>,
    next_id: u64,
    pending: PendingResponses,
    session_id: String,
    _child: Child,
    _reader: thread::JoinHandle<()>,
}

impl AcpStdioClient {
    pub fn spawn(worktree_path: &Path) -> Result<Self, String> {
        let mut child = spawn_cursor_acp_process(worktree_path)?;
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
            _child: child,
            _reader: reader,
        };
        client.initialize()?;
        client.session_id = client.session_new(worktree_path)?;
        Ok(client)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.events.try_recv().ok()
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn send_prompt(&mut self, text: &str) -> Result<(), String> {
        self.call(
            "session/prompt",
            json!({
                "sessionId": self.session_id,
                "prompt": [{ "type": "text", "text": text }],
            }),
        )?;
        Ok(())
    }

    pub fn cancel_prompt(&mut self) -> Result<(), String> {
        self.call("session/cancel", json!({ "sessionId": self.session_id }))?;
        Ok(())
    }

    pub fn respond_client_request(&mut self, id: &Value, result: Value) -> Result<(), String> {
        self.write_response(id, result)
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.call(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": { "name": "ajax-web", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        self.write_notification("notifications/initialized", json!({}))?;
        Ok(())
    }

    fn session_new(&mut self, worktree_path: &Path) -> Result<String, String> {
        let response = self.call(
            "session/new",
            json!({ "cwd": worktree_path.display().to_string() }),
        )?;
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
        self.pending.lock().unwrap().insert(id, tx);
        self.write_request(method, params, id)?;
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(message)) => Err(message),
            Err(_) => Err(format!("acp request timed out: {method}")),
        }
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

fn write_line(stdin: &mut ChildStdin, payload: &Value) -> Result<(), String> {
    let mut line = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .map_err(|error| format!("acp stdin write failed: {error}"))
}

pub(crate) fn cursor_acp_program_candidates() -> [(&'static str, &'static [&'static str]); 2] {
    [("agent", &["acp"]), ("cursor", &["agent", "acp"])]
}

fn spawn_cursor_acp_process(worktree_path: &Path) -> Result<Child, String> {
    let mut last_error = String::from("failed to spawn cursor acp process");
    for (program, args) in cursor_acp_program_candidates() {
        let mut command = Command::new(program);
        command.args(args.iter().copied());
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
            let responder = pending.lock().unwrap().remove(&id);
            if let Some(tx) = responder {
                if let Some(error) = value.get("error") {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("acp error")
                        .to_string();
                    let _ = tx.send(Err(message));
                } else {
                    let result = value.get("result").cloned().unwrap_or(Value::Null);
                    let _ = tx.send(Ok(result));
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
    fn cursor_acp_command_prefers_agent_binary() {
        let candidates = cursor_acp_program_candidates();
        assert_eq!(candidates[0].0, "agent");
        assert_eq!(candidates[0].1, &["acp"]);
        assert_eq!(candidates[1].0, "cursor");
    }
}
