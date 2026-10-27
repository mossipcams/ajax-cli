use super::tests::{app_with, TestBridge};
use crate::slices::web_session::SessionActivity;
use ajax_core::{
    models::{LiveStatusKind, TaskId},
    registry::Registry as _,
};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

fn release_gate(release: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, cvar) = &**release;
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    cvar.notify_all();
}

fn live_kind(
    state: &super::WebAppState<super::tests::OkRunner, TestBridge>,
    handle: &str,
) -> Option<LiveStatusKind> {
    let guard = state.shared();
    guard
        .context
        .registry
        .get_task(&TaskId::new(handle))
        .and_then(|task| task.live_status.as_ref().map(|status| status.kind))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deferred_activity_reports_apply_in_order_once_control_lane_frees() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, TestBridge::default(), "deferred-activity-a");
    let deferred = Arc::new(Mutex::new(std::collections::VecDeque::new()));

    let lane_held = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let state_for_lane = state.clone();
    let lane_held_signal = Arc::clone(&lane_held);
    let release_for_lane = Arc::clone(&release);
    let lane_task = tokio::task::spawn_blocking(move || {
        let _lane = state_for_lane.control_lane.blocking_lock();
        lane_held_signal.notify_one();
        let (lock, cvar) = &*release_for_lane;
        let mut released = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = cvar
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    });
    tokio::time::timeout(Duration::from_secs(5), lane_held.notified())
        .await
        .expect("control lane holder never started");

    let state_for_report = state.clone();
    let deferred_for_report = Arc::clone(&deferred);
    let (started, ended) = tokio::spawn(async move {
        let started = state_for_report.report_or_defer_task_session_activity(
            &deferred_for_report,
            "web/fix-login",
            SessionActivity::TurnStarted,
        );
        let ended = state_for_report.report_or_defer_task_session_activity(
            &deferred_for_report,
            "web/fix-login",
            SessionActivity::TurnEnded,
        );
        (started, ended)
    })
    .await
    .expect("report worker should not panic");

    assert!(started, "TurnStarted must be accepted while lane is busy");
    assert!(ended, "TurnEnded must queue behind TurnStarted");
    assert_eq!(
        live_kind(&state, "web/fix-login"),
        None,
        "deferred reports must not apply while the lane is held"
    );

    release_gate(&release);
    lane_task.await.expect("lane holder should finish");

    let deadline = Instant::now() + Duration::from_secs(5);
    while live_kind(&state, "web/fix-login") != Some(LiveStatusKind::Done) {
        assert!(
            Instant::now() < deadline,
            "drainer never applied deferred reports; live status is {:?}",
            live_kind(&state, "web/fix-login")
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(deferred.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_handle_report_is_rejected_without_queueing() {
    let context = crate::test_support::context_with_fix_login_task();
    let (state, _cookie, _app) = app_with(context, TestBridge::default(), "deferred-activity-b");
    let deferred = Arc::new(Mutex::new(std::collections::VecDeque::new()));

    let state_for_report = state.clone();
    let deferred_for_report = Arc::clone(&deferred);
    let accepted = tokio::spawn(async move {
        state_for_report.report_or_defer_task_session_activity(
            &deferred_for_report,
            "web/missing",
            SessionActivity::TurnStarted,
        )
    })
    .await
    .expect("report worker should not panic");

    assert!(
        !accepted,
        "unknown handle is a real failure, not a busy lane"
    );
    assert!(deferred.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn free_control_lane_applies_activity_synchronously() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, TestBridge::default(), "deferred-activity-c");
    let deferred = Arc::new(Mutex::new(std::collections::VecDeque::new()));

    let state_for_report = state.clone();
    let deferred_for_report = Arc::clone(&deferred);
    let accepted = tokio::spawn(async move {
        state_for_report.report_or_defer_task_session_activity(
            &deferred_for_report,
            "web/fix-login",
            SessionActivity::TurnStarted,
        )
    })
    .await
    .expect("report worker should not panic");

    assert!(accepted);
    assert_eq!(
        live_kind(&state, "web/fix-login"),
        Some(LiveStatusKind::AgentRunning)
    );
    assert!(deferred.lock().unwrap().is_empty());
}
