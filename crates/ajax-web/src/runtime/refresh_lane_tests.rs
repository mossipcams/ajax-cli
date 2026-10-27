use super::tests::{app_with, blocking_refresh_bridge, context_with_task, get, release_gate};
use axum::http::StatusCode;
use std::{
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
