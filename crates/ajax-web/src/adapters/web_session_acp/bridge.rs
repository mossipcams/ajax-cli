//! Authenticated orchestration-chat WebSocket bridge over the ACP host.

use super::hub::{permission_response, WebSessionHub};
use crate::slices::web_session::{
    normalize_session_model, SessionAttachPlan, SessionClientMessage, SessionServerEvent,
};
use axum::extract::ws::{Message, WebSocket};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;

pub async fn bridge_task_session_socket(
    mut socket: WebSocket,
    hub: Arc<WebSessionHub>,
    plan: SessionAttachPlan,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    let client = match hub.acquire(&plan.qualified_handle, &plan.worktree_path, &model) {
        Ok(client) => client,
        Err(error) => {
            let _ = send_event(
                &mut socket,
                &SessionServerEvent::Error {
                    message: error.clone(),
                },
            )
            .await;
            hub.release(&handle);
            return;
        }
    };

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

    // This socket's position in the shared transcript.
    let mut cursor = 0usize;

    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        match handle_client_message(
                            &mut socket,
                            &client,
                            &hub,
                            &handle,
                            &plan.worktree_path,
                            &text,
                            &mut generation,
                        )
                        .await
                        {
                            ClientHandleResult::Continue => {}
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
                // Another socket may have respawned the ACP child; replay the
                // retained transcript so this tab recovers mid-chat.
                let current_generation = hub.generation(&handle);
                if current_generation != generation {
                    generation = current_generation;
                    cursor = 0;
                    let model = hub.model(&handle).unwrap_or_else(|| "auto".to_string());
                    if !send_event(&mut socket, &SessionServerEvent::Ready { model }).await {
                        hub.release(&handle);
                        return;
                    }
                }

                // Cursor starts at 0, so the first pass replays the whole
                // transcript — that is what makes a reload resume mid-turn and
                // lets a second device see the same conversation rather than
                // half of it.
                hub.pump(&handle);
                let (outbound, next) = hub.read_from(&handle, cursor);
                cursor = next;
                for event in outbound {
                    if !send_event(&mut socket, &event).await {
                        hub.release(&handle);
                        return;
                    }
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

async fn handle_client_message(
    socket: &mut WebSocket,
    client: &std::sync::Mutex<super::client::AcpStdioClient>,
    hub: &WebSessionHub,
    handle: &str,
    worktree_path: &std::path::Path,
    text: &str,
    generation: &mut u64,
) -> ClientHandleResult {
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

    match message {
        SessionClientMessage::Prompt { text } => {
            let result = {
                let mut guard = client.lock().unwrap();
                guard.begin_prompt(&text)
            };
            if let Err(error) = result {
                let ok = send_event(socket, &SessionServerEvent::Error { message: error }).await;
                return if ok {
                    ClientHandleResult::Continue
                } else {
                    ClientHandleResult::Stop
                };
            }
            // The transcript is the server's, so the operator's own turn is
            // recorded here rather than only in the sending browser.
            hub.record(
                handle,
                SessionServerEvent::Message {
                    role: "user".to_string(),
                    text,
                },
            );
            ClientHandleResult::Continue
        }
        SessionClientMessage::Cancel => {
            let result = {
                let mut guard = client.lock().unwrap();
                guard.begin_cancel()
            };
            if let Err(error) = result {
                let ok = send_event(socket, &SessionServerEvent::Error { message: error }).await;
                return if ok {
                    ClientHandleResult::Continue
                } else {
                    ClientHandleResult::Stop
                };
            }
            ClientHandleResult::Continue
        }
        SessionClientMessage::SetModel { model } => {
            let model = match normalize_session_model(&model) {
                Ok(model) => model,
                Err(error) => {
                    let ok =
                        send_event(socket, &SessionServerEvent::Error { message: error }).await;
                    return if ok {
                        ClientHandleResult::Continue
                    } else {
                        ClientHandleResult::Stop
                    };
                }
            };
            let result = hub.respawn(handle, worktree_path, &model);
            match result {
                Ok((_, next_generation)) => {
                    *generation = next_generation;
                    // Keep this socket's cursor: it already shows the transcript.
                    // Peer sockets notice the generation bump, reset to 0, and replay.
                    let ok = send_event(
                        socket,
                        &SessionServerEvent::Ready {
                            model: hub.model(handle).unwrap_or(model),
                        },
                    )
                    .await;
                    if ok {
                        ClientHandleResult::Continue
                    } else {
                        ClientHandleResult::Stop
                    }
                }
                Err(error) => {
                    let ok =
                        send_event(socket, &SessionServerEvent::Error { message: error }).await;
                    if ok {
                        ClientHandleResult::Continue
                    } else {
                        ClientHandleResult::Stop
                    }
                }
            }
        }
        SessionClientMessage::Permission {
            request_id,
            approved,
            reason,
        } => {
            let id = parse_json_rpc_id(&request_id);
            let result = {
                let mut guard = client.lock().unwrap();
                guard.respond_client_request(&id, permission_response(approved, reason.as_deref()))
            };
            if let Err(error) = result {
                let ok = send_event(socket, &SessionServerEvent::Error { message: error }).await;
                return if ok {
                    ClientHandleResult::Continue
                } else {
                    ClientHandleResult::Stop
                };
            }
            hub.record(
                handle,
                SessionServerEvent::PermissionResolved {
                    request_id,
                    approved,
                },
            );
            ClientHandleResult::Continue
        }
    }
}

fn parse_json_rpc_id(raw: &str) -> Value {
    if let Ok(n) = raw.parse::<u64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Value::Number(n.into());
    }
    Value::String(raw.to_string())
}

async fn send_event(socket: &mut WebSocket, event: &SessionServerEvent) -> bool {
    match serde_json::to_string(event) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}
