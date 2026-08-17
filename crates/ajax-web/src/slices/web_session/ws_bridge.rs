//! Authenticated orchestration-chat WebSocket bridge over the task-session directory.

use super::{
    apply_client_message, ApplyClientMessageOutcome, SessionAttachPlan, SessionClientMessage,
    SessionServerEvent, TaskSessionDirectory,
};
use axum::extract::ws::{Message, WebSocket};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;
pub(crate) const MAX_SESSION_FRAME_BYTES: usize = 4096;
pub(crate) const SESSION_PING_INTERVAL: Duration = Duration::from_secs(20);

pub(crate) fn should_send_keepalive(since_last_write: Duration) -> bool {
    since_last_write >= SESSION_PING_INTERVAL
}

async fn release_slot(directory: &Arc<TaskSessionDirectory>, handle: &str) {
    directory.release(handle).await;
}

pub(crate) async fn bridge_task_session_socket(
    mut socket: WebSocket,
    directory: Arc<TaskSessionDirectory>,
    plan: SessionAttachPlan,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    if let Err(error) = directory
        .acquire(&handle, &plan.worktree_path, &model, plan.agent)
        .await
    {
        let _ = send_event(
            &mut socket,
            &SessionServerEvent::Error {
                message: error.clone(),
            },
        )
        .await;
        return;
    }

    let snapshot = directory.attach_snapshot(&handle, model).await;
    let mut generation = snapshot.generation;
    let mut cursor = snapshot.cursor;
    for event in snapshot.replayed {
        if !send_event(&mut socket, &event).await {
            release_slot(&directory, &handle).await;
            return;
        }
    }
    if !send_event(&mut socket, &snapshot.ready).await {
        release_slot(&directory, &handle).await;
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
                            &directory,
                            &handle,
                            &plan.worktree_path,
                            &text,
                            &mut generation,
                        )
                        .await
                        {
                            ClientHandleResult::Continue => {
                                if !flush_outbound(
                                    &mut socket,
                                    &directory,
                                    &handle,
                                    &mut cursor,
                                    &mut generation,
                                    &mut last_write,
                                )
                                .await
                                {
                                    release_slot(&directory, &handle).await;
                                    return;
                                }
                            }
                            ClientHandleResult::Stop => {
                                release_slot(&directory, &handle).await;
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
                    &directory,
                    &handle,
                    &mut cursor,
                    &mut generation,
                    &mut last_write,
                )
                .await
                {
                    release_slot(&directory, &handle).await;
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

    release_slot(&directory, &handle).await;
}

enum ClientHandleResult {
    Continue,
    Stop,
}

async fn handle_inbound_text(
    socket: &mut WebSocket,
    directory: &Arc<TaskSessionDirectory>,
    handle: &str,
    worktree_path: &std::path::Path,
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

    match apply_client_message(directory, handle, worktree_path, message, generation).await {
        Ok(ApplyClientMessageOutcome::Applied) => ClientHandleResult::Continue,
        Ok(ApplyClientMessageOutcome::ModelChanged { model }) => {
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

async fn flush_outbound(
    socket: &mut WebSocket,
    directory: &Arc<TaskSessionDirectory>,
    handle: &str,
    cursor: &mut usize,
    generation: &mut u64,
    last_write: &mut Instant,
) -> bool {
    let batch = directory
        .collect_outbound(handle, *cursor, *generation)
        .await;
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
