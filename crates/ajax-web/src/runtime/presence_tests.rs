use super::tests::{app_with, json_of, post_json, TestBridge};
use super::BROWSER_CONNECTED_TTL;
use ajax_core::{commands::CommandContext, config::Config, registry::InMemoryRegistry};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::time::Duration;
use tower::ServiceExt;

fn empty_context() -> CommandContext<InMemoryRegistry> {
    CommandContext::new(Config::default(), InMemoryRegistry::default())
}

#[test]
fn browser_connected_ttl_is_45_seconds() {
    assert_eq!(BROWSER_CONNECTED_TTL, Duration::from_secs(45));
}

#[tokio::test]
async fn presence_hidden_clears_browser_connected_immediately() {
    let (state, cookie, app) = app_with(
        empty_context(),
        TestBridge::default(),
        "presence-hidden-clears",
    );

    let visible = post_json(&app, &cookie, "/api/presence", r#"{"visible":true}"#).await;
    assert_eq!(visible.status(), StatusCode::OK);
    assert!(state.browser_connected());

    let hidden = post_json(&app, &cookie, "/api/presence", r#"{"visible":false}"#).await;
    assert_eq!(hidden.status(), StatusCode::OK);
    assert_eq!(json_of(hidden).await["ok"], true);
    assert!(!state.browser_connected());
}

#[tokio::test]
async fn presence_visible_marks_browser_connected() {
    let (state, cookie, app) = app_with(
        empty_context(),
        TestBridge::default(),
        "presence-visible-marks",
    );

    assert!(!state.browser_connected());
    let response = post_json(&app, &cookie, "/api/presence", r#"{"visible":true}"#).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.browser_connected());
}

#[tokio::test]
async fn presence_rejects_invalid_body_and_leaves_state_unchanged() {
    let (state, cookie, app) = app_with(
        empty_context(),
        TestBridge::default(),
        "presence-invalid-body",
    );

    let response = post_json(&app, &cookie, "/api/presence", r#"{"visible":"yes"}"#).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_of(response).await["ok"], false);
    assert!(!state.browser_connected());

    let response = post_json(&app, &cookie, "/api/presence", "not json").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!state.browser_connected());
}

#[tokio::test]
async fn presence_requires_browser_session() {
    let (state, _cookie, app) = app_with(
        empty_context(),
        TestBridge::default(),
        "presence-requires-session",
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/presence")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"visible":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!state.browser_connected());
}
