use super::task_session_exit::{ledger_phase, ACP_PROCESS_EXITED};
use super::test_support::{fake_acp_fixture, pump_until, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store::prompt_ledger::{ForcePersistFailGuard, PromptPhase};
use ajax_core::models::AgentClient;
use std::{thread, time::Duration};

fn exit_error_count(events: &[SessionServerEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                SessionServerEvent::Error { message } if message == ACP_PROCESS_EXITED
            )
        })
        .count()
}

#[test]
fn exit_during_prompt_interrupts_active_row_and_preserves_queue() {
    let dir = scratch_dir("phase2-exit-prompt");
    let handle = "web/phase2-exit-prompt";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("active");
            directory
                .submit_prompt_with_id(handle, "queued-1".into(), "queued".into())
                .expect("queued");
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                exit_error_count(events) == 1
                    && events.iter().any(|event| {
                        matches!(
                            event,
                            SessionServerEvent::Error { message }
                                if message.contains("active-1") && message.contains("interrupted")
                        )
                    })
            });
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Interrupted)
            );
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1"),
                Some(PromptPhase::Queued)
            );
            let (events, _) = directory.read_from(handle, 0);
            assert!(!events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "queued"
            )));
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn idle_unexpected_exit_replacement_dispatches_preserved_queue() {
    let dir = scratch_dir("phase2-idle-exit-queue");
    let handle = "web/phase2-idle-exit-queue";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt_with_id(handle, "queued-1".into(), "first".into())
            .expect("queued only");
        directory.kill_host_for_test(handle);
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            exit_error_count(events) == 1
        });
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("replacement acquire");
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "user" && text == "first"
                )
            })
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unexpected_exit_reconciliation_is_idempotent() {
    let dir = scratch_dir("phase2-exit-idempotent");
    let handle = "web/phase2-exit-idempotent";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.kill_host_for_test(handle);
        for _ in 0..5 {
            directory.pump(handle);
        }
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(exit_error_count(&events), 1);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn replacement_reaps_prior_child_before_installing_new_one() {
    let dir = scratch_dir("phase2-reap-prior");
    let handle = "web/phase2-reap-prior";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--exclusive-session-new"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            let pid1 = directory.child_id(handle).expect("pid1");
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                exit_error_count(events) == 1
            });
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("replacement acquire must reap dead child first");
            let pid2 = directory.child_id(handle).expect("pid2");
            assert_ne!(pid1, pid2);
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1086_exit_interruption_persist_failure_blocks_healthy_replacement() {
    let dir = scratch_dir("phase2-issue-1086");
    let handle = "web/phase2-issue-1086";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("active");
            directory
                .submit_prompt_with_id(handle, "queued-1".into(), "next".into())
                .expect("queued");
            let _fail = ForcePersistFailGuard::enable();
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                exit_error_count(events) >= 1
            });
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Dispatching)
            );
            let blocked = directory.acquire(handle, &dir, "auto", AgentClient::Cursor);
            assert!(
                blocked.is_err(),
                "replacement must not install while exit interruption is pending"
            );
            assert_eq!(
                directory
                    .eviction_snapshot(handle)
                    .expect("snapshot")
                    .holders,
                1,
                "blocked replacement acquire must not leak a holder"
            );
            drop(_fail);
            thread::sleep(Duration::from_millis(100));
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("later acquire retries recovery");
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Interrupted)
            );
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "next"
                    )
                })
            });
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn expected_detach_does_not_emit_unexpected_exit_error() {
    let dir = scratch_dir("phase2-detach-no-exit-error");
    let handle = "web/phase2-detach-no-exit-error";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.detach_session(handle);
        thread::sleep(Duration::from_millis(200));
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(exit_error_count(&events), 0);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn late_exited_after_reconcile_strips_duplicate_exit_errors() {
    let dir = scratch_dir("phase2-late-exited");
    let handle = "web/phase2-late-exited";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.kill_host_for_test(handle);
        for _ in 0..10 {
            directory.pump(handle);
        }
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(exit_error_count(&events), 1);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn child_exit_after_successful_prompt_terminal_stays_completed() {
    let dir = scratch_dir("phase2-exit-after-success");
    let handle = "web/phase2-exit-after-success";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory
            .submit_prompt_with_id(handle, "active-1".into(), "hello".into())
            .expect("submit");
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::TurnEnd { stop_reason: Some(reason) }
                        if reason == "end_turn"
                )
            })
        });
        assert_eq!(
            ledger_phase(&dir, handle, "active-1"),
            Some(PromptPhase::Completed)
        );
        directory.kill_host_for_test(handle);
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            exit_error_count(events) == 1
        });
        assert_eq!(
            ledger_phase(&dir, handle, "active-1"),
            Some(PromptPhase::Completed)
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Error { message }
                if message.contains("active-1") && message.contains("interrupted")
        )));
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn child_exit_after_terminal_captured_does_not_mark_interrupted() {
    let dir = scratch_dir("phase2-exit-terminal-persist-fail");
    let handle = "web/phase2-exit-terminal-persist-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("submit");
            let _fail = ForcePersistFailGuard::enable();
            directory.cancel(handle, true).expect("cancel");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Error { message }
                            if message.contains("failed to persist prompt ownership")
                    )
                })
            });
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                exit_error_count(events) >= 1
            });
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Dispatching)
            );
            let (events, _) = directory.read_from(handle, 0);
            assert!(!events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Error { message }
                    if message.contains("active-1")
                        && message.contains("interrupted")
                        && message.contains("ACP process exited")
            )));
            drop(_fail);
            pump_until(&directory, handle, Duration::from_secs(5), |_| {
                ledger_phase(&dir, handle, "active-1") == Some(PromptPhase::Completed)
            });
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}
