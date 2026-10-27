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

#[test]
fn issue_977_startup_prunes_unowned_persisted_sessions() {
    let dir = scratch_dir("startup-prune");
    let owned_handle = "web/owned";
    let stale_handle = "web/stale-dropped";
    web_session_store::append_events(&dir, owned_handle, &[note("keep")]);
    web_session_store::append_events(&dir, stale_handle, &[note("drop")]);

    let mut owned = std::collections::HashSet::new();
    owned.insert(owned_handle.to_string());

    let pruned = prune_stale_persisted_sessions(&super::SessionStorage::under(dir.clone()), &owned);
    assert_eq!(pruned, vec![stale_handle.to_string()]);
    assert_eq!(
        web_session_store::load::<serde_json::Value>(&dir, owned_handle).events,
        vec![note("keep")]
    );
    assert!(
        web_session_store::load::<serde_json::Value>(&dir, stale_handle)
            .events
            .is_empty(),
        "stale transcript must be deleted at startup"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_977_startup_keeps_owned_recoverable_task_transcripts() {
    let dir = scratch_dir("startup-keep");
    let handle = "web/fix-login";
    web_session_store::append_events(&dir, handle, &[note("recoverable")]);

    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let owned = owned_session_handles(&context);

    let pruned = prune_stale_persisted_sessions(&super::SessionStorage::under(dir.clone()), &owned);
    assert!(pruned.is_empty(), "owned transcript must survive prune");
    assert_eq!(
        web_session_store::load::<serde_json::Value>(&dir, handle).events,
        vec![note("recoverable")]
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_977_removed_tasks_are_not_session_owners() {
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Removed;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);

    assert!(!is_session_owned(&context, "web/fix-login"));
    assert!(owned_session_handles(&context).is_empty());
}

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

#[test]
fn issue_1436_registry_reload_shuts_down_live_sessions_for_unowned_handles() {
    use super::test_support::BlockingSessionDirectory;
    use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
    use ajax_core::models::AgentClient;
    use std::collections::HashSet;

    let dir = scratch_dir("reload-unowned");
    let kept = "web/kept";
    let dropped = "web/dropped";
    web_session_store::append_events(&dir, kept, &[note("kept transcript")]);
    web_session_store::append_events(&dir, dropped, &[note("dropped transcript")]);
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = super::test_support::fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            for handle in [kept, dropped] {
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire before reload");
            }
        });
    });

    let owned: HashSet<String> = [kept.to_string()].into_iter().collect();
    directory.runtime_handle().block_on(
        directory
            .inner()
            .cleanup_unowned(|handle| owned.contains(handle)),
    );

    assert!(
        !directory.inner().has_live_entry(dropped),
        "unowned session actor must be shut down on reload"
    );
    assert!(
        web_session_store::load::<serde_json::Value>(&dir, dropped)
            .events
            .is_empty(),
        "unowned session transcript must be deleted on reload"
    );
    assert!(
        directory.inner().has_live_entry(kept),
        "owned session actor must stay live"
    );
    assert!(
        web_session_store::load::<serde_json::Value>(&dir, kept)
            .events
            .contains(&note("kept transcript")),
        "owned session transcript must survive reload"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1436_handle_owned_again_before_cleanup_keeps_its_session() {
    use super::test_support::BlockingSessionDirectory;
    use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
    use ajax_core::models::AgentClient;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = scratch_dir("reowned");
    let recreated = "web/recreated";
    web_session_store::append_events(&dir, recreated, &[note("recreated transcript")]);
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = super::test_support::fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            directory
                .acquire(recreated, &dir, "auto", AgentClient::Cursor)
                .expect("acquire before reload");
        });
    });

    // The first check is the candidate scan and reports the handle unowned.
    // Every later check is the re-check at cleanup time, which reports it
    // owned again because the task was recreated before cleanup ran.
    let checks = AtomicUsize::new(0);
    directory.runtime_handle().block_on(
        directory
            .inner()
            .cleanup_unowned(|_| checks.fetch_add(1, Ordering::SeqCst) > 0),
    );

    assert!(
        checks.load(Ordering::SeqCst) > 1,
        "ownership must be re-checked before the actor is shut down"
    );
    assert!(
        directory.inner().has_live_entry(recreated),
        "recreated handle must keep its live session"
    );
    assert!(
        web_session_store::load::<serde_json::Value>(&dir, recreated)
            .events
            .contains(&note("recreated transcript")),
        "recreated handle must keep its stored transcript"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn issue_1436_registry_reload_keeps_sessions_when_every_handle_is_owned() {
    use super::test_support::BlockingSessionDirectory;
    use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
    use ajax_core::models::AgentClient;
    use std::collections::HashSet;

    let dir = scratch_dir("reload-owned");
    let first = "web/first";
    let second = "web/second";
    web_session_store::append_events(&dir, first, &[note("first transcript")]);
    web_session_store::append_events(&dir, second, &[note("second transcript")]);
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = super::test_support::fake_acp_fixture();

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--hold-prompt"], || {
            for handle in [first, second] {
                directory
                    .acquire(handle, &dir, "auto", AgentClient::Cursor)
                    .expect("acquire before reload");
            }
        });
    });

    let owned: HashSet<String> = [first, second].into_iter().map(str::to_string).collect();
    directory.runtime_handle().block_on(
        directory
            .inner()
            .cleanup_unowned(|handle| owned.contains(handle)),
    );

    for handle in [first, second] {
        assert!(
            directory.inner().has_live_entry(handle),
            "owned session {handle} must stay live"
        );
        assert!(
            !web_session_store::load::<serde_json::Value>(&dir, handle)
                .events
                .is_empty(),
            "owned transcript {handle} must survive"
        );
    }

    let _ = std::fs::remove_dir_all(dir);
}
