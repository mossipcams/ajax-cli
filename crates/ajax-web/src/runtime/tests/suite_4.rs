use super::*;

#[tokio::test]
async fn action_endpoint_executes_bridge_action_and_returns_cockpit() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "action");

    let response = post_json(
        &app,
        &cookie,
        "/api/actions",
        r#"{"task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["state_changed"], true);
    assert!(body["cockpit"].is_object());
    assert_eq!(
        state.shared().bridge.operate,
        Some(OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "review".to_string(),
            confirmed: false,
            branch_adoption: None,
        })
    );
}

#[tokio::test]
async fn get_task_detail_returns_json_for_existing_handle() {
    let (_state, cookie, app) = app_with(context_with_task(), TestBridge::default(), "detail");

    let response = get(&app, &cookie, "/api/tasks/web/fix-login").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let body = json_of(response).await;
    assert_eq!(body["qualified_handle"], "web/fix-login");
    assert_eq!(body["title"], "Fix login");
    assert_eq!(body["branch"], "ajax/fix-login");
}

#[tokio::test]
async fn get_task_detail_allows_encoded_handle_named_terminal() {
    let task = crate::test_support::task_in("ajax-cli", "terminal", "Terminal");
    let context = crate::test_support::context_with_tasks(&["ajax-cli"], vec![task]);
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "detail-terminal-handle");

    let response = get(&app, &cookie, "/api/tasks/ajax-cli%2Fterminal").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let body = json_of(response).await;
    assert_eq!(body["qualified_handle"], "ajax-cli/terminal");
    assert_eq!(body["title"], "Terminal");
    assert_eq!(body["branch"], "ajax/terminal");
}

#[tokio::test]
async fn axum_task_terminal_requires_browser_session_cookie() {
    let state = super::WebAppState::new(
        context_with_task(),
        OkRunner,
        TestBridge::default(),
        scratch_dir("terminal-auth"),
    );
    let app = super::axum_app(state);

    let response = get_public(&app, "/api/tasks/web%2Ffix-login/terminal").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_task_terminal_rejects_non_upgrade_requests() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "terminal-upgrade",
    );

    let response = get(&app, &cookie, "/api/tasks/web%2Ffix-login/terminal").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "websocket upgrade required"
    );
}

#[tokio::test]
async fn axum_task_terminal_rejects_cross_site_websocket_origin() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "terminal-cross-origin",
    );

    let response = websocket_get(
        &app,
        &cookie,
        "/api/tasks/web%2Ffix-login/terminal",
        Some("https://evil.example"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "websocket origin forbidden"
    );
}

#[tokio::test]
async fn axum_task_terminal_marks_browser_connected_after_origin_ok() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "terminal-same-origin-browser-connected",
    );

    assert!(!state.browser_connected());

    let response = websocket_get(
        &app,
        &cookie,
        "/api/tasks/web%2Ffix-login/terminal",
        Some("https://localhost"),
    )
    .await;

    // The same-origin request passed the websocket origin gate; the exact
    // upgrade outcome (101/400) is irrelevant once the handler ran past it.
    assert_ne!(response.status(), StatusCode::FORBIDDEN);

    assert!(state.browser_connected());
}

#[tokio::test]
async fn axum_task_stt_requires_browser_session_cookie() {
    let state = super::WebAppState::new(
        context_with_task(),
        OkRunner,
        TestBridge::default(),
        scratch_dir("stt-auth"),
    );
    let app = super::axum_app(state);

    let response = get_public(&app, "/api/tasks/web%2Ffix-login/stt").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_task_stt_rejects_non_upgrade_requests() {
    let (_state, cookie, app) = app_with(context_with_task(), TestBridge::default(), "stt-upgrade");

    let response = get(&app, &cookie, "/api/tasks/web%2Ffix-login/stt").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "websocket upgrade required"
    );
}

#[tokio::test]
async fn axum_task_stt_rejects_cross_site_websocket_origin() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "stt-cross-origin",
    );

    let response = websocket_get(
        &app,
        &cookie,
        "/api/tasks/web%2Ffix-login/stt",
        Some("https://evil.example"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "websocket origin forbidden"
    );
}

#[test]
fn websocket_origin_policy_accepts_same_origin_host() {
    let request = AxumRequest::builder()
        .header("host", "localhost")
        .header("origin", "https://localhost")
        .body(Body::empty())
        .unwrap();

    assert!(super::websocket_origin_allowed(request.headers()));
}

#[tokio::test]
async fn axum_task_keys_route_is_not_supported() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "terminal-keys-removed",
    );
    let response = post_json(&app, &cookie, "/api/tasks/web%2Ffix-login/keys", "{}").await;

    assert_json_not_found(response, "not found").await;
}

#[tokio::test]
async fn axum_task_snapshot_route_is_not_supported() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "terminal-snapshot-removed",
    );
    let response = get(&app, &cookie, "/api/tasks/web%2Ffix-login/snapshot").await;

    assert_json_not_found(response, "not found").await;
}

#[tokio::test]
async fn get_task_detail_returns_text_404_for_unknown_handle() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "detail-missing");

    let response = get(&app, &cookie, "/api/tasks/web/missing").await;

    assert_json_not_found(response, "task not found").await;
}

#[tokio::test]
async fn unknown_in_memory_api_path_stays_generic_404() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "missing-api");

    let response = get(&app, &cookie, "/api/missing").await;

    assert_json_not_found(response, "not found").await;
}

#[test]
fn operation_helpers_accept_typed_requests_without_json_roundtrip() {
    let production_source = include_str!("../mod.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default();

    assert!(
        !production_source.contains("serde_json::to_string(&request).unwrap_or_default()"),
        "operation routes should not serialize typed requests back to JSON for internal helpers"
    );
    assert!(
        !production_source.contains("fn handle_action_request<C: CommandRunner>(\n    body: &str,"),
        "handle_action_request should accept MobileActionRequest directly"
    );
    assert!(
        !production_source
            .contains("fn handle_start_task_request<C: CommandRunner>(\n    body: &str,"),
        "handle_start_task_request should accept StartTaskRequest directly"
    );
    assert!(
        !production_source
            .contains("let request: MobileActionRequest = serde_json::from_str(body)"),
        "action helper should not reparse MobileActionRequest from JSON"
    );
    assert!(
        !production_source.contains(
            "let request: crate::slices::operate::StartTaskRequest = serde_json::from_str(body)"
        ),
        "start helper should not reparse StartTaskRequest from JSON"
    );
}

#[tokio::test]
async fn post_tasks_endpoint_delegates_to_start_bridge_method() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "start");

    let response = post_json(
        &app,
        &cookie,
        "/api/tasks",
        r#"{"repo":"web","title":"Fix login","agent":"codex","request_id":"req-1"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    assert_eq!(body["ok"], true);
    assert!(body["cockpit"].is_object());
    assert_eq!(
        state.shared().bridge.start,
        Some(crate::slices::operate::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            request_id: "req-1".to_string(),
        })
    );
}

#[tokio::test]
async fn action_endpoint_keeps_start_out_of_bridge() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "native-action");

    let response = post_json(
        &app,
        &cookie,
        "/api/actions",
        r#"{"task_handle":"web/fix-login","action":"start"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(state.shared().bridge.operate, None);
}

#[tokio::test]
async fn push_path_does_not_start_nested_runtime() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "push-runtime");

    let config = get(&app, &cookie, "/api/push/config").await;
    assert_eq!(config.status(), StatusCode::NOT_FOUND);

    let subscribe = post_json(
        &app,
        &cookie,
        "/api/push/subscribe",
        r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
    )
    .await;
    assert_eq!(subscribe.status(), StatusCode::NOT_FOUND);

    let unsubscribe = post_json(
        &app,
        &cookie,
        "/api/push/unsubscribe",
        r#"{"endpoint":"https://push.example/x"}"#,
    )
    .await;
    assert_eq!(unsubscribe.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_health_remains_responsive_after_terminal_disconnect_cleanup() {
    let state = super::WebAppState::new(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        OkRunner,
        TestBridge::default(),
        scratch_dir("terminal-cleanup-health"),
    );
    let app = super::axum_app(state);

    let cleanup = tokio::spawn(async move {
        crate::adapters::terminal_pty::simulate_terminal_disconnect_cleanup_for_tests(
            Duration::from_millis(50),
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let health_started = std::time::Instant::now();
    let health = app
        .oneshot(
            AxumRequest::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("health request should complete");
    let health_elapsed = health_started.elapsed();

    assert_eq!(health.status(), StatusCode::OK);
    assert!(
        health_elapsed < Duration::from_millis(150),
        "health took {health_elapsed:?} while terminal cleanup was in flight"
    );

    cleanup.await.expect("terminal cleanup should finish");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_cockpit_returns_current_projection_while_control_lane_is_busy() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            refresh_entered: Some(Arc::clone(&entered)),
            refresh_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-cockpit-busy-lane",
    );

    let refresh_app = app.clone();
    let refresh_cookie = cookie.clone();
    let first_cockpit =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("cockpit refresh never entered the bridge");

    let concurrent_app = app.clone();
    let concurrent_cookie = cookie.clone();
    let second_cockpit =
        tokio::spawn(async move { get(&concurrent_app, &concurrent_cookie, "/api/cockpit").await });

    let second_result = tokio::time::timeout(Duration::from_millis(150), second_cockpit).await;

    release_gate(&release);

    let second = second_result
        .expect("second cockpit GET should complete promptly while refresh holds the lane")
        .unwrap();

    assert_eq!(second.status(), StatusCode::OK);
    let body = json_of(second).await;
    assert!(
        body["cards"]
            .as_array()
            .expect("cockpit body should include cards")
            .iter()
            .any(|card| card["qualified_handle"] == "web/fix-login"),
        "fallback cockpit read should include the existing web/fix-login card"
    );
    assert_eq!(
        state.shared().bridge.refresh_calls.load(Ordering::SeqCst),
        1,
        "busy-lane cockpit read must not start another refresh"
    );

    let first = tokio::time::timeout(Duration::from_secs(5), first_cockpit)
        .await
        .expect("first cockpit refresh response timed out")
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

    assert_eq!(
        get(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        state.shared().bridge.refresh_count,
        2,
        "later polls should still use the normal refresh path after TTL expires"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            refresh_entered: Some(Arc::clone(&entered)),
            refresh_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-op-waits-refresh",
    );

    let refresh_app = app.clone();
    let refresh_cookie = cookie.clone();
    let cockpit =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("cockpit refresh never entered the bridge");

    let mutate_app = app.clone();
    let mutate_cookie = cookie.clone();
    let mutation = tokio::spawn(async move {
        post_json(
            &mutate_app,
            &mutate_cookie,
            "/api/operations",
            r#"{"request_id":"op-wait-1","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    let operate_calls_during_refresh = state.shared().bridge.operate_calls.load(Ordering::SeqCst);

    release_gate(&release);

    let cockpit = tokio::time::timeout(Duration::from_secs(5), cockpit)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();
    let mutation = tokio::time::timeout(Duration::from_secs(5), mutation)
        .await
        .expect("operation response timed out")
        .unwrap();

    assert_eq!(cockpit.status(), StatusCode::OK);
    assert_eq!(mutation.status(), StatusCode::OK);
    assert_eq!(
        operate_calls_during_refresh, 0,
        "operation must wait for the in-flight cockpit refresh (control lane)"
    );
    assert_eq!(
        state.shared().bridge.refresh_count,
        1,
        "refresh state must be committed, not discarded by the racing mutation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_web_repo(),
        TestBridge {
            refresh_entered: Some(Arc::clone(&entered)),
            refresh_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-start-waits-refresh",
    );

    let refresh_app = app.clone();
    let refresh_cookie = cookie.clone();
    let cockpit =
        tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("cockpit refresh never entered the bridge");

    let mutate_app = app.clone();
    let mutate_cookie = cookie.clone();
    let mutation = tokio::spawn(async move {
        post_json(
            &mutate_app,
            &mutate_cookie,
            "/api/tasks",
            r#"{"request_id":"start-wait-1","repo":"web","title":"Fix login","agent":"codex"}"#,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    let start_calls_during_refresh = state.shared().bridge.start_calls.load(Ordering::SeqCst);

    release_gate(&release);

    let cockpit = tokio::time::timeout(Duration::from_secs(5), cockpit)
        .await
        .expect("cockpit refresh response timed out")
        .unwrap();
    let mutation = tokio::time::timeout(Duration::from_secs(5), mutation)
        .await
        .expect("start task response timed out")
        .unwrap();

    assert_eq!(cockpit.status(), StatusCode::OK);
    assert_eq!(mutation.status(), StatusCode::OK);
    assert_eq!(
        start_calls_during_refresh, 0,
        "task start must wait for the in-flight cockpit refresh (control lane)"
    );
    assert_eq!(
        state.shared().bridge.refresh_count,
        1,
        "refresh state must be committed, not discarded by the racing mutation"
    );
}
