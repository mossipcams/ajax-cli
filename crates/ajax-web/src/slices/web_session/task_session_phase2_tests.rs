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
fn harness_switch_ends_interrupted_prompt_1189() {
    let dir = scratch_dir("harness-switch-terminal");
    let handle = "web/harness-switch-terminal";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = dir.join("pending-cancel.js");
    std::fs::copy(fake_acp_fixture(), &script).unwrap();
    let source = std::fs::read_to_string(&script).unwrap();
    let cancel_guard = "if (msg.method === 'session/cancel' && heldPromptId !== null) {";
    assert!(source.contains(cancel_guard));
    std::fs::write(&script, source.replace(cancel_guard, "if (false) {")).unwrap();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "interrupted-1".into(), "hold".into())
                .expect("active prompt");
            assert_eq!(
                ledger_phase(&dir, handle, "interrupted-1"),
                Some(PromptPhase::Dispatching)
            );
            directory
                .runtime_handle()
                .block_on(directory.inner().reset_harness_context(
                    handle,
                    &dir,
                    AgentClient::Codex,
                    "auto",
                ))
                .expect("harness switch");

            let stored =
                crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle);
            let terminal = stored.events.iter().position(|event| matches!(
                event,
                SessionServerEvent::TurnEnd { stop_reason: Some(reason) } if reason == "cancelled"
            )).expect("interrupted prompt must end before the harness-switch note");
            let note = stored.events.iter().position(|event| matches!(
                event,
                SessionServerEvent::Message { text, .. } if text == "Client switched harness. Context reset."
            )).expect("harness-switch note");
            assert!(terminal < note);
            assert_eq!(
                stored
                    .events
                    .iter()
                    .filter(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
                    .count(),
                1
            );
            assert_eq!(
                ledger_phase(&dir, handle, "interrupted-1"),
                Some(PromptPhase::Interrupted)
            );
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn normal_cancel_then_harness_switch_emits_one_terminal() {
    let dir = scratch_dir("cancel-switch-terminal");
    let handle = "web/cancel-switch-terminal";
    let directory = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "cancelled-1".into(), "hold".into())
                .expect("active prompt");
            directory.cancel(handle, false).expect("normal cancel");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| matches!(event, SessionServerEvent::TurnEnd { stop_reason: Some(reason) } if reason == "cancelled"))
            });
            directory
                .runtime_handle()
                .block_on(directory.inner().reset_harness_context(
                    handle,
                    &dir,
                    AgentClient::Codex,
                    "auto",
                ))
                .expect("harness switch");
            let (events, _) = directory.read_from(handle, 0);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
                    .count(),
                1
            );
            assert_eq!(
                ledger_phase(&dir, handle, "cancelled-1"),
                Some(PromptPhase::Completed)
            );
        });
    });
    let _ = std::fs::remove_dir_all(dir);
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
            assert!(events.iter().any(|event| matches!(
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
#[test]
fn cross_harness_cold_acquire_does_not_resume_a_foreign_session_id() {
    let dir = scratch_dir("cross-harness-resume");
    let handle = "web/cross-harness-resume";
    let same_dir = scratch_dir("cross-harness-resume-same");
    let same_handle = "web/cross-harness-resume-same";

    // A Claude run must not resume under Codex; a Codex run may resume.
    let store = ajax_core::registry::SqliteRegistryStore::new(dir.join("ajax.db"));
    crate::adapters::web_session_store::try_save_meta(
        &store,
        &dir,
        handle,
        "claude",
        Some("foreign-session-id"),
        "auto",
    )
    .expect("seed stored meta");
    let script = fake_acp_fixture();
    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Codex)
            .expect("cold acquire as codex");
        let stored = crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_ne!(
            stored.acp_session_id.as_deref(),
            Some("foreign-session-id"),
            "a claude session id must never be resumed under codex"
        );

        // Seed a real fixture session so a same-harness cold acquire can resume it.
        let initial_directory = BlockingSessionDirectory::new(same_dir.clone());
        initial_directory
            .acquire(same_handle, &same_dir, "auto", AgentClient::Codex)
            .expect("seed codex session");
        let own_session_id =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&same_dir, same_handle)
                .acp_session_id
                .expect("saved codex session id");
        initial_directory.detach_session(same_handle);
        let same_directory = BlockingSessionDirectory::new(same_dir.clone());
        same_directory
            .acquire(same_handle, &same_dir, "auto", AgentClient::Codex)
            .expect("cold acquire as codex");
        let same_stored =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&same_dir, same_handle);
        assert_eq!(same_stored.acp_session_id, Some(own_session_id));
    });

    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(same_dir);
}

fn assert_failed_context_reset_preserves_live_queue(operation: &str) {
    let dir = scratch_dir(operation);
    let handle = "web/reset-ledger-failure-1324";
    let directory = BlockingSessionDirectory::new(dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire old harness");
            // An untracked active prompt isolates queue-discard persistence from terminal persistence.
            directory
                .submit_prompt_with_id(handle, String::new(), "hold".into())
                .expect("active prompt");
            directory
                .submit_prompt_with_id(handle, "queued-1324".into(), "queued".into())
                .expect("queue prompt");
            let old_pid = directory.child_id(handle).expect("old child");
            let old_generation = directory.generation(handle);
            let old_session =
                crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                    .acp_session_id;
            let fail = ForcePersistFailGuard::enable();
            let result =
                match operation {
                    "switch" => directory.runtime_handle().block_on(
                        directory.inner().reset_harness_context(
                            handle,
                            &dir,
                            AgentClient::Codex,
                            "auto",
                        ),
                    ),
                    "clear" => directory.start_fresh(handle, &dir),
                    "acquire" => directory.acquire(handle, &dir, "auto", AgentClient::Codex),
                    _ => unreachable!(),
                };
            let error = result.expect_err("failed queue discard must fail the reset");
            assert!(
                error.contains("forced prompt ledger persist failure"),
                "{error}"
            );
            assert_eq!(directory.child_id(handle), Some(old_pid));
            assert_eq!(directory.generation(handle), old_generation);
            assert_eq!(directory.eviction_snapshot(handle).unwrap().holders, 1);
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1324"),
                Some(PromptPhase::Queued),
            );
            assert_eq!(
                crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                    .acp_session_id,
                old_session,
            );
            drop(fail);
            directory.pump(handle);
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1324"),
                Some(PromptPhase::Queued),
                "failed reset must not cancel the old client's active prompt",
            );
            directory
                .cancel(handle, true)
                .expect("cancel old active prompt");
            pump_until(&directory, handle, Duration::from_secs(5), |_| {
                matches!(
                    ledger_phase(&dir, handle, "queued-1324"),
                    Some(PromptPhase::Dispatching) | Some(PromptPhase::Completed)
                )
            });
            assert_eq!(directory.child_id(handle), Some(old_pid));
            directory.detach_session(handle);
        });
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1324_harness_switch_ledger_failure_preserves_queue_and_client() {
    assert_failed_context_reset_preserves_live_queue("switch");
}

#[test]
fn issue_1324_clear_context_ledger_failure_preserves_queue_and_client() {
    assert_failed_context_reset_preserves_live_queue("clear");
}

#[test]
fn issue_1324_warm_acquire_propagates_reset_failure() {
    assert_failed_context_reset_preserves_live_queue("acquire");
}

#[test]
fn issue_1324_warm_acquire_converges_to_registry_agent() {
    use ajax_core::registry::{Registry, SqliteRegistryStore};

    let dir = scratch_dir("warm-harness-switch-1324");
    let handle = "web/fix-login";
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Cursor;
    task.worktree_path = dir.clone();
    task.set_skip_interactive_agent(true);
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    store.save(&context.registry).expect("save original agent");
    let directory = BlockingSessionDirectory::new(dir.clone());
    // Distinct fresh IDs let this test detect an accidental resume across clients.
    let script = dir.join("unique-session.js");
    let source = std::fs::read_to_string(fake_acp_fixture()).expect("fixture");
    let session_id = "const sessionId = 'fake-sess-1';";
    assert!(source.contains(session_id));
    std::fs::write(
        &script,
        source.replace(session_id, "const sessionId = `fake-sess-${process.pid}`;"),
    )
    .expect("fixture with unique session IDs");

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire old harness");
        let old_pid = directory.child_id(handle).expect("old child");
        let old_generation = directory.generation(handle);
        let old_session =
            crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle)
                .acp_session_id;
        let task_id = ajax_core::models::TaskId::new(handle);
        context
            .registry
            .get_task_mut(&task_id)
            .unwrap()
            .selected_agent = AgentClient::Codex;
        store
            .save(&context.registry)
            .expect("persist switch intent");
        let registry = store.load().expect("reload registry");
        let new_agent = registry.get_task(&task_id).unwrap().selected_agent;

        directory
            .acquire(handle, &dir, "auto", new_agent)
            .expect("warm acquire must converge");
        let new_pid = directory.child_id(handle).expect("new child");
        assert_ne!(new_pid, old_pid, "must replace the old harness client");
        assert_eq!(directory.generation(handle), old_generation + 1);
        assert_eq!(directory.eviction_snapshot(handle).unwrap().holders, 2);
        let stored = crate::adapters::web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(stored.harness, "codex");
        assert_ne!(stored.acp_session_id, old_session);
        assert_eq!(
            stored
                .events
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionServerEvent::Message { text, .. }
                        if text == "Client switched harness. Context reset."
                ))
                .count(),
            1
        );

        directory
            .acquire(handle, &dir, "auto", new_agent)
            .expect("same-harness acquire");
        assert_eq!(directory.child_id(handle), Some(new_pid));
        assert_eq!(directory.generation(handle), old_generation + 1);
        directory.detach_session(handle);
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1324_idle_switch_discards_queued_prompts() {
    use crate::adapters::web_session_store::prompt_ledger;
    use ajax_core::registry::SqliteRegistryStore;

    let dir = scratch_dir("idle-switch-queue-1324");
    let handle = "web/idle-switch-queue-1324";
    let directory = BlockingSessionDirectory::new(dir.clone());
    assert!(!directory.inner().has_live_entry(handle));

    // No live session: seed the SQLite ledger with queued prompts directly.
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut ledger = prompt_ledger::try_load(&store, &dir, handle).expect("load empty ledger");
    ledger.upsert_queued(
        "queued-a-1324".into(),
        "queued a".into(),
        "queued a".into(),
        Vec::new(),
    );
    ledger.upsert_queued(
        "queued-b-1324".into(),
        "queued b".into(),
        "queued b".into(),
        Vec::new(),
    );
    prompt_ledger::try_persist(&store, &dir, handle, &ledger).expect("persist queued prompts");

    directory
        .runtime_handle()
        .block_on(
            directory
                .inner()
                .reset_harness_context(handle, &dir, AgentClient::Codex, "auto"),
        )
        .expect("idle harness switch");

    let reloaded = prompt_ledger::try_load(&store, &dir, handle).expect("reload ledger");
    assert_eq!(reloaded.queued_count(), 0);
}

fn fake_acp_exclusive_lock_pid(dir: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(dir.join(".fake-acp-exclusive-lock"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn assert_no_live_fake_acp_fixture(dir: &std::path::Path) {
    if let Some(pid) = fake_acp_exclusive_lock_pid(dir) {
        assert!(
            !pid_alive(pid),
            "discarded ACP child pid {pid} must not keep running"
        );
    }
}

#[test]
fn issue_1327_cold_acquire_fails_when_child_identity_cannot_be_recorded() {
    use crate::adapters::web_session_store::set_force_child_identity_write_fail;

    let dir = scratch_dir("child-identity-acquire-fail-1327");
    let handle = "web/child-identity-acquire-fail-1327";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--exclusive-session-new"], || {
            set_force_child_identity_write_fail(true);
            let error = directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect_err("child identity persist failure must fail cold acquire")
                .to_string();
            set_force_child_identity_write_fail(false);
            assert!(
                error.contains("child identity could not be recorded"),
                "{error}"
            );
            assert!(
                directory.child_id(handle).is_none(),
                "failed acquire must not leave a live client"
            );
            assert_no_live_fake_acp_fixture(&dir);

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire succeeds once child identity can be recorded");
            let run = super::SessionStorage::under(dir.clone())
                .registry_store()
                .latest_run_for_task(handle)
                .unwrap()
                .unwrap();
            let pid = directory.child_id(handle).expect("child after success");
            assert_eq!(run.child, Some((pid as i64, pid as i64)));
            assert_eq!(
                run.child_started,
                super::process_identity::start_token(pid as i64)
            );
        });
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1327_replacement_acquire_fails_when_child_identity_cannot_be_recorded() {
    use crate::adapters::web_session_store::set_force_child_identity_write_fail;

    let dir = scratch_dir("child-identity-replace-fail-1327");
    let handle = "web/child-identity-replace-fail-1327";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--exclusive-session-new"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("initial acquire");
            directory.kill_host_for_test(handle);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                exit_error_count(events) == 1
            });

            set_force_child_identity_write_fail(true);
            let error = directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect_err("child identity persist failure must fail replacement acquire")
                .to_string();
            set_force_child_identity_write_fail(false);
            assert!(
                error.contains("child identity could not be recorded"),
                "{error}"
            );
            assert!(
                directory.child_id(handle).is_none(),
                "failed replacement must not leave a live client"
            );
            assert_no_live_fake_acp_fixture(&dir);

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("replacement acquire after hook disabled");
            assert!(directory.child_id(handle).is_some());
        });
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}
