use super::hub::{
    already_noted, context_reset_note, map_acp_session_update_with_startup, map_request_finished,
    permission_response, slot_must_replace, TranscriptLog, WebSessionHub, MAX_IDLE_SESSIONS,
};
use super::store::{self, MAX_LOG_EVENTS};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::slices::web_session::{map_acp_session_update, SessionServerEvent, MAX_QUEUED_PROMPTS};
use ajax_core::models::AgentClient;
use serde_json::json;
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-session-hub-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_acp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

fn note(text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
    }
}

fn user_msg(text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "user".to_string(),
        text: text.to_string(),
    }
}

fn pump_until<F>(hub: &WebSessionHub, handle: &str, timeout: Duration, mut done: F)
where
    F: FnMut(&[SessionServerEvent]) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        hub.pump(handle);
        let (events, _) = hub.read_from(handle, 0);
        if done(&events) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out; events={events:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

// The browser treats an `agent` message as a live turn, so this host note has
// to be a note — otherwise replaying it leaves the thread reading "Working"
// with nothing running. Restarts must not stack copies of it either.
#[test]
fn the_context_reset_note_is_host_commentary_and_is_written_once() {
    let note = context_reset_note();
    let crate::slices::web_session::SessionServerEvent::Message { role, .. } = &note else {
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
fn hub_release_is_noop_when_handle_missing() {
    let hub = WebSessionHub::new(scratch_dir("release"));
    hub.release("web/fix-login");
    assert_eq!(hub.read_from("web/fix-login", 0), (Vec::new(), 0));
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
fn reading_an_unknown_handle_loads_from_disk_when_present() {
    let dir = scratch_dir("disk-read");
    let handle = "web/fix-login";
    let events = vec![note("persisted")];
    store::append_events(&dir, handle, &events);
    let hub = WebSessionHub::new(dir.clone());
    let (loaded, next) = hub.read_from(handle, 0);
    assert_eq!(loaded, events);
    assert_eq!(next, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reading_an_unknown_handle_leaves_the_cursor_untouched_when_disk_empty() {
    let hub = WebSessionHub::new(scratch_dir("unknown"));
    assert_eq!(hub.read_from("web/none", 7), (Vec::new(), 7));
}

#[test]
fn finished_prompt_reports_turn_end_with_stop_reason() {
    let event = map_request_finished("session/prompt", Ok(json!({ "stopReason": "end_turn" })));
    assert_eq!(
        event,
        Some(SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        })
    );
}

#[test]
fn finished_non_prompt_request_reports_nothing() {
    assert_eq!(map_request_finished("session/cancel", Ok(json!({}))), None);
}

#[test]
fn failed_request_reports_error() {
    let event = map_request_finished("session/prompt", Err("boom".to_string()));
    assert_eq!(
        event,
        Some(SessionServerEvent::Error {
            message: "boom".to_string(),
        })
    );
}

#[test]
fn drain_maps_session_update_notifications() {
    let update = json!({
        "sessionId": "sess",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" }
        }
    });
    let events = map_acp_session_update(&update);
    assert_eq!(events.len(), 1);
}

#[test]
fn pi_startup_info_is_a_note_instead_of_agent_prose() {
    let startup = "pi v0.80.10 ---\nContext\n/repo/AGENTS.md";
    let update = json!({
        "sessionId": "sess",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": startup }
        }
    });

    assert_eq!(
        map_acp_session_update_with_startup(&update, Some(startup)),
        vec![SessionServerEvent::Message {
            role: "note".to_string(),
            text: startup.to_string(),
        }]
    );
}

#[test]
fn submit_prompt_records_user_message_and_starts_when_idle() {
    let dir = scratch_dir("submit-idle");
    let handle = "web/submit-idle";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        hub.submit_prompt(handle, "hello".to_string())
            .expect("submit");
        pump_until(&hub, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { text, .. } if text == "pong"
                )
            })
        });
        let (events, _) = hub.read_from(handle, 0);
        assert!(events.contains(&user_msg("hello")));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn submit_prompt_queues_while_in_flight() {
    let dir = scratch_dir("submit-queue");
    let handle = "web/submit-queue";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            hub.submit_prompt(handle, "first".to_string())
                .expect("first");
            hub.submit_prompt(handle, "second".to_string())
                .expect("second");

            hub.cancel(handle, true).expect("cancel releases hold");
            pump_until(&hub, handle, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .filter(|event| {
                        matches!(
                            event,
                            SessionServerEvent::Message { role, text, .. }
                                if role == "user" && (text == "first" || text == "second")
                        )
                    })
                    .count()
                    >= 2
                    && events.iter().any(|event| {
                        matches!(
                            event,
                            SessionServerEvent::Message { text, .. } if text == "pong"
                        )
                    })
            });
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn submit_prompt_cap_drops_oldest_while_in_flight() {
    let dir = scratch_dir("submit-cap");
    let handle = "web/submit-cap";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            hub.submit_prompt(handle, "hold".to_string()).expect("hold");
            for i in 0..MAX_QUEUED_PROMPTS {
                hub.submit_prompt(handle, format!("q{i}")).expect("queue");
            }
            hub.submit_prompt(handle, "overflow".to_string())
                .expect("overflow");

            hub.cancel(handle, true).expect("cancel");
            pump_until(&hub, handle, Duration::from_secs(15), |events| {
                agent_pong_count(events) >= MAX_QUEUED_PROMPTS
            });
            let (events, _) = hub.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "q0"
            )));
            assert_eq!(agent_pong_count(&events), MAX_QUEUED_PROMPTS);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

fn agent_pong_count(events: &[SessionServerEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "agent" && text == "pong"
            )
        })
        .count()
}

#[test]
fn cancel_keep_queue_false_clears_queued_prompts() {
    let dir = scratch_dir("cancel-clear");
    let handle = "web/cancel-clear";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            hub.submit_prompt(handle, "first".to_string())
                .expect("first");
            hub.submit_prompt(handle, "queued".to_string())
                .expect("queued");
            hub.cancel(handle, false).expect("cancel clear");

            hub.submit_prompt(handle, "after".to_string())
                .expect("after");
            pump_until(&hub, handle, Duration::from_secs(5), |events| {
                agent_pong_count(events) >= 1
            });
            let (events, _) = hub.read_from(handle, 0);
            assert_eq!(
                agent_pong_count(&events),
                1,
                "queued prompt must not run after cancel cleared the queue"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancel_keep_queue_true_preserves_queued_prompts() {
    let dir = scratch_dir("cancel-keep");
    let handle = "web/cancel-keep";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            hub.submit_prompt(handle, "first".to_string())
                .expect("first");
            hub.submit_prompt(handle, "kept".to_string()).expect("kept");
            hub.cancel(handle, true).expect("cancel keep");

            pump_until(&hub, handle, Duration::from_secs(5), |events| {
                events.iter().any(
                |event| matches!(event, SessionServerEvent::Message { text, .. } if text == "pong")
            )
            });
            let (events, _) = hub.read_from(handle, 0);
            assert!(events.contains(&user_msg("kept")));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn answer_permission_records_permission_resolved() {
    let dir = scratch_dir("permission");
    let handle = "web/permission";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            hub.acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            hub.submit_prompt(handle, "permission".to_string())
                .expect("prompt");
            pump_until(&hub, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "42"
                ))
            });
            hub.answer_permission(handle, "42", true, Some("ok"))
                .expect("answer");
            let (events, _) = hub.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::PermissionResolved {
                    request_id,
                    approved: true,
                } if request_id == "42"
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn permission_response_matches_779_shape() {
    assert_eq!(
        permission_response(true, Some("because")),
        json!({ "approved": true, "reason": "because" })
    );
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
    let dir = scratch_dir("dropped-cursor");
    let handle = "web/fix-login";
    let events: Vec<_> = (0..MAX_LOG_EVENTS + 5)
        .map(|i| note(&i.to_string()))
        .collect();
    store::append_events(&dir, handle, &events);
    let loaded = store::load(&dir, handle);
    assert_eq!(loaded.dropped, 5);
    assert_eq!(loaded.events.len(), MAX_LOG_EVENTS);

    let hub = WebSessionHub::new(dir.clone());
    let (events, next) = hub.read_from(handle, 0);
    assert_eq!(events.len(), MAX_LOG_EVENTS);
    assert_eq!(events[0], note("5"));
    assert_eq!(next, MAX_LOG_EVENTS + 5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idle_eviction_preserves_slots_with_in_flight_turn() {
    let dir = scratch_dir("evict-inflight");
    let handle_a = "web/evict-inflight-a";
    let handle_c = "web/evict-inflight-c";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("acquire a");
            hub.submit_prompt(handle_a, "first".to_string())
                .expect("first");
            hub.release(handle_a);

            for i in 0..MAX_IDLE_SESSIONS {
                let handle = format!("web/evict-inflight-idle-{i}");
                hub.acquire(&handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire idle");
                hub.release(&handle);
            }

            hub.acquire(handle_c, &dir, "auto", AgentClient::Cursor)
                .expect("acquire c");
            hub.release(handle_c);

            hub.acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire a");
            let (events, _) = hub.read_from(handle_a, 0);
            assert!(
                events.contains(&user_msg("first")),
                "in-flight slot must survive idle eviction"
            );
            hub.cancel(handle_a, true).expect("cancel in-flight");
            pump_until(&hub, handle_a, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idle_eviction_preserves_slots_with_queued_prompts() {
    let dir = scratch_dir("evict-queue");
    let handle_a = "web/evict-a";
    let handle_c = "web/evict-c";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            hub.acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("acquire a");
            hub.submit_prompt(handle_a, "first".to_string())
                .expect("first");
            hub.submit_prompt(handle_a, "kept".to_string())
                .expect("kept");
            hub.release(handle_a);

            for i in 0..MAX_IDLE_SESSIONS {
                let handle = format!("web/evict-idle-{i}");
                hub.acquire(&handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire idle");
                hub.release(&handle);
            }

            hub.acquire(handle_c, &dir, "auto", AgentClient::Cursor)
                .expect("acquire c");
            hub.release(handle_c);

            hub.acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire a");
            hub.cancel(handle_a, true).expect("cancel keep queue");

            pump_until(&hub, handle_a, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { text, .. } if text == "pong"
                    )
                }) && events.contains(&user_msg("kept"))
            });
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}
