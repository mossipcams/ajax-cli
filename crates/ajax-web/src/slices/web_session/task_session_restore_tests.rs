//! Restore and respawn behavior at the session-host level: a stored ACP
//! session id means restore — never a silent fresh session
//! ([#1151](https://github.com/mossipcams/ajax-cli/issues/1151)).

use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::{SessionError, SessionServerEvent};
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use ajax_core::models::AgentClient;
use std::{
    thread,
    time::{Duration, Instant},
};

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
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect_err("acquire must fail closed when restore fails");
            assert!(
                SessionError::classify_spawn(&error).is_restore_unavailable(),
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
