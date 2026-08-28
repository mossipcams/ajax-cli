use super::context_continuity::ContextState;
use super::test_support::{
    agent_pong_count, fake_acp_fixture, has_message, pump_until, scratch_dir,
    BlockingSessionDirectory, CONTEXT_RESET_NOTE,
};
use super::SessionServerEvent;
use super::{
    acp_drain::{PromptTerminal, PromptTerminalOutcome},
    task_session::ActivePrompt,
};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger::{
    self, ForcePersistFailGuard, PromptLedger, PromptPhase,
};
use ajax_core::models::AgentClient;
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
fn queued_prompt_kept_when_transcript_durability_fault_blocks_dispatch() {
    let dir = scratch_dir("queued-transcript-fault");
    let handle = "web/queued-transcript-fault";
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("active");
            directory
                .submit_prompt_with_id(handle, "queued-1".into(), "next".into())
                .expect("queued");

            let _fail = web_session_store::ForceAppendFailGuard::enable();
            directory.record(
                handle,
                SessionServerEvent::Message {
                    role: "agent".to_string(),
                    text: "mid-turn".to_string(),
                    content_blocks: Vec::new(),
                    item_id: "mid-turn".to_string(),
                    message_id: None,
                },
            );
            drop(_fail);

            directory.cancel(handle, true).expect("cancel");
            thread::sleep(Duration::from_millis(150));
            directory.pump(handle);

            assert_eq!(
                ledger_phase(&dir, handle, "queued-1"),
                Some(PromptPhase::Queued),
                "queued prompt must remain when transcript durability blocks dispatch"
            );
            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "user" && text == "next"
                )),
                "queued prompt must not dispatch while transcript durability fault is set"
            );

            directory.release(handle);
            drop(directory);
        });

        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("re-acquire");
        pump_until(&directory, handle, Duration::from_secs(5), |events| {
            agent_pong_count(events) >= 1
                && events.iter().any(|event| {
                    matches!(
                        event,
                        SessionServerEvent::Message { role, text, .. }
                            if role == "user" && text == "next"
                    )
                })
        });
        assert_eq!(
            ledger_phase(&dir, handle, "queued-1"),
            Some(PromptPhase::Completed)
        );
    });
    let _ = std::fs::remove_dir_all(dir);
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
    )
    .unwrap();

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

#[test]
fn transcript_append_failure_rejects_submit_without_ack_or_dispatch() {
    let dir = scratch_dir("transcript-submit-fail");
    let handle = "web/transcript-submit-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let _fail = web_session_store::ForceAppendFailGuard::enable();
        let result = directory.submit_prompt_with_id(handle, "prompt-1".into(), "hello".into());
        drop(_fail);
        assert!(
            result.is_err(),
            "transcript append failure must reject submit: {result:?}"
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(
            !has_message(&events, "user", "hello"),
            "unpersisted user event must not appear in transcript"
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SessionServerEvent::PromptAccepted { client_message_id }
                    if client_message_id == "prompt-1"
            )),
            "failed submit must not acknowledge prompt"
        );
        pump_until(&directory, handle, Duration::from_secs(2), |_| true);
        let (events, _) = directory.read_from(handle, 0);
        assert_eq!(
            agent_pong_count(&events),
            0,
            "failed submit must not dispatch to ACP"
        );
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn mid_turn_transcript_append_failure_surfaces_in_snapshot_and_blocks_prompt() {
    let dir = scratch_dir("transcript-mid-turn-fail");
    let handle = "web/transcript-mid-turn-fail";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let rt = directory.runtime_handle();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".into(), "hold".into())
                .expect("submit");
        });

        let _fail = web_session_store::ForceAppendFailGuard::enable();
        directory.record(
            handle,
            SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "mid-turn".to_string(),
                content_blocks: Vec::new(),
                item_id: "mid-turn".to_string(),
                message_id: None,
            },
        );
        drop(_fail);

        let attach = rt.block_on(directory.inner().attach_snapshot(
            handle,
            "auto".to_string(),
            None,
        ));
        assert!(
            attach.snapshot.transcript_error.is_some(),
            "mid-turn append failure must surface transcriptError"
        );

        let next = directory.submit_prompt_with_id(handle, "next-1".into(), "blocked".into());
        assert!(
            next.is_err(),
            "transcript durability fault must block the next prompt"
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SessionServerEvent::PromptAccepted { client_message_id }
                    if client_message_id == "next-1"
            )),
            "blocked prompt must not be acknowledged"
        );
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pump_path_transcript_append_failure_surfaces_in_collect_outbound() {
    let dir = scratch_dir("pump-transcript-fail-outbound");
    let handle = "web/pump-transcript-fail-outbound";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "hold-1".into(), "hold".into())
                .expect("submit held prompt");
        });

        let _fail = web_session_store::ForceAppendFailGuard::enable();
        directory.kill_host_for_test(handle);
        pump_until(
            &directory,
            handle,
            std::time::Duration::from_secs(2),
            |_| true,
        );
        drop(_fail);

        let blocked_probe =
            directory.submit_prompt_with_id(handle, "probe-1".into(), "probe".into());
        assert!(
            blocked_probe.is_err(),
            "pump-path append failure must set transcript durability fault before collect_outbound"
        );

        let generation = directory.generation(handle);
        let (_, cursor) = directory.read_from(handle, 0);
        let batch = directory.collect_outbound(handle, cursor, generation);
        let snapshot = batch
            .snapshot
            .expect("collect_outbound must include snapshot after pump-path append failure");
        assert!(
            snapshot.transcript_error.is_some(),
            "pump-path append failure must surface transcriptError without attach_snapshot"
        );

        let blocked = directory.submit_prompt_with_id(handle, "next-1".into(), "blocked".into());
        assert!(
            blocked.is_err(),
            "transcript durability fault must block the next prompt"
        );
    });
    let _ = std::fs::remove_dir_all(dir);
}
