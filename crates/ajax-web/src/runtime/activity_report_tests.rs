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

#[test]
fn deferred_activity_pending_matches_only_queued_handles() {
    let deferred: Arc<super::state::DeferredActivityReports> =
        Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let pending = super::state::deferred_activity_pending(Arc::clone(&deferred));
    deferred
        .lock()
        .unwrap()
        .push_back(("web/fix-login".to_string(), SessionActivity::TurnStarted));

    assert!(pending("web/fix-login"));
    assert!(!pending("web/other"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deferred_activity_stays_pending_until_it_is_applied() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, _cookie, _app) = app_with(context, TestBridge::default(), "deferred-activity-d");
    let deferred = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let pending = super::state::deferred_activity_pending(Arc::clone(&deferred));

    let lane = state.control_lane.lock().await;
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
    assert!(accepted, "TurnStarted must be accepted while lane is busy");
    let applying = state.shared();
    drop(lane);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        pending("web/fix-login"),
        "a queued report must stay pending while the drainer applies it"
    );
    drop(applying);

    let deadline = Instant::now() + Duration::from_secs(5);
    while live_kind(&state, "web/fix-login") != Some(LiveStatusKind::AgentRunning) {
        assert!(
            Instant::now() < deadline,
            "drainer never applied the deferred report; live status is {:?}",
            live_kind(&state, "web/fix-login")
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!pending("web/fix-login"));
}

#[derive(Clone)]
struct DivergedDiskBridge {
    disk: ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
    reloaded: bool,
}

impl crate::runtime::RuntimeBridge<super::tests::OkRunner> for DivergedDiskBridge {
    fn refresh_cockpit(
        &mut self,
        _context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
        _runner: &mut super::tests::OkRunner,
        _tier: ajax_core::runtime_refresh::RefreshTier,
        _deliver_notifications: bool,
    ) -> Result<bool, crate::WebError> {
        Ok(false)
    }

    fn execute_operate(
        &mut self,
        _request: crate::slices::operate::OperateRequest,
        _context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
        _runner: &mut super::tests::OkRunner,
    ) -> Result<crate::slices::operate::OperateOutcome, crate::runtime::ActionFailure> {
        unreachable!("not exercised")
    }

    fn execute_start_task(
        &mut self,
        _request: crate::slices::operate::StartTaskRequest,
        _context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
        _runner: &mut super::tests::OkRunner,
    ) -> Result<crate::slices::operate::OperateOutcome, crate::runtime::ActionFailure> {
        unreachable!("not exercised")
    }

    fn persist_registry_snapshot(
        &mut self,
        _context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
    ) -> Result<(), crate::WebError> {
        if self.reloaded {
            Ok(())
        } else {
            Err(crate::WebError::CommandFailed(
                "state conflict for web/fix-login: disk and in-memory task facts diverged"
                    .to_string(),
            ))
        }
    }

    fn reload_registry_from_disk(
        &mut self,
        context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
    ) -> Result<bool, crate::WebError> {
        *context = self.disk.clone();
        self.reloaded = true;
        Ok(true)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn activity_report_recovers_when_disk_diverged_from_memory() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let bridge = DivergedDiskBridge {
        disk: context.clone(),
        reloaded: false,
    };
    let state = super::WebAppState::new(
        context,
        super::tests::OkRunner,
        bridge,
        super::tests::scratch_dir("diverged-activity"),
    );

    let state_for_report = state.clone();
    let result = tokio::spawn(async move {
        state_for_report
            .report_task_session_activity("web/fix-login", SessionActivity::CommandRunning)
    })
    .await
    .expect("report worker should not panic");

    assert_eq!(result, Ok(()));
    let guard = state.shared();
    let live = guard
        .context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .and_then(|task| task.live_status.as_ref().map(|status| status.kind));
    assert_eq!(live, Some(LiveStatusKind::CommandRunning));
}
