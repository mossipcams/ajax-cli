use super::*;

#[test]
fn cockpit_refresh_that_drops_a_task_shuts_down_its_live_session() {
    use crate::adapters::web_session_acp::with_test_acp_program;
    use ajax_core::models::AgentClient;

    let worktree = scratch_dir("unowned-refresh-worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let handle = "web/fix-login";
    let mut task = crate::test_support::fix_login_task();
    task.worktree_path = worktree.clone();
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let bridge = TestBridge {
        drop_task_on_refresh: Some(handle.to_string()),
        ..TestBridge::default()
    };
    let state = WebAppState::new(
        context,
        OkRunner,
        bridge,
        scratch_dir("unowned-refresh-storage"),
    );
    let directory = Arc::clone(&state.task_session_directory);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("test runtime");
    let script =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js");

    with_test_acp_program(&script, || {
        rt.block_on(directory.acquire(handle, &worktree, "auto", AgentClient::Cursor))
    })
    .expect("acquire before refresh");
    assert!(directory.has_live_entry(handle));

    rt.block_on(refresh_cockpit_and_cache(&state, RefreshTier::Live, false));

    // Cleanup is spawned onto the runtime, so wait for the actor to shut down.
    rt.block_on(async {
        for _ in 0..200 {
            if !directory.has_live_entry(handle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(
        !directory.has_live_entry(handle),
        "a cockpit refresh that drops an owned task must shut down its live session"
    );

    let _ = std::fs::remove_dir_all(worktree);
}
