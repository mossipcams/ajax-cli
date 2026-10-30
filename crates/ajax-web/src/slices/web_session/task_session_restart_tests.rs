use super::task_session_restart::{restart_session, RestartError};
use super::test_support::{fake_acp_fixture, has_message, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, with_test_claude_sdk_module,
};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger;
use ajax_core::{
    commands::CommandContext,
    config::Config,
    models::{AgentClient, Task, TaskId},
    registry::{InMemoryRegistry, Registry},
};
use std::path::Path;
use std::time::Duration;

fn session_new_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn chat_context(
    handle: &str,
    worktree: &Path,
    agent: AgentClient,
) -> CommandContext<InMemoryRegistry> {
    let mut task = Task::new(
        TaskId::new(format!("task-{handle}")),
        "web",
        handle,
        "Chat",
        format!("ajax/{handle}"),
        "main",
        worktree.to_path_buf(),
        format!("web-{handle}"),
        "task",
        agent,
    );
    task.set_skip_interactive_agent(true);
    let mut registry = InMemoryRegistry::default();
    registry.create_task(task).unwrap();
    CommandContext::new(Config::default(), registry)
}

#[test]
fn restart_respawns_stored_session_and_keeps_transcript_and_queue() {
    let dir = scratch_dir("restart-resume");
    let handle = "web/restart-resume";
    let script = fake_acp_fixture();
    let context = chat_context("restart-resume", &dir, AgentClient::Cursor);
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
                .block_on(restart_session(directory.inner(), &context, handle))
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
    let context = chat_context("restart-holder", &dir, AgentClient::Cursor);
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
                .block_on(restart_session(directory.inner(), &context, handle))
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
    let context = chat_context("restart-attached", &dir, AgentClient::Cursor);
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
                .block_on(restart_session(directory.inner(), &context, handle))
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
    let context = chat_context("restart-missing", &dir.join("gone"), AgentClient::Cursor);
    let directory = BlockingSessionDirectory::new(dir.clone());

    with_test_acp_program(&script, || {
        let error = directory
            .runtime_handle()
            .block_on(restart_session(directory.inner(), &context, handle))
            .expect_err("missing worktree must fail");
        assert_eq!(error, RestartError::WorktreeMissing);
        assert_eq!(session_new_count(&dir), 0);
        assert!(directory.child_id(handle).is_none());
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restart_wedged_claude_sdk_replaces_process_resumes_session_and_keeps_queue() {
    let dir = scratch_dir("restart-wedged-claude");
    let handle = "web/restart-wedged-claude";
    let context = chat_context("restart-wedged-claude", &dir, AgentClient::Claude);
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

                tokio::time::timeout(
                    Duration::from_secs(10),
                    restart_session(session, &context, handle),
                )
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
