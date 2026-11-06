use super::tests::{app_with, get, json_of, TestBridge};
use crate::slices::web_session::{SessionActivity, SessionServerEvent};
use ajax_core::{
    models::{LifecycleStatus, LiveStatusKind, TaskId},
    registry::Registry as _,
    ui_state::TaskStatus,
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

fn live_kind<B>(
    state: &super::WebAppState<super::tests::OkRunner, B>,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn per_task_activity_error_restores_task_and_falls_back_to_reload() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.lifecycle_status = LifecycleStatus::Active;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut bridge = TestBridge::default();
    bridge.disk_context = Arc::new(Mutex::new(Some(context.clone())));
    bridge.record_activity_failures = 1;
    let reload_calls = Arc::clone(&bridge.reload_calls);
    let state = super::WebAppState::new(
        context,
        super::tests::OkRunner,
        bridge,
        super::tests::scratch_dir("conflicting-activity-reload"),
    );

    let state_for_report = state.clone();
    let result = tokio::spawn(async move {
        state_for_report
            .report_task_session_activity("web/fix-login", SessionActivity::CommandRunning)
    })
    .await
    .expect("report worker should not panic");

    assert_eq!(result, Ok(()));
    assert_eq!(reload_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        live_kind(&state, "web/fix-login"),
        Some(LiveStatusKind::CommandRunning)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_activity_write_and_reload_leave_the_task_unchanged() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.lifecycle_status = LifecycleStatus::Active;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut bridge = TestBridge::default();
    bridge.record_activity_failures = usize::MAX;
    bridge.reload_fails = true;
    let state = super::WebAppState::new(
        context,
        super::tests::OkRunner,
        bridge,
        super::tests::scratch_dir("failed-activity-restore"),
    );
    let before = state
        .shared()
        .context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .cloned()
        .expect("fix-login task");

    let state_for_report = state.clone();
    let result = tokio::spawn(async move {
        state_for_report
            .report_task_session_activity("web/fix-login", SessionActivity::CommandRunning)
    })
    .await
    .expect("report worker should not panic");

    assert!(result.is_err(), "both write paths failed; report must fail");
    let guard = state.shared();
    assert_eq!(
        guard
            .context
            .registry
            .get_task(&TaskId::new("web/fix-login")),
        Some(&before)
    );
}

async fn cockpit_card_status(
    app: &axum::Router,
    cookie: &str,
) -> (serde_json::Value, serde_json::Value) {
    let body = json_of(get(app, cookie, "/api/cockpit").await).await;
    (
        body["cards"][0]["status"].clone(),
        body["cards"][0]["status_explanation"].clone(),
    )
}

async fn detail_status(
    app: &axum::Router,
    cookie: &str,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let body = json_of(get(app, cookie, "/api/tasks/web/fix-login").await).await;
    (
        body["status"].clone(),
        body["status_explanation"].clone(),
        body["live_status_kind"].clone(),
    )
}

async fn record_turn_start(state: &super::WebAppState<super::tests::OkRunner, TestBridge>) {
    state
        .task_session_directory
        .record(
            "web/fix-login",
            SessionServerEvent::PromptAccepted {
                client_message_id: "prompt-1".to_string(),
            },
        )
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_session_status_is_served_while_the_turn_start_report_is_deferred() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, cookie, app) = app_with(context, TestBridge::default(), "deferred-activity-e");
    let running = serde_json::to_value(TaskStatus::Running).unwrap();
    let before = cockpit_card_status(&app, &cookie).await;
    assert_ne!(before.0, running, "the task starts out not running");

    let lane = state.control_lane.lock().await;
    let flight = state.refresh_flight.lock().await;
    record_turn_start(&state).await;

    assert_eq!(
        live_kind(&state, "web/fix-login"),
        None,
        "the persisted registry still shows the pre-turn status"
    );
    let during = cockpit_card_status(&app, &cookie).await;
    assert_eq!(during.0, running);
    let detail = detail_status(&app, &cookie).await;
    assert_eq!(detail.0, running);
    assert_eq!(detail.2, serde_json::json!("AgentRunning"));

    drop(flight);
    drop(lane);
    let deadline = Instant::now() + Duration::from_secs(5);
    while live_kind(&state, "web/fix-login") != Some(LiveStatusKind::AgentRunning)
        || !state.task_session_directory.live_activities().is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "drainer never applied the deferred turn start; live status is {:?}",
            live_kind(&state, "web/fix-login")
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(cockpit_card_status(&app, &cookie).await, during);
    assert_eq!(detail_status(&app, &cookie).await, detail);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_session_overlay_is_dropped_when_the_session_actor_ends() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, cookie, app) = app_with(context, TestBridge::default(), "deferred-activity-f");
    let running = serde_json::to_value(TaskStatus::Running).unwrap();
    let before = cockpit_card_status(&app, &cookie).await;

    let lane = state.control_lane.lock().await;
    let flight = state.refresh_flight.lock().await;
    record_turn_start(&state).await;
    assert_eq!(
        state.task_session_directory.live_activities(),
        vec![("web/fix-login".to_string(), SessionActivity::TurnStarted)]
    );
    assert_eq!(cockpit_card_status(&app, &cookie).await.0, running);

    state
        .task_session_directory
        .drop_session("web/fix-login")
        .await;

    assert!(state.task_session_directory.live_activities().is_empty());
    assert_eq!(cockpit_card_status(&app, &cookie).await, before);
    assert_ne!(detail_status(&app, &cookie).await.0, running);

    drop(flight);
    drop(lane);
    let deadline = Instant::now() + Duration::from_secs(5);
    while live_kind(&state, "web/fix-login") != Some(LiveStatusKind::AgentRunning) {
        assert!(
            Instant::now() < deadline,
            "the deferred turn start must still reach the registry"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[derive(Clone, Default)]
struct ConflictedWholeRegistryBridge {
    calls: Arc<Mutex<Vec<String>>>,
}

impl crate::runtime::RuntimeBridge<super::tests::OkRunner> for ConflictedWholeRegistryBridge {
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
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push("persist".to_string());
        Err(crate::WebError::CommandFailed(
            "state conflict for web/fix-login: task facts diverged".to_string(),
        ))
    }

    fn record_task_activity(
        &mut self,
        _context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
        task_id: &TaskId,
        _previous: &ajax_core::models::Task,
    ) -> Result<(), crate::WebError> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!("record:{}", task_id.as_str()));
        Ok(())
    }
}

fn two_tasks_with_conflicted_a(
    b_lifecycle: LifecycleStatus,
) -> (
    super::WebAppState<super::tests::OkRunner, ConflictedWholeRegistryBridge>,
    Arc<Mutex<Vec<String>>>,
) {
    let mut task_a = crate::test_support::fix_login_task();
    task_a.set_skip_interactive_agent(true);
    let mut task_b = crate::test_support::task_in("web", "other", "Other");
    task_b.set_skip_interactive_agent(true);
    task_b.lifecycle_status = b_lifecycle;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task_a, task_b]);
    let bridge = ConflictedWholeRegistryBridge::default();
    let calls = Arc::clone(&bridge.calls);
    let state = super::WebAppState::new(
        context,
        super::tests::OkRunner,
        bridge,
        super::tests::scratch_dir("task-activity-write"),
    );
    (state, calls)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn non_lifecycle_activity_persists_per_task_past_another_tasks_conflict() {
    let (state, calls) = two_tasks_with_conflicted_a(LifecycleStatus::Active);

    let state_for_report = state.clone();
    let result = tokio::spawn(async move {
        state_for_report.report_task_session_activity("web/other", SessionActivity::TurnEnded)
    })
    .await
    .expect("report worker should not panic");

    assert_eq!(result, Ok(()));
    assert_eq!(live_kind(&state, "web/other"), Some(LiveStatusKind::Done));
    assert_eq!(live_kind(&state, "web/fix-login"), None);
    let guard = state.shared();
    assert_eq!(guard.revision, 1);
    assert!(guard.cockpit_cache.is_none());
    drop(guard);
    assert_eq!(
        calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_slice(),
        ["record:web/other"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_changing_activity_keeps_whole_registry_path() {
    let (state, calls) = two_tasks_with_conflicted_a(LifecycleStatus::Reviewable);

    let state_for_report = state.clone();
    let result = tokio::spawn(async move {
        state_for_report.report_task_session_activity("web/other", SessionActivity::CommandRunning)
    })
    .await
    .expect("report worker should not panic");

    assert!(result.is_err());
    assert_eq!(live_kind(&state, "web/other"), None);
    let guard = state.shared();
    assert_eq!(guard.revision, 0);
    let lifecycle = guard
        .context
        .registry
        .get_task(&TaskId::new("web/other"))
        .map(|task| task.lifecycle_status);
    assert_eq!(lifecycle, Some(LifecycleStatus::Reviewable));
    drop(guard);
    let calls = calls
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert!(calls.iter().any(|call| call == "persist"));
    assert!(!calls.iter().any(|call| call.starts_with("record:")));
}
