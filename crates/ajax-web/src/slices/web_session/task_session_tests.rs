use super::test_support::{
    agent_pong_count, fake_acp_fixture, note, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::{SessionServerEvent, MAX_QUEUED_PROMPTS};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::{
    thread,
    time::{Duration, Instant},
};

#[test]
fn release_is_noop_when_handle_missing() {
    let directory = BlockingSessionDirectory::new(scratch_dir("release"));
    directory.release("web/fix-login");
    assert_eq!(directory.read_from("web/fix-login", 0), (Vec::new(), 0));
}

#[test]
fn reading_an_unknown_handle_loads_from_disk_when_present() {
    let dir = scratch_dir("disk-read");
    let handle = "web/fix-login";
    let events = vec![note("persisted")];
    web_session_store::append_events(&dir, handle, &events);
    let directory = BlockingSessionDirectory::new(dir.clone());
    let (loaded, next) = directory.read_from(handle, 0);
    assert_eq!(loaded, events);
    assert_eq!(next, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reading_an_unknown_handle_leaves_the_cursor_untouched_when_disk_empty() {
    let directory = BlockingSessionDirectory::new(scratch_dir("unknown"));
    assert_eq!(directory.read_from("web/none", 7), (Vec::new(), 7));
}

#[test]
fn submit_prompt_records_user_message_and_starts_when_idle() {
    let dir = scratch_dir("submit-idle");
    let handle = "web/submit-idle";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt(handle, "hello".to_string())
            .expect("submit");
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { text, .. } if text == "pong"
                )
            })
        });
        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { role, text, .. }
                if role == "user" && text == "hello"
        )));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn duplicate_client_prompt_id_is_not_dispatched_twice() {
    let dir = scratch_dir("duplicate-prompt");
    let handle = "web/duplicate-prompt";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt_with_id(handle, "prompt-1".to_string(), "first".to_string())
            .expect("first");
        directory
            .submit_prompt_with_id(handle, "prompt-1".to_string(), "duplicate".to_string())
            .expect("duplicate");

        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            agent_pong_count(events) == 1
        });
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "user" && text == "first"
                ))
                .count(),
            1
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { role, text, .. }
                if role == "user" && text == "duplicate"
        )));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn per_session_progress_advances_queued_prompts_without_a_socket() {
    let dir = scratch_dir("background-pump");
    let handle = "web/background-pump";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt(handle, "first".to_string())
            .expect("first");
        directory
            .submit_prompt(handle, "second".to_string())
            .expect("second");
        directory.release(handle);

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (events, _) = directory.read_from(handle, 0);
            if agent_pong_count(&events) >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "queued prompt stalled: {events:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn submit_prompt_queues_while_in_flight() {
    let dir = scratch_dir("submit-queue");
    let handle = "web/submit-queue";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "first".to_string())
                .expect("first");
            directory
                .submit_prompt(handle, "second".to_string())
                .expect("second");

            directory
                .cancel(handle, true)
                .expect("cancel releases hold");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
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
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "hold".to_string())
                .expect("hold");
            for i in 0..MAX_QUEUED_PROMPTS {
                directory
                    .submit_prompt(handle, format!("q{i}"))
                    .expect("queue");
            }
            directory
                .submit_prompt(handle, "overflow".to_string())
                .expect("overflow");

            directory.cancel(handle, true).expect("cancel");
            pump_until(&directory, handle, Duration::from_secs(15), |events| {
                agent_pong_count(events) >= MAX_QUEUED_PROMPTS
            });
            let (events, _) = directory.read_from(handle, 0);
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

#[test]
fn cancel_keep_queue_false_clears_queued_prompts() {
    let dir = scratch_dir("cancel-clear");
    let handle = "web/cancel-clear";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "first".to_string())
                .expect("first");
            directory
                .submit_prompt(handle, "queued".to_string())
                .expect("queued");
            directory.cancel(handle, false).expect("cancel clear");

            directory
                .submit_prompt(handle, "after".to_string())
                .expect("after");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                agent_pong_count(events) >= 1
            });
            let (events, _) = directory.read_from(handle, 0);
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
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "first".to_string())
                .expect("first");
            directory
                .submit_prompt(handle, "kept".to_string())
                .expect("kept");
            directory.cancel(handle, true).expect("cancel keep");

            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(
                |event| matches!(event, SessionServerEvent::Message { text, .. } if text == "pong")
            )
            });
            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "kept"
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn answer_permission_records_permission_resolved() {
    let dir = scratch_dir("permission");
    let handle = "web/permission";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "permission".to_string())
                .expect("prompt");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "42"
                ))
            });
            directory
                .answer_permission(handle, "42", true, Some("ok"))
                .expect("answer");
            let (events, _) = directory.read_from(handle, 0);
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
fn cancel_records_permission_resolved_for_prompts_it_answered() {
    let dir = scratch_dir("cancel-permission");
    let handle = "web/cancel-permission";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "permission".to_string())
                .expect("prompt");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "42"
                ))
            });

            directory.cancel(handle, false).expect("cancel");

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::PermissionResolved {
                        request_id,
                        approved: false,
                    } if request_id == "42"
                )),
                "cancel left the permission request unresolved: {events:?}"
            );
            assert!(!events.iter().any(|event| matches!(
                event,
                SessionServerEvent::PermissionRequest { request_id, .. } if request_id == "42"
            )));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

fn log_contains_text(directory: &BlockingSessionDirectory, handle: &str, needle: &str) -> bool {
    let (events, _) = directory.read_from(handle, 0);
    events.iter().any(|event| match event {
        SessionServerEvent::Message { text, .. } => text.contains(needle),
        _ => false,
    })
}

fn pump_until_pong_or_turn_end(
    directory: &BlockingSessionDirectory,
    handle: &str,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        let done = events.iter().any(|event| match event {
            SessionServerEvent::TurnEnd { .. } => true,
            SessionServerEvent::Message { text, .. } => text == "pong",
            _ => false,
        });
        if done {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for pong or turn_end; events={events:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn g1_respawns_after_child_death_and_prompt_works() {
    let dir = scratch_dir("g1");
    let script = fake_acp_fixture();
    let handle = "web/g1-respawn";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let pid1 = directory.child_id(handle).expect("pid1");
        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("second acquire");
        let pid2 = directory.child_id(handle).expect("pid2");
        assert_ne!(pid1, pid2);

        directory
            .submit_prompt(handle, "hi".to_string())
            .expect("submit_prompt");
        pump_until_pong_or_turn_end(&directory, handle, Duration::from_secs(5));
        assert!(directory.generation(handle) > 0);
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn g1_load_fail_appends_context_reset_note() {
    let dir = scratch_dir("load-fail");
    let script = fake_acp_fixture();
    let handle = "web/g1-load-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire after load-fail spawn");
        });

        assert!(log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn g1_successful_load_drains_replay_from_transcript() {
    let dir = scratch_dir("load-drain");
    let script = fake_acp_fixture();
    let handle = "web/g1-load-drain";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "user".to_string(),
                text: "seed".to_string(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "prior".to_string(),
                item_id: "prior-agent".to_string(),
                message_id: None,
            },
        );
        let (_, cursor) = directory.read_from(handle, 0);
        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire after successful load");

        let (delta, _) = directory.read_from(handle, cursor);
        assert!(
            !delta.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { text, .. } if text == "replayed"
            )),
            "replayed session/update must not reach the transcript"
        );
        let (full, _) = directory.read_from(handle, 0);
        assert!(!full.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { text, .. } if text == "replayed"
        )));
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
    });

    let _ = std::fs::remove_dir_all(dir);
}
