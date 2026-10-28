use super::task_session_restart::{restart_session, RestartError};
use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
use crate::adapters::web_session_store;
use crate::adapters::web_session_store::prompt_ledger;
use ajax_core::{
    commands::CommandContext,
    config::Config,
    models::{AgentClient, Task, TaskId},
    registry::{InMemoryRegistry, Registry},
};
use std::path::Path;

fn session_new_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn chat_context(handle: &str, worktree: &Path) -> CommandContext<InMemoryRegistry> {
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
        AgentClient::Cursor,
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
    let context = chat_context("restart-resume", &dir);
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
fn restart_with_missing_worktree_errors_without_spawning() {
    let dir = scratch_dir("restart-missing");
    let handle = "web/restart-missing";
    let script = fake_acp_fixture();
    let context = chat_context("restart-missing", &dir.join("gone"));
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
