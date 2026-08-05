use super::*;

#[test]
fn operator_input_sink_calls_bridge_once_and_bumps_revision_and_clears_cache_when_acknowledged() {
    let bridge = TestBridge {
        acknowledge_result: Ok(true),
        ..TestBridge::default()
    };
    let acknowledge_calls = Arc::clone(&bridge.acknowledge_calls);
    let state = state_with_bridge_and_task(bridge);
    {
        let mut guard = state.shared();
        guard.cockpit_cache = Some(super::CockpitCacheEntry {
            response: super::Response {
                status_code: 200,
                content_type: "application/json; charset=utf-8",
                body: Vec::new(),
            },
            cached_at: Instant::now(),
            revision: 0,
        });
    }
    let base_revision = state.shared().revision;

    let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
    sink();
    sink();

    assert_eq!(acknowledge_calls.load(Ordering::SeqCst), 2);
    let guard = state.shared();
    assert_eq!(guard.revision, base_revision + 2);
    assert!(guard.cockpit_cache.is_none());
}

#[test]
fn operator_input_sink_leaves_revision_and_cache_untouched_when_bridge_returns_false() {
    let bridge = TestBridge {
        acknowledge_result: Ok(false),
        ..TestBridge::default()
    };
    let acknowledge_calls = Arc::clone(&bridge.acknowledge_calls);
    let state = state_with_bridge_and_task(bridge);
    {
        let mut guard = state.shared();
        guard.cockpit_cache = Some(super::CockpitCacheEntry {
            response: super::Response {
                status_code: 200,
                content_type: "application/json; charset=utf-8",
                body: Vec::new(),
            },
            cached_at: Instant::now(),
            revision: 0,
        });
    }
    let base_revision = state.shared().revision;
    let cache_was = state.shared().cockpit_cache.is_some();

    let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
    sink();

    assert_eq!(acknowledge_calls.load(Ordering::SeqCst), 1);
    let guard = state.shared();
    assert_eq!(guard.revision, base_revision);
    assert_eq!(guard.cockpit_cache.is_some(), cache_was);
}

#[test]
fn logging_web_listening_writes_to_ajax_log() {
    let logs_dir = logging_test_logs_dir();
    super::log_web_listening("127.0.0.1", 9443);

    let contents = read_logging_test_log(logs_dir);
    assert!(
        contents.contains("listening"),
        "expected listening in log: {contents}"
    );
    assert!(
        contents.contains("127.0.0.1"),
        "expected host in log: {contents}"
    );
    assert!(
        contents.contains("9443"),
        "expected port in log: {contents}"
    );
}

#[test]
fn logging_operate_unknown_action_includes_action_field() {
    let logs_dir = logging_test_logs_dir();
    let mut context = context_with_task();
    let mut runner = RecordingCommandRunner::default();

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "not-a-real-action".to_string(),
            confirmed: false,
            branch_adoption: None,
        },
    )
    .unwrap_err();

    assert!(matches!(error, OperateError::UnknownAction(_)));

    let contents = read_logging_test_log(logs_dir);
    assert!(
        contents.contains("action=") && contents.contains("not-a-real-action"),
        "expected action field in log: {contents}"
    );
    assert!(
        contents.contains("outcome=\"err\"") || contents.contains("outcome=err"),
        "expected err outcome in log: {contents}"
    );
}

fn logging_test_logs_dir() -> &'static std::path::Path {
    use std::{
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };

    static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
    static INIT: Mutex<()> = Mutex::new(());

    let _guard = INIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    LOGS_DIR.get_or_init(|| {
        let logs_dir =
            std::env::temp_dir().join(format!("ajax_web_logging_tests_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&logs_dir);
        ajax_core::logging::init_to_logs_dir(&logs_dir);
        logs_dir
    })
}

fn read_logging_test_log(logs_dir: &std::path::Path) -> String {
    std::fs::read_to_string(logs_dir.join("ajax.log")).expect("ajax.log should exist")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_optimistic_recovers_durable_operate_after_revision_cas_loss() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let bridge = TestBridge {
        operate_entered: Some(Arc::clone(&entered)),
        operate_release: Some(Arc::clone(&release)),
        acknowledge_result: Ok(true),
        operate_result: Ok(OperateOutcome {
            state_changed: true,
            output: "reviewed".to_string(),
        }),
        ..TestBridge::default()
    };
    let reload_calls = Arc::clone(&bridge.reload_calls);
    let (state, cookie, app) = app_with(context_with_task(), bridge, "run-optimistic-cas-recover");

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"req-cas-recover","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("operation never entered the bridge");

    let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
    sink();

    release_gate(&release);

    let response = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .expect("operation timed out")
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_of(response).await;
    assert_eq!(json["ok"], true);
    assert_eq!(json["request_id"], "req-cas-recover");
    assert_eq!(json["output"], "reviewed");
    assert!(json["cockpit"].is_object());
    assert_eq!(reload_calls.load(Ordering::SeqCst), 1);

    let replay = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"req-cas-recover","task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay_json = json_of(replay).await;
    assert_eq!(replay_json["ok"], true);
    assert_eq!(replay_json["request_id"], "req-cas-recover");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_optimistic_keeps_conflict_when_cas_loss_is_not_durable() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let bridge = TestBridge {
        operate_entered: Some(Arc::clone(&entered)),
        operate_release: Some(Arc::clone(&release)),
        acknowledge_result: Ok(true),
        operate_result: Ok(OperateOutcome {
            state_changed: false,
            output: String::new(),
        }),
        ..TestBridge::default()
    };
    let reload_calls = Arc::clone(&bridge.reload_calls);
    let (state, cookie, app) =
        app_with(context_with_task(), bridge, "run-optimistic-cas-ephemeral");

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"req-cas-ephemeral","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("operation never entered the bridge");

    let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
    sink();

    release_gate(&release);

    let response = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .expect("operation timed out")
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "req-cas-ephemeral");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("cockpit state changed"));
    assert_eq!(reload_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_optimistic_installs_operate_clone_when_durable_cas_loss_has_no_disk_reload() {
    use ajax_core::models::TaskId;
    use ajax_core::registry::Registry;

    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let bridge = TestBridge {
        operate_entered: Some(Arc::clone(&entered)),
        operate_release: Some(Arc::clone(&release)),
        acknowledge_result: Ok(true),
        clear_registry_on_operate: true,
        operate_result: Ok(OperateOutcome {
            state_changed: true,
            output: "cleared".to_string(),
        }),
        ..TestBridge::default()
    };
    let reload_calls = Arc::clone(&bridge.reload_calls);
    let (state, cookie, app) = app_with(context_with_task(), bridge, "run-optimistic-cas-no-disk");

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"req-cas-no-disk","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("operation never entered the bridge");

    let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
    sink();

    release_gate(&release);

    let response = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .expect("operation timed out")
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_of(response).await;
    assert_eq!(json["ok"], true);
    assert_eq!(json["request_id"], "req-cas-no-disk");
    assert_eq!(json["output"], "cleared");
    assert_eq!(reload_calls.load(Ordering::SeqCst), 1);
    assert!(
        state
            .shared()
            .context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none(),
        "shared state should reflect the operate clone with cleared registry"
    );
}
