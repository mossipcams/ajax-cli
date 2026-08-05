//! Task/cockpit/diff/terminal/STT/operate Axum handlers.

use crate::runtime::bridge::{handle_refreshed_cockpit_request, RuntimeBridge};
use crate::runtime::state::{CockpitCacheEntry, WebAppState};
use crate::{
    adapters::http::{
        json_response, json_value_response, response_from_web_error, web_error_response,
    },
    slices::cockpit,
    WebError,
};
use ajax_core::{adapters::CommandRunner, runtime_refresh::RefreshTier};
use axum::{
    extract::{Path as AxumPath, Request as AxumRequest, State},
    response::Response as AxumResponse,
};
use std::time::Instant;

use super::live::{axum_task_stt, axum_task_terminal, axum_task_web_session};

pub(crate) async fn axum_cockpit<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    state.mark_browser_cockpit_seen();
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    tokio::task::spawn_blocking(move || match state.control_lane.try_lock() {
        Ok(_lane) => refresh_cockpit_and_cache_locked(&state, RefreshTier::Live, false),
        Err(_) => {
            let guard = state.shared();
            match serde_json::to_value(cockpit::browser_cockpit_view(&guard.context)) {
                Ok(value) => json_value_response(200, value),
                Err(error) => web_error_response(WebError::JsonSerialization(error.to_string())),
            }
        }
    })
    .await
    .unwrap_or_else(|error| {
        web_error_response(WebError::CommandFailed(format!(
            "cockpit refresh worker failed: {error}"
        )))
    })
}

/// Refresh the cockpit projection and cache the response, delivering
/// declarative push as a side effect when requested. The cockpit handler,
/// the background push tick, and task mutations/task starts all serialize on
/// the same control lane, so a mutation cannot race an in-flight refresh and
/// discard its committed state.
pub(crate) async fn refresh_cockpit_and_cache<C, B>(
    state: &WebAppState<C, B>,
    tier: RefreshTier,
    deliver_notifications: bool,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let _lane = state.control_lane.blocking_lock();
        refresh_cockpit_and_cache_locked(&state, tier, deliver_notifications)
    })
    .await
    .unwrap_or_else(|error| {
        web_error_response(WebError::CommandFailed(format!(
            "cockpit refresh worker failed: {error}"
        )))
    })
}

pub(crate) fn refresh_cockpit_and_cache_locked<C, B>(
    state: &WebAppState<C, B>,
    tier: RefreshTier,
    deliver_notifications: bool,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    let (mut context, mut runner, mut bridge, base_revision) = {
        let guard = state.shared();
        (
            guard.context.clone(),
            guard.runner.clone(),
            guard.bridge.clone(),
            guard.revision,
        )
    };
    let result = handle_refreshed_cockpit_request(
        &mut context,
        &mut runner,
        &mut bridge,
        tier,
        deliver_notifications,
    );
    if deliver_notifications
        && result.is_ok()
        && crate::slices::push::deliver_attention_pushes(&mut context, &state.push)
    {
        let _ = bridge.persist_registry_snapshot(&mut context);
    }
    let cached_response = match &result {
        Ok(response) => Some(response.clone()),
        Err(_) => None,
    };
    {
        let mut guard = state.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.bridge = bridge;
            if let Some(response) = cached_response {
                guard.cockpit_cache = Some(CockpitCacheEntry {
                    response,
                    cached_at: Instant::now(),
                    revision: guard.revision,
                });
            }
        }
    }
    match result {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

pub(crate) async fn axum_task_detail<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let guard = state.shared();
    match cockpit::browser_task_detail_view(&guard.context, &handle) {
        Some(detail) => json_value_response(200, serde_json::to_value(detail).unwrap_or_default()),
        None => json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "task not found" }),
        ),
    }
}

pub(crate) async fn axum_task_get<C, B>(
    State(state): State<WebAppState<C, B>>,
    AxumPath(handle): AxumPath<String>,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if req.uri().path().ends_with("/terminal") {
        let Some(task_handle) = handle.strip_suffix("/terminal") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_terminal(State(state), task_handle.to_string(), req).await;
    }
    if req.uri().path().ends_with("/stt") {
        let Some(task_handle) = handle.strip_suffix("/stt") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_stt(State(state), task_handle.to_string(), req).await;
    }
    if req.uri().path().ends_with("/web-session") {
        let Some(task_handle) = handle.strip_suffix("/web-session") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_web_session(State(state), task_handle.to_string(), req).await;
    }
    if let Some(task_handle) = handle.strip_suffix("/symbols") {
        return axum_task_symbols(State(state), task_handle.to_string(), req).await;
    }
    if handle.ends_with("/snapshot") {
        return json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "not found" }),
        );
    }
    if let Some(task_handle) = handle.strip_suffix("/pull-requests") {
        return axum_task_pull_requests(State(state), task_handle.to_string()).await;
    }
    if let Some(task_handle) = handle.strip_suffix("/diff") {
        return axum_task_diff(State(state), task_handle.to_string(), req).await;
    }
    axum_task_detail::<C, B>(State(state), handle).await
}

pub(crate) async fn axum_task_symbols<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let query = parse_symbols_query(req.uri().query().unwrap_or(""));

    let response = tokio::task::spawn_blocking(move || {
        let guard = state.shared();
        match crate::slices::web_session::prepare_web_session(&guard.context, &handle) {
            Ok(plan) => {
                let symbols =
                    crate::slices::web_session::search_worktree_symbols(&plan.worktree_path, &query);
                json_value_response(
                    200,
                    serde_json::json!({ "ok": true, "symbols": symbols }),
                )
            }
            Err(crate::slices::web_session::WebSessionRouteError::TaskNotFound) => {
                json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                )
            }
            Err(crate::slices::web_session::WebSessionRouteError::WorktreeMissing) => {
                json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "worktree missing" }),
                )
            }
            Err(crate::slices::web_session::WebSessionRouteError::AgentNotSupported) => {
                json_value_response(
                    422,
                    serde_json::json!({ "ok": false, "error": "ajax web session requires cursor agent" }),
                )
            }
        }
    })
    .await
    .unwrap_or_else(|error| {
        web_error_response(WebError::CommandFailed(format!(
            "symbol search worker failed: {error}"
        )))
    });
    response
}

fn parse_symbols_query(query: &str) -> String {
    query
        .split('&')
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            if key == "q" {
                Some(percent_decode(value))
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

pub(crate) async fn axum_task_pull_requests<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let response = tokio::task::spawn_blocking(move || {
        state.run_read(|context, runner, _bridge| {
            let (response, metadata_changed) = match crate::slices::diff_review::list_task_pull_requests(
                context,
                runner,
                &handle,
            ) {
                Ok(projection) => {
                    let metadata_changed = projection.metadata_changed;
                    let response = json_response(
                        200,
                        serde_json::json!({ "pull_requests": projection.pull_requests }),
                    );
                    (response, metadata_changed)
                }
                Err(crate::slices::diff_review::DiffReviewRouteError::TaskNotFound) => (
                    json_response(
                        404,
                        serde_json::json!({ "ok": false, "error": "task not found" }),
                    ),
                    false,
                ),
                Err(crate::slices::diff_review::DiffReviewRouteError::Unobservable(reason)) => (
                    json_response(502, serde_json::json!({ "ok": false, "error": reason })),
                    false,
                ),
                Err(crate::slices::diff_review::DiffReviewRouteError::PrNotFound(number)) => (
                    json_response(
                        404,
                        serde_json::json!({ "ok": false, "error": format!("PR #{number} not found") }),
                    ),
                    false,
                ),
            };
            (
                response.unwrap_or_else(|error| response_from_web_error(error, None)),
                metadata_changed,
            )
        })
    })
    .await
    .unwrap_or_else(|error| {
        response_from_web_error(
            WebError::CommandFailed(format!("diff review worker failed: {error}")),
            None,
        )
    });
    response.into_axum_response()
}

pub(crate) async fn axum_task_diff<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let query = req.uri().query().unwrap_or("");
    let force_local = query
        .split('&')
        .any(|part| part == "local=1" || part == "local=true");
    let pr_number = query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key == "pr" {
            value.parse::<u64>().ok()
        } else {
            None
        }
    });

    let response = tokio::task::spawn_blocking(move || {
        state.run_read(|context, runner, _bridge| {
            let (response, metadata_changed) = match crate::slices::diff_review::task_diff_projection(
                context,
                runner,
                &handle,
                pr_number,
                force_local,
            ) {
                Ok(projection) => {
                    let metadata_changed = projection.metadata_changed;
                    let response =
                        json_response(200, serde_json::to_value(projection.diff).unwrap_or_default());
                    (response, metadata_changed)
                }
                Err(crate::slices::diff_review::DiffReviewRouteError::TaskNotFound) => (
                    json_response(
                        404,
                        serde_json::json!({ "ok": false, "error": "task not found" }),
                    ),
                    false,
                ),
                Err(crate::slices::diff_review::DiffReviewRouteError::Unobservable(reason)) => (
                    json_response(502, serde_json::json!({ "ok": false, "error": reason })),
                    false,
                ),
                Err(crate::slices::diff_review::DiffReviewRouteError::PrNotFound(number)) => (
                    json_response(
                        404,
                        serde_json::json!({ "ok": false, "error": format!("PR #{number} not found") }),
                    ),
                    false,
                ),
            };
            (
                response.unwrap_or_else(|error| response_from_web_error(error, None)),
                metadata_changed,
            )
        })
    })
    .await
    .unwrap_or_else(|error| {
        response_from_web_error(
            WebError::CommandFailed(format!("diff review worker failed: {error}")),
            None,
        )
    });
    response.into_axum_response()
}
