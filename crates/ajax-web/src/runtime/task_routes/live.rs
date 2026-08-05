//! Task/cockpit/diff/terminal/STT/operate Axum handlers.

use crate::runtime::bridge::{
    handle_action_request, operation_error_response, operation_success_response,
    MobileActionRequest, RuntimeBridge,
};
use crate::runtime::state::{operator_input_sink, GateRejection, WebAppState};
use crate::{
    adapters::http::{
        json_value_response, operation_response_with_request_id, response_from_web_error,
        text_axum_response,
    },
    WebError,
};
use ajax_core::{adapters::CommandRunner, registry::Registry as _};
use axum::{
    body::Bytes,
    extract::{
        ws::WebSocketUpgrade, FromRequestParts, Path as AxumPath, Request as AxumRequest, State,
    },
    http::{header, HeaderMap},
    response::Response as AxumResponse,
    Json,
};
use std::sync::Arc;

pub(crate) async fn axum_task_terminal<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if !req
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return text_axum_response(400, "websocket upgrade required");
    }
    if !websocket_origin_allowed(req.headers()) {
        return text_axum_response(403, "websocket origin forbidden");
    }
    // A same-origin browser client reached the terminal socket; refresh
    // cockpit presence so the notify tick stays suppressed while it is open.
    state.mark_browser_cockpit_seen();

    let plan = {
        let guard = state.shared();
        match crate::slices::terminal::prepare_task_terminal(&guard.context, &handle) {
            Ok(plan) => plan,
            Err(crate::slices::terminal::TerminalRouteError::TaskNotFound) => {
                return json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                );
            }
            Err(crate::slices::terminal::TerminalRouteError::SessionMissing) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "tmux session missing" }),
                );
            }
        }
    };

    let seed_history = crate::adapters::terminal_pty::seed_history_from_query(req.uri().query());
    let client_id = crate::adapters::terminal_pty::client_id_from_query(req.uri().query());
    let on_operator_input = operator_input_sink(&state, plan.qualified_handle.clone());
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::adapters::terminal_pty::bridge_task_terminal_socket(
            socket,
            plan,
            seed_history,
            client_id,
            on_operator_input,
        )
        .await;
    })
}

pub(crate) async fn axum_task_stt<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if !req
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return text_axum_response(400, "websocket upgrade required");
    }
    if !websocket_origin_allowed(req.headers()) {
        return text_axum_response(403, "websocket origin forbidden");
    }

    let task_found = {
        let guard = state.shared();
        guard
            .context
            .registry
            .list_tasks()
            .into_iter()
            .any(|task| task.qualified_handle() == handle)
    };
    if !task_found {
        return json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "task not found" }),
        );
    }

    let provider = Arc::clone(&state.stt_provider);
    let finalization_timeout_ms = state.stt_finalization_timeout_ms;
    let phrase_end_silence_ms = state.stt_phrase_end_silence_ms;
    let pause_grace_period_ms = state.stt_pause_grace_period_ms;
    let language = state.stt_language.clone();
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::adapters::stt_provider::bridge_task_stt_socket(
            socket,
            provider,
            finalization_timeout_ms,
            phrase_end_silence_ms,
            pause_grace_period_ms,
            language,
        )
        .await;
    })
}

pub(crate) async fn axum_task_web_session<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if !req
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return text_axum_response(400, "websocket upgrade required");
    }
    if !websocket_origin_allowed(req.headers()) {
        return text_axum_response(403, "websocket origin forbidden");
    }

    let plan = {
        let guard = state.shared();
        match crate::slices::web_session::prepare_web_session(&guard.context, &handle) {
            Ok(plan) => plan,
            Err(crate::slices::web_session::WebSessionRouteError::TaskNotFound) => {
                return json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                );
            }
            Err(crate::slices::web_session::WebSessionRouteError::WorktreeMissing) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "worktree missing" }),
                );
            }
            Err(crate::slices::web_session::WebSessionRouteError::AgentNotSupported) => {
                return json_value_response(
                    422,
                    serde_json::json!({ "ok": false, "error": "ajax web session requires cursor agent" }),
                );
            }
        }
    };

    let worktree = plan.worktree_path;
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::adapters::web_session_rpc::bridge_task_web_session_socket(socket, worktree).await;
    })
}

pub(crate) fn websocket_origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    origin_authority(origin).is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

pub(crate) fn origin_authority(origin: &str) -> Option<&str> {
    let (scheme, rest) = origin.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return None;
    }
    Some(authority)
}

pub(crate) async fn axum_task_post<C, B>(
    State(_state): State<WebAppState<C, B>>,
    AxumPath(_handle): AxumPath<String>,
    _body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    json_value_response(
        404,
        serde_json::json!({ "ok": false, "error": "not found" }),
    )
}

pub(crate) async fn axum_start_task<C, B>(
    State(state): State<WebAppState<C, B>>,
    Json(request): Json<crate::slices::operate::StartTaskRequest>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let request_id = request.request_id.trim().to_string();
    if request_id.is_empty() {
        return json_value_response(
            400,
            serde_json::json!({ "ok": false, "error": "request_id is required" }),
        );
    }
    if !crate::slices::operate::supported_start_agent(&request.agent) {
        return json_value_response(
            400,
            serde_json::json!({
                "ok": false,
                "request_id": request_id,
                "error": format!("unsupported agent: {}", request.agent),
            }),
        );
    }
    let task_key = ajax_core::commands::start_task_identity(&request.repo, &request.title)
        .as_str()
        .to_string();
    if let Err(rejection) = state.operations().try_begin(Some(&request_id), &task_key) {
        return gate_rejection_response(rejection, Some(&request_id), &task_key, "task start");
    }
    let error_request_id = request_id.clone();
    let response = tokio::task::spawn_blocking(move || {
        let _lane = state.control_lane.blocking_lock();
        let response = state.run_optimistic(
            Some(&request_id),
            "cockpit state changed while task start was running",
            |context, runner, bridge| {
                let result = bridge.execute_start_task(request, context, runner);
                let (durable, http_result) = match result {
                    Ok(outcome) => {
                        let durable = outcome.state_changed;
                        (durable, operation_success_response(outcome, context))
                    }
                    Err(error) => {
                        let durable = error.state_changed;
                        (durable, operation_error_response(error, context))
                    }
                };
                let response = match http_result {
                    Ok(response) => operation_response_with_request_id(response, Some(&request_id)),
                    Err(error) => response_from_web_error(error, Some(&request_id)),
                };
                (response, durable)
            },
        );
        state
            .operations()
            .finish(Some(&request_id), &task_key, &response);
        response
    })
    .await
    .unwrap_or_else(|error| {
        response_from_web_error(
            WebError::CommandFailed(format!("task start worker failed: {error}")),
            Some(&error_request_id),
        )
    });
    response.into_axum_response()
}

/// Turn a gate rejection into the route response: replay the completed
/// response or report that a `{noun} already in progress` conflict.
pub(crate) fn gate_rejection_response(
    rejection: GateRejection,
    request_id: Option<&str>,
    task: &str,
    noun: &str,
) -> AxumResponse {
    match rejection {
        GateRejection::Replay(response) => {
            tracing::warn!(
                target: "ajax_web",
                request_id = ?request_id,
                task = %task,
                outcome = "replay",
                "operate gate"
            );
            response.into_axum_response()
        }
        GateRejection::Conflict => {
            tracing::warn!(
                target: "ajax_web",
                request_id = ?request_id,
                task = %task,
                outcome = "conflict",
                "operate gate"
            );
            json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": format!("{noun} already in progress"),
                }),
            )
        }
    }
}

pub(crate) async fn axum_action<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let request: MobileActionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return json_value_response(
                400,
                serde_json::json!({
                    "ok": false,
                    "error": format!("json parse failed: {error}"),
                }),
            );
        }
    };
    // The browser is actively driving an operate/action; refresh cockpit
    // presence so the background notify tick stays suppressed while it works.
    state.mark_browser_cockpit_seen();
    let request_id = request.request_id.clone();
    let task_key = request.task_handle.clone();
    let action = request.action.clone();
    if let Err(rejection) = state
        .operations()
        .try_begin(request_id.as_deref(), &task_key)
    {
        return gate_rejection_response(rejection, request_id.as_deref(), &task_key, "operation");
    }

    tracing::info!(
        target: "ajax_web",
        request_id = ?request_id,
        task = %task_key,
        action = %action,
        "operate begin"
    );

    let log_request_id = request_id.clone();
    let log_task_key = task_key.clone();
    let log_action = action.clone();
    let error_request_id = request_id.clone();
    let response = tokio::task::spawn_blocking(move || {
        let _lane = state.control_lane.blocking_lock();
        let response = state.run_optimistic(
            request_id.as_deref(),
            "cockpit state changed while operation was running",
            |context, runner, bridge| match handle_action_request(request, context, runner, bridge)
            {
                Ok((response, durable)) => (
                    operation_response_with_request_id(response, request_id.as_deref()),
                    durable,
                ),
                Err(error) => (response_from_web_error(error, request_id.as_deref()), false),
            },
        );
        state
            .operations()
            .finish(request_id.as_deref(), &task_key, &response);
        response
    })
    .await
    .unwrap_or_else(|error| {
        response_from_web_error(
            WebError::CommandFailed(format!("operation worker failed: {error}")),
            error_request_id.as_deref(),
        )
    });

    if response.status_code >= 400 {
        tracing::warn!(
            target: "ajax_web",
            request_id = ?log_request_id,
            task = %log_task_key,
            action = %log_action,
            status = response.status_code,
            outcome = "err",
            "operate end"
        );
    } else {
        tracing::info!(
            target: "ajax_web",
            request_id = ?log_request_id,
            task = %log_task_key,
            action = %log_action,
            status = response.status_code,
            outcome = "ok",
            "operate end"
        );
    }

    response.into_axum_response()
}
