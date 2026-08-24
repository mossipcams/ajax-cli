//! Authenticated orchestration-chat WebSocket bridge over the task-session directory.

use super::{
    apply_client_message, ApplyClientMessageOutcome, PersistSessionModel, ReportSessionActivity,
    SessionActivityReporter, SessionAttachPlan, SessionClientMessage, SessionEventEnvelope,
    SessionSnapshot, TaskSessionDirectory,
};
use axum::extract::ws::{Message, WebSocket};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;
/// Per-frame WebSocket ceiling for session client messages (prompts, cancel, etc.).
pub(crate) const MAX_SESSION_FRAME_BYTES: usize = 256 * 1024;
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
    client_cursor: Option<usize>,
    persist_session_model: Option<PersistSessionModel>,
    report_activity: Option<ReportSessionActivity>,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    if let Err(error) = directory
        .acquire(&handle, &plan.worktree_path, &model, plan.agent)
        .await
    {
        let _ = send_error(&mut socket, &error).await;
        return;
    }

    let attach = directory
        .attach_snapshot(&handle, model, client_cursor)
        .await;
    let mut generation = attach.generation;
    let mut cursor = attach.snapshot.cursor;
    if !send_snapshot(&mut socket, &attach.snapshot).await {
        release_slot(&directory, &handle).await;
        return;
    }
    for envelope in attach.replayed {
        if !send_envelope(&mut socket, &envelope).await {
            release_slot(&directory, &handle).await;
            return;
        }
    }

    let mut last_write = Instant::now();
    let mut activity = ActivityFeed {
        reporter: SessionActivityReporter::default(),
        report: report_activity.as_ref(),
    };
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
                            persist_session_model.clone(),
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
                                    &mut activity,
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
                    &mut activity,
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
    persist_session_model: Option<PersistSessionModel>,
) -> ClientHandleResult {
    if text.len() > MAX_SESSION_FRAME_BYTES {
        let ok = send_error(socket, "input frame too large").await;
        return if ok {
            ClientHandleResult::Continue
        } else {
            ClientHandleResult::Stop
        };
    }

    let message: SessionClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            let ok = send_error(socket, &format!("invalid session message: {error}")).await;
            return if ok {
                ClientHandleResult::Continue
            } else {
                ClientHandleResult::Stop
            };
        }
    };

    match apply_client_message(
        directory,
        handle,
        worktree_path,
        message,
        generation,
        persist_session_model,
    )
    .await
    {
        Ok(ApplyClientMessageOutcome::Applied) => ClientHandleResult::Continue,
        Ok(ApplyClientMessageOutcome::ModelChanged { persist_warning }) => {
            if let Some(warn) = persist_warning {
                let _ = send_error(socket, &warn).await;
            }
            ClientHandleResult::Continue
        }
        Err(error) => {
            let ok = send_error(socket, &error).await;
            if ok {
                ClientHandleResult::Continue
            } else {
                ClientHandleResult::Stop
            }
        }
    }
}

/// The socket's view of ACP run-state: what it has already reported, and where
/// to send the next change. Paired because neither is useful alone.
struct ActivityFeed<'a> {
    reporter: SessionActivityReporter,
    report: Option<&'a ReportSessionActivity>,
}

impl ActivityFeed<'_> {
    fn observe(&mut self, envelope: &SessionEventEnvelope) {
        let Some(activity) = self.reporter.observe(&envelope.payload) else {
            return;
        };
        if let Some(report) = self.report {
            report(activity);
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
    activity: &mut ActivityFeed<'_>,
) -> bool {
    let batch = directory
        .collect_outbound(handle, *cursor, *generation)
        .await;
    *generation = batch.generation;
    *cursor = batch.cursor;

    let wrote = batch.snapshot.is_some() || !batch.events.is_empty();
    if let Some(snapshot) = batch.snapshot {
        if !send_snapshot(socket, &snapshot).await {
            return false;
        }
    }
    for envelope in batch.events {
        // Report off the same stream the browser reads, so the task page and
        // the chat head cannot disagree about whether a turn is in flight.
        activity.observe(&envelope);
        if !send_envelope(socket, &envelope).await {
            return false;
        }
    }
    if wrote {
        *last_write = Instant::now();
    }
    true
}

async fn send_snapshot(socket: &mut WebSocket, snapshot: &SessionSnapshot) -> bool {
    match serde_json::to_string(snapshot) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn send_envelope(socket: &mut WebSocket, envelope: &SessionEventEnvelope) -> bool {
    match serde_json::to_string(envelope) {
        Ok(payload) => socket.send(Message::Text(payload.into())).await.is_ok(),
        Err(_) => false,
    }
}

async fn send_error(socket: &mut WebSocket, message: &str) -> bool {
    let envelope = SessionEventEnvelope::new(
        0,
        super::SessionServerEvent::Error {
            message: message.to_string(),
        },
    );
    send_envelope(socket, &envelope).await
}
