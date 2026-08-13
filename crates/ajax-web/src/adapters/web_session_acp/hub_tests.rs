use super::hub::{
    map_request_finished, permission_response, slot_must_replace, TranscriptLog, WebSessionHub,
};
use super::store::{self, MAX_LOG_EVENTS};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::slices::web_session::{map_acp_session_update, SessionServerEvent, MAX_QUEUED_PROMPTS};
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
fn submit_prompt_records_user_message_and_starts_when_idle() {
    let dir = scratch_dir("submit-idle");
    let handle = "web/submit-idle";
    let hub = WebSessionHub::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        hub.acquire(handle, &dir, "auto").expect("acquire");
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
            hub.acquire(handle, &dir, "auto").expect("acquire");
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
            hub.acquire(handle, &dir, "auto").expect("acquire");
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
            hub.acquire(handle, &dir, "auto").expect("acquire");
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
            hub.acquire(handle, &dir, "auto").expect("acquire");
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
        hub.acquire(handle, &dir, "auto").expect("acquire");
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

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn permission_response_matches_779_shape() {
    assert_eq!(
        permission_response(true, Some("because")),
        json!({ "approved": true, "reason": "because" })
    );
}
