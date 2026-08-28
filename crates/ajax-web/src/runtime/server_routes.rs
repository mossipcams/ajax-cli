//! Runtime control and server lifecycle HTTP handlers.

use crate::{
    adapters::http::{bytes_axum_response, Response},
    runtime::WebAppState,
    slices::runtime_control,
};
use ajax_core::adapters::CommandRunner;
use axum::{
    extract::State,
    response::{IntoResponse, Response as AxumResponse},
};

use crate::runtime::bridge::RuntimeBridge;

pub async fn axum_server_runtime<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let restart_script_env = std::env::var(crate::adapters::server::RESTART_SCRIPT_ENV).ok();
    let restart_port_env = std::env::var(crate::adapters::server::RESTART_PORT_ENV).ok();
    let ajax_profile = std::env::var(crate::adapters::server::AJAX_PROFILE_ENV).ok();
    let cwd = std::env::current_dir().ok();
    let runner = {
        let shared = state.shared();
        shared.runner.clone()
    };
    let body = match tokio::task::spawn_blocking(move || {
        let mut runner = runner;
        runtime_control::runtime_status_json(
            &mut runner,
            runtime_control::RuntimeStatusInput {
                version: crate::slices::install::app_version(),
                profile: crate::adapters::server::resolved_web_profile_from_env()
                    .unwrap_or_else(|| "unknown".to_string()),
                test_in_stable: crate::adapters::server::test_in_stable_enabled_from_env(),
                restart_script_env: restart_script_env.as_deref(),
                restart_port_env: restart_port_env.as_deref(),
                ajax_profile: ajax_profile.as_deref(),
                cwd: cwd.as_deref(),
            },
        )
    })
    .await
    {
        Ok(body) => body,
        Err(_) => serde_json::json!({ "ok": false, "error": "runtime status worker failed" }),
    };
    axum::Json(body).into_response()
}

pub async fn axum_server_restart() -> AxumResponse {
    response_to_axum(runtime_control::handle_server_restart())
}

pub async fn axum_server_update() -> AxumResponse {
    response_to_axum(runtime_control::handle_server_update())
}

pub async fn axum_server_test_in_stable() -> AxumResponse {
    response_to_axum(handle_server_test_in_stable())
}

fn handle_server_test_in_stable() -> Response {
    if !crate::adapters::server::test_in_stable_enabled_from_env() {
        return Response {
            status_code: 404,
            content_type: "application/json; charset=utf-8",
            body: br#"{"ok":false,"error":"test in stable is not available"}"#.to_vec(),
        };
    }
    let restarting = crate::adapters::server::test_in_stable_restarts_current_instance();
    crate::adapters::server::schedule_test_in_stable();
    let body = if restarting {
        br#"{"ok":true,"restarting":true}"#.to_vec()
    } else {
        br#"{"ok":true,"restarting":false}"#.to_vec()
    };
    Response {
        status_code: 200,
        content_type: "application/json; charset=utf-8",
        body,
    }
}

fn response_to_axum(response: Response) -> AxumResponse {
    bytes_axum_response(response.status_code, response.content_type, response.body).into_response()
}
