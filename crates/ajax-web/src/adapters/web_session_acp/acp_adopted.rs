use std::collections::HashSet;
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use agent_client_protocol::schema::v1::{SessionInfoUpdate, SessionNotification, SessionUpdate};
use serde_json::{json, Value};

use super::client::AcpClientEvent;
use super::config_options::read_model_applied;

const PROMPT_METHOD: &str = "session/prompt";
const SESSION_UPDATE_METHOD: &str = "session/update";
const SHUTDOWN_LINE: &[u8] = b"SHUTDOWN\n";

pub(super) fn session_update_event(notification: SessionNotification) -> Option<AcpClientEvent> {
    match notification.update {
        SessionUpdate::ConfigOptionUpdate(update) => Some(AcpClientEvent::ConfigOptionsUpdated {
            applied_model: read_model_applied(Some(&update.config_options)).unwrap_or_default(),
            config_options: update.config_options,
        }),
        SessionUpdate::AvailableCommandsUpdate(update) => {
            Some(AcpClientEvent::AvailableCommandsUpdated {
                available_commands: update.available_commands,
            })
        }
        SessionUpdate::SessionInfoUpdate(update) => session_title_from_update(&update)
            .map(|title| AcpClientEvent::SessionInfoUpdated { title }),
        _ => Some(AcpClientEvent::SessionUpdate(Box::new(notification))),
    }
}

fn session_title_from_update(update: &SessionInfoUpdate) -> Option<Option<String>> {
    match update.title.as_opt_ref() {
        None => None,
        Some(None) => Some(None),
        Some(Some(title)) => {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                Some(None)
            } else {
                Some(Some(trimmed.to_string()))
            }
        }
    }
}

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct AcpAdoptedClient {
    events: Receiver<AcpClientEvent>,
    writer: SharedWriter,
    shim_pid: u32,
    session_id: String,
    session_new_result: Value,
    turn_done: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}

struct ReplayState {
    events: Sender<AcpClientEvent>,
    writer: SharedWriter,
    prompt_request_id: u64,
    answered: HashSet<String>,
    turn_done: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}

impl ReplayState {
    fn send(&self, event: AcpClientEvent) {
        let _ = self.events.send(event);
    }

    fn finish_turn(&self, result: Result<Value, String>) {
        self.turn_done.store(true, Ordering::Release);
        self.send(AcpClientEvent::RequestFinished {
            id: self.prompt_request_id,
            method: PROMPT_METHOD,
            result,
        });
    }

    fn exit(&self) {
        self.exited.store(true, Ordering::Release);
        self.send(AcpClientEvent::Exited);
    }

    fn abort_on_agent_request(&self, method: &str) {
        write_shutdown(&self.writer);
        self.send(AcpClientEvent::Error(format!(
            "reconnected agent requested input ({method}); the adopted turn cannot answer it"
        )));
        if !self.turn_done.load(Ordering::Acquire) {
            self.finish_turn(Ok(json!({ "stopReason": "cancelled" })));
        }
        self.exit();
    }
}

fn write_shutdown(writer: &SharedWriter) {
    if let Ok(mut writer) = writer.lock() {
        let _ = writer.write_all(SHUTDOWN_LINE);
        let _ = writer.flush();
    }
}

impl AcpAdoptedClient {
    pub fn adopt(
        reader: Box<dyn Read + Send>,
        writer: Box<dyn Write + Send>,
        shim_pid: u32,
        session_id: String,
        prompt_request_id: u64,
        answered: HashSet<String>,
    ) -> Self {
        let (events_tx, events) = mpsc::channel();
        let turn_done = Arc::new(AtomicBool::new(false));
        let exited = Arc::new(AtomicBool::new(false));
        let writer: SharedWriter = Arc::new(Mutex::new(writer));
        let state = ReplayState {
            events: events_tx,
            writer: Arc::clone(&writer),
            prompt_request_id,
            answered,
            turn_done: Arc::clone(&turn_done),
            exited: Arc::clone(&exited),
        };
        thread::spawn(move || replay_lines(reader, &state));
        let session_new_result = serde_json::json!({ "sessionId": session_id });
        Self {
            events,
            writer,
            shim_pid,
            session_id,
            session_new_result,
            turn_done,
            exited,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn session_new_result(&self) -> &Value {
        &self.session_new_result
    }

    pub fn child_pid(&self) -> u32 {
        self.shim_pid
    }

    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.events.try_recv().ok()
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn host_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn prompt_in_flight(&self) -> bool {
        !self.turn_done.load(Ordering::Acquire)
    }

    pub fn shutdown(&mut self) {
        write_shutdown(&self.writer);
        self.exited.store(true, Ordering::Release);
    }
}

fn replay_lines(mut reader: Box<dyn Read + Send>, state: &ReplayState) {
    let mut pending = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        pending.extend_from_slice(&chunk[..count]);
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            if !replay_line(&line, state) {
                return;
            }
        }
    }
    if !pending.is_empty() && !replay_line(&pending, state) {
        return;
    }
    state.exit();
}

fn replay_line(line: &[u8], state: &ReplayState) -> bool {
    let Ok(message) = serde_json::from_slice::<Value>(line) else {
        return true;
    };
    let method = message.get("method").and_then(Value::as_str);
    match (method, message.get("id")) {
        (Some(SESSION_UPDATE_METHOD), None) => {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let event = match serde_json::from_value::<SessionNotification>(params.clone()) {
                Ok(notification) => session_update_event(notification),
                Err(_) => Some(AcpClientEvent::UnknownSessionUpdate(params)),
            };
            if let Some(event) = event {
                state.send(event);
            }
            true
        }
        (Some(method), Some(id)) => {
            if state.answered.contains(&id.to_string()) {
                return true;
            }
            state.abort_on_agent_request(method);
            false
        }
        (None, Some(_)) => {
            if let Some(result) = message.get("result") {
                if result.get("stopReason").is_some() {
                    state.finish_turn(Ok(result.clone()));
                }
            } else if let Some(error) = message.get("error") {
                state.finish_turn(Err(rpc_error_text(error)));
            }
            true
        }
        _ => true,
    }
}

fn rpc_error_text(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| error.to_string(), str::to_owned)
}
