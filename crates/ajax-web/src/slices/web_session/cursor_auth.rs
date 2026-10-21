//! Host-owned Cursor SDK sign-in routes and helpers (slice 4).
//! Responses carry only sanitized status; login URLs may be shown to the operator.

use axum::{
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::env;
use std::path::PathBuf;

pub use crate::adapters::web_session_acp::{
    cursor_auth_renew, cursor_auth_sign_in, cursor_auth_sign_out, cursor_auth_status,
    CursorAuthSignInResult, CursorAuthSignOutResult, CursorAuthStatus,
};

fn auth_cwd() -> PathBuf {
    env::temp_dir()
}

#[derive(Debug, Serialize)]
struct CursorAuthStatusBody {
    ok: bool,
    present: bool,
    #[serde(rename = "accountLabel", skip_serializing_if = "Option::is_none")]
    account_label: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CursorAuthSignInBody {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(rename = "accountLabel", skip_serializing_if = "Option::is_none")]
    account_label: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    #[serde(rename = "loginUrl", skip_serializing_if = "Option::is_none")]
    login_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CursorAuthSignOutBody {
    ok: bool,
    #[serde(rename = "signedOut", skip_serializing_if = "Option::is_none")]
    signed_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn status_body(status: CursorAuthStatus) -> CursorAuthStatusBody {
    CursorAuthStatusBody {
        ok: true,
        present: status.present,
        account_label: status.account_label,
        expires_at: status.expires_at,
        error: None,
    }
}

fn sign_in_body(result: CursorAuthSignInResult) -> CursorAuthSignInBody {
    CursorAuthSignInBody {
        ok: true,
        status: Some(result.status),
        account_label: result.account_label,
        expires_at: result.expires_at,
        login_url: result.login_url,
        error: None,
    }
}

fn sign_out_body(result: CursorAuthSignOutResult) -> CursorAuthSignOutBody {
    CursorAuthSignOutBody {
        ok: true,
        signed_out: Some(result.signed_out),
        error: None,
    }
}

pub async fn axum_cursor_auth_status() -> AxumResponse {
    let cwd = auth_cwd();
    match tokio::task::spawn_blocking(move || cursor_auth_status(&cwd)).await {
        Ok(Ok(status)) => Json(status_body(status)).into_response(),
        Ok(Err(error)) => Json(CursorAuthStatusBody {
            ok: false,
            present: false,
            account_label: None,
            expires_at: None,
            error: Some(error),
        })
        .into_response(),
        Err(_) => Json(CursorAuthStatusBody {
            ok: false,
            present: false,
            account_label: None,
            expires_at: None,
            error: Some("cursor auth status worker failed".to_string()),
        })
        .into_response(),
    }
}

pub async fn axum_cursor_auth_sign_in() -> AxumResponse {
    let cwd = auth_cwd();
    match tokio::task::spawn_blocking(move || cursor_auth_sign_in(&cwd)).await {
        Ok(Ok(result)) => Json(sign_in_body(result)).into_response(),
        Ok(Err(error)) => Json(CursorAuthSignInBody {
            ok: false,
            status: None,
            account_label: None,
            expires_at: None,
            login_url: None,
            error: Some(error),
        })
        .into_response(),
        Err(_) => Json(CursorAuthSignInBody {
            ok: false,
            status: None,
            account_label: None,
            expires_at: None,
            login_url: None,
            error: Some("cursor auth sign-in worker failed".to_string()),
        })
        .into_response(),
    }
}

pub async fn axum_cursor_auth_renew() -> AxumResponse {
    let cwd = auth_cwd();
    match tokio::task::spawn_blocking(move || cursor_auth_renew(&cwd)).await {
        Ok(Ok(result)) => Json(sign_in_body(result)).into_response(),
        Ok(Err(error)) => Json(CursorAuthSignInBody {
            ok: false,
            status: None,
            account_label: None,
            expires_at: None,
            login_url: None,
            error: Some(error),
        })
        .into_response(),
        Err(_) => Json(CursorAuthSignInBody {
            ok: false,
            status: None,
            account_label: None,
            expires_at: None,
            login_url: None,
            error: Some("cursor auth renew worker failed".to_string()),
        })
        .into_response(),
    }
}

pub async fn axum_cursor_auth_sign_out() -> AxumResponse {
    let cwd = auth_cwd();
    match tokio::task::spawn_blocking(move || cursor_auth_sign_out(&cwd)).await {
        Ok(Ok(result)) => Json(sign_out_body(result)).into_response(),
        Ok(Err(error)) => Json(CursorAuthSignOutBody {
            ok: false,
            signed_out: None,
            error: Some(error),
        })
        .into_response(),
        Err(_) => Json(CursorAuthSignOutBody {
            ok: false,
            signed_out: None,
            error: Some("cursor auth sign-out worker failed".to_string()),
        })
        .into_response(),
    }
}

/// Mount Cursor SDK auth API routes on an existing Axum router (state type preserved).
pub fn mount_cursor_auth_routes<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .route("/api/cursor/auth/status", get(axum_cursor_auth_status))
        .route("/api/cursor/auth/sign-in", post(axum_cursor_auth_sign_in))
        .route("/api/cursor/auth/renew", post(axum_cursor_auth_renew))
        .route("/api/cursor/auth/sign-out", post(axum_cursor_auth_sign_out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::web_session_acp::with_test_cursor_sdk_module;
    use std::fs;
    use std::path::Path;

    const LOGIN_SECRET: &str = "sk-fake-cursor-login-SECRET";

    fn write_auth_fake_sdk(dir: &Path) -> PathBuf {
        let sdk_path = dir.join("fake_cursor_sdk_auth.mjs");
        let source = [
            "export const Agent = { create: () => Promise.resolve({ agentId: 'auth-1', close() {}, send: () => Promise.resolve({ stream: async function* () {}, wait: async () => ({ status: 'finished' }) }) }) };",
            "const key = process.env.FAKE_AUTH_KEY ?? 'sk-fake-default';",
            "export const Cursor = {",
            "  auth: { login(options) {",
            "    if (typeof options?.onLoginUrl === 'function') options.onLoginUrl('https://cursor.com/sdk/login?code=fake-login-code-123');",
            "    return Promise.resolve({ apiKey: key, expiresAt: '2099-01-01T00:00:00.000Z' });",
            "  }},",
            "  me(options) { return Promise.resolve({ email: 'operator@example.com', hasKey: Boolean(options?.apiKey) }); },",
            "};",
        ]
            .join("\n");
        fs::write(&sdk_path, source).expect("write fake auth sdk");
        sdk_path
    }

    struct TempHome {
        previous: Option<String>,
        path: PathBuf,
    }

    impl TempHome {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("ajax-cursor-auth-home-{}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp home");
            let previous = std::env::var("HOME").ok();
            // SAFETY: single-threaded cursor_sdk tests; restored on drop.
            unsafe { std::env::set_var("HOME", &path) };
            unsafe { std::env::set_var("FAKE_AUTH_KEY", LOGIN_SECRET) };
            Self { previous, path }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe { std::env::set_var("HOME", value) },
                None => unsafe { std::env::remove_var("HOME") },
            }
            unsafe { std::env::remove_var("FAKE_AUTH_KEY") };
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn assert_no_secret(text: &str) {
        assert!(
            !text.contains(LOGIN_SECRET),
            "raw key leaked into output: {text}"
        );
    }

    #[test]
    fn cursor_auth_sign_in_returns_login_url_and_sanitized_status() {
        let _home = TempHome::new();
        let sdk_dir =
            std::env::temp_dir().join(format!("ajax-cursor-auth-sdk-{}", std::process::id()));
        fs::create_dir_all(&sdk_dir).expect("sdk dir");
        let sdk_path = write_auth_fake_sdk(&sdk_dir);
        let cwd = std::env::temp_dir().join(format!("ajax-cursor-auth-cwd-{}", std::process::id()));
        fs::create_dir_all(&cwd).expect("cwd");

        with_test_cursor_sdk_module(&sdk_path, || {
            let result = cursor_auth_sign_in(&cwd).expect("sign-in");
            assert_eq!(result.status, "signed-in");
            assert_eq!(
                result.login_url.as_deref(),
                Some("https://cursor.com/sdk/login?code=fake-login-code-123")
            );
            assert_eq!(
                result.account_label.as_deref(),
                Some("operator@example.com")
            );
            let encoded = serde_json::to_string(&result).expect("json");
            assert_no_secret(&encoded);
        });
    }

    #[tokio::test]
    async fn cursor_auth_status_http_route_returns_sanitized_json() {
        use tower::ServiceExt;
        let app = mount_cursor_auth_routes(Router::new());
        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/api/cursor/auth/status")
            .body(axum::body::Body::empty())
            .expect("request");
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("\"present\""));
        assert!(!text.contains(LOGIN_SECRET));
    }

    #[test]
    fn cursor_auth_status_reports_present_after_sign_in() {
        let _home = TempHome::new();
        let sdk_dir = std::env::temp_dir().join(format!(
            "ajax-cursor-auth-sdk-status-{}",
            std::process::id()
        ));
        fs::create_dir_all(&sdk_dir).expect("sdk dir");
        let sdk_path = write_auth_fake_sdk(&sdk_dir);
        let cwd = std::env::temp_dir().join(format!(
            "ajax-cursor-auth-cwd-status-{}",
            std::process::id()
        ));
        fs::create_dir_all(&cwd).expect("cwd");

        with_test_cursor_sdk_module(&sdk_path, || {
            cursor_auth_sign_in(&cwd).expect("sign-in");
            let status = cursor_auth_status(&cwd).expect("status");
            assert!(status.present);
            let encoded = serde_json::to_string(&status).expect("json");
            assert_no_secret(&encoded);
        });
    }
}
