//! Authenticated Ajax Web Session WebSocket bridge over the process-local hub.

use super::{AgentAcpError, AgentAcpEvent};
use crate::adapters::web_session_hub::{HubClientEvent, HubSubscription, WebSessionHub};
use crate::slices::web_session::{
    WebSessionClientMessage, WebSessionServerEvent, WebSessionStatus, WEB_SESSION_PROTOCOL_VERSION,
};
use axum::extract::ws::{Message, WebSocket};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_CONTROL_BYTES: usize = 65_536;
const EVENT_POLL_MS: u64 = 20;

fn session_id_for(handle: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{handle}-{millis}")
}

async fn send_event(socket: &mut WebSocket, event: &WebSessionServerEvent) -> bool {
    match serde_json::to_string(event) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn send_error(socket: &mut WebSocket, code: &str, message: impl Into<String>) -> bool {
    send_event(
        socket,
        &WebSessionServerEvent::Error {
            version: WEB_SESSION_PROTOCOL_VERSION,
            code: code.to_string(),
            message: message.into(),
        },
    )
    .await
}

async fn send_status(socket: &mut WebSocket, state: WebSessionStatus) -> bool {
    send_event(
        socket,
        &WebSessionServerEvent::Status {
            version: WEB_SESSION_PROTOCOL_VERSION,
            state,
        },
    )
    .await
}

fn map_acp_event(event: AgentAcpEvent) -> Vec<WebSessionServerEvent> {
    match event {
        AgentAcpEvent::PromptStarted => vec![WebSessionServerEvent::Status {
            version: WEB_SESSION_PROTOCOL_VERSION,
            state: WebSessionStatus::Running,
        }],
        AgentAcpEvent::AssistantDelta { text } if !text.is_empty() => {
            vec![WebSessionServerEvent::AssistantDelta {
                version: WEB_SESSION_PROTOCOL_VERSION,
                text,
            }]
        }
        AgentAcpEvent::AssistantDelta { .. } => Vec::new(),
        AgentAcpEvent::AgentSettled => vec![
            WebSessionServerEvent::Settled {
                version: WEB_SESSION_PROTOCOL_VERSION,
            },
            WebSessionServerEvent::Status {
                version: WEB_SESSION_PROTOCOL_VERSION,
                state: WebSessionStatus::Waiting,
            },
        ],
        AgentAcpEvent::Error { message } => vec![WebSessionServerEvent::Error {
            version: WEB_SESSION_PROTOCOL_VERSION,
            code: "provider_error".to_string(),
            message,
        }],
        AgentAcpEvent::Exited => vec![WebSessionServerEvent::Closed {
            version: WEB_SESSION_PROTOCOL_VERSION,
        }],
        AgentAcpEvent::OperatorRequest { .. } => Vec::new(),
    }
}

pub(crate) async fn bridge_task_web_session_socket(
    mut socket: WebSocket,
    handle: String,
    worktree: PathBuf,
    hub: Arc<WebSessionHub>,
) {
    let display_session_id = session_id_for(&handle);
    let hub_for_attach = Arc::clone(&hub);
    let handle_for_attach = handle.clone();
    let attach_result =
        tokio::task::spawn_blocking(move || hub_for_attach.attach(&handle_for_attach, worktree))
            .await;

    let subscription = match attach_result {
        Ok(Ok(subscription)) => subscription,
        Ok(Err(error)) => {
            let code = match &error {
                AgentAcpError::StartupFailed(_) => "provider_startup_failed",
                AgentAcpError::HandshakeFailed(_) => "provider_handshake_failed",
                _ => "provider_error",
            };
            let _ = send_error(&mut socket, code, error.to_string()).await;
            let _ = send_event(
                &mut socket,
                &WebSessionServerEvent::Closed {
                    version: WEB_SESSION_PROTOCOL_VERSION,
                },
            )
            .await;
            return;
        }
        Err(error) => {
            let _ = send_error(
                &mut socket,
                "provider_handshake_failed",
                format!("attach worker failed: {error}"),
            )
            .await;
            let _ = send_event(
                &mut socket,
                &WebSessionServerEvent::Closed {
                    version: WEB_SESSION_PROTOCOL_VERSION,
                },
            )
            .await;
            return;
        }
    };

    if !send_event(
        &mut socket,
        &WebSessionServerEvent::Ready {
            version: WEB_SESSION_PROTOCOL_VERSION,
            session_id: display_session_id,
        },
    )
    .await
    {
        return;
    }
    if !send_status(&mut socket, WebSessionStatus::Waiting).await {
        return;
    }

    // Replay any pending operator requests already parked on this hub.
    for event in hub.pending_snapshot(&handle) {
        if !send_event(&mut socket, &event).await {
            return;
        }
    }

    run_bridge_loop(&mut socket, &handle, &hub, &subscription).await;

    let _ = send_event(
        &mut socket,
        &WebSessionServerEvent::Closed {
            version: WEB_SESSION_PROTOCOL_VERSION,
        },
    )
    .await;
}

async fn run_bridge_loop(
    socket: &mut WebSocket,
    handle: &str,
    hub: &Arc<WebSessionHub>,
    subscription: &HubSubscription,
) {
    loop {
        hub.poll_peer_into_subscribers(handle);
        if drain_hub_events(socket, handle, hub, subscription)
            .await
            .is_none()
        {
            return;
        }

        let received =
            tokio::time::timeout(Duration::from_millis(EVENT_POLL_MS), socket.recv()).await;
        let message = match received {
            Err(_) => continue,
            Ok(None) | Ok(Some(Err(_))) => break,
            Ok(Some(Ok(Message::Close(_)))) => break,
            Ok(Some(Ok(Message::Ping(payload)))) => {
                if socket.send(Message::Pong(payload)).await.is_err() {
                    break;
                }
                continue;
            }
            Ok(Some(Ok(Message::Pong(_)))) => continue,
            Ok(Some(Ok(Message::Binary(_)))) => {
                let _ =
                    send_error(socket, "invalid_control", "binary frames are not supported").await;
                continue;
            }
            Ok(Some(Ok(Message::Text(text)))) => text,
        };

        if message.len() > MAX_CONTROL_BYTES {
            let _ = send_error(
                socket,
                "control_too_large",
                "session control frame too large",
            )
            .await;
            break;
        }

        let control = match serde_json::from_str::<WebSessionClientMessage>(&message) {
            Ok(control) => control,
            Err(_) => {
                let _ =
                    send_error(socket, "invalid_control", "malformed session control frame").await;
                continue;
            }
        };

        if !handle_client_control(socket, handle, hub, control).await {
            return;
        }
    }
}

async fn drain_hub_events(
    socket: &mut WebSocket,
    handle: &str,
    hub: &Arc<WebSessionHub>,
    subscription: &HubSubscription,
) -> Option<()> {
    while let Some(event) = WebSessionHub::try_recv(subscription) {
        match event {
            HubClientEvent::Local(AgentAcpEvent::Exited) => {
                let _ = send_event(
                    socket,
                    &WebSessionServerEvent::Closed {
                        version: WEB_SESSION_PROTOCOL_VERSION,
                    },
                )
                .await;
                return None;
            }
            HubClientEvent::Local(local) => {
                if let AgentAcpEvent::Error { message } = &local {
                    let failed = WebSessionServerEvent::AttentionRequired {
                        version: WEB_SESSION_PROTOCOL_VERSION,
                        handle: handle.to_string(),
                        request_id: format!(
                            "failed-{}",
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|duration| duration.as_millis())
                                .unwrap_or(0)
                        ),
                        kind: crate::slices::web_session::AttentionKind::Failed,
                        title: "Session failed".to_string(),
                        summary: message.clone(),
                        options: Some(vec!["stop".to_string(), "retry".to_string()]),
                    };
                    hub.publish_attention(failed);
                }
                for server_event in map_acp_event(local) {
                    if matches!(server_event, WebSessionServerEvent::Closed { .. }) {
                        let _ = send_event(socket, &server_event).await;
                        return None;
                    }
                    if !send_event(socket, &server_event).await {
                        return None;
                    }
                }
            }
            HubClientEvent::Attention(attention) => {
                if !send_event(socket, &attention).await {
                    return None;
                }
            }
        }
    }
    Some(())
}

async fn handle_client_control(
    socket: &mut WebSocket,
    handle: &str,
    hub: &Arc<WebSessionHub>,
    control: WebSessionClientMessage,
) -> bool {
    match control {
        WebSessionClientMessage::Prompt { version, message } => {
            if version != WEB_SESSION_PROTOCOL_VERSION {
                let _ = send_error(
                    socket,
                    "version_mismatch",
                    format!("unsupported session protocol version {version}"),
                )
                .await;
                return true;
            }
            if let Err(error) = hub.send_prompt(handle, &message) {
                let _ = send_error(socket, "provider_write_failed", error.to_string()).await;
                return false;
            }
            send_status(socket, WebSessionStatus::Running).await
        }
        WebSessionClientMessage::Abort { version } => {
            if version != WEB_SESSION_PROTOCOL_VERSION {
                let _ = send_error(
                    socket,
                    "version_mismatch",
                    format!("unsupported session protocol version {version}"),
                )
                .await;
                return true;
            }
            let _ = hub.send_cancel(handle);
            true
        }
        WebSessionClientMessage::AttentionRespond {
            version,
            target_handle,
            request_id,
            response,
        } => {
            if version != WEB_SESSION_PROTOCOL_VERSION {
                let _ = send_error(
                    socket,
                    "version_mismatch",
                    format!("unsupported session protocol version {version}"),
                )
                .await;
                return true;
            }
            match hub.respond_attention(&target_handle, &request_id, response) {
                Ok(_) => true,
                Err(error) => {
                    let _ = send_event(
                        socket,
                        &WebSessionServerEvent::AttentionError {
                            version: WEB_SESSION_PROTOCOL_VERSION,
                            handle: target_handle,
                            request_id,
                            code: error.code().to_string(),
                            message: error.message(),
                        },
                    )
                    .await;
                    true
                }
            }
        }
    }
}
