//! ACP `session/close` on child teardown when advertised.

use super::context_continuity::ContextState;
use super::test_support::{
    fake_acp_fixture, has_message, pump_until, scratch_dir, BlockingSessionDirectory,
};
use super::transcript::{with_test_idle_release_grace, MAX_IDLE_SESSIONS};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, AcpStdioClient,
};
use ajax_core::models::AgentClient;
use std::{path::Path, time::Duration};

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
fn idle_eviction_detach_skips_session_close_and_resumes() {
    // Invariant 6: restorable idle slots detach without ACP session/close.
    let dir = scratch_dir("idle-evict-no-close");
    let handle_a = "web/idle-evict-no-close-a";
    let handle_trigger = "web/idle-evict-no-close-trigger";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();
    let marker = close_marker_path(&dir);
    let _ = std::fs::remove_file(&marker);

    with_test_idle_release_grace(Duration::ZERO, || {
        with_test_acp_program(&script, || {
            with_test_acp_extra_args(&["--session-close"], || {
                directory
                    .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire a");
                seed_user_turn(&directory, handle_a);
                directory.release(handle_a);

                pump_until(&directory, handle_a, Duration::from_secs(5), |_| {
                    directory
                        .eviction_snapshot(handle_a)
                        .is_some_and(|snapshot| snapshot.evictable)
                });

                let child_before = directory.child_id(handle_a).expect("child before");

                for i in 0..MAX_IDLE_SESSIONS {
                    let handle = format!("web/idle-evict-no-close-idle-{i}");
                    directory
                        .acquire(&handle, &dir, "auto", AgentClient::Cursor)
                        .expect("acquire idle");
                    directory.release(&handle);
                }

                directory
                    .acquire(handle_trigger, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire trigger");
                directory.release(handle_trigger);

                assert!(
                    !marker.exists(),
                    "idle eviction must not send session/close"
                );

                directory
                    .acquire(handle_a, &dir, "auto", AgentClient::Cursor)
                    .expect("re-acquire a");
                let child_after = directory.child_id(handle_a).expect("child after");
                assert_ne!(
                    child_before, child_after,
                    "idle cap must evict the restorable disconnected session"
                );
                let (events, _) = directory.read_from(handle_a, 0);
                assert!(
                    !has_message(&events, "note", CONTEXT_RESET_NOTE),
                    "resume after idle eviction must keep model context: {events:?}"
                );
            });
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
            let attach = directory
                .runtime_handle()
                .block_on(
                    directory
                        .inner()
                        .attach_snapshot(handle, "auto".to_string(), None),
                );
            assert_eq!(
                attach.snapshot.context_state,
                ContextState::Unavailable,
                "closed sessions must enter unavailable context, not silently resume"
            );
            assert!(
                directory
                    .submit_prompt(handle, "blocked".to_string())
                    .is_err(),
                "closed sessions must reject prompts until explicit recovery"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}
