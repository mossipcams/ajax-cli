use super::test_support::{
    agent_pong_count, fake_acp_fixture, log_contains_text, note, pump_until,
    pump_until_pong_or_turn_end, scratch_dir, BlockingSessionDirectory, CONTEXT_RESET_NOTE,
};
use super::{SessionServerEvent, MAX_QUEUED_PROMPTS};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store::{
    self,
    prompt_ledger::{self, PromptLedger},
};
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
    web_session_store::append_events(&dir, handle, &events).unwrap();
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
fn viewer_disconnect_release_reacquire_preserves_live_context_and_queued_work() {
    let dir = scratch_dir("viewer-disconnect-context");
    let handle = "web/viewer-disconnect-context";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let child_before = directory.child_id(handle).expect("child before disconnect");
        let session_before = directory
            .stored_acp_session_id(&dir, handle)
            .expect("stored session id before disconnect");
        let epoch_before = directory
            .attach_snapshot(handle, "auto")
            .snapshot
            .context_epoch;

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
                "queued work must continue without a viewer lease: {events:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire after viewer disconnect");
        assert_eq!(
            directory.child_id(handle),
            Some(child_before),
            "viewer reconnect must reuse the same live ACP child"
        );
        assert_eq!(
            directory.stored_acp_session_id(&dir, handle),
            Some(session_before.clone()),
            "viewer reconnect must not replace the stored ACP session id"
        );
        assert_eq!(
            directory
                .attach_snapshot(handle, "auto")
                .snapshot
                .context_epoch,
            epoch_before,
            "viewer reconnect must not advance context epoch"
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn queued_prompts_continue_after_operator_disconnects() {
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
fn prompt_submitted_while_busy_runs_after_active_prompt() {
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
            let (events, _) = directory.read_from(handle, 0);
            let submitted: Vec<&str> = events
                .iter()
                .filter_map(|event| match event {
                    SessionServerEvent::Message { role, text, .. } if role == "user" => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(submitted, ["first", "second"]);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn submit_prompt_cap_rejects_without_dropping_acknowledged_queue() {
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
                    .submit_prompt_with_id(handle, format!("queued-{i}"), format!("q{i}"))
                    .expect("queue");
            }
            let overflow = directory.submit_prompt_with_id(
                handle,
                "overflow".to_string(),
                "overflow".to_string(),
            );
            assert_eq!(overflow, Err("prompt queue is full".to_string()));

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
fn cancel_can_discard_queued_prompts() {
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
fn cancel_can_preserve_queued_prompts() {
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
fn answer_permission_records_resolved_when_acp_request_is_gone_issue_1018() {
    let dir = scratch_dir("permission-stale-answer");
    let handle = "web/permission-stale-answer";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.record(
            handle,
            SessionServerEvent::PermissionRequest {
                request_id: "p1".to_string(),
                title: Some("Run?".to_string()),
                detail: None,
            },
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(
            directory
                .inner()
                .answer_permission(handle, "p1", true, None),
        )
        .expect("stale permission answer should succeed");

        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| matches!(
            event,
            SessionServerEvent::PermissionResolved {
                request_id,
                approved: true,
            } if request_id == "p1"
        )));

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert!(attach.snapshot.pending_permission.is_none());
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn permission_auto_approved_without_surfacing_operator_prompt() {
    let dir = scratch_dir("permission-auto");
    let handle = "web/permission-auto";
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
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::PermissionRequest { .. })),
                "auto-approved permission must not surface: {events:?}"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancel_ends_turn_after_auto_approved_permission_hold() {
    let dir = scratch_dir("cancel-permission-hold");
    let handle = "web/cancel-permission-hold";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--permission", "--permission-hold"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "permission".to_string())
                .expect("prompt");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "agent" && text == "permission:selected:allow-once"
                    )
                })
            });

            directory.cancel(handle, false).expect("cancel");

            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::TurnEnd {
                            stop_reason: Some(reason)
                        } if reason == "cancelled"
                    )
                })
            });

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::PermissionRequest { .. })),
                "auto-approved permission must not surface: {events:?}"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn duplicate_prompt_id_uses_ledger_not_transcript_for_dedupe() {
    let dir = scratch_dir("ledger-dedupe");
    let handle = "web/ledger-dedupe";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt_with_id(handle, "prompt-1".to_string(), "first".to_string())
            .expect("first");
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            agent_pong_count(events) == 1
        });

        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "prompt-1".to_string(),
            "first".to_string(),
            "first".to_string(),
            Vec::new(),
        );
        ledger.mark_completed("prompt-1");
        prompt_ledger::persist(&dir, handle, &ledger).expect("seed ledger");

        directory
            .submit_prompt_with_id(handle, "prompt-1".to_string(), "retry".to_string())
            .expect("duplicate ack");
        pump_until(&directory, handle, Duration::from_secs(2), |_| true);
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(agent_pong_count(&events), 1);
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { role, text, .. }
                if role == "user" && text == "retry"
        )));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn recover_queued_prompts_after_session_recreation() {
    let dir = scratch_dir("ledger-recover-queued");
    let handle = "web/ledger-recover-queued";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "in-flight".to_string(), "hold".to_string())
                .expect("hold");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "recovered".to_string())
                .expect("queue");

            directory.release(handle);
            drop(directory);
        });

        let ledger = prompt_ledger::load(&dir, handle).expect("load ledger");
        assert!(ledger.entry("queued-1").is_some());

        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire");

        pump_until(&directory, handle, Duration::from_secs(10), |events| {
            agent_pong_count(events) >= 1
                && events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "recovered"
                    )
                })
        });
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(agent_pong_count(&events), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn recover_dispatching_prompt_is_interrupted_without_retry() {
    let dir = scratch_dir("ledger-recover-dispatching");
    let handle = "web/ledger-recover-dispatching";
    let script = fake_acp_fixture();

    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "dispatching-1".to_string(),
        "orphan".to_string(),
        "orphan".to_string(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("dispatching-1"));
    prompt_ledger::persist(&dir, handle, &ledger).expect("seed ledger");

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let (events, _) = directory.read_from(handle, 0);
        assert!(events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Error { message }
                if message.contains("dispatching-1") && message.contains("interrupted")
        )));
        assert_eq!(
            prompt_ledger::load(&dir, handle)
                .expect("load ledger")
                .entry("dispatching-1")
                .map(|entry| entry.phase),
            Some(prompt_ledger::PromptPhase::Interrupted)
        );
    });

    let _ = std::fs::remove_dir_all(dir);
}

fn ledger_phase(
    state_dir: &std::path::Path,
    handle: &str,
    client_message_id: &str,
) -> Option<prompt_ledger::PromptPhase> {
    prompt_ledger::load(state_dir, handle)
        .ok()?
        .entry(client_message_id)
        .map(|entry| entry.phase)
}

#[test]
fn thought_only_turn_completes_from_command_result_not_stream_chunk() {
    let dir = scratch_dir("thought-only");
    let handle = "web/thought-only";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--thought-only"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "thought-1".to_string(), "think".to_string())
                .expect("submit");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "thought" && text == "thinking-only"
            )));
            assert_eq!(
                ledger_phase(&dir, handle, "thought-1"),
                Some(prompt_ledger::PromptPhase::Completed)
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tool_only_turn_completes_from_command_result() {
    let dir = scratch_dir("tool-only");
    let handle = "web/tool-only";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--tool-only"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "tool-1".to_string(), "tools".to_string())
                .expect("submit");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
            assert_eq!(
                ledger_phase(&dir, handle, "tool-1"),
                Some(prompt_ledger::PromptPhase::Completed)
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn no_agent_text_turn_completes_from_command_result() {
    let dir = scratch_dir("no-agent-text");
    let handle = "web/no-agent-text";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--no-agent-text"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "empty-1".to_string(), "silent".to_string())
                .expect("submit");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
            assert_eq!(
                ledger_phase(&dir, handle, "empty-1"),
                Some(prompt_ledger::PromptPhase::Completed)
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_prompt_rpc_error_marks_interrupted_and_blocks_retry() {
    let dir = scratch_dir("prompt-fail");
    let handle = "web/prompt-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--prompt-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "fail-1".to_string(), "boom".to_string())
                .expect("dispatch");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Error { message } if message.contains("prompt failed")
                    )
                })
            });
            assert_eq!(
                ledger_phase(&dir, handle, "fail-1"),
                Some(prompt_ledger::PromptPhase::Interrupted)
            );
            let retry =
                directory.submit_prompt_with_id(handle, "fail-1".to_string(), "retry".to_string());
            assert!(retry.is_err());
            assert!(retry
                .unwrap_err()
                .contains("interrupted and was not executed"));
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancel_finalizes_active_prompt_once_and_advances_queue_in_order() {
    let dir = scratch_dir("cancel-ledger-order");
    let handle = "web/cancel-ledger-order";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "hold".to_string())
                .expect("active");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "next".to_string())
                .expect("queued");
            directory.cancel(handle, true).expect("cancel");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "next"
                    )
                }) && events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::TurnEnd {
                            stop_reason: Some(reason)
                        } if reason == "cancelled"
                    )
                })
            });
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(prompt_ledger::PromptPhase::Completed)
            );
            directory.pump(handle);
            directory.pump(handle);
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(prompt_ledger::PromptPhase::Completed),
                "duplicate terminal transition must not rewrite ledger"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
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
fn issue_1031_load_replay_not_in_transcript_after_attach_pump() {
    let dir = scratch_dir("load-drain-pump");
    let script = fake_acp_fixture();
    let handle = "web/issue-1031-load-pump";
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
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        let (_, cursor) = directory.read_from(handle, 0);
        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire after successful load");
        directory.pump(handle);

        let (delta, _) = directory.read_from(handle, cursor);
        assert!(
            !delta.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { text, .. } if text == "replayed"
            )),
            "load replay must not reach JSONL after attach pump (#1031)"
        );
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
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "prior".to_string(),
                content_blocks: Vec::new(),
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
