use super::*;
use ajax_core::{models::TaskId, registry::Registry as _};

fn unsaved_bridge() -> TestBridge {
    TestBridge {
        persist_result: Err(crate::WebError::CommandFailed("disk full".to_string())),
        ..TestBridge::default()
    }
}

fn fix_login(state: &WebAppState<OkRunner, TestBridge>) -> ajax_core::models::Task {
    state
        .shared()
        .context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("task")
        .clone()
}

#[test]
fn issue_1224_session_model_change_fails_when_registry_is_not_saved() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, unsaved_bridge(), "model-unsaved-1224");
    let revision = state.shared().revision;

    let error = state
        .persist_task_session_model("web/fix-login", "composer-2.5")
        .expect_err("an unsaved model change must not report success");

    assert!(error.contains("was not saved"), "{error}");
    assert_eq!(fix_login(&state).session_model(), None);
    assert_eq!(state.shared().revision, revision);
}

#[test]
fn issue_1225_acp_promotion_fails_when_registry_is_not_saved() {
    let mut task = crate::test_support::fix_login_task();
    task.worktree_path = scratch_dir("promotion-unsaved-1225-worktree");
    std::fs::create_dir_all(&task.worktree_path).expect("worktree");
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, unsaved_bridge(), "promotion-unsaved-1225");

    let result = state.prepare_task_session_attach("web/fix-login", "auto");

    assert_eq!(
        result.err(),
        Some(crate::slices::web_session::SessionRouteError::PromotionNotSaved)
    );
    assert!(
        !fix_login(&state).skip_interactive_agent(),
        "an unsaved promotion must not stay applied in memory"
    );
}

#[tokio::test]
async fn issue_1226_harness_switch_fails_when_registry_is_not_saved() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let before = task.selected_agent;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, cookie, app) = app_with(context, unsaved_bridge(), "swap-unsaved-1226");

    let response = post_json(
        &app,
        &cookie,
        "/api/tasks/web%2Ffix-login",
        r#"{"agent":"claude"}"#,
    )
    .await;

    assert_ne!(response.status(), StatusCode::OK);
    assert_eq!(fix_login(&state).selected_agent, before);
}

#[test]
fn issue_1227_session_activity_fails_when_registry_is_not_saved() {
    use crate::slices::web_session::SessionActivity;

    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, unsaved_bridge(), "activity-unsaved-1227");
    let before = fix_login(&state);
    let revision = state.shared().revision;

    let error = state
        .report_task_session_activity("web/fix-login", SessionActivity::TurnStarted)
        .expect_err("unsaved activity must not be treated as committed");

    assert!(error.contains("was not saved"), "{error}");
    assert_eq!(fix_login(&state), before);
    assert_eq!(state.shared().revision, revision);
}

#[test]
fn issue_1232_terminal_input_acknowledgment_runs_without_the_shared_state_lock() {
    let bridge = TestBridge {
        acknowledge_result: Ok(true),
        ..TestBridge::default()
    };
    let probe = Arc::clone(&bridge.acknowledge_probe);
    let state = state_with_bridge_and_task(bridge);
    let shared = Arc::clone(&state.shared);
    let lock_was_free = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = Arc::clone(&lock_was_free);
    *probe.lock().unwrap() = Some(Box::new(move || {
        seen.store(shared.try_lock().is_ok(), Ordering::SeqCst);
    }));
    let revision = state.shared().revision;

    super::operator_input_sink(&state, "web/fix-login".to_string())();

    assert!(
        lock_was_free.load(Ordering::SeqCst),
        "the shared state lock must be free while the acknowledgment persists"
    );
    assert_eq!(state.shared().revision, revision + 1);
}

struct SwapRun {
    status: StatusCode,
    body: serde_json::Value,
    state: super::WebAppState<super::OkRunner, TestBridge>,
    persisted: Option<CommandContext<InMemoryRegistry>>,
}

fn run_cursor_to_claude_swap(tag: &str, claude_program: &std::path::Path) -> SwapRun {
    use crate::adapters::web_session_acp::{with_test_acp_program, with_test_claude_sdk_module};
    use ajax_core::models::AgentClient;
    use std::sync::Arc;

    let worktree = scratch_dir(&format!("{tag}-worktree"));
    std::fs::create_dir_all(&worktree).unwrap();
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.selected_agent = AgentClient::Cursor;
    task.worktree_path = worktree.clone();
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let cursor_program =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js");
    let bridge = TestBridge::default();
    let persisted_slot = Arc::clone(&bridge.persisted_snapshot);
    let (state, cookie, app) = app_with(context, bridge, tag);
    let directory = Arc::clone(&state.task_session_directory);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");

    let (status, body) = with_test_acp_program(&cursor_program, || {
        rt.block_on(async {
            directory
                .acquire("web/fix-login", &worktree, "auto", AgentClient::Cursor)
                .await
                .expect("acquire");
            assert!(directory.has_live_entry("web/fix-login"));
        });
        with_test_claude_sdk_module(claude_program, || {
            rt.block_on(async {
                let response = post_json(
                    &app,
                    &cookie,
                    "/api/tasks/web%2Ffix-login",
                    r#"{"agent":"claude"}"#,
                )
                .await;
                let status = response.status();
                (status, json_of(response).await)
            })
        })
    });
    let persisted = persisted_slot.lock().unwrap().clone();
    SwapRun {
        status,
        body,
        state,
        persisted,
    }
}

fn exit_immediately_program(tag: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_dir(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("claude-exit-before-handshake.mjs");
    std::fs::write(&script, "#!/usr/bin/env node\nprocess.exit(1);\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[test]
fn issue_1546_failed_swap_keeps_registry_on_old_harness() {
    use ajax_core::models::AgentClient;

    let failing = exit_immediately_program("issue-1546-failing-claude");
    let run = run_cursor_to_claude_swap("issue-1546-failed-swap", &failing);

    assert_eq!(run.status, StatusCode::CONFLICT);
    assert_eq!(run.body["ok"], false);
    assert_eq!(run.body["code"], "command_failed");
    let error = run.body["error"].as_str().unwrap_or_default();
    assert!(error.contains("claude"), "{error}");
    assert_eq!(fix_login(&run.state).selected_agent, AgentClient::Cursor);
    let saved = run.persisted.expect("registry snapshot must be persisted");
    assert_eq!(
        saved
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("task")
            .selected_agent,
        AgentClient::Cursor
    );
}

#[test]
fn issue_1546_successful_swap_moves_registry() {
    use ajax_core::models::AgentClient;

    let healthy = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_claude_sdk.mjs");
    let run = run_cursor_to_claude_swap("issue-1546-ok-swap", &healthy);

    assert_eq!(run.status, StatusCode::OK);
    assert_eq!(run.body["ok"], true);
    assert_eq!(fix_login(&run.state).selected_agent, AgentClient::Claude);
    let saved = run.persisted.expect("registry snapshot must be persisted");
    assert_eq!(
        saved
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("task")
            .selected_agent,
        AgentClient::Claude
    );
}

#[tokio::test]
async fn issue_1546_rollback_keeps_unrelated_task_changes() {
    use ajax_core::models::{AgentClient, LifecycleStatus};

    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.selected_agent = AgentClient::Claude;
    task.set_session_model(Some("sonnet"));
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) =
        app_with(context, TestBridge::default(), "issue-1546-rollback-keeps");
    state
        .shared()
        .context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("task")
        .lifecycle_status = LifecycleStatus::Reviewable;

    crate::runtime::task_routes::rollback_harness_swap(
        state.clone(),
        "web/fix-login".to_string(),
        AgentClient::Claude,
        (AgentClient::Cursor, None),
    )
    .await
    .expect("rollback");

    let task = fix_login(&state);
    assert_eq!(task.selected_agent, AgentClient::Cursor);
    assert_eq!(task.session_model(), None);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
}
