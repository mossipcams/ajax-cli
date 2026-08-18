use super::*;

#[tokio::test]
async fn axum_cockpit_serves_cached_projection_within_refresh_ttl() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "axum-cockpit-cache",
    );

    for _ in 0..2 {
        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );
    }

    assert_eq!(state.shared().bridge.refresh_count, 1);
}

#[test]
fn browser_connected_is_false_until_marked_and_expires_after_ttl() {
    let state = super::WebAppState::new(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        OkRunner,
        TestBridge::default(),
        scratch_dir("browser-connected-ttl"),
    );

    assert!(!state.browser_connected());
    state.mark_browser_cockpit_seen();
    assert!(state.browser_connected());

    let aged = Instant::now() - super::BROWSER_CONNECTED_TTL - Duration::from_secs(1);
    state.set_browser_cockpit_seen_at_for_test(aged);
    assert!(!state.browser_connected());
}

#[tokio::test]
async fn axum_cockpit_marks_browser_connected_only_with_foreground_header() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "axum-cockpit-browser-connected",
    );

    for _ in 0..2 {
        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );
    }
    assert!(
        !state.browser_connected(),
        "background/data polls must not suppress push"
    );

    assert_eq!(
        get_foreground(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
    assert!(state.browser_connected());
}

#[tokio::test]
async fn axum_cockpit_foreground_header_marks_even_on_cache_hit() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "axum-cockpit-foreground-cache-hit",
    );

    assert_eq!(
        get_foreground(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
    state.set_browser_cockpit_seen_at_for_test(
        Instant::now() - super::BROWSER_CONNECTED_TTL - Duration::from_secs(1),
    );
    assert!(!state.browser_connected());

    assert_eq!(
        get_foreground(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );
    assert!(state.browser_connected());
}

#[tokio::test]
async fn axum_operations_marks_browser_connected() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(
        context,
        TestBridge::default(),
        "axum-operations-browser-connected",
    );

    assert!(!state.browser_connected());

    let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
    let response = post_json(&app, &cookie, "/api/operations", operation).await;
    assert_eq!(response.status(), StatusCode::OK);

    assert!(state.browser_connected());
}

#[tokio::test]
async fn refresh_cockpit_and_cache_passes_deliver_notifications_flag() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "deliver-notifications-flag",
    );

    super::refresh_cockpit_and_cache(&state, RefreshTier::Live, false).await;
    assert_eq!(
        state.shared().bridge.deliver_notifications_flags,
        vec![false]
    );
    assert_eq!(state.shared().bridge.refresh_tier, Some(RefreshTier::Live));

    tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

    super::refresh_cockpit_and_cache(&state, RefreshTier::Full, true).await;
    assert_eq!(
        state.shared().bridge.deliver_notifications_flags,
        vec![false, true]
    );
    assert_eq!(state.shared().bridge.refresh_tier, Some(RefreshTier::Full));
}

#[tokio::test]
async fn notify_refresh_path_uses_full_tier_and_suppresses_delivery_while_connected() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "notify-full-connected",
    );

    state.mark_browser_cockpit_seen();
    assert!(state.browser_connected());

    super::refresh_cockpit_and_cache(&state, RefreshTier::Full, !state.browser_connected()).await;

    let bridge = &state.shared().bridge;
    assert_eq!(bridge.refresh_tier, Some(RefreshTier::Full));
    assert_eq!(bridge.deliver_notifications_flags, vec![false]);
}

#[tokio::test]
async fn notify_refresh_path_delivers_when_browser_disconnected() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "notify-full-disconnected",
    );

    assert!(!state.browser_connected());

    super::refresh_cockpit_and_cache(&state, RefreshTier::Full, !state.browser_connected()).await;

    let bridge = &state.shared().bridge;
    assert_eq!(bridge.refresh_tier, Some(RefreshTier::Full));
    assert_eq!(bridge.deliver_notifications_flags, vec![true]);
}

fn sample_push_subscription() -> crate::slices::push::PushSubscription {
    use crate::slices::push::{PushSubscription, PushSubscriptionKeys};
    const P256DH: &str =
        "BLn9b-VR0ca83knDNZ32dCHGyjJp-1riX9ZTN40MqV8K_LpQmLqxC_DoHvqvFXO_nGdAB4W9dogZb_sM-uV4JbY";
    const AUTH: &str = "_ordMnz7uTCmrpBTeUV4Bw";
    PushSubscription {
        endpoint: "https://web.push.apple.com/messages/1".to_string(),
        keys: PushSubscriptionKeys {
            p256dh: P256DH.to_string(),
            auth: AUTH.to_string(),
        },
        navigate: None,
    }
}

#[tokio::test]
async fn push_tick_logic_skips_refresh_while_browser_connected() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "push-tick-skip-connected",
    );
    state
        .push
        .upsert_subscription(sample_push_subscription(), "https://cockpit.example/")
        .expect("subscription");
    state.mark_browser_cockpit_seen();
    assert!(state.browser_connected());

    if !state.browser_connected() && state.push.has_subscriptions() {
        super::refresh_cockpit_and_cache(&state, RefreshTier::Full, true).await;
    }
    assert_eq!(state.shared().bridge.refresh_count, 0);
}

#[tokio::test]
async fn push_tick_logic_runs_full_refresh_when_disconnected_with_subscriptions() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "push-tick-run-disconnected",
    );
    state
        .push
        .upsert_subscription(sample_push_subscription(), "https://cockpit.example/")
        .expect("subscription");
    assert!(!state.browser_connected());

    if !state.browser_connected() && state.push.has_subscriptions() {
        super::refresh_cockpit_and_cache(&state, RefreshTier::Full, true).await;
    }
    assert_eq!(state.shared().bridge.refresh_count, 1);
    assert_eq!(state.shared().bridge.refresh_tier, Some(RefreshTier::Full));
    assert_eq!(
        state.shared().bridge.deliver_notifications_flags,
        vec![true]
    );
}

#[tokio::test]
async fn refresh_cockpit_and_cache_refreshes_once_and_caches() {
    let (state, _cookie, _app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "tick-refresh-cache",
    );

    super::refresh_cockpit_and_cache(&state, RefreshTier::Full, true).await;
    assert_eq!(state.shared().bridge.refresh_count, 1);

    // Within the cache TTL the tick shares the handler's cached response.
    super::refresh_cockpit_and_cache(&state, RefreshTier::Full, true).await;
    assert_eq!(state.shared().bridge.refresh_count, 1);
}

#[tokio::test]
async fn axum_cockpit_refreshes_again_after_ttl_expires() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "axum-cockpit-ttl",
    );

    assert_eq!(
        get(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );

    tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

    assert_eq!(
        get(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );

    assert_eq!(state.shared().bridge.refresh_count, 2);
}

#[tokio::test]
async fn axum_operation_invalidates_cockpit_refresh_cache() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge::default(),
        "axum-cockpit-invalidate",
    );

    assert_eq!(
        get(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );

    let operation = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"invalidate-1","task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;
    assert_eq!(operation.status(), StatusCode::OK);

    assert_eq!(
        get(&app, &cookie, "/api/cockpit").await.status(),
        StatusCode::OK
    );

    assert_eq!(state.shared().bridge.refresh_count, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_cockpit_polls_share_one_refresh() {
    let (state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            refresh_delay: Duration::from_millis(200),
            ..TestBridge::default()
        },
        "axum-cockpit-single-flight",
    );

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move { get(&first_app, &first_cookie, "/api/cockpit").await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    let second = get(&app, &cookie, "/api/cockpit").await;

    assert_eq!(first.await.unwrap().status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);

    assert_eq!(state.shared().bridge.refresh_count, 1);
}

#[tokio::test]
async fn axum_router_reports_shell_version() {
    let _profile = EnvVarGuard::set("AJAX_WEB_RESTART_PROFILE", "prod");
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "axum-version");

    let response = get(&app, &cookie, "/api/version").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let value = json_of(response).await;
    let version = value["version"].as_str().expect("version string");
    assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
    assert_eq!(version, crate::slices::install::app_version());
    assert_eq!(value["test_in_stable"], false);
    assert_eq!(value["profile"], "prod");
}

#[tokio::test]
async fn axum_operation_preserves_branch_adoption_confirmation() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "adoption-confirm");

    let response = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"task_handle":"web/fix-login","action":"repair","confirmed":true,"branch_adoption":{"expected_branch":"ajax/fix-login","observed_branch":"fix/pane-stuck"}}"#,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        state.shared().bridge.operate,
        Some(OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: true,
            branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                expected_branch: "ajax/fix-login".to_string(),
                observed_branch: "fix/pane-stuck".to_string(),
            }),
        })
    );
}

#[tokio::test]
async fn axum_operations_are_idempotent_by_request_id() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (state, cookie, app) = app_with(context, TestBridge::default(), "axum-idempotency");

    let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
    let first = post_json(&app, &cookie, "/api/operations", operation).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_json = json_of(first).await;
    assert_eq!(first_json["ok"], true);
    assert_eq!(first_json["request_id"], "req-1");
    assert!(first_json["cockpit"].is_object());

    let second = post_json(&app, &cookie, "/api/operations", operation).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(json_of(second).await, first_json);

    assert_eq!(state.shared().bridge.operate_count, 1);
}

#[tokio::test]
async fn axum_task_starts_are_idempotent_by_request_id() {
    let (state, cookie, app) = app_with(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        TestBridge::default(),
        "axum-start-idempotency",
    );
    let request = r#"{"request_id":"start-1","repo":"web","title":"Fix login","agent":"codex"}"#;

    for _ in 0..2 {
        let response = post_json(&app, &cookie, "/api/tasks", request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = json_of(response).await;
        assert_eq!(json["ok"], true);
        assert_eq!(json["request_id"], "start-1");
        assert!(json["cockpit"].is_object());
    }

    assert_eq!(state.shared().bridge.start_count, 1);
}

#[tokio::test]
async fn axum_task_start_rejects_unsupported_agent_before_bridge() {
    let (state, cookie, app) = app_with(
        CommandContext::new(Config::default(), InMemoryRegistry::default()),
        TestBridge::default(),
        "axum-start-agent-allowlist",
    );

    let response = post_json(
        &app,
        &cookie,
        "/api/tasks",
        r#"{"request_id":"start-shell","repo":"web","title":"Fix login","agent":"/bin/sh"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("unsupported agent"));
    assert_eq!(state.shared().bridge.start_count, 0);
}

#[tokio::test]
async fn axum_operation_parse_errors_are_json() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "axum-json-error");

    let response = post_json(&app, &cookie, "/api/operations", "{not-json").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert!(json["error"].as_str().unwrap_or_default().contains("json"));
}

#[tokio::test]
async fn operation_endpoint_returns_refreshed_cockpit_on_bridge_error() {
    let (_state, cookie, app) = app_with(
        context_with_task(),
        TestBridge {
            operate_result: Err(ActionFailure {
                message: "bridge failed".to_string(),
                code: "command_failed".to_string(),
                state_changed: true,
            }),
            ..TestBridge::default()
        },
        "axum-operation-error",
    );

    let response = post_json(
        &app,
        &cookie,
        "/api/operations",
        r#"{"request_id":"op-error-1","task_handle":"web/fix-login","action":"review"}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = json_of(response).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "op-error-1");
    assert_eq!(json["state_changed"], true);
    assert_eq!(json["error"], "bridge failed");
    assert_eq!(json["code"], "command_failed");
    assert!(json["cockpit"].is_object());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_start_task_rejects_concurrent_colliding_normalized_identity() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_web_repo(),
        TestBridge {
            start_entered: Some(Arc::clone(&entered)),
            start_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-start-collision",
    );

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/tasks",
            r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("first start request never entered the bridge");

    let conflict = tokio::time::timeout(
        Duration::from_secs(5),
        post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-b","repo":"web","title":"Fix login!","agent":"codex"}"#,
        ),
    )
    .await
    .expect("second start request timed out");

    release_gate(&release);

    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let json = json_of(conflict).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "start-b");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("already in progress"));

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first)
            .await
            .expect("first start request timed out")
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(state.shared().bridge.start_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_start_task_rejects_when_action_operation_is_in_flight_before_bridge_side_effects() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_two_tasks(),
        TestBridge {
            operate_entered: Some(Arc::clone(&entered)),
            operate_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-start-blocked-by-action",
    );

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/operations",
            r#"{"request_id":"op-a","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("first operation request never entered the bridge");

    let conflict = tokio::time::timeout(
            Duration::from_secs(5),
            post_json(
                &app,
                &cookie,
                "/api/tasks",
                r#"{"request_id":"start-a","repo":"web","title":"Start while action runs","agent":"codex"}"#,
            ),
        )
        .await
        .expect("start request timed out");

    release_gate(&release);

    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let json = json_of(conflict).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["request_id"], "start-a");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("already in progress"));

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first)
            .await
            .expect("first operation request timed out")
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let guard = state.shared();
    assert_eq!(guard.bridge.start_calls.load(Ordering::SeqCst), 0);
    assert_eq!(guard.bridge.operate_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn axum_start_task_duplicate_request_id_does_not_clear_original_in_flight_marker() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (state, cookie, app) = app_with(
        context_with_web_repo(),
        TestBridge {
            start_entered: Some(Arc::clone(&entered)),
            start_release: Some(Arc::clone(&release)),
            ..TestBridge::default()
        },
        "axum-start-duplicate-request-id",
    );

    let first_app = app.clone();
    let first_cookie = cookie.clone();
    let first = tokio::spawn(async move {
        post_json(
            &first_app,
            &first_cookie,
            "/api/tasks",
            r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("first start request never entered the bridge");

    let duplicate_same_task = post_json(
        &app,
        &cookie,
        "/api/tasks",
        r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
    )
    .await;
    assert_eq!(duplicate_same_task.status(), StatusCode::CONFLICT);

    let duplicate_different_task = tokio::time::timeout(
        Duration::from_secs(5),
        post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-a","repo":"web","title":"Different task","agent":"codex"}"#,
        ),
    )
    .await
    .expect("duplicate start request timed out");

    release_gate(&release);

    assert_eq!(duplicate_different_task.status(), StatusCode::CONFLICT);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first)
            .await
            .expect("first start request timed out")
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(state.shared().bridge.start_calls.load(Ordering::SeqCst), 1);
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
