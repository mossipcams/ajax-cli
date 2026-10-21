//! Focused HTTP checks for Cursor SDK auth routes (no key material in bodies).

use ajax_web::slices::web_session::cursor_auth::mount_cursor_auth_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

async fn get_body(app: &Router, path: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn cursor_auth_status_route_returns_sanitized_json_fields() {
    let app = mount_cursor_auth_routes(Router::new());
    let (status, body) = get_body(&app, "/api/cursor/auth/status").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"ok\""));
    assert!(body.contains("\"present\""));
    assert!(!body.contains("apiKey"));
    assert!(!body.contains("sk-"));
}
