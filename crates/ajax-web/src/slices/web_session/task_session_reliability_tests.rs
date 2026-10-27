use super::test_support::{
    agent_pong_count, fake_acp_fixture, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::SessionServerEvent;
use super::{
    acp_drain::{PromptTerminal, PromptTerminalOutcome},
    task_session::ActivePrompt,
    SessionActivity,
};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger::{
    self, ForcePersistFailGuard, PromptLedger, PromptPhase,
};
use ajax_core::models::AgentClient;
use std::sync::Arc;
use std::{thread, time::Duration};

fn ledger_phase(state_dir: &std::path::Path, handle: &str, id: &str) -> Option<PromptPhase> {
    prompt_ledger::load(state_dir, handle)
        .ok()?
        .entry(id)
        .map(|entry| entry.phase)
}

#[test]
fn mismatched_terminal_request_does_not_finish_active_prompt() {
    let mut active = ActivePrompt::new(41, Some("active-1".to_string()));
    assert!(!active.capture_terminal(PromptTerminal {
        request_id: 40,
        outcome: PromptTerminalOutcome::Success,
        events: Vec::new(),
    }));
    assert!(!active.has_pending_terminal());
    assert_eq!(active.request_id(), 41);
    assert!(active.capture_terminal(PromptTerminal {
        request_id: 41,
        outcome: PromptTerminalOutcome::Success,
        events: Vec::new(),
    }));
    assert!(!active.capture_terminal(PromptTerminal {
        request_id: 41,
        outcome: PromptTerminalOutcome::Failed,
        events: Vec::new(),
    }));
}

#[test]
fn ledger_write_failure_rejects_submit_without_acknowledgement() {
    let dir = scratch_dir("ledger-write-fail");
    let handle = "web/ledger-write-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let _fail = ForcePersistFailGuard::enable();
        let result = directory.submit_prompt_with_id(handle, "prompt-fail".into(), "hello".into());
        drop(_fail);
        assert!(result.is_err(), "persist failure must reject submit");
        let (events, _) = directory.read_from(handle, 0);
        assert!(!events.iter().any(|event| matches!(
            event,
            SessionServerEvent::PromptAccepted { client_message_id }
                if client_message_id == "prompt-fail"
        )));
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn terminal_persist_failure_retries_before_fifo_advances() {
    let dir = scratch_dir("terminal-persist-retry");
    let handle = "web/terminal-persist-retry";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            run_terminal_persist_retry(&directory, &dir, handle);
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

fn run_terminal_persist_retry(
    directory: &BlockingSessionDirectory,
    dir: &std::path::Path,
    handle: &str,
) {
    directory
        .acquire(handle, dir, "auto", AgentClient::Cursor)
        .expect("acquire");
    directory
        .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
        .expect("active");
    directory
        .submit_prompt_with_id(handle, "queued-1".into(), "next".into())
        .expect("queued");
    let _fail = ForcePersistFailGuard::enable();
    directory.cancel(handle, true).expect("cancel");
    thread::sleep(Duration::from_millis(150));
    drop(_fail);
    assert_eq!(
        ledger_phase(dir, handle, "active-1"),
        Some(PromptPhase::Dispatching)
    );
    pump_until(directory, handle, Duration::from_secs(5), |events| {
        events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, text, .. } if role == "user" && text == "next"
            )
        })
    });
    assert_eq!(
        ledger_phase(dir, handle, "active-1"),
        Some(PromptPhase::Completed)
    );
}

#[test]
fn queued_dispatch_persist_failure_keeps_prompt_for_retry() {
    let dir = scratch_dir("dispatch-persist-retry");
    let handle = "web/dispatch-persist-retry";
    let script = fake_acp_fixture();
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "queued-1".into(),
        "recovered".into(),
        "recovered".into(),
        Vec::new(),
    );
    prompt_ledger::persist(&dir, handle, &ledger).expect("seed ledger");
    web_session_store::append_events(
        &dir,
        handle,
        &[SessionServerEvent::Message {
            role: "user".to_string(),
            text: "recovered".to_string(),
            content_blocks: Vec::new(),
            item_id: "u:queued-1".to_string(),
            message_id: None,
        }],
    );

    with_test_acp_program(&script, || run_dispatch_persist_retry(&dir, handle));
    let _ = std::fs::remove_dir_all(dir);
}

fn run_dispatch_persist_retry(dir: &std::path::Path, handle: &str) {
    let _fail = ForcePersistFailGuard::enable();
    let directory = BlockingSessionDirectory::new(dir.to_path_buf());
    directory
        .acquire(handle, dir, "auto", AgentClient::Cursor)
        .expect("acquire");
    thread::sleep(Duration::from_millis(150));
    let (events, _) = directory.read_from(handle, 0);
    drop(_fail);
    assert_eq!(
        ledger_phase(dir, handle, "queued-1"),
        Some(PromptPhase::Queued)
    );
    assert_eq!(agent_pong_count(&events), 0);
    pump_until(&directory, handle, Duration::from_secs(5), |events| {
        agent_pong_count(events) >= 1
    });
}

#[test]
fn first_acquire_recover_failure_leaves_no_durable_spawn_side_effects() {
    let dir = scratch_dir("first-acquire-recover-fail");
    let handle = "web/first-acquire-recover-fail";
    let script = fake_acp_fixture();
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "orphan".into(),
        "orphan".into(),
        "orphan".into(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("orphan"));
    prompt_ledger::persist(&dir, handle, &ledger).expect("seed ledger");
    web_session_store::append_events(
        &dir,
        handle,
        &[SessionServerEvent::Message {
            role: "user".to_string(),
            text: "seed".to_string(),
            content_blocks: Vec::new(),
            item_id: "seed".to_string(),
            message_id: None,
        }],
    );

    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        let _fail = ForcePersistFailGuard::enable();
        let result = directory.acquire(handle, &dir, "auto", AgentClient::Cursor);
        drop(_fail);
        assert!(
            result.is_err(),
            "recover must fail before durable spawn metadata is written"
        );
        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert!(stored.acp_session_id.is_none());
        assert!(
            !stored.events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "note" && text == CONTEXT_RESET_NOTE
            )),
            "failed first acquire must not persist a context-reset note"
        );
    });
    let _ = std::fs::remove_dir_all(dir);
}

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

#[test]
fn activity_report_failure_does_not_block_prompt_submit_or_dispatch() {
    let dir = scratch_dir("activity-report-fault");
    let handle = "web/activity-report-fault";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    directory.inner().set_report_session_activity(Arc::new(
        |_qualified_handle: &str, _activity: SessionActivity| false,
    ));

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("first submit must succeed before activity fault");

            directory.record(
                handle,
                SessionServerEvent::TurnEnd {
                    stop_reason: Some("end_turn".to_string()),
                },
            );

            let generation = directory.generation(handle);
            let (_, cursor) = directory.read_from(handle, 0);
            let batch = directory.collect_outbound(handle, cursor, generation);
            let snapshot = batch
                .snapshot
                .expect("activity report failure must push outbound snapshot");
            assert!(
                snapshot
                    .transcript_error
                    .as_deref()
                    .is_some_and(|msg| msg.contains("task activity report failed")),
                "activity fault surfaces on outbound snapshot: {:?}",
                snapshot.transcript_error
            );

            let result =
                directory.submit_prompt_with_id(handle, "next-1".into(), "after-fault".into());
            assert!(
                result.is_ok(),
                "activity report failure must not latch transcript durability: {result:?}"
            );
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn queued_while_busy_user_message_is_in_log_before_turn_end() {
    let dir = scratch_dir("queued-jsonl");
    let handle = "web/queued-jsonl";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "second".to_string())
                .expect("queued");

            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| {
                matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "user" && text == "second"
                )
            }));
            assert!(!events
                .iter()
                .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. })));
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1"),
                Some(PromptPhase::Queued)
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn updating_queued_prompt_id_does_not_dispatch_twice() {
    let dir = scratch_dir("queued-update");
    let handle = "web/queued-update";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "draft".to_string())
                .expect("queued");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "revised".to_string())
                .expect("update");

            directory.cancel(handle, true).expect("cancel");
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "revised"
                    )
                }) && events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { text, .. } if text == "pong"
                    )
                })
            });
            let (events, _) = directory.read_from(handle, 0);
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "revised"
                    ))
                    .count(),
                1
            );
            assert_eq!(agent_pong_count(&events), 1);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn empty_text_withdraw_removes_queued_ledger_row() {
    let dir = scratch_dir("queued-withdraw");
    let handle = "web/queued-withdraw";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "second".to_string())
                .expect("queued");
            assert_eq!(
                ledger_phase(&dir, handle, "queued-1"),
                Some(PromptPhase::Queued)
            );
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), String::new())
                .expect("withdraw");
            assert_eq!(ledger_phase(&dir, handle, "queued-1"), None);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn transcript_append_failure_backlog_recovers_in_order() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("append-backlog-retry");
    let handle = "web/append-backlog-retry";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "first".into())
                .expect("active");

            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
                .expect("chmod state dir read-only");
            directory.record(
                handle,
                SessionServerEvent::Message {
                    role: "note".to_string(),
                    text: "backlog-a".to_string(),
                    content_blocks: Vec::new(),
                    item_id: "backlog-a".to_string(),
                    message_id: None,
                },
            );
            directory.record(
                handle,
                SessionServerEvent::Message {
                    role: "note".to_string(),
                    text: "backlog-b".to_string(),
                    content_blocks: Vec::new(),
                    item_id: "backlog-b".to_string(),
                    message_id: None,
                },
            );

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::Message { text, .. }
                        if text == "backlog-a" || text == "backlog-b"
                )),
                "failed appends must not reach the log: {events:?}"
            );

            let fail_terminal = ForcePersistFailGuard::enable();
            directory
                .cancel(handle, true)
                .expect("cancel accepted for retry");
            thread::sleep(Duration::from_millis(200));
            assert_ne!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Completed),
                "turn must not complete while the backlog is unpersisted"
            );

            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
                .expect("chmod state dir writable");
            let (pending, _) = directory.read_from(handle, 0);
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Dispatching),
                "a failed terminal transaction must retain the active turn"
            );
            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            for events in [&pending, &stored.events] {
                assert!(
                    !events.iter().any(|event| matches!(
                        event,
                        SessionServerEvent::Message { text, .. }
                            if text == "backlog-a" || text == "backlog-b"
                    )),
                    "backlog must wait for the terminal transaction: {events:?}"
                );
            }
            drop(fail_terminal);
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { text, .. } if text == "backlog-b"
                    )
                })
            });

            let (events, _) = directory.read_from(handle, 0);
            let index_of = |text: &str| {
                events.iter().position(|event| {
                    matches!(event, SessionServerEvent::Message { text: t, .. } if t == text)
                })
            };
            let a = index_of("backlog-a").expect("backlog-a must persist after recovery");
            let b = index_of("backlog-b").expect("backlog-b must persist after recovery");
            assert!(a < b, "backlog must persist oldest-first: {events:?}");
            let terminal = events
                .iter()
                .position(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
                .expect("terminal event persisted");
            assert!(b < terminal, "backlog must precede turn completion");
            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            assert_eq!(
                stored.events, events,
                "durable and visible order must match"
            );
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Completed)
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn issue_1325_next_append_persists_backlog_before_new_events() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir("append-backlog-next-batch");
    let handle = "web/append-backlog-next-batch";
    let script = fake_acp_fixture();
    with_test_acp_program(&script, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let message = |text: &str| SessionServerEvent::Message {
            role: "note".into(),
            text: text.into(),
            content_blocks: Vec::new(),
            item_id: text.into(),
            message_id: None,
        };
        let (_, before) = directory.read_from(handle, 0);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
            .expect("block append");
        directory.record(handle, message("first"));
        let (failed, cursor) = directory.read_from(handle, before);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
            .expect("restore appends");
        assert!(failed.is_empty());
        assert_eq!(cursor, before, "failed append must not consume a sequence");

        directory.record(handle, message("second"));
        let (events, cursor) = directory.read_from(handle, before);
        assert_eq!(events, vec![message("first"), message("second")]);
        assert_eq!(cursor, before + 2);
        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        assert_eq!(&stored.events[before..], events.as_slice());
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn issue_1325_activity_waits_for_append_retry_commit() {
    assert_activity_waits_for_transcript_commit(false);
}

#[cfg(unix)]
#[test]
fn issue_1325_activity_waits_for_terminal_retry_commit() {
    assert_activity_waits_for_transcript_commit(true);
}

#[cfg(unix)]
fn assert_activity_waits_for_transcript_commit(recover_with_terminal: bool) {
    use std::{os::unix::fs::PermissionsExt, sync::Mutex};

    let dir = scratch_dir("activity-after-transcript");
    let handle = "web/activity-after-transcript";
    let reports = Arc::new(Mutex::new(Vec::new()));
    let permission = SessionServerEvent::PermissionRequest {
        request_id: "persist-first".into(),
        title: Some("Run?".into()),
        detail: None,
    };
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = Arc::clone(&reports);
    let report_dir = dir.clone();
    directory.inner().set_report_session_activity(Arc::new(
        move |handle: &str, activity: SessionActivity| {
            let stored = web_session_store::load::<SessionServerEvent>(&report_dir, handle);
            let phase = ledger_phase(&report_dir, handle, "active-1");
            reported
                .lock()
                .unwrap()
                .push((activity, stored.events, phase));
            true
        },
    ));

    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("active prompt");
            let (_, before) = directory.read_from(handle, 0);
            reports.lock().unwrap().clear();

            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
                .expect("block transcript append");
            directory.record(handle, permission.clone());
            let (failed, cursor) = directory.read_from(handle, before);
            let fail_terminal = ForcePersistFailGuard::enable();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
                .expect("restore transcript append");
            assert!(failed.is_empty(), "failed batch must not be visible");
            assert_eq!(cursor, before);
            assert!(
                reports.lock().unwrap().is_empty(),
                "failed append must not report activity or status"
            );
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Dispatching)
            );

            if recover_with_terminal {
                directory.cancel(handle, true).expect("cancel");
                thread::sleep(Duration::from_millis(200));
                let (pending, _) = directory.read_from(handle, before);
                assert!(pending.is_empty(), "failed terminal must retain backlog");
                assert!(
                    reports.lock().unwrap().is_empty(),
                    "failed terminal commit must not report backlog or terminal activity"
                );
                assert_eq!(
                    ledger_phase(&dir, handle, "active-1"),
                    Some(PromptPhase::Dispatching),
                    "turn must remain active until backlog and terminal persist"
                );
            } else {
                directory.record(
                    handle,
                    SessionServerEvent::Message {
                        role: "note".into(),
                        text: "after backlog".into(),
                        content_blocks: Vec::new(),
                        item_id: "after-backlog".into(),
                        message_id: None,
                    },
                );
                let (events, _) = directory.read_from(handle, before);
                assert_eq!(events.len(), 2);
                assert_eq!(events[0], permission);
                assert!(
                    matches!(&events[1], SessionServerEvent::Message { text, .. }
                    if text == "after backlog")
                );
                assert_eq!(
                    reports.lock().unwrap()[0].0,
                    SessionActivity::AwaitingOperator
                );
            }
            drop(fail_terminal);
            if !recover_with_terminal {
                directory.cancel(handle, true).expect("cancel");
            }
            pump_until(&directory, handle, Duration::from_secs(5), |events| {
                events
                    .iter()
                    .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
            });
            let (events, _) = directory.read_from(handle, before);
            assert_eq!(
                events[0], permission,
                "backlog must precede terminal events"
            );
            assert!(matches!(
                events.last(),
                Some(SessionServerEvent::TurnEnd { .. })
            ));
            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            assert_eq!(&stored.events[before..], events.as_slice());
            assert_eq!(
                ledger_phase(&dir, handle, "active-1"),
                Some(PromptPhase::Completed)
            );

            directory.read_from(handle, 0);
            let reports = reports.lock().unwrap();
            let waiting: Vec<_> = reports
                .iter()
                .filter(|(activity, _, _)| *activity == SessionActivity::AwaitingOperator)
                .collect();
            assert_eq!(
                waiting.len(),
                1,
                "backlog activity must be reported exactly once"
            );
            assert!(
                waiting[0].1.contains(&permission),
                "activity must follow durable events"
            );
            let terminal_activity = if recover_with_terminal {
                SessionActivity::TurnFailed
            } else {
                SessionActivity::TurnEnded
            };
            let ended: Vec<_> = reports
                .iter()
                .filter(|(activity, _, _)| *activity == terminal_activity)
                .collect();
            assert_eq!(
                ended.len(),
                1,
                "terminal activity must be reported exactly once"
            );
            assert_eq!(ended[0].2, Some(PromptPhase::Completed));
            assert_eq!(
                ended[0].1, stored.events,
                "completion must follow the transcript commit"
            );
        });
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn acquire_blocks_while_foreign_live_child_is_recorded_issue_1327() {
    use super::process_identity;
    use super::SessionStorage;
    use crate::adapters::web_session_store::record_child_identity;
    use ajax_core::models::AgentClient;
    use std::process::{Command, Stdio};

    fn spawn_sleep_child() -> (i64, std::process::Child) {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        (child.id() as i64, child)
    }

    let dir = scratch_dir("foreign-child-acquire-block");
    let handle = "web/foreign-child-acquire-block";
    let store = SessionStorage::under(dir.clone()).registry_store();
    super::orphan_sweep::sweep_once(&store, &dir, "test-host");
    let (pid, mut sleep_child) = spawn_sleep_child();
    let token = process_identity::start_token(pid).expect("live child token");
    record_child_identity(
        &store,
        &dir,
        handle,
        "cursor",
        Some((pid, token.clone())),
        "foreign-boot",
    )
    .expect("seed foreign child row");

    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        let err = directory
            .acquire_typed(handle, &dir, "auto", AgentClient::Cursor)
            .expect_err("acquire must fail while foreign child is live");
        assert!(
            err.to_string().contains("still shutting down"),
            "unexpected error: {err}"
        );
        assert!(
            directory.child_id(handle).is_none(),
            "no ACP client must be created while blocked"
        );

        Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("kill sleep helper");
        sleep_child.wait().expect("wait on sleep helper");

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire after foreign child is gone");
        let acp_pid = directory.child_id(handle).expect("ACP child after acquire");
        assert_ne!(acp_pid as i64, pid, "ACP child must be a new process");
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn acquire_does_not_block_on_own_boot_child_row_issue_1327() {
    use super::process_identity;
    use super::SessionStorage;
    use crate::adapters::web_session_store::record_child_identity;
    use ajax_core::models::AgentClient;
    use std::process::{Command, Stdio};

    let dir = scratch_dir("own-boot-child-acquire");
    let handle = "web/own-boot-child-acquire";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let host_boot = directory.inner().ownership_boot_id();
    let store = SessionStorage::under(dir.clone()).registry_store();

    let mut child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn sleep helper");
    let pid = child.id() as i64;
    let token = process_identity::start_token(pid).expect("live child token");
    record_child_identity(
        &store,
        &dir,
        handle,
        "cursor",
        Some((pid, token)),
        &host_boot,
    )
    .expect("seed own-boot child row");

    let script = fake_acp_fixture();
    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("own-boot row must not block acquire");
    });

    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");
    let _ = std::fs::remove_dir_all(dir);
}
