use super::test_support::{fake_acp_fixture, pump_until, scratch_dir, BlockingSessionDirectory};
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
    assert!(!events.iter().any(|event| matches!(
        event,
        SessionServerEvent::Message { role, text, .. }
            if role == "user" && text == "recovered"
    )));
    pump_until(&directory, handle, Duration::from_secs(5), |events| {
        events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "recovered"
            )
        })
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
