//! Authenticated orchestration-chat WebSocket bridge over the ACP host.

use super::hub::WebSessionHub;
use crate::slices::web_session::{
    normalize_session_model, SessionAttachPlan, SessionClientMessage, SessionServerEvent,
};
use axum::extract::ws::{Message, WebSocket};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;
const MAX_SESSION_FRAME_BYTES: usize = 4096;

/// Idle keepalive. An idle chat writes nothing for minutes, and the browser
/// WebSocket API cannot send pings from JS — so if the server does not ping,
/// nothing on either end notices a half-open socket: the hub keeps the slot
/// held and the composer keeps accepting sends that go nowhere.
const SESSION_PING_INTERVAL: Duration = Duration::from_secs(20);

pub(crate) fn should_send_keepalive(since_last_write: Duration) -> bool {
    since_last_write >= SESSION_PING_INTERVAL
}

/// Every hub call is synchronous and can block for the length of an ACP
/// handshake or command timeout (`HANDSHAKE_TIMEOUT`), and several hold the
/// hub's session lock while they do. Run them all on the blocking pool: inline
/// on the socket's runtime worker, one wedged harness parks a thread per open
/// session and the server stops answering everything, health included. Guarded
/// by `axum_session_socket_does_not_block_health`.
async fn on_hub<T, F>(hub: &Arc<WebSessionHub>, work: F) -> T
where
    F: FnOnce(&WebSessionHub) -> T + Send + 'static,
    T: Send + 'static,
{
    let hub = Arc::clone(hub);
    tokio::task::spawn_blocking(move || work(&hub))
        .await
        .expect("web session hub task panicked")
}

async fn release_slot(hub: &Arc<WebSessionHub>, handle: &str) {
    let handle = handle.to_string();
    on_hub(hub, move |hub| hub.release(&handle)).await;
}

pub async fn bridge_task_session_socket(
    mut socket: WebSocket,
    hub: Arc<WebSessionHub>,
    plan: SessionAttachPlan,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    let acquired = {
        let handle = plan.qualified_handle.clone();
        let worktree_path = plan.worktree_path.clone();
        let model = model.clone();
        let agent = plan.agent;
        on_hub(&hub, move |hub| {
            hub.acquire(&handle, &worktree_path, &model, agent)
        })
        .await
    };
    if let Err(error) = acquired {
        let _ = send_event(
            &mut socket,
            &SessionServerEvent::Error {
                message: error.clone(),
            },
        )
        .await;
        return;
    }

    // Replay first, then `ready`: the transcript has no turn-start marker, so
    // whatever it implies about a live turn must be overruled by the host's own
    // answer — otherwise a note written after the last turn reads as "Working".
    let (mut generation, replayed, mut cursor, ready) = {
        let handle = handle.clone();
        let model = model.clone();
        on_hub(&hub, move |hub| {
            let generation = hub.generation(&handle);
            let (replayed, cursor) = hub.read_from(&handle, 0);
            let ready = SessionServerEvent::Ready {
                model: hub.model(&handle).unwrap_or(model),
                busy: hub.busy(&handle),
            };
            (generation, replayed, cursor, ready)
        })
        .await
    };
    for event in replayed {
        if !send_event(&mut socket, &event).await {
            release_slot(&hub, &handle).await;
            return;
        }
    }
    if !send_event(&mut socket, &ready).await {
        release_slot(&hub, &handle).await;
        return;
    }

    let mut last_write = Instant::now();
    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                        last_write = Instant::now();
                    }
                    Some(Ok(Message::Pong(_))) => {}
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
                                    &mut last_write,
                                )
                                .await
                                {
                                    release_slot(&hub, &handle).await;
                                    return;
                                }
                            }
                            ClientHandleResult::Stop => {
                                release_slot(&hub, &handle).await;
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
                    &mut last_write,
                )
                .await
                {
                    release_slot(&hub, &handle).await;
                    return;
                }
                if should_send_keepalive(last_write.elapsed()) {
                    if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                    last_write = Instant::now();
                }
            }
        }
    }

    release_slot(&hub, &handle).await;
}

enum ClientHandleResult {
    Continue,
    Stop,
}

async fn handle_inbound_text(
    socket: &mut WebSocket,
    hub: &Arc<WebSessionHub>,
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

    let applied = {
        let handle = handle.to_string();
        let worktree_path = worktree_path.to_path_buf();
        let mut next_generation = *generation;
        let (applied, next_generation) = on_hub(hub, move |hub| {
            let applied =
                apply_client_message(hub, &handle, &worktree_path, message, &mut next_generation);
            (applied, next_generation)
        })
        .await;
        *generation = next_generation;
        applied
    };

    match applied {
        Ok(ApplyClientMessageOutcome::Applied) => ClientHandleResult::Continue,
        Ok(ApplyClientMessageOutcome::ModelChanged { model }) => {
            // A model swap respawns the child, so nothing is in flight.
            let ok = send_event(socket, &SessionServerEvent::Ready { model, busy: false }).await;
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
        SessionClientMessage::Prompt {
            text,
            client_message_id,
        } => {
            if client_message_id.trim().is_empty() {
                return Err("prompt clientMessageId is required".to_string());
            }
            hub.submit_prompt_with_id(handle, client_message_id, text)?;
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

/// What one flush owes the socket. Collected in a single visit to the hub so a
/// 50ms tick makes one hop to the blocking pool instead of four.
pub(crate) struct OutboundBatch {
    pub(crate) generation: u64,
    pub(crate) cursor: usize,
    /// Present when the ACP child was replaced: the socket replays from zero.
    pub(crate) ready: Option<SessionServerEvent>,
    pub(crate) events: Vec<SessionServerEvent>,
}

pub(crate) fn collect_outbound(
    hub: &WebSessionHub,
    handle: &str,
    cursor: usize,
    generation: u64,
) -> OutboundBatch {
    let current_generation = hub.generation(handle);
    let (cursor, ready) = if current_generation == generation {
        (cursor, None)
    } else {
        (
            0,
            Some(SessionServerEvent::Ready {
                model: hub.model(handle).unwrap_or_else(|| "auto".to_string()),
                busy: hub.busy(handle),
            }),
        )
    };

    hub.pump(handle);
    let (events, next) = hub.read_from(handle, cursor);
    OutboundBatch {
        generation: current_generation,
        cursor: next,
        ready,
        events,
    }
}

async fn flush_outbound(
    socket: &mut WebSocket,
    hub: &Arc<WebSessionHub>,
    handle: &str,
    cursor: &mut usize,
    generation: &mut u64,
    last_write: &mut Instant,
) -> bool {
    let batch = {
        let handle = handle.to_string();
        let cursor = *cursor;
        let generation = *generation;
        on_hub(hub, move |hub| {
            collect_outbound(hub, &handle, cursor, generation)
        })
        .await
    };
    *generation = batch.generation;
    *cursor = batch.cursor;

    let wrote = batch.ready.is_some() || !batch.events.is_empty();
    if let Some(ready) = batch.ready {
        if !send_event(socket, &ready).await {
            return false;
        }
    }
    for event in batch.events {
        if !send_event(socket, &event).await {
            return false;
        }
    }
    if wrote {
        *last_write = Instant::now();
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
    use ajax_core::models::AgentClient;
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
    fn keepalive_waits_for_silence_then_pings() {
        assert!(!should_send_keepalive(Duration::ZERO));
        assert!(!should_send_keepalive(
            SESSION_PING_INTERVAL - Duration::from_millis(1)
        ));
        assert!(should_send_keepalive(SESSION_PING_INTERVAL));
        assert!(should_send_keepalive(SESSION_PING_INTERVAL * 3));
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
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let mut generation = hub.generation(handle);
            apply_client_message(
                &hub,
                handle,
                &dir,
                SessionClientMessage::Prompt {
                    text: "hello".to_string(),
                    client_message_id: "prompt-1".to_string(),
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
