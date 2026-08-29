use super::session_cleanup::{
    is_session_owned, owned_session_handles, prune_stale_persisted_sessions,
};
use crate::adapters::web_session_store;
use ajax_core::models::LifecycleStatus;

fn note(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "message",
        "role": "agent",
        "text": text,
    })
}

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-session-cleanup-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// GitHub issue #977: startup must delete persisted sessions with no registry owner.
#[test]
fn issue_977_startup_prunes_unowned_persisted_sessions() {
    let dir = scratch_dir("startup-prune");
    let owned_handle = "web/owned";
    let stale_handle = "web/stale-dropped";
    web_session_store::append_events(&dir, owned_handle, &[note("keep")]);
    web_session_store::append_events(&dir, stale_handle, &[note("drop")]);

    let mut owned = std::collections::HashSet::new();
    owned.insert(owned_handle.to_string());

    let pruned = prune_stale_persisted_sessions(&dir, &owned);
    assert_eq!(pruned, vec![stale_handle.to_string()]);
    assert!(web_session_store::session_path(&dir, owned_handle).is_file());
    assert!(
        !web_session_store::session_path(&dir, stale_handle).exists(),
        "stale transcript must be deleted at startup"
    );

    let _ = std::fs::remove_dir_all(dir);
}

// GitHub issue #977: recoverable registry tasks keep their transcripts.
#[test]
fn issue_977_startup_keeps_owned_recoverable_task_transcripts() {
    let dir = scratch_dir("startup-keep");
    let handle = "web/fix-login";
    web_session_store::append_events(&dir, handle, &[note("recoverable")]);

    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let owned = owned_session_handles(&context);

    let pruned = prune_stale_persisted_sessions(&dir, &owned);
    assert!(pruned.is_empty(), "owned transcript must survive prune");
    assert_eq!(
        web_session_store::load::<serde_json::Value>(&dir, handle).events,
        vec![note("recoverable")]
    );

    let _ = std::fs::remove_dir_all(dir);
}

// GitHub issue #977: Removed registry rows do not own a session.
#[test]
fn issue_977_removed_tasks_are_not_session_owners() {
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Removed;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);

    assert!(!is_session_owned(&context, "web/fix-login"));
    assert!(owned_session_handles(&context).is_empty());
}

// GitHub issue #977: Drop/recreate must not reload a prior handle's transcript.
#[test]
fn issue_977_drop_cleanup_isolates_handle_reuse() {
    use super::test_support::BlockingSessionDirectory;
    use super::SessionServerEvent;
    use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
    use ajax_core::models::AgentClient;

    let dir = scratch_dir("drop-reuse");
    let handle = "web/reuse-handle";
    web_session_store::append_events(&dir, handle, &[note("old transcript")]);
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = super::test_support::fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .expect("acquire before drop");
            directory
                .submit_prompt(handle, "before drop".to_string())
                .expect("prompt before drop");
        });
    });

    directory.cleanup_session(handle);
    assert!(
        !web_session_store::session_path(&dir, handle).exists(),
        "drop cleanup must delete persisted transcript"
    );

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire after drop");
    });

    let (events, _) = directory.read_from(handle, 0);
    assert!(
        !events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Message { text, .. }
                if text == "old transcript" || text == "before drop"
        )),
        "reused handle must not replay the dropped task's transcript"
    );

    let _ = std::fs::remove_dir_all(dir);
}
