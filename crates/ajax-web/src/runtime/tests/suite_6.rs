//! Registry mutations must not report success when the snapshot was not saved.

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
