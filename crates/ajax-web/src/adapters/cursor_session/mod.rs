//! Cursor orchestration-chat ACP attach, per-handle idle slots, and catalog.

mod acp;
mod models;
mod owner;
mod transcript;

pub use models::{list_cursor_models_sync, parse_models_stdout, CursorModel};

use axum::extract::ws::{Message, WebSocket};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{broadcast, Mutex};

use crate::ports::web_session::SessionAttachPlan;

use owner::SessionSlot;

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

pub struct SessionHost {
    // ponytail: one mutex held across first spawn; per-handle lock if map contention shows up.
    slots: Mutex<HashMap<String, Arc<SessionSlot>>>,
}

impl SessionHost {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    async fn get_or_create_slot(
        &self,
        plan: &SessionAttachPlan,
        model: &str,
        state_dir: &Path,
    ) -> Result<Arc<SessionSlot>, String> {
        let mut slots = self.slots.lock().await;
        if let Some(slot) = slots.get(&plan.qualified_handle).cloned() {
            return Ok(slot);
        }

        let loaded_disk = transcript::load_transcript(state_dir, &plan.qualified_handle);
        let spawn_model = loaded_disk.model.as_deref().unwrap_or(model);
        let resume_id = loaded_disk.acp_session_id.as_deref();

        let (acp_event_tx, acp_event_rx) = tokio::sync::mpsc::unbounded_channel();
        let spawn =
            acp::spawn_acp_session(&plan.worktree, spawn_model, resume_id, acp_event_tx.clone())
                .await?;
        transcript::append_session_meta(
            state_dir,
            &plan.qualified_handle,
            &spawn.session.session_id,
            spawn_model,
        )?;

        let mut transcript_events = loaded_disk.events;
        if !transcript_events.is_empty() && !spawn.loaded {
            let note = json!({
                "type": "message",
                "role": "agent",
                "text": CONTEXT_RESET_NOTE,
            });
            transcript_events.push(note.clone());
            transcript::append_event(state_dir, &plan.qualified_handle, &note)?;
        }

        let (event_tx, _) = broadcast::channel(64);
        let transcript = Arc::new(Mutex::new(transcript_events));
        let slot = Arc::new(owner::start_owner(
            plan.worktree.clone(),
            state_dir.to_path_buf(),
            plan.qualified_handle.clone(),
            spawn_model.to_string(),
            spawn.session,
            acp_event_tx,
            acp_event_rx,
            event_tx,
            transcript,
        ));

        slots.insert(plan.qualified_handle.clone(), Arc::clone(&slot));
        Ok(slot)
    }
}

impl Default for SessionHost {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn list_cursor_models() -> Result<Vec<CursorModel>, String> {
    tokio::task::spawn_blocking(list_cursor_models_sync)
        .await
        .map_err(|error| format!("models worker failed: {error}"))?
}

pub async fn attach_session_socket(
    mut socket: WebSocket,
    plan: SessionAttachPlan,
    model: String,
    state_dir: Arc<PathBuf>,
    host: Arc<SessionHost>,
) {
    let slot = match host
        .get_or_create_slot(&plan, &model, state_dir.as_ref())
        .await
    {
        Ok(slot) => slot,
        Err(_) => {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };

    let mut events = slot.subscribe();
    let replay = slot.transcript.lock().await.clone();

    let ready_model = slot.model.lock().await.clone();
    if !send_json(
        &mut socket,
        json!({ "type": "ready", "model": ready_model }),
    )
    .await
    {
        return;
    }

    for event in replay {
        if !send_json(&mut socket, event).await {
            return;
        }
    }

    idle_socket_loop(&mut socket, slot, &mut events).await;
}

async fn send_json(socket: &mut WebSocket, value: Value) -> bool {
    match serde_json::to_string(&value) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn idle_socket_loop(
    socket: &mut WebSocket,
    slot: Arc<SessionSlot>,
    events: &mut broadcast::Receiver<Value>,
) {
    loop {
        tokio::select! {
            ws = socket.recv() => {
                match ws {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<Value>(text.as_ref()) {
                            match frame.get("type").and_then(Value::as_str) {
                                Some("prompt") => {
                                    if let Some(text) = frame.get("text").and_then(Value::as_str) {
                                        slot.send_prompt(text.to_string());
                                    }
                                }
                                Some("cancel") => {
                                    let keep_queue = frame
                                        .get("keepQueue")
                                        .and_then(Value::as_bool)
                                        .unwrap_or(false);
                                    slot.send_cancel(keep_queue);
                                }
                                Some("set_model") => {
                                    if let Some(model) = frame.get("model").and_then(Value::as_str) {
                                        slot.send_set_model(model.to_string());
                                    }
                                }
                                Some("permission") => {
                                    if let (Some(request_id), Some(approved)) = (
                                        frame.get("requestId").and_then(Value::as_str),
                                        frame.get("approved").and_then(Value::as_bool),
                                    ) {
                                        slot.send_permission(request_id.to_string(), approved);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            event = events.recv() => {
                match event {
                    Ok(value) => {
                        if !send_json(socket, value).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
