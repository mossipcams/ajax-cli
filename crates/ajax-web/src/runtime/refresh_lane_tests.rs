use super::tests::{
    app_with, blocking_notification_bridge, blocking_refresh_bridge, context_with_task, get,
    release_gate, unsaved_notification_bridge,
};
use ajax_core::registry::Registry as _;
use axum::http::StatusCode;
use std::{
    sync::atomic::Ordering,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use tokio::sync::Notify;

type ProbeGate = (Arc<Notify>, Arc<(Mutex<bool>, Condvar)>);

fn probe_gate() -> ProbeGate {
    (
        Arc::new(Notify::new()),
        Arc::new((Mutex::new(false), Condvar::new())),
    )
}

/// Starts a GET /api/cockpit whose refresh probe blocks in the bridge, and waits
/// until it has entered. The probe holds `refresh_flight` but must not hold the
/// control lane.
async fn start_blocked_refresh(
    app: &axum::Router,
    cookie: &str,
    entered: &Notify,
) -> tokio::task::JoinHandle<axum::response::Response> {
    let refresh_app = app.clone();
    let refresh_cookie = cookie.to_string();
    let refresh =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("cockpit refresh never entered the bridge");
    refresh
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cockpit_probe_runs_without_holding_control_lane() {
    let (entered, release) = probe_gate();
    let (state, cookie, app) = app_with(
        context_with_task(),
        blocking_refresh_bridge(Arc::clone(&entered), Arc::clone(&release)),
        "refresh-lane-a",
    );
    let refresh = start_blocked_refresh(&app, &cookie, &entered).await;

    let lane_free = state.control_lane.try_lock().is_ok();
    release_gate(&release);
    let response = tokio::time::timeout(Duration::from_secs(5), refresh)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();

    assert!(lane_free, "control lane must be free while the probe runs");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_activity_report_succeeds_while_probe_runs() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (entered, release) = probe_gate();
    let (state, cookie, app) = app_with(
        context,
        blocking_refresh_bridge(Arc::clone(&entered), Arc::clone(&release)),
        "refresh-lane-b",
    );
    let refresh = start_blocked_refresh(&app, &cookie, &entered).await;

    let report_state = state.clone();
    let report = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::spawn(async move {
            report_state.report_task_session_activity(
                "web/fix-login",
                crate::slices::web_session::SessionActivity::TurnStarted,
            )
        }),
    )
    .await;
    release_gate(&release);
    let response = tokio::time::timeout(Duration::from_secs(5), refresh)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();

    let report = report
        .expect("session activity report must not wait behind the probe")
        .expect("report worker should not panic");
    assert_eq!(report, Ok(()), "activity report must not hit a busy lane");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_model_persist_completes_while_probe_runs() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (entered, release) = probe_gate();
    let (state, cookie, app) = app_with(
        context,
        blocking_refresh_bridge(Arc::clone(&entered), Arc::clone(&release)),
        "refresh-lane-c",
    );
    let refresh = start_blocked_refresh(&app, &cookie, &entered).await;

    let persist_state = state.clone();
    let persist = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::spawn(async move {
            persist_state.persist_task_session_model("web/fix-login", "composer-2.5")
        }),
    )
    .await;
    release_gate(&release);
    let response = tokio::time::timeout(Duration::from_secs(5), refresh)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();

    let persist = persist
        .expect("session model persist must not wait behind the probe")
        .expect("persist worker should not panic");
    assert_eq!(persist, Ok(()));
    assert_eq!(response.status(), StatusCode::OK);
}

/// A non-skip-interactive task whose CI monitor reports a failed episode with no
/// delivery recorded, so `pending_for_task` yields an agent notification.
fn task_with_pending_ci_failure() -> ajax_core::models::Task {
    let mut task = crate::test_support::fix_login_task();
    task.metadata.insert(
        ajax_core::agent_notification::CI_MONITOR_STATE_KEY.to_string(),
        serde_json::json!({
            "status": "failed",
            "pr_number": 7,
            "head_sha": "abc123",
            "episode_id": "ci-failed:web/fix-login:7:abc123:1",
            "failed_checks": [{ "name": "CI", "link": null, "identity": "run:1" }],
        })
        .to_string(),
    );
    task
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agent_notification_delivery_runs_without_control_lane() {
    let (entered, release) = probe_gate();
    let context =
        crate::test_support::context_with_tasks(&["web"], vec![task_with_pending_ci_failure()]);
    let (state, cookie, app) = app_with(
        context,
        blocking_notification_bridge(Arc::clone(&entered), Arc::clone(&release)),
        "notification-lane-a",
    );
    let refresh_app = app.clone();
    let refresh_cookie = cookie.clone();
    let refresh =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("agent notification delivery never started");

    let lane_free = state.control_lane.try_lock().is_ok();
    release_gate(&release);
    let response = tokio::time::timeout(Duration::from_secs(5), refresh)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();

    assert!(
        lane_free,
        "control lane must be free while a notification is delivered"
    );
    assert_eq!(response.status(), StatusCode::OK);
    let guard = state.shared();
    let task = guard
        .context
        .registry
        .list_tasks()
        .into_iter()
        .next()
        .expect("task");
    assert_eq!(
        ajax_core::agent_notification::pending_for_task(task),
        None,
        "delivered notification must no longer be pending"
    );
    assert_eq!(guard.bridge.notification_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delivered_agent_notification_is_not_redelivered_after_cache_expiry() {
    let (entered, release) = probe_gate();
    release_gate(&release);
    let context =
        crate::test_support::context_with_tasks(&["web"], vec![task_with_pending_ci_failure()]);
    let (state, cookie, app) = app_with(
        context,
        blocking_notification_bridge(entered, release),
        "notification-lane-b",
    );

    let first = get(&app, &cookie, "/api/cockpit").await;
    assert_eq!(first.status(), StatusCode::OK);
    state.shared().cockpit_cache = None;
    let second = get(&app, &cookie, "/api/cockpit").await;

    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        state
            .shared()
            .bridge
            .notification_calls
            .load(Ordering::SeqCst),
        1,
        "a delivered notification must not be sent again on a later refresh"
    );
}

/// Delivery that succeeds in memory but whose registry persist fails must still
/// swap the delivered marks in, so a later refresh does not redeliver.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unsaved_notification_delivery_is_not_redelivered_on_next_refresh() {
    let (entered, release) = probe_gate();
    release_gate(&release);
    let context =
        crate::test_support::context_with_tasks(&["web"], vec![task_with_pending_ci_failure()]);
    let (state, cookie, app) = app_with(
        context,
        unsaved_notification_bridge(entered, release),
        "notification-lane-unsaved",
    );

    let first = get(&app, &cookie, "/api/cockpit").await;
    assert_eq!(first.status(), StatusCode::OK);
    state.shared().cockpit_cache = None;
    let second = get(&app, &cookie, "/api/cockpit").await;

    assert_eq!(second.status(), StatusCode::OK);
    let guard = state.shared();
    assert_eq!(
        guard.bridge.notification_calls.load(Ordering::SeqCst),
        1,
        "a failed persist must not cause the notification to be delivered again"
    );
    let task = guard
        .context
        .registry
        .list_tasks()
        .into_iter()
        .next()
        .expect("task");
    assert_eq!(
        ajax_core::agent_notification::pending_for_task(task),
        None,
        "delivered notification must stay delivered in shared state after a failed persist"
    );
}
