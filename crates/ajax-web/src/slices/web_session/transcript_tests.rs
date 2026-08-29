use super::transcript::{already_noted, context_reset_note, slot_must_replace, TranscriptLog};
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
fn slot_must_replace_when_host_is_dead_or_model_changes() {
    assert!(!slot_must_replace(true, "auto", "auto", false));
    assert!(slot_must_replace(false, "auto", "auto", false));
    assert!(slot_must_replace(true, "auto", "auto", true));
    assert!(slot_must_replace(true, "auto", "composer-2.5", false));
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
