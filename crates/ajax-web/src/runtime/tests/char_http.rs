//! HTTP-level orchestration-chat characterization tests.

use super::char_support::{
    char_app, connect_session_ws, install_fake_agent, prepare_worktree, serve_plain_http,
    session_ws_path, wait_for_type,
};
use super::{
    app_with, context_with_task, get, get_public, json_of, post_json, websocket_get, TestBridge,
};
use ajax_core::{
    commands::CommandContext, config::Config, models::AgentClient, registry::InMemoryRegistry,
};
use axum::http::StatusCode;
use std::time::Duration;

#[tokio::test]
async fn oc_acp_ws_auth() {
    let worktree = prepare_worktree("oc-acp-ws-auth");
    let (_state, _cookie, app, _worktree) = char_app(worktree, "oc-acp-ws-auth");
    let response = get_public(&app, &session_ws_path("web/fix-login", None)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oc_acp_ws_origin() {
    let worktree = prepare_worktree("oc-acp-ws-origin");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-acp-ws-origin");
    let response = websocket_get(
        &app,
        &cookie,
        &session_ws_path("web/fix-login", None),
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn oc_acp_ws_upgrade_required() {
    let worktree = prepare_worktree("oc-acp-ws-upgrade");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-acp-ws-upgrade");
    let response = get(&app, &cookie, &session_ws_path("web/fix-login", None)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oc_acp_non_cursor_409() {
    let (_state, cookie, app) =
        app_with(context_with_task(), TestBridge::default(), "oc-non-cursor");
    let response = websocket_get(
        &app,
        &cookie,
        &session_ws_path("web/fix-login", None),
        Some("http://localhost"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "session chat requires cursor orchestration");
}

#[tokio::test]
async fn oc_acp_task_not_found_404() {
    let worktree = prepare_worktree("oc-task-not-found");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-task-not-found");
    let response = websocket_get(
        &app,
        &cookie,
        &session_ws_path("web/missing", None),
        Some("http://localhost"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_of(response).await;
    assert_eq!(body["error"], "task not found");
}

#[tokio::test]
async fn oc_acp_worktree_missing_409() {
    let worktree = prepare_worktree("oc-worktree-missing");
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Cursor;
    task.worktree_path = worktree.join("does-not-exist");
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "oc-worktree-missing");
    let response = websocket_get(
        &app,
        &cookie,
        &session_ws_path("web/fix-login", None),
        Some("http://localhost"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_of(response).await;
    assert_eq!(body["error"], "worktree missing");
}

#[tokio::test]
async fn oc_start_non_cursor_reject() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "oc-start-reject");
    let response = post_json(
        &app,
        &cookie,
        "/api/tasks",
        r#"{"repo":"web","title":"Fix login","agent":"codex","request_id":"req-oc","orchestration_chat":true}"#,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(
        body["error"],
        "orchestration chat requires the cursor agent"
    );
}

#[tokio::test]
async fn oc_model_catalog() {
    let _agent = install_fake_agent(&[]);
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let (_state, cookie, app) = app_with(context, TestBridge::default(), "oc-model-catalog");
    let response = get(&app, &cookie, "/api/session/models").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await;
    let models = body["models"].as_array().expect("models array");
    assert!(
        models.iter().any(|model| model["id"] == "auto"),
        "catalog must include auto: {body:#?}"
    );
    assert!(
        models.iter().any(|model| model["id"] == "composer-2.5"),
        "fake agent models must surface composer-2.5: {body:#?}"
    );
}

#[tokio::test]
async fn oc_flag_server_ignorant() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-flag-server");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-flag-server");
    let (addr, _shutdown) = serve_plain_http(app).await;
    let mut ws = connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    assert_eq!(ready["model"], "auto");
    // conflict: docs say browser flag gates attach; server only checks Cursor + worktree.
}
