use super::*;

#[test]
fn operation_coordinator_prunes_completed_request_ids() {
    let mut coordinator = super::OperationCoordinator::default();

    for index in 0..=128 {
        let request_id = format!("req-{index}");
        coordinator.store_completed_response(
            request_id.clone(),
            super::Response {
                status_code: 200,
                content_type: "application/json; charset=utf-8",
                body: serde_json::to_vec(&serde_json::json!({
                    "ok": true,
                    "request_id": request_id,
                }))
                .unwrap(),
            },
        );
    }

    assert!(coordinator.completed_response("req-0").is_none());
    assert!(coordinator.completed_response("req-128").is_some());
    coordinator
        .in_flight_requests
        .insert("req-live".to_string());
    assert!(coordinator.has_in_flight_mutation());
}

#[test]
fn committed_operation_fixture_matches_production_response_builder() {
    let context = crate::slices::cockpit::tests::browser_contract_context();
    let response = super::operation_success_response(
        OperateOutcome {
            state_changed: true,
            output: "Operation completed successfully.".to_string(),
        },
        &context,
    )
    .unwrap();
    let actual: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let committed: serde_json::Value =
        serde_json::from_str(include_str!("../../../web/src/fixtures/operation.json")).unwrap();

    assert_eq!(committed, actual);
}

#[tokio::test]
async fn start_task_endpoint_returns_refreshed_cockpit_on_bridge_error() {
    let (_state, cookie, app) = app_with(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        TestBridge {
            start_result: Err(ActionFailure {
                message: "start failed".to_string(),
                code: "command_failed".to_string(),
                state_changed: true,
            }),
            ..TestBridge::default()
        },
        "axum-start-error",
    );

    let response = post_json(
        &app,
        &cookie,
        "/api/tasks",
        r#"{"request_id":"start-error-1","repo":"web","title":"Fix login","agent":"codex"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "start-error-1");
    assert_eq!(json["state_changed"], true);
    assert_eq!(json["error"], "start failed");
    assert_eq!(json["code"], "command_failed");
    assert!(json["cockpit"].is_object());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_blocks_conflicting_task_operations() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let bridge = TestBridge {
        operate_delay: Duration::from_millis(150),
        ..TestBridge::default()
    };
    let (state, cookie, app) = app_with(context, bridge, "axum-conflict");

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(25)).await;

    let conflict = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"req-b","task_handle":"web/fix-login","action":"ship"}"#,
    )
    .await;

    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let json = json_of(conflict).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "req-b");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("already in progress"));

    assert_eq!(first.await.unwrap().status(), StatusCode::OK);
    assert_eq!(
        state.shared().bridge.operate_calls.load(Ordering::SeqCst),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_rejects_concurrent_different_task_operations_before_bridge_side_effects() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_two_tasks(),
        TestBridge {
            operate_entered: Some(Arc::clone(&entered)),
            operate_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-concurrent-different-tasks",
    );

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("first request never entered the bridge");

    let conflict = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"req-b","task_handle":"api/fix-auth","action":"ship"}"#,
    )
    .await;

    release_gate(&release);

    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let json = json_of(conflict).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "req-b");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("already in progress"));

    assert_eq!(first.await.unwrap().status(), StatusCode::OK);
    assert_eq!(state.shared().bridge.operate_count, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_health_stays_responsive_during_slow_cockpit_refresh() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            refresh_delay: Duration::from_millis(400),
            ..TestBridge::default()
        },
        "axum-health-cockpit",
    );

    let slow_app = app.clone();
    let slow_cookie = cookie.clone();
    let cockpit = tokio::spawn(async move { get(&slow_app, &slow_cookie, "/api/cockpit").await });

    let health_started = std::time::Instant::now();
    let health = app
        .oneshot(
            AxumRequest::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let health_elapsed = health_started.elapsed();

    assert_eq!(health.status(), StatusCode::OK);
    assert!(
        health_elapsed < Duration::from_millis(150),
        "health took {health_elapsed:?} while cockpit refresh was in flight"
    );
    assert_eq!(cockpit.await.unwrap().status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_task_start_does_not_block_health() {
    let state = super::WebAppState::new(
        context_with_web_repo(),
        OkRunner,
        TestBridge {
            start_delay: Duration::from_millis(400),
            ..TestBridge::default()
        },
        scratch_dir("axum-start-health"),
    );
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);
    let health_started = Instant::now();

    let (start, (health, health_elapsed)) = tokio::join!(
        biased;
        post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-health-1","repo":"web","title":"Fix login","agent":"codex"}"#,
        ),
        async {
            let response = get_public(&app, "/api/health").await;
            (response, health_started.elapsed())
        },
    );

    assert_eq!(start.status(), StatusCode::OK);
    assert_eq!(health.status(), StatusCode::OK);
    assert!(
        health_elapsed < Duration::from_millis(150),
        "health took {health_elapsed:?} while task start was in flight"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_diff_review_does_not_block_health() {
    let state = super::WebAppState::new(
        context_with_task(),
        SlowDiffRunner {
            delay: Duration::from_millis(400),
            entered: None,
        },
        TestBridge::default(),
        scratch_dir("axum-diff-health"),
    );
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);
    let health_started = Instant::now();

    let (diff, (health, health_elapsed)) = tokio::join!(
        biased;
        get(
            &app,
            &cookie,
            "/api/tasks/web/fix-login/pull-requests",
        ),
        async {
            let response = get_public(&app, "/api/health").await;
            (response, health_started.elapsed())
        },
    );

    assert_eq!(diff.status(), StatusCode::OK);
    assert_eq!(health.status(), StatusCode::OK);
    assert!(
        health_elapsed < Duration::from_millis(150),
        "health took {health_elapsed:?} while Diff Review was in flight"
    );
}

/// Regression guard for #898.
///
/// Every other blocking surface in this crate was moved off the async workers
/// and locked down by a health-isolation test (`axum_task_start_does_not_block_health`,
/// `axum_diff_review_does_not_block_health`,
/// `axum_health_stays_responsive_during_slow_cockpit_refresh`). The ACP session
/// socket was never in that sweep: `bridge_task_session_socket` calls
/// `hub.acquire` — an ACP handshake bounded only by `HANDSHAKE_TIMEOUT` (45s) —
/// inline on the worker that upgraded the socket. A harness that never answers
/// `initialize` therefore parks one runtime thread per open session and the
/// whole cockpit stops serving, health included.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_session_socket_does_not_block_health() {
    // A real process that holds its stdio open and never answers `initialize`,
    // the way a wedged harness does. It exits on its own so the runtime is not
    // parked for the full 45s handshake timeout after the assertion. (`cat`
    // cannot play this part: it echoes the handshake back, which fails the
    // request fast instead of hanging it.)
    let hanging_acp = std::path::PathBuf::from("/bin/sleep");
    assert!(
        hanging_acp.exists(),
        "test needs a program that never answers on stdout"
    );

    let worktrees = scratch_dir("axum-session-health-worktrees");
    let context = context_with_provisioned_cursor_tasks(&worktrees);
    let state = super::WebAppState::new(
        context,
        OkRunner,
        TestBridge::default(),
        scratch_dir("axum-session-health"),
    );
    let cookie = browser_session_cookie(&state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, super::axum_app(state)).await.unwrap();
    });

    crate::adapters::web_session_acp::set_test_acp_command(Some((&hanging_acp, &["2"])));

    // One session socket per runtime worker. Client I/O runs on plain threads,
    // not the runtime, so the probe still works once every worker is parked.
    let sockets: Vec<_> = ["web/fix-login", "api/fix-auth"]
        .into_iter()
        .map(|handle| {
            let cookie = cookie.clone();
            std::thread::spawn(move || {
                websocket_upgrade_blocking(
                    address,
                    &cookie,
                    &format!("/api/tasks/{handle}/session"),
                )
            })
        })
        .map(|joined| joined.join().unwrap())
        .collect();

    // A 101 only proves the upgrade was written; let both socket tasks reach
    // `hub.acquire` before timing health.
    std::thread::sleep(Duration::from_millis(100));

    let (health, health_elapsed) = std::thread::spawn(move || {
        let started = Instant::now();
        let response = http_get_blocking(address, "/api/health", Duration::from_millis(500));
        (response, started.elapsed())
    })
    .join()
    .unwrap();

    // Clear the global override before asserting: a panic here must not leave
    // every later ACP spawn in this process pointed at the hanging program.
    crate::adapters::web_session_acp::set_test_acp_command(None);
    server.abort();
    drop(sockets);
    let _ = std::fs::remove_dir_all(&worktrees);

    let response = health.expect("health request stalled while session sockets were connecting");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        health_elapsed < Duration::from_millis(150),
        "health took {health_elapsed:?} while session sockets were mid-ACP-handshake"
    );
}

/// Two provisioned Cursor tasks with worktrees on disk, so `prepare_task_session`
/// admits both session sockets.
fn context_with_provisioned_cursor_tasks(
    worktrees: &std::path::Path,
) -> CommandContext<InMemoryRegistry> {
    let tasks = [
        ("web", "fix-login", "Fix login"),
        ("api", "fix-auth", "Fix auth"),
    ]
    .into_iter()
    .map(|(repo, handle, title)| {
        let worktree = worktrees.join(format!("{repo}-{handle}"));
        std::fs::create_dir_all(&worktree).expect("worktree dir");
        let mut task = crate::test_support::task_in(repo, handle, title);
        task.selected_agent = ajax_core::models::AgentClient::Cursor;
        task.set_skip_interactive_agent(true);
        task.worktree_path = worktree;
        task
    })
    .collect();
    crate::test_support::context_with_tasks(&["web", "api"], tasks)
}

/// Raw WebSocket upgrade on a blocking socket. Returns the stream so the caller
/// keeps the connection (and the server-side socket task) alive.
fn websocket_upgrade_blocking(
    address: std::net::SocketAddr,
    cookie: &str,
    path: &str,
) -> std::net::TcpStream {
    let mut stream = std::net::TcpStream::connect(address).expect("connect");
    stream
        .write_all(
            format!(
                "GET {path} HTTP/1.1\r\n\
                 Host: {address}\r\n\
                 Origin: http://{address}\r\n\
                 Cookie: {cookie}\r\n\
                 Connection: Upgrade\r\n\
                 Upgrade: websocket\r\n\
                 Sec-WebSocket-Version: 13\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
            )
            .as_bytes(),
        )
        .expect("write upgrade");
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => head.push(byte[0]),
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 101"),
        "session upgrade refused: {head}"
    );
    stream
}

/// Plain-HTTP GET on a blocking socket, bounded by a read timeout so a stalled
/// runtime fails the test instead of hanging it.
fn http_get_blocking(
    address: std::net::SocketAddr,
    path: &str,
    timeout: Duration,
) -> std::io::Result<String> {
    let mut stream = std::net::TcpStream::connect(address)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes(),
    )?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_diff_review_returns_ok_when_revision_bumps_during_projection() {
    let entered = Arc::new(Notify::new());
    let state = super::WebAppState::new(
        context_with_task(),
        SlowDiffRunner {
            delay: Duration::from_millis(400),
            entered: Some(Arc::clone(&entered)),
        },
        TestBridge::default(),
        scratch_dir("axum-diff-revision-race"),
    );
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state.clone());

    let diff_app = app.clone();
    let diff_cookie = cookie.clone();
    let diff = tokio::spawn(async move {
        get(
            &diff_app,
            &diff_cookie,
            "/api/tasks/web/fix-login/pull-requests",
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("diff projection never entered the runner");

    let operation = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"req-diff-race","task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;
    assert_eq!(operation.status(), StatusCode::OK);

    let diff = diff.await.unwrap();
    assert_eq!(diff.status(), StatusCode::OK);
    let json = json_of(diff).await;
    assert!(json["pull_requests"].is_array());
}

#[tokio::test]
async fn axum_diff_review_runner_panic_returns_internal_server_error() {
    let state = super::WebAppState::new(
        context_with_task(),
        PanickingDiffRunner,
        TestBridge::default(),
        scratch_dir("axum-diff-panic"),
    );
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state);

    let response = get(&app, &cookie, "/api/tasks/web/fix-login/pull-requests").await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("diff review worker failed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn axum_cockpit_refresh_does_not_overwrite_concurrent_operation_state() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            refresh_delay: Duration::from_millis(250),
            ..TestBridge::default()
        },
        "axum-refresh-operation-race",
    );

    let refresh_app = app.clone();
    let refresh_cookie = cookie.clone();
    let cockpit =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });
    tokio::time::sleep(Duration::from_millis(25)).await;

    let operation = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"req-race","task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;

    assert_eq!(operation.status(), StatusCode::OK);
    assert_eq!(cockpit.await.unwrap().status(), StatusCode::OK);

    let guard = state.shared();
    assert_eq!(guard.bridge.operate_count, 1);
    assert_eq!(
        guard
            .bridge
            .operate
            .as_ref()
            .map(|request| request.action.as_str()),
        Some("review")
    );
}

#[tokio::test]
async fn runtime_routes_to_vertical_slices() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "routes");

    let shell = get_public(&app, "/").await;
    assert_eq!(shell.status(), StatusCode::OK);
    assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
    let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
    assert!(std::str::from_utf8(&shell_body)
        .unwrap()
        .contains("Ajax Cockpit"));

    let cockpit = get(&app, &cookie, "/api/cockpit").await;
    assert_eq!(cockpit.status(), StatusCode::OK);
    assert_eq!(
        cockpit.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    assert_eq!(json_of(cockpit).await["cards"], serde_json::json!([]));
}

#[tokio::test]
async fn cockpit_api_refreshes_before_rendering() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "refresh");

    let response = get(&app, &cookie, "/api/cockpit").await;

    assert_eq!(response.status(), StatusCode::OK);
    let bridge = &state.shared().bridge;
    assert!(bridge.refreshed);
    assert_eq!(bridge.refresh_tier, Some(RefreshTier::Live));
}

#[tokio::test]
async fn server_restart_endpoint_returns_restarting_json() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "restart");

    let response = post_json(&app, &cookie, "/api/server/restart", "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["restarting"], true);
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: ajax-web runtime tests are not run in parallel with other
        // env-mutating tests in this module.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[tokio::test]
async fn test_in_stable_endpoint_returns_not_found_when_disabled() {
    let _profile = EnvVarGuard::set("AJAX_WEB_RESTART_PROFILE", "prod");
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "test-in-stable-disabled");

    let response = post_json(&app, &cookie, "/api/server/test-in-stable", "").await;

    assert_json_not_found(response, "test in stable is not available").await;
}

#[tokio::test]
async fn test_in_stable_endpoint_returns_restarting_when_enabled() {
    let root = std::env::temp_dir().join(format!(
        "ajax-test-in-stable-endpoint-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).expect("create scripts dir");
    let restart = scripts.join("dev-web-restart.sh");
    std::fs::write(&restart, "#!/bin/sh\n").expect("write restart script");
    std::fs::write(scripts.join("test-in-stable.sh"), "#!/bin/sh\n").expect("write wrapper");
    let _script = EnvVarGuard::set(
        "AJAX_WEB_RESTART_SCRIPT",
        restart.to_str().expect("restart path"),
    );
    let _profile = EnvVarGuard::set("AJAX_WEB_RESTART_PROFILE", "stable");
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "test-in-stable-enabled");

    let response = post_json(&app, &cookie, "/api/server/test-in-stable", "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["restarting"], true);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn test_in_stable_endpoint_returns_spawn_only_when_dev_profile() {
    let root = std::env::temp_dir().join(format!(
        "ajax-test-in-stable-endpoint-dev-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let scripts = root.join("scripts");
    std::fs::create_dir_all(&scripts).expect("create scripts dir");
    let restart = scripts.join("dev-web-restart.sh");
    std::fs::write(&restart, "#!/bin/sh\n").expect("write restart script");
    std::fs::write(scripts.join("test-in-stable.sh"), "#!/bin/sh\n").expect("write wrapper");
    let _script = EnvVarGuard::set(
        "AJAX_WEB_RESTART_SCRIPT",
        restart.to_str().expect("restart path"),
    );
    let _profile = EnvVarGuard::set("AJAX_WEB_RESTART_PROFILE", "dev");
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "test-in-stable-dev");

    let response = post_json(&app, &cookie, "/api/server/test-in-stable", "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["restarting"], false);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn dev_deploy_status_and_reject_non_ajax_paths() {
    use ajax_core::{
        config::ManagedRepo,
        models::{AgentClient, Task, TaskId},
        registry::Registry as _,
    };

    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(Task::new(
            TaskId::new("autosnooze/other"),
            "autosnooze",
            "other",
            "Other",
            "feat/other",
            "main",
            "/tmp/other",
            "ajax-autosnooze-other",
            "task",
            AgentClient::Codex,
        ))
        .unwrap();
    let context = CommandContext::new(
        Config {
            repos: vec![
                ManagedRepo::new("ajax-cli", "/Users/matt/Desktop/Projects/ajax-cli", "main"),
                ManagedRepo::new("autosnooze", "/tmp/autosnooze", "main"),
            ],
            ..Config::default()
        },
        registry,
    );
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "dev-deploy");

    let status = get(&app, &cookie, "/api/dev-deploy").await;
    assert_eq!(status.status(), StatusCode::OK);
    let status_body = json_of(status).await;
    assert_eq!(status_body["ok"], true);
    assert_eq!(status_body["deploy"]["shared_slot"], true);
    assert!(status_body["deploy"].get("open_url").is_none());
    assert_eq!(status_body["deploy"]["phase"], "ready_to_deploy");

    let rejected = post_json(
        &app,
        &cookie,
        "/api/dev-deploy",
        r#"{"task_handle":"autosnooze/other"}"#,
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    let rejected_body = json_of(rejected).await;
    assert_eq!(rejected_body["ok"], false);
    assert!(rejected_body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("ajax-cli"));
}
