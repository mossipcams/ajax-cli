//! HTTP handlers for the Cursor SDK sign-in flow. The browser only ever
//! receives phases and status payloads (login URL, email, expiry) — never key
//! material.

use crate::adapters::web_session_acp::CursorLogin;
use axum::{http::StatusCode, response::IntoResponse, Json};

const WORKER_FAILED: &str = "cursor login worker failed";

/// `GET /api/cursor/auth`: current credential state. 200 with the status on
/// success; 502 when the credential check reports an error.
pub async fn axum_cursor_auth_status() -> axum::response::Response {
    match tokio::task::spawn_blocking(|| CursorLogin::global().status()).await {
        Ok(Ok(status)) => Json(status).into_response(),
        Ok(Err(text)) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": text })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": WORKER_FAILED })),
        )
            .into_response(),
    }
}

/// `GET /api/cursor/auth/login`: current login phase.
pub async fn axum_cursor_login_phase() -> axum::response::Response {
    cursor_login_phase_response().await
}

/// `POST /api/cursor/auth/login`: start a sign-in flow (returns the phase).
pub async fn axum_cursor_login_start() -> axum::response::Response {
    match tokio::task::spawn_blocking(|| CursorLogin::global().start()).await {
        Ok(phase) => Json(phase).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": WORKER_FAILED })),
        )
            .into_response(),
    }
}

/// `POST /api/cursor/auth/login/cancel`: cancel the running sign-in flow.
pub async fn axum_cursor_login_cancel() -> axum::response::Response {
    match tokio::task::spawn_blocking(|| CursorLogin::global().cancel()).await {
        Ok(phase) => Json(phase).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": WORKER_FAILED })),
        )
            .into_response(),
    }
}

async fn cursor_login_phase_response() -> axum::response::Response {
    match tokio::task::spawn_blocking(|| CursorLogin::global().phase()).await {
        Ok(phase) => Json(phase).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": WORKER_FAILED })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::web_session_acp::{CursorCredentialStatus, CursorLoginPhase};
    use axum::{routing::get, Router};
    use serde_json::json;
    use tower::ServiceExt;

    #[test]
    fn login_phase_serde_shapes() {
        assert_eq!(
            serde_json::to_value(CursorLoginPhase::Waiting { url: "u".into() }).unwrap(),
            json!({ "phase": "waiting", "url": "u" })
        );
        assert_eq!(
            serde_json::to_value(CursorLoginPhase::Idle).unwrap(),
            json!({ "phase": "idle" })
        );
        assert_eq!(
            serde_json::to_value(CursorLoginPhase::Succeeded {
                email: Some("e".into()),
                expires_at_ms: Some(5)
            })
            .unwrap(),
            json!({ "phase": "succeeded", "email": "e", "expiresAtMs": 5 })
        );
        assert_eq!(
            serde_json::to_value(CursorLoginPhase::Failed {
                message: "m".into()
            })
            .unwrap(),
            json!({ "phase": "failed", "message": "m" })
        );
    }

    #[test]
    fn credential_status_serde_shape() {
        assert_eq!(
            serde_json::to_value(CursorCredentialStatus {
                present: true,
                valid: Some(false),
                email: None,
                expires_at_ms: Some(5),
                error: Some("x".into())
            })
            .unwrap(),
            json!({ "present": true, "valid": false, "email": null, "expiresAtMs": 5, "error": "x" })
        );
    }

    #[tokio::test]
    async fn phase_route_returns_idle_json() {
        let router = Router::new().route("/api/cursor/auth/login", get(axum_cursor_login_phase));
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/cursor/auth/login")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({ "phase": "idle" })
        );
    }
}
