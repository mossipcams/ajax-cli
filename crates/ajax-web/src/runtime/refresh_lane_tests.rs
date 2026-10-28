use super::tests::{
    app_with, blocking_notification_bridge, blocking_refresh_bridge, context_with_task, get,
    json_of, post_json, release_gate, scratch_dir, unsaved_notification_bridge, TestBridge,
};
use crate::adapters::web_session_acp::{set_test_acp_command, with_test_handshake_timeout};
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
async fn background_loops_start_push_tick_once() {
    let (state, _cookie, _app) =
        app_with(context_with_task(), TestBridge::default(), "push-tick-once");
    let metrics = tokio::runtime::Handle::current().metrics();
    let alive_before = metrics.num_alive_tasks();

    super::spawn_background_loops(&state);

    let started = metrics.num_alive_tasks() - alive_before;
    assert_eq!(
        started, 3,
        "expected push flusher, one push tick, and heartbeat loops; a second push tick loop would add a fourth"
    );
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chat_remediation_delivery_runs_without_control_lane() {
    let skill_root = scratch_dir("chat-remediation-skills");
    let skill_dir = skill_root.join("gh-fix-ci");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
    std::env::set_var("AJAX_SKILL_ROOT", &skill_root);
    let worktree = scratch_dir("chat-remediation-worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.worktree_path = worktree;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (state, cookie, app) = app_with(context, TestBridge::default(), "chat-remediation-lane");
    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_acp.js")
        .to_string_lossy()
        .into_owned();
    set_test_acp_command(Some((std::path::Path::new("node"), &[script.as_str()])));

    let lane = state.control_lane.lock().await;
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        post_json(
            &app,
            &cookie,
            "/api/actions",
            r#"{"request_id":"chat-remediation-lane-a","task_handle":"web/fix-login","action":"fix-ci"}"#,
        ),
    )
    .await;
    drop(lane);
    set_test_acp_command(None);

    let response = response.expect("chat remediation waited on the held control lane");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["request_id"], "chat-remediation-lane-a");
    assert_eq!(body["output"], "fix-ci: skill brief sent to chat task");
}

async fn wait_until(mut ready: impl FnMut() -> bool, failure: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect(failure);
}

#[test]
fn chat_remediation_releases_operation_gate_when_client_disconnects() {
    let skill_root = scratch_dir("chat-remediation-disconnect-skills");
    let skill_dir = skill_root.join("gh-fix-ci");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
    std::env::set_var("AJAX_SKILL_ROOT", &skill_root);
    let worktree = scratch_dir("chat-remediation-disconnect-worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.worktree_path = worktree;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    set_test_acp_command(Some((std::path::Path::new("sleep"), &["60"])));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    with_test_handshake_timeout(200, || {
        runtime.block_on(async {
            let (state, cookie, app) =
                app_with(context, TestBridge::default(), "chat-remediation-disconnect");
            let remediate = |request_id: &str| {
                format!(
                    r#"{{"request_id":"{request_id}","task_handle":"web/fix-login","action":"fix-ci"}}"#
                )
            };
            let request = tokio::spawn({
                let app = app.clone();
                let cookie = cookie.clone();
                let body = remediate("chat-remediation-disconnect-a");
                async move { post_json(&app, &cookie, "/api/actions", &body).await }
            });
            wait_until(
                || state.operations().has_in_flight_mutation(),
                "chat remediation never claimed the operation gate",
            )
            .await;

            request.abort();
            wait_until(
                || !state.operations().has_in_flight_mutation(),
                "dropped chat remediation left the operation gate held",
            )
            .await;

            let response = post_json(
                &app,
                &cookie,
                "/api/actions",
                &remediate("chat-remediation-disconnect-b"),
            )
            .await;
            let body = json_of(response).await;
            assert_ne!(
                body["error"], "operation already in progress",
                "a following action was rejected by a leaked operation gate"
            );
        })
    });
    set_test_acp_command(None);
}
