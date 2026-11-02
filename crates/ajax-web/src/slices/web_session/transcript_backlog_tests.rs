use super::acp_drain::{PromptTerminal, PromptTerminalOutcome};
use super::execution_ownership::ExecutionOwnership;
use super::prompt_queue::ActivePrompt;
use super::task_session::TaskSessionState;
use super::task_session_commit::BacklogRetryBackoff;
use super::test_support::{fake_acp_fixture, note, scratch_dir, BlockingSessionDirectory};
use super::{SessionServerEvent, SessionStorage};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store::{
    self,
    prompt_ledger::{self, PromptLedger, PromptPhase},
};
use ajax_core::models::AgentClient;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn make_transcript_unreadable(dir: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = fs::remove_file(dir.join(format!("ajax.db{suffix}")));
    }
    fs::create_dir_all(dir.join("ajax.db")).expect("block registry database path");
}

fn snapshot_registry_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    ["", "-journal", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let name = format!("ajax.db{suffix}");
            fs::read(dir.join(&name)).ok().map(|bytes| (name, bytes))
        })
        .collect()
}

fn restore_registry_files(dir: &Path, files: Vec<(String, Vec<u8>)>) {
    fs::remove_dir(dir.join("ajax.db")).expect("restore registry database path");
    for (name, bytes) in files {
        fs::write(dir.join(name), bytes).expect("restore registry file");
    }
}

fn message_index(events: &[SessionServerEvent], text: &str) -> Option<usize> {
    events
        .iter()
        .position(|event| matches!(event, SessionServerEvent::Message { text: t, .. } if t == text))
}

fn wait_for_message_on_tick(directory: &BlockingSessionDirectory, handle: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (events, _) = directory.read_from(handle, 0);
        if message_index(&events, text).is_some() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "backlog was not drained on the poll tick: {events:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn ledger_phase(state_dir: &Path, handle: &str, id: &str) -> Option<PromptPhase> {
    prompt_ledger::load(state_dir, handle)
        .ok()?
        .entry(id)
        .map(|entry| entry.phase)
}

fn bare_state(dir: &Path, handle: &str) -> TaskSessionState {
    let storage = SessionStorage::under(dir.to_path_buf());
    let ownership = Arc::new(ExecutionOwnership::new(storage.clone()));
    TaskSessionState::new(
        handle.to_string(),
        storage,
        None,
        None,
        PromptLedger::default(),
        None,
        ownership,
    )
}

fn pending_terminal_state(dir: &Path, handle: &str) -> TaskSessionState {
    let mut state = bare_state(dir, handle);
    state.prompts.prompt_ledger.upsert_queued(
        "active-1".to_string(),
        "first".to_string(),
        "first".to_string(),
        Vec::new(),
    );
    state.prompts.prompt_ledger.mark_dispatching("active-1");
    let mut active = ActivePrompt::new(1, Some("active-1".to_string()));
    assert!(active.capture_terminal(PromptTerminal {
        request_id: 1,
        outcome: PromptTerminalOutcome::Success,
        events: vec![SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        }],
    }));
    state.prompts.active_prompt = Some(active);
    state.unpersisted = vec![note("backlog-a"), note("backlog-b")];
    state
}

#[test]
fn issue_1435_shutdown_flush_finalizes_a_pending_terminal_through_the_ledger() {
    let dir = scratch_dir("issue-1435-terminal-flush");
    let handle = "web/issue-1435-terminal-flush";

    let mut state = pending_terminal_state(&dir, handle);
    state.flush_unpersisted_backlog();

    assert!(
        state.prompts.active_prompt.is_none(),
        "the pending terminal must finish during the shutdown flush"
    );
    assert!(state.unpersisted.is_empty());
    assert_eq!(
        ledger_phase(&dir, handle, "active-1"),
        Some(PromptPhase::Completed),
        "the terminal must commit through the prompt ledger"
    );
    let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
    let a = message_index(&stored.events, "backlog-a").expect("backlog-a committed");
    let b = message_index(&stored.events, "backlog-b").expect("backlog-b committed");
    let end = stored
        .events
        .iter()
        .position(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
        .expect("terminal TurnEnd committed");
    assert!(
        a < b && b < end,
        "backlog then terminal must persist in order: {:?}",
        stored.events
    );

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1435_backlog_retry_is_skipped_while_a_terminal_is_pending() {
    let dir = scratch_dir("issue-1435-retry-skip");
    let handle = "web/issue-1435-retry-skip";

    let mut state = pending_terminal_state(&dir, handle);
    state.retry_unpersisted_backlog();

    assert_eq!(
        state.unpersisted.len(),
        2,
        "a retry must leave the backlog for the terminal commit"
    );
    assert!(state.prompts.active_prompt.is_some());
    let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
    assert!(
        message_index(&stored.events, "backlog-a").is_none(),
        "a retry must not persist the backlog while the terminal is pending: {:?}",
        stored.events
    );

    state.prompts.active_prompt = None;
    state.retry_unpersisted_backlog();
    assert!(state.unpersisted.is_empty());
    let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
    assert!(
        message_index(&stored.events, "backlog-b").is_some(),
        "the same backlog must persist once no terminal is pending"
    );

    drop(state);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1435_backlog_drains_on_the_poll_tick_without_an_acp_client() {
    let dir = scratch_dir("issue-1435-no-client-tick");
    let handle = "web/issue-1435-no-client-tick";
    let missing_program = dir.join("missing-acp-program");

    with_test_acp_program(&missing_program, || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        assert!(
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .is_err(),
            "a missing agent program must leave the session without a client"
        );
        assert_eq!(directory.child_id(handle), None);

        let saved = snapshot_registry_files(&dir);
        make_transcript_unreadable(&dir);
        directory.record(handle, note("backlog-a"));
        directory.record(handle, note("backlog-b"));
        restore_registry_files(&dir, saved);

        wait_for_message_on_tick(&directory, handle, "backlog-b");

        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        let a = message_index(&stored.events, "backlog-a").expect("backlog-a persisted");
        let b = message_index(&stored.events, "backlog-b").expect("backlog-b persisted");
        assert!(
            a < b,
            "backlog must persist oldest-first: {:?}",
            stored.events
        );
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1435_shutdown_flushes_the_backlog_in_order() {
    let dir = scratch_dir("issue-1435-shutdown-flush");
    let handle = "web/issue-1435-shutdown-flush";

    with_test_acp_program(&fake_acp_fixture(), || {
        let directory = BlockingSessionDirectory::new(dir.clone());
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");

        let saved = snapshot_registry_files(&dir);
        make_transcript_unreadable(&dir);
        directory.record(handle, note("backlog-a"));
        directory.record(handle, note("backlog-b"));
        restore_registry_files(&dir, saved);

        directory.drop_session(handle);

        let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
        let a = message_index(&stored.events, "backlog-a")
            .expect("backlog-a must be committed at shutdown");
        let b = message_index(&stored.events, "backlog-b")
            .expect("backlog-b must be committed at shutdown");
        assert!(
            a < b,
            "backlog must persist oldest-first: {:?}",
            stored.events
        );
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1435_mid_turn_backlog_commits_while_the_prompt_has_no_terminal() {
    let dir = scratch_dir("issue-1435-mid-turn");
    let handle = "web/issue-1435-mid-turn";

    with_test_acp_program(&fake_acp_fixture(), || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            let directory = BlockingSessionDirectory::new(dir.clone());
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "open-1".into(), "first".into())
                .expect("active prompt");

            let saved = snapshot_registry_files(&dir);
            make_transcript_unreadable(&dir);
            directory.record(handle, note("backlog-a"));
            directory.record(handle, note("backlog-b"));
            restore_registry_files(&dir, saved);

            wait_for_message_on_tick(&directory, handle, "backlog-b");

            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            let a = message_index(&stored.events, "backlog-a").expect("backlog-a persisted");
            let b = message_index(&stored.events, "backlog-b").expect("backlog-b persisted");
            assert!(
                a < b,
                "backlog must persist oldest-first: {:?}",
                stored.events
            );
        });
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn issue_1435_backoff_doubles_from_two_seconds_to_a_thirty_second_cap() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    assert!(backoff.is_due(start), "the first retry must not be delayed");

    let mut now = start;
    for seconds in [2, 4, 8, 16, 30, 30] {
        let delay = Duration::from_secs(seconds);
        backoff.record_failure(now);
        assert!(
            !backoff.is_due(now + delay - Duration::from_millis(1)),
            "retry must wait {delay:?} after a failure"
        );
        assert!(
            backoff.is_due(now + delay),
            "retry must be due once {delay:?} has passed"
        );
        now += delay;
    }
}

#[test]
fn issue_1435_backoff_from_an_append_failure_waits_the_initial_delay() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    backoff.record_failure_if_unscheduled(start);

    assert!(
        !backoff.is_due(start + Duration::from_millis(1999)),
        "the first tick after an append failure must not retry the backlog"
    );
    assert!(backoff.is_due(start + Duration::from_secs(2)));

    backoff.record_failure_if_unscheduled(start + Duration::from_secs(1));
    assert!(
        backoff.is_due(start + Duration::from_secs(2)),
        "a backlog that grows while a retry is scheduled keeps the schedule"
    );
}

#[test]
fn issue_1435_backoff_resets_after_a_successful_commit() {
    let start = Instant::now();
    let mut backoff = BacklogRetryBackoff::default();
    backoff.record_failure(start);
    backoff.record_failure(start);
    backoff.record_success();

    assert!(
        backoff.is_due(start),
        "a successful commit clears the backoff"
    );
    backoff.record_failure(start);
    assert!(!backoff.is_due(start + Duration::from_millis(1999)));
    assert!(backoff.is_due(start + Duration::from_secs(2)));
}
