use super::{
    apply_client_message, ApplyClientMessageOutcome, PersistSessionModel, SessionAttachPlan,
    SessionClientMessage, SessionError, SessionEventEnvelope, SessionSnapshot,
    TaskSessionDirectory,
};
use axum::extract::ws::{Message, WebSocket};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

const EVENT_POLL_MS: u64 = 50;
/// Before blocking on a (possibly slow) harness restore, give `snapshotTail`
/// clients an early storage-only snapshot so the browser is not blank while
/// the session actor is still acquiring.
const EARLY_SNAPSHOT_AFTER: Duration = Duration::from_millis(150);
pub(crate) const MAX_SESSION_FRAME_BYTES: usize = super::prompt_content::MAX_PROMPT_FRAME_BYTES;
pub(crate) const SESSION_PING_INTERVAL: Duration = Duration::from_secs(20);

pub(crate) fn should_send_keepalive(since_last_write: Duration) -> bool {
    since_last_write >= SESSION_PING_INTERVAL
}

async fn release_slot(directory: &Arc<TaskSessionDirectory>, handle: &str) {
    directory.release(handle).await;
}

/// Cursor the live attach resumes from after an early storage-only snapshot.
/// Only an inline transcript means the browser consumed history up to
/// `early.cursor`; a non-reset early snapshot replays nothing, so the client's
/// own cursor must stay put or the events in between would never be sent.
pub(crate) fn resume_cursor_after_early(
    early: &SessionSnapshot,
    client_cursor: Option<usize>,
) -> Option<usize> {
    if early.transcript.is_some() {
        Some(early.cursor)
    } else {
        client_cursor
    }
}

/// A reset attach inlines the newest transcript into its snapshot. When the
/// client opts in with `snapshotTail=1`, skip re-sending that same retained
/// history as per-event envelopes. Resumable reconnects (no inline
/// transcript) and legacy clients keep the full replay.
pub(crate) fn should_skip_replayed_envelopes(
    tail_snapshot: bool,
    snapshot: &SessionSnapshot,
) -> bool {
    tail_snapshot && snapshot.transcript.is_some()
}
/// Decide which frames a batch yields for one client. A reset snapshot with
/// an inline transcript (and a `snapshotTail=1` client) already carries the
/// same history, so re-sent envelopes are dropped; everything else flows.
pub(crate) fn outbound_frames(
    batch_snapshot: Option<SessionSnapshot>,
    batch_events: Vec<SessionEventEnvelope>,
    tail_snapshot: bool,
) -> (Option<SessionSnapshot>, Vec<SessionEventEnvelope>) {
    let events = match &batch_snapshot {
        Some(snapshot) if should_skip_replayed_envelopes(tail_snapshot, snapshot) => Vec::new(),
        _ => batch_events,
    };
    (batch_snapshot, events)
}
pub(crate) async fn bridge_task_session_socket(
    mut socket: WebSocket,
    directory: Arc<TaskSessionDirectory>,
    plan: SessionAttachPlan,
    client_cursor: Option<usize>,
    tail_snapshot: bool,
    persist_session_model: Option<PersistSessionModel>,
) {
    let handle = plan.qualified_handle.clone();
    let model = plan.model.clone();
    // Race the (potentially minutes-long) harness restore against a short
    // timer so tail clients see a storage-only snapshot immediately. Legacy
    // clients keep the old behavior: just await the acquire.
    let mut early_cursor: Option<usize> = None;
    let acquire_result: Result<(), SessionError> = {
        // One acquire future for the whole race: dropping it on the timer arm
        // would leave the Acquire command already queued in the actor while a
        // second Acquire is sent, double-counting the holder.
        let acquire = directory.acquire_typed(&handle, &plan.worktree_path, &model, plan.agent);
        tokio::pin!(acquire);
        if tail_snapshot {
            tokio::select! {
                result = &mut acquire => result,
                _ = sleep(EARLY_SNAPSHOT_AFTER) => {
                    let early =
                        directory.detached_attach_snapshot(&handle, model.clone(), client_cursor);
                    early_cursor = resume_cursor_after_early(&early.snapshot, client_cursor);
                    if !send_snapshot(&mut socket, &early.snapshot.with_agent_pending(true)).await {
                        // The acquire is still in flight and may have succeeded:
                        // wait for it so the holder taken by it is released.
                        if acquire.await.is_ok() {
                            release_slot(&directory, &handle).await;
                        }
                        return;
                    }
                    acquire.await
                }
            }
        } else {
            acquire.await
        }
    };

    if let Err(error) = acquire_result {
        if !error.is_restore_unavailable() {
            let _ = send_error(&mut socket, &error.to_string()).await;
            return;
        }
        if !send_error(&mut socket, &error.to_string()).await {
            return;
        }
        loop {
            let Some(message) = socket.recv().await else {
                return;
            };
            let Ok(message) = message else { return };
            let Message::Text(text) = message else {
                if let Message::Ping(payload) = message {
                    if socket.send(Message::Pong(payload)).await.is_err() {
                        return;
                    }
                } else if matches!(message, Message::Close(_)) {
                    return;
                }
                continue;
            };
            let Ok(message) = serde_json::from_str::<SessionClientMessage>(&text) else {
                let _ = send_error(&mut socket, "invalid session recovery message").await;
                continue;
            };
            let result = match message {
                SessionClientMessage::RetryRestore => {
                    directory
                        .acquire_typed(&handle, &plan.worktree_path, &model, plan.agent)
                        .await
                }
                SessionClientMessage::StartFresh => directory
                    .start_fresh(&handle, &plan.worktree_path)
                    .await
                    .map_err(SessionError::protocol),
                _ => {
                    let _ = send_error(
                        &mut socket,
                        "restore recovery requires Retry or Start fresh",
                    )
                    .await;
                    continue;
                }
            };
            match result {
                Ok(()) => break,
                Err(error) => {
                    if !send_error(&mut socket, &error.to_string()).await {
                        return;
                    }
                }
            }
        }
    }

    let attach = directory
        .attach_snapshot(&handle, model, early_cursor.or(client_cursor))
        .await;
    let mut generation = attach.generation;
    let mut cursor = attach.snapshot.cursor;
    if !send_snapshot(&mut socket, &attach.snapshot).await {
        release_slot(&directory, &handle).await;
        return;
    }
    if !should_skip_replayed_envelopes(tail_snapshot, &attach.snapshot) {
        for envelope in attach.replayed {
            if !send_envelope(&mut socket, &envelope).await {
                release_slot(&directory, &handle).await;
                return;
            }
        }
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
                                    tail_snapshot,
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
                    tail_snapshot,
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

async fn flush_outbound(
    socket: &mut WebSocket,
    directory: &Arc<TaskSessionDirectory>,
    handle: &str,
    cursor: &mut usize,
    generation: &mut u64,
    tail_snapshot: bool,
    last_write: &mut Instant,
) -> bool {
    let batch = directory
        .collect_outbound(handle, *cursor, *generation)
        .await;
    *generation = batch.generation;
    *cursor = batch.cursor;

    let (snapshot, events) = outbound_frames(batch.snapshot, batch.events, tail_snapshot);
    let wrote = snapshot.is_some() || !events.is_empty();
    if let Some(snapshot) = snapshot {
        if !send_snapshot(socket, &snapshot).await {
            return false;
        }
    }
    for envelope in events {
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

#[cfg(test)]
mod early_cursor_tests {
    use super::resume_cursor_after_early;
    use crate::slices::web_session::protocol::SessionChrome;
    use crate::slices::web_session::SessionSnapshot;

    fn snapshot(cursor: usize, reset: bool) -> SessionSnapshot {
        SessionSnapshot::new(
            cursor,
            "auto".to_string(),
            false,
            reset,
            None,
            None,
            SessionChrome::default(),
        )
    }

    #[test]
    fn reset_snapshot_with_transcript_advances_to_snapshot_cursor() {
        let early = snapshot(9, true).with_transcript(Vec::new(), None);
        assert_eq!(resume_cursor_after_early(&early, Some(3)), Some(9));
        assert_eq!(resume_cursor_after_early(&early, None), Some(9));
    }

    #[test]
    fn non_reset_snapshot_without_transcript_keeps_client_cursor() {
        let early = snapshot(9, false);
        assert_eq!(resume_cursor_after_early(&early, Some(3)), Some(3));
    }

    #[test]
    fn no_transcript_and_no_client_cursor_stays_none() {
        let early = snapshot(9, false);
        assert_eq!(resume_cursor_after_early(&early, None), None);
    }
}
