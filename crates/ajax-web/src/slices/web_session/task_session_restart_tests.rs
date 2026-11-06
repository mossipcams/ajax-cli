use super::prompt_content::PromptContentBlockWire;
use super::session_activity_directory_tests::{record_activities, wait_for_last_activity};
use super::task_session_exit::ACP_PROCESS_EXITED;
use super::test_support::{fake_acp_fixture, has_message, scratch_dir, BlockingSessionDirectory};
use super::{SessionActivity, SessionServerEvent};
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, with_test_claude_sdk_module,
};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger;
use ajax_core::models::AgentClient;
use base64::Engine;
use std::path::Path;
use std::time::{Duration, Instant};

fn session_new_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

#[test]
fn restart_respawns_stored_session_and_keeps_transcript_and_queue() {
    let dir = scratch_dir("restart-resume");
    let handle = "web/restart-resume";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

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
            let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
                .acp_session_id
                .expect("session id persisted");

            directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            assert!(directory.child_id(handle).is_some());
            assert_eq!(
                web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
                Some(stored_id)
            );
            assert_eq!(session_new_count(&dir), 1);
            let (events, _) = directory.read_from(handle, 0);
            assert!(events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Message { role, text, .. }
                    if role == "user" && text == "first"
            )));
            assert!(prompt_ledger::load(&dir, handle)
                .expect("ledger")
                .entry("queued-1")
                .is_some());
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_leaves_no_holder_so_respawned_session_is_idle_evictable() {
    let dir = scratch_dir("restart-holder");
    let handle = "web/restart-holder";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");
            directory.release(handle);

            directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            assert!(directory.child_id(handle).is_some());
            let snapshot = directory.eviction_snapshot(handle).expect("snapshot");
            assert_eq!(snapshot.holders, 0);
            assert!(
                snapshot.evictable,
                "a restarted, released session must be evictable when idle"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_with_attached_holder_keeps_it_and_prompts_still_succeed() {
    let dir = scratch_dir("restart-attached");
    let handle = "web/restart-attached";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");

            directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            let snapshot = directory.eviction_snapshot(handle).expect("snapshot");
            assert_eq!(snapshot.holders, 1);
            assert!(
                !snapshot.evictable,
                "a restarted session with an attached browser must not be evictable"
            );
            directory
                .submit_prompt_with_id(handle, "after-1".to_string(), "after".to_string())
                .expect("prompt after restart must succeed for the attached connection");
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_with_missing_worktree_errors_without_spawning() {
    let dir = scratch_dir("restart-missing");
    let handle = "web/restart-missing";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        let error = directory
            .runtime_handle()
            .block_on(directory.inner().restart_agent(handle, &dir.join("gone")))
            .expect_err("missing worktree must fail");
        assert_eq!(error, "worktree missing");
        assert_eq!(session_new_count(&dir), 0);
        assert!(directory.child_id(handle).is_none());
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_wedged_claude_sdk_replaces_process_resumes_session_and_keeps_queue() {
    let dir = scratch_dir("restart-wedged-claude");
    let handle = "web/restart-wedged-claude";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs");

    with_test_claude_sdk_module(&fixture, || {
        directory.runtime_handle().block_on(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                let session = directory.inner();
                session
                    .acquire(handle, &dir, "auto", AgentClient::Claude)
                    .await
                    .expect("acquire Claude SDK session");
                session
                    .submit_prompt_with_id(
                        handle,
                        "active-wedge".into(),
                        "please WEDGE".into(),
                        Vec::new(),
                    )
                    .await
                    .expect("submit wedged prompt");
                loop {
                    let (events, _) = session.read_from(handle, 0).await;
                    if has_message(&events, "agent", "wedged") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                let old_pid = session.child_id(handle).await.expect("old harness pid");
                let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
                    .acp_session_id
                    .expect("stored session id");
                session
                    .submit_prompt_with_id(
                        handle,
                        "queued-after-wedge".into(),
                        "second".into(),
                        Vec::new(),
                    )
                    .await
                    .expect("queue follow-up");
                let ledger = prompt_ledger::load(&dir, handle).expect("ledger before restart");
                assert_eq!(
                    ledger.entry("active-wedge").unwrap().phase,
                    prompt_ledger::PromptPhase::Dispatching
                );
                assert_eq!(
                    ledger.entry("queued-after-wedge").unwrap().phase,
                    prompt_ledger::PromptPhase::Queued
                );

                tokio::time::timeout(Duration::from_secs(10), session.restart_agent(handle, &dir))
                    .await
                    .expect("restart must not wait for the wedged interrupt")
                    .expect("restart wedged session");

                let new_pid = session
                    .child_id(handle)
                    .await
                    .expect("replacement harness pid");
                assert_ne!(
                    old_pid, new_pid,
                    "restart must create a new harness process"
                );
                assert!(
                    !std::process::Command::new("kill")
                        .args(["-0", &old_pid.to_string()])
                        .stderr(std::process::Stdio::null())
                        .status()
                        .expect("probe old harness pid")
                        .success(),
                    "old harness must be gone after restart"
                );
                loop {
                    let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
                    let queued = ledger
                        .entry("queued-after-wedge")
                        .expect("restart must preserve the queued prompt");
                    assert!(matches!(
                        queued.phase,
                        prompt_ledger::PromptPhase::Queued
                            | prompt_ledger::PromptPhase::Dispatching
                            | prompt_ledger::PromptPhase::Completed
                    ));
                    if queued.phase == prompt_ledger::PromptPhase::Completed {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                assert_eq!(
                    web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
                    Some(stored_id),
                    "replacement must resume the stored session"
                );
                let (events, _) = session.read_from(handle, 0).await;
                assert!(
                    has_message(&events, "agent", "ok"),
                    "replacement must serve the queued prompt"
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "user" && text == "second"))
                        .count(),
                    1
                );
                assert_eq!(session.child_id(handle).await, Some(new_pid));
                session.drop_session(handle).await;
            })
            .await
            .expect("wedged restart scenario must finish within 30 seconds");
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_resends_the_wedged_prompt_with_a_new_id() {
    let dir = scratch_dir("restart-resend");
    let handle = "web/restart-resend";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs");

    with_test_claude_sdk_module(&fixture, || {
        directory.runtime_handle().block_on(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                let session = directory.inner();
                session
                    .acquire(handle, &dir, "auto", AgentClient::Claude)
                    .await
                    .expect("acquire Claude SDK session");
                session
                    .submit_prompt_with_id(
                        handle,
                        "active-wedge".into(),
                        "please WEDGE".into(),
                        Vec::new(),
                    )
                    .await
                    .expect("submit wedged prompt");
                loop {
                    let (events, _) = session.read_from(handle, 0).await;
                    if has_message(&events, "agent", "wedged") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }

                let resent = tokio::time::timeout(
                    Duration::from_secs(10),
                    session.restart_agent(handle, &dir),
                )
                .await
                .expect("restart must not wait for the wedged interrupt")
                .expect("restart wedged session");
                assert!(resent, "restart must report that it re-sent the prompt");

                loop {
                    let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
                    if ledger.entry("active-wedge-restart").is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }

                let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
                assert_eq!(
                    ledger.entry("active-wedge").unwrap().phase,
                    prompt_ledger::PromptPhase::Interrupted
                );
                assert!(ledger.entry("active-wedge-restart").is_some());
                let (events, _) = session.read_from(handle, 0).await;
                assert!(
                    !events.iter().any(|event| matches!(event,
                        SessionServerEvent::Error { message } if message.contains("was not retried"))),
                    "a re-sent prompt must not be reported as not retried"
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event,
                            SessionServerEvent::Message { role, text, .. }
                                if role == "user" && text == "please WEDGE"))
                        .count(),
                    2,
                    "the resubmitted prompt must add a second user row for the same text"
                );
                session.drop_session(handle).await;
            })
            .await
            .expect("wedged resend scenario must finish within 30 seconds");
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_with_no_in_flight_prompt_resends_nothing() {
    let dir = scratch_dir("restart-no-resend");
    let handle = "web/restart-no-resend";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");

            let resent = directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            assert!(!resent);
            let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
            assert!(ledger.entries.is_empty());
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_resends_the_wedged_prompt_with_its_image_blocks() {
    let dir = scratch_dir("restart-image-resend");
    let handle = "web/restart-image-resend";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_claude_sdk.mjs");

    with_test_claude_sdk_module(&fixture, || {
        directory.runtime_handle().block_on(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                let session = directory.inner();
                session
                    .acquire(handle, &dir, "auto", AgentClient::Claude)
                    .await
                    .expect("acquire Claude SDK session");
                session
                    .submit_prompt_with_id(
                        handle,
                        "active-wedge".into(),
                        "please WEDGE with an image".into(),
                        vec![PromptContentBlockWire::Image {
                            data: tiny_png_base64(),
                            mime_type: "image/png".to_string(),
                        }],
                    )
                    .await
                    .expect("submit wedged prompt");
                loop {
                    let (events, _) = session.read_from(handle, 0).await;
                    if has_message(&events, "agent", "wedged") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }

                tokio::time::timeout(Duration::from_secs(10), session.restart_agent(handle, &dir))
                    .await
                    .expect("restart must not wait for the wedged interrupt")
                    .expect("restart wedged session");

                loop {
                    let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
                    if ledger.entry("active-wedge-restart").is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }

                let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
                let original = ledger
                    .entry("active-wedge")
                    .expect("original entry present");
                let restarted = ledger
                    .entry("active-wedge-restart")
                    .expect("restarted entry present");
                assert_eq!(original.phase, prompt_ledger::PromptPhase::Interrupted);
                assert!(!original.content_blocks.is_empty());
                assert_eq!(
                    restarted.content_blocks, original.content_blocks,
                    "the resubmitted prompt must carry the same content blocks"
                );
                session.drop_session(handle).await;
            })
            .await
            .expect("wedged image resend scenario must finish within 30 seconds");
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prompt_after_child_exit_revives_and_dispatches_1557() {
    let dir = scratch_dir("prompt-after-exit-1557");
    let handle = "web/prompt-after-exit-1557";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let stored_id = web_session_store::load::<SessionServerEvent>(&dir, handle)
            .acp_session_id
            .expect("session id persisted");
        let old_pid = directory.child_id(handle).expect("child before exit");

        directory.kill_host_for_test(handle);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (events, _) = directory.read_from(handle, 0);
            if events.iter().any(|event| {
                matches!(event, SessionServerEvent::Error { message } if message == ACP_PROCESS_EXITED)
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "child exit must be reconciled");
            std::thread::sleep(Duration::from_millis(20));
        }

        directory
            .submit_prompt_with_id(handle, "after-exit".to_string(), "ping".to_string())
            .expect("a prompt submitted after child exit must revive the child");

        assert_ne!(directory.child_id(handle), Some(old_pid));
        assert_eq!(
            web_session_store::load::<SessionServerEvent>(&dir, handle).acp_session_id,
            Some(stored_id),
            "revival must resume the stored session"
        );
        assert_eq!(
            session_new_count(&dir),
            1,
            "revival must not create a session"
        );
        loop {
            let (events, _) = directory.read_from(handle, 0);
            if has_message(&events, "agent", "pong") {
                break;
            }
            assert!(
                Instant::now() < deadline + Duration::from_secs(5),
                "revived child must dispatch the prompt"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn withdrawing_queued_prompt_after_child_exit_neither_revives_nor_dispatches_it() {
    let dir = scratch_dir("withdraw-after-exit");
    let handle = "web/withdraw-after-exit";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1".to_string(), "first".to_string())
                .expect("first");
            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), "ping".to_string())
                .expect("queued");

            directory.kill_host_for_test(handle);
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let (events, _) = directory.read_from(handle, 0);
                if events.iter().any(|event| {
                    matches!(event, SessionServerEvent::Error { message } if message == ACP_PROCESS_EXITED)
                }) {
                    break;
                }
                assert!(Instant::now() < deadline, "child exit must be reconciled");
                std::thread::sleep(Duration::from_millis(20));
            }
            let child_after_exit = directory.child_id(handle);

            directory
                .submit_prompt_with_id(handle, "queued-1".to_string(), String::new())
                .expect("withdrawing a queued prompt after child exit must succeed");

            assert!(
                prompt_ledger::load(&dir, handle)
                    .expect("ledger after withdrawal")
                    .entry("queued-1")
                    .is_none(),
                "the withdrawn prompt must leave the ledger"
            );
            let settle = Instant::now() + Duration::from_secs(2);
            while Instant::now() < settle {
                let (events, _) = directory.read_from(handle, 0);
                assert!(
                    !has_message(&events, "agent", "pong"),
                    "a withdrawn prompt must never be executed"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(
                directory.child_id(handle),
                child_after_exit,
                "withdrawal must not spawn a child"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_with_no_in_flight_prompt_reports_turn_ended_1574() {
    let dir = scratch_dir("restart-turn-ended-1574");
    let handle = "web/restart-turn-ended-1574";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());
    let reported = record_activities(&directory);

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire");
            directory
                .submit_prompt_with_id(handle, "active-1574".to_string(), "first".to_string())
                .expect("first");
            directory.release(handle);
            directory.kill_host_for_test(handle);
            wait_for_last_activity(&reported, SessionActivity::TurnFailed);

            directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            assert_eq!(
                reported.lock().expect("reported lock").last().copied(),
                Some(SessionActivity::TurnEnded),
                "a restart that resends nothing must settle a failed task back to idle"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_keeps_subscribers_and_appends_host_note_1574() {
    let dir = scratch_dir("restart-subscriber-1574");
    let handle = "web/restart-subscriber-1574";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let before = directory.collect_outbound(handle, 0, directory.generation(handle));

        directory
            .runtime_handle()
            .block_on(directory.inner().restart_agent(handle, &dir))
            .expect("restart");

        let after_restart = directory.collect_outbound(handle, before.cursor, before.generation);
        assert!(
            after_restart.snapshot.is_some() || !after_restart.events.is_empty(),
            "an attached subscriber must receive the restart"
        );
        let (events, _) = directory.read_from(handle, 0);
        assert!(has_message(&events, "note", "Agent restarted."));

        directory
            .submit_prompt_with_id(handle, "ping-1574".to_string(), "ping".to_string())
            .expect("prompt after restart");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut cursor = after_restart.cursor;
        let mut generation = after_restart.generation;
        let mut delivered = false;
        while !delivered {
            let batch = directory.collect_outbound(handle, cursor, generation);
            cursor = batch.cursor;
            generation = batch.generation;
            delivered = batch.events.iter().any(|envelope| {
                matches!(
                    &envelope.payload,
                    SessionServerEvent::Message { role, text, .. }
                        if role == "agent" && text == "pong"
                )
            });
            assert!(
                Instant::now() < deadline,
                "the subscriber must keep receiving events after the restart"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_with_active_and_queued_prompts_keeps_queued_prompt() {
    let dir = scratch_dir("restart-active-queued");
    let handle = "web/restart-active-queued";
    let script = fake_acp_fixture();
    let directory = BlockingSessionDirectory::new(dir.clone());

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

            directory
                .runtime_handle()
                .block_on(directory.inner().restart_agent(handle, &dir))
                .expect("restart");

            let ledger = prompt_ledger::load(&dir, handle).expect("ledger after restart");
            assert_eq!(
                ledger.entry("queued-1").expect("queued entry").phase,
                prompt_ledger::PromptPhase::Dispatching,
                "the queued prompt must be dispatched on the replacement child"
            );
            assert_eq!(
                ledger
                    .entry("active-1-restart")
                    .expect("re-sent active prompt")
                    .phase,
                prompt_ledger::PromptPhase::Queued,
                "the re-sent active prompt must wait behind the queued prompt"
            );

            let (events, _) = directory.read_from(handle, 0);
            assert!(
                !events.iter().any(|event| matches!(event,
                    SessionServerEvent::Error { message } if message.contains("was interrupted"))),
                "a queued prompt must not be reported as interrupted"
            );
        });
    });

    let _ = std::fs::remove_dir_all(dir);
}

fn tiny_png_base64() -> String {
    base64::engine::general_purpose::STANDARD
        .encode([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}
