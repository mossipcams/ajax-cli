use super::protocol::{PendingPermission, SessionChrome, SessionSnapshot};
use super::replay::{attach_inline_tail, build_attach, plan_replay, read_before, TAIL_EVENT_LIMIT};
use super::transcript::TranscriptLog;
use crate::adapters::web_session_store::MAX_LOG_EVENTS;
use crate::slices::web_session::output_content::OutputContentBlockWire;
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

fn user_note(text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "user".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: format!("u-{text}"),
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
fn reset_snapshot_carries_only_tail_with_transcript_from() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 3)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    let (snapshot, replayed) = build_attach(
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
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT);
    let first_cursor = log.dropped + MAX_LOG_EVENTS - TAIL_EVENT_LIMIT;
    assert_eq!(transcript.first().map(|e| e.cursor), Some(first_cursor));
    assert_eq!(snapshot.transcript_from, Some(first_cursor));

    assert_eq!(replayed.len(), MAX_LOG_EVENTS);
    assert_eq!(replayed.first().map(|e| e.cursor), Some(log.dropped));
}

#[test]
fn reset_snapshot_small_log_transcript_from_is_retention_floor() {
    let log = TranscriptLog::from_events(vec![note("one"), note("two")], 5);
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
    assert_eq!(transcript, replayed);
    assert_eq!(transcript.len(), 2);
    assert_eq!(snapshot.transcript_from, Some(log.dropped));
}

#[test]
fn reset_snapshot_empty_log_transcript_from_is_next_cursor() {
    let log = TranscriptLog::default();
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
    assert_eq!(snapshot.transcript, Some(Vec::new()));
    let next = log.dropped + log.events.len();
    assert_eq!(snapshot.transcript_from, Some(next));
    assert_eq!(snapshot.transcript_floor, Some(log.dropped));
}

#[test]
fn resumable_snapshot_omits_transcript_from() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(1),
        SessionChrome::default(),
    );
    assert!(!snapshot.reset);
    assert!(snapshot.transcript.is_none());
    assert!(snapshot.transcript_from.is_none());
}

#[test]
fn open_permission_older_than_tail_limit_stays_inside_transcript() {
    let mut events = vec![SessionServerEvent::PermissionRequest {
        request_id: "p1".to_string(),
        title: Some("Run?".to_string()),
        detail: None,
    }];
    events.extend(
        (0..TAIL_EVENT_LIMIT + 100)
            .map(|i| note(&format!("tail-{i}")))
            .collect::<Vec<_>>(),
    );
    let mut log = TranscriptLog::default();
    log.append(events);
    assert!(log.events.len() > TAIL_EVENT_LIMIT);

    let (snapshot, replayed) = build_attach(
        &log,
        "auto".to_string(),
        true,
        None,
        SessionChrome::default(),
    );
    assert!(snapshot.reset);
    assert!(snapshot.pending_permission.is_some());
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript, replayed);
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT + 101);
    assert_eq!(snapshot.transcript_from, Some(0));
    assert!(matches!(
        transcript[0].payload,
        SessionServerEvent::PermissionRequest { ref request_id, .. }
            if request_id == "p1"
    ));
}

#[test]
fn reset_tail_snaps_back_to_user_message_boundary() {
    let mut events = vec![user_note("go")];
    events.extend(
        (0..TAIL_EVENT_LIMIT + 9)
            .map(|i| note(&format!("agent-{i}")))
            .collect::<Vec<_>>(),
    );
    let mut log = TranscriptLog::default();
    log.append(events);
    assert!(log.events.len() > TAIL_EVENT_LIMIT);

    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT + 10);
    assert_eq!(transcript.first().map(|e| e.cursor), Some(0));
    assert!(matches!(
        transcript[0].payload,
        SessionServerEvent::Message { ref role, .. } if role == "user"
    ));
    assert_eq!(snapshot.transcript_from, Some(0));
}

#[test]
fn reset_tail_falls_back_to_naive_start_when_user_message_beyond_limit() {
    let mut events = vec![user_note("go")];
    events.extend(
        (0..TAIL_EVENT_LIMIT * 2)
            .map(|i| note(&format!("agent-{i}")))
            .collect::<Vec<_>>(),
    );
    let mut log = TranscriptLog::default();
    log.append(events);

    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT);
    let first_cursor = log.dropped + log.events.len() - TAIL_EVENT_LIMIT;
    assert_eq!(transcript.first().map(|e| e.cursor), Some(first_cursor));
    assert_eq!(snapshot.transcript_from, Some(first_cursor));
}

#[test]
fn reset_snapshot_transcript_floor_is_log_dropped() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 3)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    assert!(log.dropped > 0);

    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        Some(0),
        SessionChrome::default(),
    );
    assert!(snapshot.transcript.is_some());
    assert_eq!(snapshot.transcript_floor, Some(log.dropped));
}

#[test]
fn read_before_returns_preceding_events_ascending() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        note("a"),
        note("b"),
        note("c"),
        note("d"),
        note("e"),
        note("f"),
    ]);
    let page = read_before(&log, 6, 3);
    assert_eq!(page.events.len(), 3);
    assert_eq!(
        page.events.iter().map(|e| e.cursor).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    assert_eq!(page.from, 3);
    assert_eq!(page.floor, 0);
}

#[test]
fn read_before_snaps_back_to_user_message() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        user_note("go"),
        note("a"),
        note("b"),
        note("c"),
        note("d"),
    ]);
    let page = read_before(&log, 5, 4);
    assert_eq!(page.from, 0);
    assert!(matches!(
        page.events[0].payload,
        SessionServerEvent::Message { ref role, .. } if role == "user"
    ));
    assert_eq!(page.events.len(), 5);
}

#[test]
fn read_before_stops_at_floor_when_events_dropped() {
    let log = TranscriptLog::from_events(vec![note("one"), note("two"), note("three")], 7);
    assert_eq!(log.dropped, 7);
    let page = read_before(&log, 10, 5);
    assert_eq!(page.floor, log.dropped);
    assert!(page.from >= page.floor);
    assert_eq!(
        page.events.iter().map(|e| e.cursor).collect::<Vec<_>>(),
        vec![7, 8, 9]
    );
}

#[test]
fn read_before_clamps_before_beyond_next_cursor() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let page = read_before(&log, 99, 3);
    assert_eq!(
        page.events.iter().map(|e| e.cursor).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(page.from, 0);
}

#[test]
fn read_before_at_floor_returns_empty_page_with_from_at_floor() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let page = read_before(&log, 0, 3);
    assert!(page.events.is_empty());
    assert_eq!(page.from, 0);
    assert_eq!(page.floor, 0);
}

#[test]
fn read_before_zero_limit_returns_empty_page_with_from_at_end() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let page = read_before(&log, 2, 0);
    assert!(page.events.is_empty());
    assert_eq!(page.from, 2);
}

#[test]
fn walking_back_with_from_reproduces_full_retained_log() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        user_note("go"),
        note("a"),
        user_note("again"),
        note("b"),
        note("c"),
        user_note("third"),
        note("d"),
    ]);
    let next = log.absolute_next_cursor();
    let mut before = next;
    let mut pages: Vec<Vec<usize>> = Vec::new();
    while before > 0 {
        let page = read_before(&log, before, 2);
        assert!(!page.events.is_empty(), "walk must terminate at floor");
        assert!(page.from <= before);
        pages.push(page.events.iter().map(|e| e.cursor).collect());
        if page.from == page.floor {
            break;
        }
        before = page.from;
    }
    let mut collected: Vec<usize> = Vec::new();
    for cursors in pages.into_iter().rev() {
        collected.extend(cursors);
    }
    assert_eq!(collected, (0..next).collect::<Vec<_>>());
}

#[test]
fn read_before_empty_page_past_filtered_permissions_still_walks_to_floor() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        user_note("go"),
        note("a"),
        note("b"),
        SessionServerEvent::PermissionRequest {
            request_id: "p1".to_string(),
            title: Some("Run?".to_string()),
            detail: None,
        },
        SessionServerEvent::PermissionRequest {
            request_id: "p2".to_string(),
            title: Some("Run again?".to_string()),
            detail: None,
        },
        SessionServerEvent::PermissionResolved {
            request_id: "p1".to_string(),
            approved: true,
        },
        SessionServerEvent::PermissionResolved {
            request_id: "p2".to_string(),
            approved: false,
        },
    ]);
    let before = 5;
    let page = read_before(&log, before, 2);
    assert!(page.events.is_empty());
    assert_eq!(page.floor, log.dropped);
    assert!(
        page.from < before,
        "empty page must make progress; got from == end"
    );

    let mut cursor = page.from;
    let mut steps = 0;
    while cursor > log.dropped {
        let next_page = read_before(&log, cursor, 2);
        assert!(
            next_page.from < cursor,
            "every non-floor page must lower the cursor"
        );
        cursor = next_page.from;
        steps += 1;
        assert!(steps <= 8, "walk must terminate at the floor");
    }
    assert_eq!(cursor, log.dropped);
}

fn reset_snapshot(next: usize) -> SessionSnapshot {
    SessionSnapshot::new(
        next,
        "auto".to_string(),
        false,
        true,
        None,
        None,
        SessionChrome::default(),
    )
}

#[test]
fn attach_inline_tail_long_log_carries_only_tail_with_from_and_floor() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 3)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    let (replayed, next) = log.read_from_enveloped(log.dropped);
    let snapshot = attach_inline_tail(reset_snapshot(next), &replayed, next, log.dropped);

    let transcript = snapshot
        .transcript
        .expect("attached snapshot carries transcript");
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT);
    let first_cursor = log.dropped + MAX_LOG_EVENTS - TAIL_EVENT_LIMIT;
    assert_eq!(transcript.first().map(|e| e.cursor), Some(first_cursor));
    assert_eq!(snapshot.transcript_from, Some(first_cursor));
    assert_eq!(snapshot.transcript_floor, Some(log.dropped));
}

#[test]
fn attach_inline_tail_open_permission_older_than_limit_stays_inside() {
    let mut events = vec![SessionServerEvent::PermissionRequest {
        request_id: "p1".to_string(),
        title: Some("Run?".to_string()),
        detail: None,
    }];
    events.extend(
        (0..TAIL_EVENT_LIMIT + 50)
            .map(|i| note(&format!("tail-{i}")))
            .collect::<Vec<_>>(),
    );
    let mut log = TranscriptLog::default();
    log.append(events);

    let (replayed, next) = log.read_from_enveloped(log.dropped);
    let mut snapshot = reset_snapshot(next);
    snapshot.pending_permission = Some(PendingPermission {
        request_id: "p1".to_string(),
        title: None,
        detail: None,
    });
    let snapshot = attach_inline_tail(snapshot, &replayed, next, log.dropped);

    let transcript = snapshot
        .transcript
        .expect("attached snapshot carries transcript");
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT + 51);
    assert_eq!(snapshot.transcript_from, Some(0));
}

#[test]
fn attach_inline_tail_uses_snapshot_pending_ids_not_log() {
    let mut events = vec![SessionServerEvent::PermissionRequest {
        request_id: "p1".to_string(),
        title: Some("Run?".to_string()),
        detail: None,
    }];
    events.extend(
        (0..TAIL_EVENT_LIMIT + 50)
            .map(|i| note(&format!("tail-{i}")))
            .collect::<Vec<_>>(),
    );
    let mut log = TranscriptLog::default();
    log.append(events);

    let (replayed, next) = log.read_from_enveloped(log.dropped);
    let mut snapshot = reset_snapshot(next);
    snapshot.pending_permission = Some(PendingPermission {
        request_id: "p2".to_string(),
        title: None,
        detail: None,
    });
    let snapshot = attach_inline_tail(snapshot, &replayed, next, log.dropped);

    let transcript = snapshot
        .transcript
        .expect("attached snapshot carries transcript");
    assert_eq!(transcript.len(), TAIL_EVENT_LIMIT);
}

#[test]
fn attach_inline_tail_empty_events_gives_empty_transcript_and_next_cursor() {
    let snapshot = attach_inline_tail(reset_snapshot(7), &[], 7, 2);
    assert_eq!(snapshot.transcript, Some(Vec::new()));
    assert_eq!(snapshot.transcript_from, Some(7));
    assert_eq!(snapshot.transcript_floor, Some(2));
}

fn stream_msg(
    text: &str,
    item_id: &str,
    blocks: Vec<OutputContentBlockWire>,
) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: blocks,
        item_id: item_id.to_string(),
        message_id: None,
    }
}

fn resource_block(uri: &str) -> OutputContentBlockWire {
    OutputContentBlockWire::Resource {
        uri: uri.to_string(),
        mime_type: None,
        text: Some("body".to_string()),
        blob: None,
    }
}

fn tool_call(call_id: &str) -> SessionServerEvent {
    SessionServerEvent::ToolCall {
        call_id: call_id.to_string(),
        title: "ls".to_string(),
        kind: "read".to_string(),
        status: "in_progress".to_string(),
        locations: Vec::new(),
        content: Vec::new(),
    }
}

#[test]
fn attach_compacts_streamed_message_versions_issue_1352() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        user_note("go"),
        stream_msg("he", "i1", Vec::new()),
        stream_msg("hel", "i1", vec![resource_block("a")]),
        stream_msg("hello", "i1", vec![resource_block("b")]),
    ]);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[0].cursor, 0);
    assert_eq!(transcript[1].cursor, 1);
    match &transcript[1].payload {
        SessionServerEvent::Message {
            text,
            content_blocks,
            item_id,
            ..
        } => {
            assert_eq!(text, "hello");
            assert_eq!(*content_blocks, vec![resource_block("b")]);
            assert_eq!(item_id, "i1");
        }
        other => panic!("expected Message payload, got {other:?}"),
    }
    assert_eq!(snapshot.cursor, 4);
    assert_eq!(snapshot.transcript_from, Some(0));
    assert_eq!(snapshot.transcript_floor, Some(0));
}

#[test]
fn attach_compaction_keeps_first_seen_item_order() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        stream_msg("t1", "i1", Vec::new()),
        stream_msg("a1", "i2", Vec::new()),
        stream_msg("t2", "i1", vec![resource_block("x")]),
        stream_msg("a2", "i2", vec![resource_block("y")]),
    ]);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[0].cursor, 0);
    assert_eq!(transcript[1].cursor, 1);
    let texts: Vec<&str> = transcript
        .iter()
        .map(|env| match &env.payload {
            SessionServerEvent::Message {
                text,
                item_id,
                content_blocks,
                ..
            } => {
                if item_id == "i1" {
                    assert_eq!(content_blocks, &[resource_block("x")]);
                } else {
                    assert_eq!(item_id.as_str(), "i2");
                    assert_eq!(content_blocks, &[resource_block("y")]);
                }
                text.as_str()
            }
            other => panic!("expected Message payload, got {other:?}"),
        })
        .collect();
    assert_eq!(texts, vec!["t2", "a2"]);
}

#[test]
fn attach_compaction_leaves_tool_calls_and_empty_item_ids_alone() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        stream_msg("same", "", Vec::new()),
        stream_msg("same", "", Vec::new()),
        tool_call("c1"),
        tool_call("c1"),
    ]);
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        SessionChrome::default(),
    );
    let transcript = snapshot
        .transcript
        .expect("reset snapshot carries transcript");
    assert_eq!(transcript.len(), 4);
    let cursors: Vec<usize> = transcript.iter().map(|env| env.cursor).collect();
    assert_eq!(cursors, vec![0, 1, 2, 3]);
    for (idx, env) in transcript.iter().enumerate() {
        match &env.payload {
            SessionServerEvent::Message { text, item_id, .. } => {
                assert_eq!(text, "same");
                assert!(item_id.is_empty());
            }
            SessionServerEvent::ToolCall { call_id, .. } => {
                assert!(idx == 2 || idx == 3);
                assert_eq!(call_id, "c1");
            }
            other => panic!("unexpected payload at {idx}: {other:?}"),
        }
    }
}

#[test]
fn read_before_compacts_streamed_versions_and_keeps_from() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        user_note("go"),
        stream_msg("he", "i1", Vec::new()),
        stream_msg("hello", "i1", vec![resource_block("z")]),
    ]);
    let page = read_before(&log, 3, 3);
    assert_eq!(page.from, 0);
    assert_eq!(page.floor, 0);
    assert_eq!(page.events.len(), 2);
    let cursors: Vec<usize> = page.events.iter().map(|env| env.cursor).collect();
    assert_eq!(cursors, vec![0, 1]);
    match &page.events[1].payload {
        SessionServerEvent::Message {
            text,
            content_blocks,
            item_id,
            ..
        } => {
            assert_eq!(text, "hello");
            assert_eq!(*content_blocks, vec![resource_block("z")]);
            assert_eq!(item_id, "i1");
        }
        other => panic!("expected Message payload, got {other:?}"),
    }
}
