//! Authenticated orchestration-chat WebSocket bridge over the ACP host.

use super::hub::{drain_acp_events, permission_response, WebSessionHub};
use crate::slices::web_session::SessionAttachPlan;
use crate::slices::web_session::{SessionClientMessage, SessionServerEvent};
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
    let client = match hub.acquire(&plan.qualified_handle, &plan.worktree_path) {
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

    if !send_event(&mut socket, &SessionServerEvent::Ready).await {
        hub.release(&handle);
        return;
    }

    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        if !handle_client_message(&mut socket, &client, &text).await {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = sleep(Duration::from_millis(EVENT_POLL_MS)) => {
                let outbound = {
                    let guard = client.lock().unwrap();
                    drain_acp_events(&guard)
                };
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

async fn handle_client_message(
    socket: &mut WebSocket,
    client: &std::sync::Mutex<super::client::AcpStdioClient>,
    text: &str,
) -> bool {
    let message: SessionClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            return send_event(
                socket,
                &SessionServerEvent::Error {
                    message: format!("invalid session message: {error}"),
                },
            )
            .await;
        }
    };

    match message {
        SessionClientMessage::Prompt { text } => {
            let result = {
                let mut guard = client.lock().unwrap();
                guard.begin_prompt(&text)
            };
            if let Err(error) = result {
                return send_event(socket, &SessionServerEvent::Error { message: error }).await;
            }
            true
        }
        SessionClientMessage::Cancel => {
            let result = {
                let mut guard = client.lock().unwrap();
                guard.begin_cancel()
            };
            if let Err(error) = result {
                return send_event(socket, &SessionServerEvent::Error { message: error }).await;
            }
            true
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
                return send_event(socket, &SessionServerEvent::Error { message: error }).await;
            }
            true
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
