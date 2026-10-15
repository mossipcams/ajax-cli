use super::protocol::SessionChrome;
use super::replay::{build_attach, plan_replay};
use super::transcript::TranscriptLog;
use crate::adapters::web_session_store::MAX_LOG_EVENTS;
use crate::slices::web_session::SessionServerEvent;

fn note(text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: format!("n-{text}"),
        message_id: None,
    }
}

#[test]
fn reconnect_before_retained_history_receives_reset_snapshot() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 3)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    let plan = plan_replay(Some(0), &log);
    assert!(plan.reset);
    assert_eq!(plan.from, log.dropped);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(0),
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
}

#[test]
fn reconnect_replays_only_transcript_events_after_cursor() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let plan = plan_replay(Some(1), &log);
    assert!(!plan.reset);
    assert_eq!(plan.from, 1);
    let (events, _) = log.read_from(plan.from);
    assert_eq!(events, vec![note("two")]);
    let (snapshot, replayed) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(1),
        SessionChrome::default(),
    );
    assert!(!snapshot.reset);
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].cursor, 1);
}

#[test]
fn cold_attach_replays_authoritative_transcript_from_start_issue_1031() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("cached")]);
    let plan = plan_replay(None, &log);
    assert!(plan.reset);
    assert_eq!(plan.from, 0);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
}

#[test]
fn resolved_permission_is_not_pending_after_reconnect_issue_1018() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        SessionServerEvent::PermissionRequest {
            request_id: "p1".to_string(),
            title: Some("Run?".to_string()),
            detail: None,
        },
        SessionServerEvent::PermissionResolved {
            request_id: "p1".to_string(),
            approved: true,
        },
    ]);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.pending_permission.is_none());
}

#[test]
fn filtered_permissions_keep_absolute_cursors() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        note("before"),
        SessionServerEvent::PermissionRequest {
            request_id: "p1".to_string(),
            title: Some("Run?".to_string()),
            detail: None,
        },
        SessionServerEvent::PermissionResolved {
            request_id: "p1".to_string(),
            approved: true,
        },
        note("after"),
    ]);
    let (envelopes, _) = log.read_from_enveloped(0);
    assert_eq!(envelopes.len(), 3);
    assert_eq!(envelopes[0].cursor, 0);
    assert_eq!(envelopes[1].cursor, 2);
    assert_eq!(envelopes[2].cursor, 3);
}

#[test]
fn reset_snapshot_carries_retained_transcript_in_cursor_order() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let (snapshot, replayed) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[0].cursor, 0);
    assert_eq!(transcript[1].cursor, 1);
    assert_eq!(transcript, replayed);
}

#[test]
fn resumable_snapshot_omits_transcript_field() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let (snapshot, replayed) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(1),
        SessionChrome::default(),
    );
    assert!(!snapshot.reset);
    assert!(snapshot.transcript.is_none());
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].cursor, 1);
}

#[test]
fn empty_reset_snapshot_carries_empty_transcript() {
    let log = TranscriptLog::default();
    let (snapshot, replayed) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
    assert_eq!(snapshot.transcript, Some(Vec::new()));
    assert!(replayed.is_empty());
}

#[test]
fn reset_transcript_preserves_retained_log_floor() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 3)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(0),
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), MAX_LOG_EVENTS);
    assert_eq!(transcript.first().map(|e| e.cursor), Some(log.dropped));
}
