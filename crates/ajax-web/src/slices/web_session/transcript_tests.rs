use super::transcript::{
    already_noted, context_reset_note, is_superseded_marker, slot_must_replace, TranscriptLog,
};
use crate::adapters::web_session_store::{self, MAX_LOG_EVENTS};
use crate::slices::web_session::SessionServerEvent;

fn note(text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: format!("t-{text}"),
        message_id: None,
    }
}

#[test]
fn the_context_reset_note_is_host_commentary_and_is_written_once() {
    let note = context_reset_note();
    let SessionServerEvent::Message { role, .. } = &note else {
        panic!("expected a message event, got {note:?}");
    };
    assert_eq!(role, "note", "an agent role would mark the thread busy");

    let mut log = TranscriptLog::default();
    assert!(!already_noted(&log, &note));
    log.append(vec![note.clone()]);
    assert!(
        already_noted(&log, &note),
        "a second restart must not append the same note again"
    );
}

#[test]
fn slot_must_replace_only_when_child_is_unhealthy() {
    assert!(!slot_must_replace(true, false), "healthy child is leased");
    assert!(slot_must_replace(false, false), "dead child must replace");
    assert!(slot_must_replace(true, true), "exited host must replace");
}

#[test]
fn a_fresh_cursor_replays_the_whole_transcript() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one"), note("two")]);
    let (events, next) = log.read_from(0);
    assert_eq!(events, vec![note("one"), note("two")]);
    assert_eq!(next, 2);
}

#[test]
fn two_cursors_each_receive_every_event() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one")]);
    let (first_a, cursor_a) = log.read_from(0);
    let (first_b, cursor_b) = log.read_from(0);
    assert_eq!(first_a, first_b);

    log.append(vec![note("two")]);
    let (next_a, _) = log.read_from(cursor_a);
    let (next_b, _) = log.read_from(cursor_b);
    assert_eq!(next_a, vec![note("two")]);
    assert_eq!(next_b, vec![note("two")]);
}

#[test]
fn a_caught_up_cursor_reads_nothing() {
    let mut log = TranscriptLog::default();
    log.append(vec![note("one")]);
    let (_, cursor) = log.read_from(0);
    assert!(log.read_from(cursor).0.is_empty());
}

#[test]
fn trimming_keeps_cursors_absolute_and_resumes_at_the_oldest_kept_event() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS + 10)
            .map(|i| note(&i.to_string()))
            .collect(),
    );
    assert_eq!(log.events.len(), MAX_LOG_EVENTS);
    assert_eq!(log.dropped, 10);

    let (events, next) = log.read_from(0);
    assert_eq!(events.len(), MAX_LOG_EVENTS);
    assert_eq!(events[0], note("10"));
    assert_eq!(next, MAX_LOG_EVENTS + 10);
}

#[test]
fn read_from_omits_resolved_permission_requests() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        SessionServerEvent::PermissionRequest {
            request_id: "7".to_string(),
            title: Some("Run tests?".to_string()),
            detail: None,
        },
        SessionServerEvent::PermissionResolved {
            request_id: "7".to_string(),
            approved: true,
        },
    ]);
    let (events, _) = log.read_from(0);
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "7"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        SessionServerEvent::PermissionResolved { request_id, .. } if request_id == "7"
    )));
}

#[test]
fn read_from_keeps_unresolved_permission_requests() {
    let mut log = TranscriptLog::default();
    log.append(vec![SessionServerEvent::PermissionRequest {
        request_id: "9".to_string(),
        title: Some("Deploy?".to_string()),
        detail: None,
    }]);
    let (events, _) = log.read_from(0);
    assert!(events.iter().any(|event| matches!(
        event,
        SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "9"
    )));
}

#[test]
fn disk_backed_read_from_honors_dropped_offset() {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-transcript-dropped-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let handle = "web/fix-login";
    let events: Vec<_> = (0..MAX_LOG_EVENTS + 5)
        .map(|i| note(&i.to_string()))
        .collect();
    web_session_store::append_events(&dir, handle, &events);
    let loaded = web_session_store::load::<SessionServerEvent>(&dir, handle);
    assert_eq!(loaded.dropped, 5);
    assert_eq!(loaded.events.len(), MAX_LOG_EVENTS);

    let (events, next) = TranscriptLog::from_events(loaded.events, loaded.dropped).read_from(0);
    assert_eq!(events.len(), MAX_LOG_EVENTS);
    assert_eq!(events[0], note("5"));
    assert_eq!(next, MAX_LOG_EVENTS + 5);
    let _ = std::fs::remove_dir_all(dir);
}

fn version(item_id: &str, text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: item_id.to_string(),
        message_id: Some(format!("m-{item_id}-{text}")),
    }
}

fn marker(item_id: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: String::new(),
        content_blocks: Vec::new(),
        item_id: item_id.to_string(),
        message_id: None,
    }
}

#[test]
fn is_superseded_marker_only_matches_empty_messages_with_an_item_id() {
    assert!(is_superseded_marker(&marker("i1")));
    let empty_without_item = SessionServerEvent::Message {
        role: "agent".to_string(),
        text: String::new(),
        content_blocks: Vec::new(),
        item_id: String::new(),
        message_id: None,
    };
    assert!(
        !is_superseded_marker(&empty_without_item),
        "a real empty message without an item_id is not a marker"
    );
    assert!(!is_superseded_marker(&version("i1", "hello")));
    assert!(!is_superseded_marker(&SessionServerEvent::UsageReset));
}

#[test]
fn superseded_cursors_find_the_newest_earlier_real_version_per_item() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        version("i1", "v1"),
        note("b"),
        version("i1", "v2"),
        version("i2", "v1"),
    ]);

    assert_eq!(
        log.superseded_cursors_for(&[version("i3", "x")]),
        Vec::<usize>::new()
    );
    let cursors = log.superseded_cursors_for(&[version("i1", "v3"), version("i2", "v2")]);
    assert_eq!(cursors, vec![2, 3]);
}

#[test]
fn superseded_cursors_never_return_markers_and_skip_past_them() {
    let mut log = TranscriptLog::default();
    log.append(vec![version("i1", "v1"), version("i1", "v2")]);
    log.mark_superseded(&[0]);

    assert_eq!(log.superseded_cursors_for(&[version("i1", "v3")]), vec![1]);
}

#[test]
fn superseded_cursors_are_sorted_deduped_and_absolute() {
    let mut log = TranscriptLog::default();
    log.append(
        (0..MAX_LOG_EVENTS)
            .map(|i| note(&format!("pad-{i}")))
            .collect(),
    );
    log.append(vec![version("i1", "v1")]);
    log.append(vec![version("i2", "v1"), version("i2", "v2")]);

    let cursors =
        log.superseded_cursors_for(&[version("i1", "v2"), version("i2", "v3"), version("i2", "x")]);
    assert_eq!(cursors, vec![MAX_LOG_EVENTS, MAX_LOG_EVENTS + 2]);
}

#[test]
fn mark_superseded_writes_the_marker_in_place_and_keeps_cursors_stable() {
    let mut log = TranscriptLog::default();
    log.append(vec![version("i1", "v1"), note("b"), version("i2", "v9")]);
    let next_before = log.absolute_next_cursor();

    log.mark_superseded(&[0, 1, 3, 99]);

    assert_eq!(log.events[0], marker("i1"));
    assert!(is_superseded_marker(&log.events[0]));
    assert!(
        !is_superseded_marker(&log.events[2]),
        "untouched entry stays real"
    );
    let SessionServerEvent::Message {
        role,
        text,
        content_blocks,
        item_id,
        message_id,
    } = &log.events[0]
    else {
        panic!("expected a message marker");
    };
    assert_eq!(role, "agent");
    assert_eq!(item_id, "i1");
    assert!(text.is_empty());
    assert!(content_blocks.is_empty());
    assert!(message_id.is_none());

    assert_eq!(log.events.len(), 3);
    assert_eq!(log.dropped, 0);
    assert_eq!(log.absolute_next_cursor(), next_before);
}

#[test]
fn reads_skip_markers_but_keep_the_absolute_cursors_of_other_events() {
    let mut log = TranscriptLog::default();
    log.append(vec![version("i1", "v1"), note("b")]);

    let cursors = log.superseded_cursors_for(&[version("i1", "v2")]);
    assert_eq!(cursors, vec![0]);
    log.mark_superseded(&cursors);
    log.append(vec![version("i1", "v2")]);

    let (events, next) = log.read_from(0);
    assert_eq!(events.len(), 2);
    assert!(!events.iter().any(is_superseded_marker));
    assert_eq!(next, 3);

    let (envelopes, next) = log.read_from_enveloped(0);
    let SessionServerEvent::Message {
        item_id: first_item,
        ..
    } = &envelopes[0].payload
    else {
        panic!("expected b first");
    };
    assert_eq!(
        first_item, "t-b",
        "the surviving events keep their positions"
    );
    let (second_item, second_text) = match &envelopes[1].payload {
        SessionServerEvent::Message { item_id, text, .. } => (item_id.clone(), text.clone()),
        other => panic!("expected i1 v2 message, got {other:?}"),
    };
    assert_eq!(second_item, "i1");
    assert_eq!(second_text, "v2");
    let read_cursors: Vec<_> = envelopes.iter().map(|envelope| envelope.cursor).collect();
    assert_eq!(read_cursors, vec![1, 2]);
    assert_eq!(next, 3);
}

#[test]
fn marker_for_matches_the_committed_log_without_mutating_it() {
    let mut original = version("i1", "v1");
    if let SessionServerEvent::Message { content_blocks, .. } = &mut original {
        content_blocks.push(super::output_content::OutputContentBlockWire::Image {
            mime_type: "image/png".into(),
            uri: None,
            data: Some("payload".into()),
        });
    }
    let mut log =
        TranscriptLog::from_events(vec![original.clone(), SessionServerEvent::UsageReset], 5);
    assert_eq!(log.marker_for(4), None);
    assert_eq!(log.marker_for(6), None);
    assert_eq!(log.marker_for(7), None);
    assert_eq!(log.marker_for(usize::MAX), None);
    let replacement = log.marker_for(5).unwrap();
    assert_eq!(replacement, marker("i1"));
    assert_eq!(log.events[0], original);
    let next = log.absolute_next_cursor();
    log.mark_superseded(&[4, 5, 6, 7, usize::MAX]);
    assert_eq!(
        log.events,
        vec![replacement, SessionServerEvent::UsageReset]
    );
    assert_eq!(log.dropped, 5);
    assert_eq!(log.absolute_next_cursor(), next);
    log.append(vec![version("i1", "v2")]);
    let (events, cursor) = log.read_from_enveloped(5);
    assert_eq!(events.last().unwrap().cursor, 7);
    assert_eq!(events.last().unwrap().payload, version("i1", "v2"));
    assert_eq!(cursor, 8);
}
