//! ACP `session/close` on child teardown when advertised.

use super::test_support::{fake_acp_fixture, has_message, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, AcpStdioClient,
};
use ajax_core::models::AgentClient;
use std::path::Path;

fn close_marker_path(worktree: &Path) -> std::path::PathBuf {
    worktree.join(".fake-acp-session-close-called")
}

#[test]
fn initialize_stores_close_advertised_on_spawn_report() {
    let dir = scratch_dir("close-advertised-report");
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            assert!(report.close_advertised);
        });
        with_test_acp_extra_args(&[], || {
            let (_client, report) =
                AcpStdioClient::spawn(AgentClient::Cursor, &dir, None, None).expect("spawn");
            assert!(!report.close_advertised);
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn advertised_close_sent_on_slot_shutdown() {
    let dir = scratch_dir("close-on-shutdown");
    let handle = "web/close-on-shutdown";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);
    let _ = std::fs::remove_file(&marker);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory.release(handle);
            directory.drop_session(handle);
        });
    });

    let recorded = std::fs::read_to_string(&marker).expect("session/close marker");
    assert_eq!(recorded.trim(), "fake-sess-1");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unadvertised_close_not_sent_on_slot_shutdown() {
    let dir = scratch_dir("close-not-sent");
    let handle = "web/close-not-sent";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);
    let _ = std::fs::remove_file(&marker);

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        directory.release(handle);
        directory.drop_session(handle);
    });

    assert!(
        !marker.exists(),
        "session/close must not run when close is not advertised"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn transcript_persists_after_advertised_close_shutdown() {
    let dir = scratch_dir("close-transcript-persist");
    let handle = "web/close-transcript-persist";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt(handle, "keep-me".to_string())
                .expect("prompt");
            directory.release(handle);
            directory.drop_session(handle);

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                has_message(&events, "user", "keep-me"),
                "JSONL transcript must survive ACP session/close"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn close_failure_still_tears_down_and_records_error() {
    let dir = scratch_dir("close-failure");
    let handle = "web/close-failure";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close", "--session-close-fail"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory.release(handle);
            directory.drop_session(handle);

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    SessionServerEvent::Error { message }
                        if message.contains("session/close")
                )),
                "close failure must surface as a session error event"
            );
        });
    });

    assert!(marker.exists(), "session/close must still be attempted");
    let _ = std::fs::remove_dir_all(dir);
}

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

fn seed_user_turn(directory: &BlockingSessionDirectory, handle: &str) {
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
}

#[test]
fn advertised_close_skipped_on_detach_and_session_resumes() {
    // #1061: idle/restart detach must keep the agent session loadable.
    let dir = scratch_dir("detach-resumes");
    let handle = "web/detach-resumes";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);
    let _ = std::fs::remove_file(&marker);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            seed_user_turn(&directory, handle);
            directory.detach_session(handle);

            assert!(
                !marker.exists(),
                "idle/restart detach must not send session/close"
            );

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire");
            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !has_message(&events, "note", CONTEXT_RESET_NOTE),
                "resume/load after detach must keep model context: {events:?}"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn advertised_close_on_drop_session_prevents_resume() {
    // #1061: task Drop remains a terminal close.
    let dir = scratch_dir("close-prevents-resume");
    let handle = "web/close-prevents-resume";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);
    let _ = std::fs::remove_file(&marker);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--session-close"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            seed_user_turn(&directory, handle);
            directory.release(handle);
            directory.drop_session(handle);

            assert!(marker.exists(), "Drop must send session/close");

            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("re-acquire after close");
            let (events, _) = directory.read_from(handle, 0);
            assert!(
                has_message(&events, "note", CONTEXT_RESET_NOTE),
                "closed sessions must not resume: {events:?}"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}
