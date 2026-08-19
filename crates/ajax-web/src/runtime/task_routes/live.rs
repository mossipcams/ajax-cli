//! Task/cockpit/diff/terminal/STT/operate Axum handlers.

use crate::runtime::bridge::{
    handle_action_request, operation_error_response, operation_success_response,
    MobileActionRequest, RuntimeBridge,
};
use crate::runtime::state::{operator_input_sink, GateRejection, WebAppState};
use crate::slices::web_session::PersistSessionModel;
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

pub(crate) async fn axum_task_session<C, B>(
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
    state.mark_browser_cockpit_seen();

    let model_raw = req
        .uri()
        .query()
        .and_then(|query| {
            query.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == "model").then_some(value)
            })
        })
        .map(percent_decode_model)
        .unwrap_or_else(|| "auto".to_string());

    let client_cursor = req
        .uri()
        .query()
        .and_then(|query| crate::slices::web_session::parse_client_cursor(Some(query)));

    let plan = {
        let guard = state.shared();
        match crate::slices::web_session::prepare_task_session(&guard.context, &handle, &model_raw)
        {
            Ok(plan) => plan,
            Err(crate::slices::web_session::SessionRouteError::TaskNotFound) => {
                return json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                );
            }
            Err(crate::slices::web_session::SessionRouteError::WorktreeMissing) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "worktree missing" }),
                );
            }
            Err(crate::slices::web_session::SessionRouteError::NotOrchestrationChat) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "session chat requires a provisioned ACP task" }),
                );
            }
        }
    };

    let directory = Arc::clone(&state.task_session_directory);
    let state_for_persist = state.clone();
    let handle_for_persist = plan.qualified_handle.clone();
    let persist_session_model: PersistSessionModel = Arc::new(move |model: &str| {
        state_for_persist.persist_task_session_model(&handle_for_persist, model)
    });
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::slices::web_session::bridge_task_session_socket(
            socket,
            directory,
            plan,
            client_cursor,
            Some(persist_session_model),
        )
        .await;
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

fn percent_decode_model(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push(char::from(hi * 16 + lo));
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(' ');
        } else {
            out.push(char::from(bytes[i]));
        }
        i += 1;
    }
    out
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Move a task to another harness: `{ "agent": "codex", "model": "..." }`.
/// Any other body stays a 404 so this route keeps its previous surface.
pub(crate) async fn axum_task_post<C, B>(
    State(state): State<WebAppState<C, B>>,
    AxumPath(handle): AxumPath<String>,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    #[derive(serde::Deserialize)]
    struct SwapAgentRequest {
        agent: String,
        #[serde(default)]
        model: Option<String>,
    }

    let Ok(request) = serde_json::from_slice::<SwapAgentRequest>(&body) else {
        return json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "not found" }),
        );
    };
    let handle = percent_decode_model(&handle);
    let directory = Arc::clone(&state.task_session_directory);
    let handle_for_apply = handle.clone();
    let apply_in_band = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let apply_flag = std::sync::Arc::clone(&apply_in_band);
    let reset_harness = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reset_flag = std::sync::Arc::clone(&reset_harness);
    let worktree_for_apply = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
    let worktree_slot = std::sync::Arc::clone(&worktree_for_apply);
    let model_for_apply = request.model.clone();
    let agent_for_reset = request.agent.clone();
    let same_harness = {
        let guard = state.shared();
        guard
            .context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == handle)
            .map(|task| {
                *worktree_slot.lock().unwrap() = Some(task.worktree_path.clone());
                !crate::slices::web_session::model_change::swap_resets_harness_context(
                    task.selected_agent,
                    &request.agent,
                )
            })
            .unwrap_or(false)
    };

    let response = tokio::task::spawn_blocking(move || {
        let _lane = state.control_lane.blocking_lock();
        state.run_optimistic(
            None,
            "cockpit state changed while the harness swap was running",
            |context, _runner, bridge| {
                let result = crate::slices::operate::swap_task_agent(
                    context,
                    &handle,
                    &request.agent,
                    request.model.as_deref(),
                );
                match result {
                    Ok(outcome) => {
                        if same_harness {
                            apply_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        } else {
                            reset_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        let _ = bridge.persist_registry_snapshot(context);
                        let response = match operation_success_response(outcome, context) {
                            Ok(response) => response,
                            Err(error) => response_from_web_error(error, None),
                        };
                        (response, true)
                    }
                    Err(error) => {
                        let status = match error {
                            crate::slices::operate::OperateError::Command(
                                ajax_core::commands::CommandError::TaskNotFound(_),
                                _,
                            ) => 404,
                            crate::slices::operate::OperateError::UnsupportedCapability(_) => 400,
                            _ => 409,
                        };
                        let body = serde_json::json!({
                            "ok": false,
                            "error": crate::slices::operate::format_operate_error(&error),
                            "code": crate::slices::operate::operate_error_code(&error),
                        });
                        (
                            match crate::runtime::json_response(status, body) {
                                Ok(response) => response,
                                Err(error) => response_from_web_error(error, None),
                            },
                            false,
                        )
                    }
                }
            },
        )
    })
    .await
    .unwrap_or_else(|error| {
        response_from_web_error(
            WebError::CommandFailed(format!("harness swap worker failed: {error}")),
            None,
        )
    });
    let worktree_path = worktree_for_apply.lock().unwrap().clone();
    if apply_in_band.load(std::sync::atomic::Ordering::SeqCst) {
        if let Some(worktree) = worktree_path {
            if let Err(error) = crate::slices::web_session::model_change::apply_persisted_model(
                &directory,
                &handle_for_apply,
                &worktree,
                model_for_apply.as_deref(),
            )
            .await
            {
                return match crate::runtime::json_response(
                    409,
                    serde_json::json!({
                        "ok": false,
                        "error": error,
                        "code": "command_failed",
                    }),
                ) {
                    Ok(response) => response.into_axum_response(),
                    Err(error) => response_from_web_error(error, None).into_axum_response(),
                };
            }
        }
    } else if reset_harness.load(std::sync::atomic::Ordering::SeqCst) {
        if let Some(worktree) = worktree_path {
            if let Err(error) = crate::slices::web_session::model_change::apply_cross_harness_reset(
                &directory,
                &handle_for_apply,
                &worktree,
                crate::slices::web_session::model_change::agent_client_from_name(&agent_for_reset),
                model_for_apply.as_deref(),
            )
            .await
            {
                return match crate::runtime::json_response(
                    409,
                    serde_json::json!({
                        "ok": false,
                        "error": error,
                        "code": "command_failed",
                    }),
                ) {
                    Ok(response) => response.into_axum_response(),
                    Err(error) => response_from_web_error(error, None).into_axum_response(),
                };
            }
        }
    }
    response.into_axum_response()
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
    if request.orchestration_chat && !crate::slices::operate::supports_acp_session(&request.agent) {
        return json_value_response(
            400,
            serde_json::json!({
                "ok": false,
                "request_id": request_id,
                "error": "orchestration chat requires an agent Ajax can start over ACP",
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
    let cleanup_after_drop = action == "drop";
    let directory = Arc::clone(&state.task_session_directory);
    let handle_for_cleanup = task_key.clone();
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

    if cleanup_after_drop && response.status_code < 400 {
        directory.cleanup_session(&handle_for_cleanup).await;
    }

    response.into_axum_response()
}
