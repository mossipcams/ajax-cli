//! Authenticated orchestration-chat WebSocket bridge over the ACP host.

use super::hub::WebSessionHub;
use crate::slices::web_session::{
    normalize_session_model, SessionAttachPlan, SessionClientMessage, SessionServerEvent,
};
use axum::extract::ws::{Message, WebSocket};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;
const MAX_SESSION_FRAME_BYTES: usize = 4096;

pub async fn bridge_task_session_socket(
    mut socket: WebSocket,
    hub: Arc<WebSessionHub>,
    plan: SessionAttachPlan,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    if let Err(error) = hub.acquire(&plan.qualified_handle, &plan.worktree_path, &model) {
        let _ = send_event(
            &mut socket,
            &SessionServerEvent::Error {
                message: error.clone(),
            },
        )
        .await;
        return;
    }

    let mut generation = hub.generation(&handle);
    if !send_event(
        &mut socket,
        &SessionServerEvent::Ready {
            model: hub.model(&handle).unwrap_or_else(|| model.clone()),
        },
    )
    .await
    {
        hub.release(&handle);
        return;
    }

    let mut cursor = 0usize;

    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        match handle_inbound_text(
                            &mut socket,
                            &hub,
                            &handle,
                            &plan.worktree_path,
                            &text,
                            &mut generation,
                        )
                        .await
                        {
                            // Inbound prompt/cancel/permission: flush log events now
                            // instead of waiting for the 50ms poll tick.
                            ClientHandleResult::Continue => {
                                if !flush_outbound(
                                    &mut socket,
                                    &hub,
                                    &handle,
                                    &mut cursor,
                                    &mut generation,
                                )
                                .await
                                {
                                    hub.release(&handle);
                                    return;
                                }
                            }
                            ClientHandleResult::Stop => {
                                hub.release(&handle);
                                return;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = sleep(Duration::from_millis(EVENT_POLL_MS)) => {
                if !flush_outbound(
                    &mut socket,
                    &hub,
                    &handle,
                    &mut cursor,
                    &mut generation,
                )
                .await
                {
                    hub.release(&handle);
                    return;
                }
            }
        }
    }

    hub.release(&handle);
}

enum ClientHandleResult {
    Continue,
    Stop,
}

async fn handle_inbound_text(
    socket: &mut WebSocket,
    hub: &WebSessionHub,
    handle: &str,
    worktree_path: &Path,
    text: &str,
    generation: &mut u64,
) -> ClientHandleResult {
    if text.len() > MAX_SESSION_FRAME_BYTES {
        let ok = send_event(
            socket,
            &SessionServerEvent::Error {
                message: "input frame too large".to_string(),
            },
        )
        .await;
        return if ok {
            ClientHandleResult::Continue
        } else {
            ClientHandleResult::Stop
        };
    }

    let message: SessionClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            let ok = send_event(
                socket,
                &SessionServerEvent::Error {
                    message: format!("invalid session message: {error}"),
                },
            )
            .await;
            return if ok {
                ClientHandleResult::Continue
            } else {
                ClientHandleResult::Stop
            };
        }
    };

    match apply_client_message(hub, handle, worktree_path, message, generation) {
        Ok(ApplyClientMessageOutcome::Applied) => ClientHandleResult::Continue,
        Ok(ApplyClientMessageOutcome::ModelChanged { model }) => {
            let ok = send_event(socket, &SessionServerEvent::Ready { model }).await;
            if ok {
                ClientHandleResult::Continue
            } else {
                ClientHandleResult::Stop
            }
        }
        Err(error) => {
            let ok = send_event(socket, &SessionServerEvent::Error { message: error }).await;
            if ok {
                ClientHandleResult::Continue
            } else {
                ClientHandleResult::Stop
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum ApplyClientMessageOutcome {
    Applied,
    ModelChanged { model: String },
}

/// Map a validated client message to hub calls. Used by the bridge and unit tests.
pub(crate) fn apply_client_message(
    hub: &WebSessionHub,
    handle: &str,
    worktree_path: &Path,
    message: SessionClientMessage,
    generation: &mut u64,
) -> Result<ApplyClientMessageOutcome, String> {
    match message {
        SessionClientMessage::Prompt { text } => {
            hub.submit_prompt(handle, text)?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::Cancel { keep_queue } => {
            hub.cancel(handle, keep_queue)?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::SetModel { model } => {
            let model = normalize_session_model(&model)?;
            let next_generation = hub.respawn(handle, worktree_path, &model)?;
            *generation = next_generation;
            Ok(ApplyClientMessageOutcome::ModelChanged {
                model: hub.model(handle).unwrap_or(model),
            })
        }
        SessionClientMessage::Permission {
            request_id,
            approved,
            reason,
        } => {
            hub.answer_permission(handle, &request_id, approved, reason.as_deref())?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
    }
}

async fn flush_outbound(
    socket: &mut WebSocket,
    hub: &WebSessionHub,
    handle: &str,
    cursor: &mut usize,
    generation: &mut u64,
) -> bool {
    let current_generation = hub.generation(handle);
    if current_generation != *generation {
        *generation = current_generation;
        *cursor = 0;
        let model = hub.model(handle).unwrap_or_else(|| "auto".to_string());
        if !send_event(socket, &SessionServerEvent::Ready { model }).await {
            return false;
        }
    }

    hub.pump(handle);
    let (outbound, next) = hub.read_from(handle, *cursor);
    *cursor = next;
    for event in outbound {
        if !send_event(socket, &event).await {
            return false;
        }
    }
    true
}

async fn send_event(socket: &mut WebSocket, event: &SessionServerEvent) -> bool {
    match serde_json::to_string(event) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::web_session_acp::with_test_acp_program;
    use std::path::PathBuf;

    fn scratch_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ajax-web-bridge-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_acp_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
    }

    #[test]
    fn max_session_frame_bytes_is_4096() {
        assert_eq!(MAX_SESSION_FRAME_BYTES, 4096);
    }

    #[test]
    fn apply_client_message_rejects_invalid_model() {
        let hub = WebSessionHub::new(std::env::temp_dir());
        let mut generation = 0;
        let error = apply_client_message(
            &hub,
            "web/fix-login",
            Path::new("/tmp"),
            SessionClientMessage::SetModel {
                model: "bad model".to_string(),
            },
            &mut generation,
        )
        .unwrap_err();
        assert!(error.contains("whitespace"));
    }

    #[test]
    fn apply_client_message_prompt_records_user_message_immediately() {
        let dir = scratch_dir("prompt-flush");
        let handle = "web/prompt-flush";
        let hub = WebSessionHub::new(dir.clone());
        let script = fake_acp_fixture();

        with_test_acp_program(&script, || {
            hub.acquire(handle, &dir, "auto").expect("acquire");
            let mut generation = hub.generation(handle);
            apply_client_message(
                &hub,
                handle,
                &dir,
                SessionClientMessage::Prompt {
                    text: "hello".to_string(),
                },
                &mut generation,
            )
            .expect("prompt");

            let (events, _) = hub.read_from(handle, 0);
            assert!(events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text }
                        if role == "user" && text == "hello"
                )
            }));
        });

        let _ = std::fs::remove_dir_all(dir);
    }
}
