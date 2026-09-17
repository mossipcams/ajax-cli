//! Restore and respawn behavior at the session-host level: a stored ACP
//! session id means restore — never a silent fresh session
//! ([#1151](https://github.com/mossipcams/ajax-cli/issues/1151)).

use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::{
    thread,
    time::{Duration, Instant},
};

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";
const CONTEXT_CLEARED_NOTE: &str = "Context cleared.";

fn log_contains_text(directory: &BlockingSessionDirectory, handle: &str, needle: &str) -> bool {
    let (events, _) = directory.read_from(handle, 0);
    events.iter().any(|event| match event {
        SessionServerEvent::Message { text, .. } => text.contains(needle),
        _ => false,
    })
}

fn session_new_count(dir: &std::path::Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
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

// Regression #1151: a failing restore is a typed error at the session-host
// level. The stored session id survives for retry, and no fresh session —
// and therefore no context-reset note — is created behind the transcript.
// Pre-#1151 this test asserted the silent fresh-session fallback.
#[test]
fn g1_load_fail_fails_closed_and_keeps_stored_session_issue_1151() {
    let dir = scratch_dir("load-fail-closed");
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
                content_blocks: Vec::new(),
                item_id: "seed-user".to_string(),
                message_id: None,
            },
        );
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            let error = directory
                .acquire_typed(handle, &dir, "auto", AgentClient::Cursor)
                .expect_err("acquire must fail closed when restore fails");
            assert!(
                error.is_restore_unavailable(),
                "expected typed restore failure, got: {error}"
            );
        });
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));

        // The stored id is retained, so a retry without the injected failure
        // restores the session instead of starting a fresh one.
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("retry acquire must restore");
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn start_fresh_after_restore_failure_is_explicit_and_single() {
    let dir = scratch_dir("start-fresh-after-restore-failure");
    let script = fake_acp_fixture();
    let handle = "web/start-fresh-after-restore-failure";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("first acquire");
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        directory.kill_host_for_test(handle);

        with_test_acp_extra_args(&["--load-fail"], || {
            directory
                .acquire_typed(handle, &dir, "auto", AgentClient::Cursor)
                .expect_err("restore must fail");
        });
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id)
        );

        directory.start_fresh(handle, &dir).expect("start fresh");

        assert_eq!(session_new_count(&dir), 2);
        assert!(log_contains_text(&directory, handle, CONTEXT_CLEARED_NOTE));
        assert_eq!(
            directory
                .read_from(handle, 0)
                .0
                .iter()
                .filter(|event| matches!(event, SessionServerEvent::Message { text, .. } if text == CONTEXT_CLEARED_NOTE))
                .count(),
            1
        );
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

// Regression #1179: ajax-web restart must restore the stored ACP session id
// instead of appending the context-reset note behind the existing transcript.
#[test]
fn issue_1179_ajax_web_restart_restores_without_context_reset_note() {
    let dir = scratch_dir("issue-1179-restart");
    let script = fake_acp_fixture();
    let handle = "web/issue-1179-restart";
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
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        directory.detach_session(handle);

        let restarted = BlockingSessionDirectory::new(dir.clone());
        restarted
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire after ajax-web restart");
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id)
        );
        assert!(!log_contains_text(&restarted, handle, CONTEXT_RESET_NOTE));
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression #1179: Cockpit reconnect must restore the stored session id when
// the live child died during the disconnect lease.
#[test]
fn issue_1179_reconnect_after_child_death_restores_without_context_reset_note() {
    let dir = scratch_dir("issue-1179-reconnect");
    let script = fake_acp_fixture();
    let handle = "web/issue-1179-reconnect";
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
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        directory.release(handle);
        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("reconnect acquire");
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id)
        );
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression #1179: a live healthy child is leased on reconnect even when
// `want_model` differs from the slot pin — no replace, no context-reset note.
#[test]
fn issue_1179_live_child_model_mismatch_leases_without_context_reset_note() {
    let dir = scratch_dir("issue-1179-live-lease");
    let script = fake_acp_fixture();
    let handle = "web/issue-1179-live-lease";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "composer-2.5", AgentClient::Cursor)
            .expect("first acquire");
        let pid1 = directory.child_id(handle).expect("pid1");
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        directory.release(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("reconnect with mismatched want model");
        let pid2 = directory.child_id(handle).expect("pid2");
        assert_eq!(pid1, pid2, "live child must be leased, not replaced");
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id)
        );
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}

// Regression #1179: a stored resume id must win over a slot/want model mismatch
// on reconnect — restore first, never silent session/new behind the transcript.
#[test]
fn issue_1179_reconnect_model_mismatch_still_restores_without_context_reset_note() {
    let dir = scratch_dir("issue-1179-model-mismatch");
    let script = fake_acp_fixture();
    let handle = "web/issue-1179-model-mismatch";
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "composer-2.5", AgentClient::Cursor)
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
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        directory.release(handle);
        directory.kill_host_for_test(handle);

        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("reconnect with mismatched want model");
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id)
        );
        assert!(!log_contains_text(&directory, handle, CONTEXT_RESET_NOTE));
        assert_eq!(session_new_count(&dir), 1);
    });

    let _ = std::fs::remove_dir_all(dir);
}
