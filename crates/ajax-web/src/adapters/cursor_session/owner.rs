//! Per-handle session owner: queue, ACP serialization, transcript, permissions.

use super::{acp, transcript};
use acp::{AcpCommand, AcpEvent, AcpSession};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, mpsc, Mutex};

const QUEUE_CAP: usize = 8;

pub struct SessionSlot {
    pub model: Arc<Mutex<String>>,
    cmd_tx: mpsc::UnboundedSender<OwnerCommand>,
    pub event_tx: broadcast::Sender<Value>,
    pub transcript: Arc<Mutex<Vec<Value>>>,
}

enum OwnerCommand {
    Prompt(String),
    Cancel { keep_queue: bool },
    SetModel(String),
    Permission { request_id: String, approved: bool },
}

struct PendingPermission {
    jsonrpc_id: u64,
    request_id: String,
    approved: Option<bool>,
}

struct OwnerState {
    worktree: PathBuf,
    state_dir: PathBuf,
    qualified_handle: String,
    model: String,
    model_cell: Arc<Mutex<String>>,
    acp: AcpSession,
    acp_dead: bool,
    acp_event_tx: mpsc::UnboundedSender<AcpEvent>,
    queue: VecDeque<String>,
    busy: bool,
    pending_permission: Option<PendingPermission>,
    transcript: Arc<Mutex<Vec<Value>>>,
    event_tx: broadcast::Sender<Value>,
}

#[allow(clippy::too_many_arguments)] // ponytail: spawn snapshot; one caller in SessionHost
pub fn start_owner(
    worktree: PathBuf,
    state_dir: PathBuf,
    qualified_handle: String,
    model: String,
    acp: AcpSession,
    acp_event_tx: mpsc::UnboundedSender<AcpEvent>,
    acp_event_rx: mpsc::UnboundedReceiver<AcpEvent>,
    event_tx: broadcast::Sender<Value>,
    transcript: Arc<Mutex<Vec<Value>>>,
) -> SessionSlot {
    let model_cell = Arc::new(Mutex::new(model.clone()));
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(owner_loop(
        OwnerState {
            worktree,
            state_dir,
            qualified_handle,
            model: model.clone(),
            model_cell: Arc::clone(&model_cell),
            acp,
            acp_dead: false,
            acp_event_tx,
            queue: VecDeque::new(),
            busy: false,
            pending_permission: None,
            transcript: Arc::clone(&transcript),
            event_tx: event_tx.clone(),
        },
        cmd_rx,
        acp_event_rx,
    ));
    SessionSlot {
        model: model_cell,
        cmd_tx,
        event_tx,
        transcript,
    }
}

impl SessionSlot {
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.event_tx.subscribe()
    }

    pub fn send_prompt(&self, text: String) {
        let _ = self.cmd_tx.send(OwnerCommand::Prompt(text));
    }

    pub fn send_cancel(&self, keep_queue: bool) {
        let _ = self.cmd_tx.send(OwnerCommand::Cancel { keep_queue });
    }

    pub fn send_set_model(&self, model: String) {
        let _ = self.cmd_tx.send(OwnerCommand::SetModel(model));
    }

    pub fn send_permission(&self, request_id: String, approved: bool) {
        let _ = self.cmd_tx.send(OwnerCommand::Permission {
            request_id,
            approved,
        });
    }
}

async fn owner_loop(
    mut state: OwnerState,
    mut cmd_rx: mpsc::UnboundedReceiver<OwnerCommand>,
    mut acp_event_rx: mpsc::UnboundedReceiver<AcpEvent>,
) {
    loop {
        tokio::select! {
            command = cmd_rx.recv() => {
                let Some(command) = command else {
                    break;
                };
                match command {
                    OwnerCommand::Prompt(text) => enqueue_or_run(&mut state, text).await,
                    OwnerCommand::Cancel { keep_queue } => cancel_turn(&mut state, keep_queue).await,
                    OwnerCommand::SetModel(model) => switch_model(&mut state, model).await,
                    OwnerCommand::Permission { request_id, approved } => {
                        resolve_permission(&mut state, request_id, approved).await;
                    }
                }
            }
            event = acp_event_rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                handle_acp_event(&mut state, event).await;
            }
        }
    }
}

async fn enqueue_or_run(state: &mut OwnerState, text: String) {
    if state.busy {
        // ponytail: silent 8-cap is observed; docs omitted it. Upgrade: surface drop/error event.
        if state.queue.len() == QUEUE_CAP {
            state.queue.pop_front();
        }
        state.queue.push_back(text);
        return;
    }
    run_prompt(state, text).await;
}

async fn run_prompt(state: &mut OwnerState, text: String) {
    if !ensure_acp_alive(state).await {
        return;
    }
    let user = json!({ "type": "message", "role": "user", "text": text });
    record_and_broadcast(state, user).await;
    let id = acp::next_rpc_id();
    state.busy = true;
    let _ = state.acp.send(AcpCommand::Prompt { id, text });
}

async fn ensure_acp_alive(state: &mut OwnerState) -> bool {
    if !state.acp_dead {
        return true;
    }
    let spawn = match acp::spawn_acp_session(
        &state.worktree,
        &state.model,
        None,
        state.acp_event_tx.clone(),
    )
    .await
    {
        Ok(spawn) => spawn,
        Err(_) => return false,
    };
    let _ = transcript::append_session_meta(
        &state.state_dir,
        &state.qualified_handle,
        &spawn.session.session_id,
        &state.model,
    );
    state.acp = spawn.session;
    state.acp_dead = false;
    true
}

async fn cancel_turn(state: &mut OwnerState, keep_queue: bool) {
    if !keep_queue {
        state.queue.clear();
    }
    if state.busy {
        let id = acp::next_rpc_id();
        let _ = state.acp.send(AcpCommand::Cancel { id });
    }
}

async fn switch_model(state: &mut OwnerState, new_model: String) {
    if state.model == new_model {
        return;
    }
    if state.busy {
        let id = acp::next_rpc_id();
        let _ = state.acp.send(AcpCommand::Cancel { id });
        state.busy = false;
    }
    state.queue.clear();
    state.pending_permission = None;

    let spawn = match acp::spawn_acp_session(
        &state.worktree,
        &new_model,
        None,
        state.acp_event_tx.clone(),
    )
    .await
    {
        Ok(spawn) => spawn,
        Err(_) => return,
    };
    let acp = spawn.session;
    let _ = transcript::append_session_meta(
        &state.state_dir,
        &state.qualified_handle,
        &acp.session_id,
        &new_model,
    );
    state.model = new_model.clone();
    *state.model_cell.lock().await = new_model.clone();
    state.acp = acp;
    state.acp_dead = false;

    let ready = json!({ "type": "ready", "model": new_model });
    record_and_broadcast(state, ready).await;

    let replay = state.transcript.lock().await.clone();
    for event in replay {
        if event.get("type") == Some(&json!("ready")) {
            continue;
        }
        let _ = state.event_tx.send(event);
    }
}

async fn resolve_permission(state: &mut OwnerState, request_id: String, approved: bool) {
    let Some(pending) = state.pending_permission.as_mut() else {
        return;
    };
    if pending.request_id != request_id {
        return;
    }
    pending.approved = Some(approved);
    let jsonrpc_id = pending.jsonrpc_id;
    let _ = state.acp.send(AcpCommand::JsonRpcResponse {
        id: jsonrpc_id,
        result: json!({ "approved": approved }),
    });
}

async fn handle_acp_event(state: &mut OwnerState, event: AcpEvent) {
    match event {
        AcpEvent::AgentMessage(text) => {
            let message = json!({ "type": "message", "role": "agent", "text": text });
            record_and_broadcast(state, message).await;
        }
        AcpEvent::WireEvent(frame) => {
            record_and_broadcast(state, frame).await;
        }
        AcpEvent::Exited => {
            state.busy = false;
            state.acp_dead = true;
            let _ = state.event_tx.send(json!({
                "type": "error",
                "message": "ACP process exited",
            }));
        }
        AcpEvent::PermissionRequest {
            jsonrpc_id,
            request_id,
            title,
            detail,
        } => {
            state.pending_permission = Some(PendingPermission {
                jsonrpc_id,
                request_id: request_id.clone(),
                approved: None,
            });
            let mut frame = json!({ "type": "permission_request", "requestId": request_id });
            if let Some(title) = title {
                frame["title"] = json!(title);
            }
            if let Some(detail) = detail {
                frame["detail"] = json!(detail);
            }
            record_and_broadcast(state, frame).await;
        }
        AcpEvent::PromptFinished { stop_reason, .. } => {
            state.busy = false;
            if let Some(pending) = state.pending_permission.take() {
                if let Some(approved) = pending.approved {
                    let resolved = json!({
                        "type": "permission_resolved",
                        "requestId": pending.request_id,
                        "approved": approved,
                    });
                    record_and_broadcast(state, resolved).await;
                }
            }
            let turn_end = json!({ "type": "turn_end", "stopReason": stop_reason });
            record_and_broadcast(state, turn_end).await;
            drain_queue(state).await;
        }
    }
}

async fn drain_queue(state: &mut OwnerState) {
    while !state.busy {
        let Some(next) = state.queue.pop_front() else {
            break;
        };
        run_prompt(state, next).await;
    }
}

async fn record_and_broadcast(state: &mut OwnerState, event: Value) {
    state.transcript.lock().await.push(event.clone());
    let _ = transcript::append_event(&state.state_dir, &state.qualified_handle, &event);
    let _ = state.event_tx.send(event);
}
