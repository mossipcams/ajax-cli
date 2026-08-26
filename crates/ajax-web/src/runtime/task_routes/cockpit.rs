//! Task/cockpit/diff/terminal/STT/operate Axum handlers.

use crate::runtime::bridge::RuntimeBridge;
use crate::runtime::state::{CockpitCacheEntry, WebAppState};
use crate::{
    adapters::http::{
        json_response, json_value_response, response_from_web_error, web_error_response,
    },
    slices::cockpit,
    WebError,
};
use ajax_core::{
    adapters::CommandRunner,
    agent_notification::{
        pending_for_task, record_delivery, AgentNotificationDelivery,
        AgentNotificationDeliveryStatus,
    },
    registry::Registry,
    runtime_refresh::RefreshTier,
};
use axum::{
    extract::{Path as AxumPath, Request as AxumRequest, State},
    http::HeaderMap,
    response::Response as AxumResponse,
};
use std::time::Instant;

use super::live::{axum_task_session, axum_task_stt, axum_task_terminal};

/// Cockpit polls send this only while the document is foreground-visible.
/// Background/Simulator polls still refresh data but must not suppress push.
pub(crate) const AJAX_FOREGROUND_HEADER: &str = "x-ajax-foreground";

pub(crate) fn request_marks_foreground_presence(headers: &HeaderMap) -> bool {
    headers
        .get(AJAX_FOREGROUND_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        })
}

pub(crate) async fn axum_cockpit<C, B>(
    State(state): State<WebAppState<C, B>>,
    headers: HeaderMap,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    // Same poll path either way — presence is header-gated, not a second request.
    if request_marks_foreground_presence(&headers) {
        state.mark_browser_cockpit_seen();
    }
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    tokio::task::spawn_blocking(move || match state.control_lane.try_lock() {
        Ok(_lane) => {
            let response = refresh_cockpit_and_cache_locked(&state, RefreshTier::Live, false, true);
            drop(_lane);
            apply_agent_notifications_outside_control_lane(&state);
            response
        }
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
        let response = {
            let _lane = state.control_lane.blocking_lock();
            refresh_cockpit_and_cache_locked(&state, tier, deliver_notifications, true)
        };
        apply_agent_notifications_outside_control_lane(&state);
        response
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
    skip_agent_notification_delivery: bool,
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
    let mut result = bridge
        .refresh_cockpit(&mut context, &mut runner, tier, deliver_notifications)
        .map(|_| ());
    if result.is_ok()
        && !skip_agent_notification_delivery
        && deliver_agent_notifications(state, &mut context, &mut runner, &mut bridge)
    {
        if let Err(error) = bridge.persist_registry_snapshot(&mut context) {
            result = Err(error);
        }
    }
    if deliver_notifications
        && result.is_ok()
        && crate::slices::push::deliver_attention_pushes(&mut context, &state.push)
    {
        let _ = bridge.persist_registry_snapshot(&mut context);
    }
    let result = result.and_then(|()| {
        json_response(
            200,
            serde_json::to_value(cockpit::browser_cockpit_view(&context))
                .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
        )
    });
    let cached_response = result.as_ref().ok().cloned();
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

fn apply_agent_notifications_outside_control_lane<C, B>(state: &WebAppState<C, B>)
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let (mut context, mut runner, mut bridge, base_revision) = {
        let guard = state.shared();
        (
            guard.context.clone(),
            guard.runner.clone(),
            guard.bridge.clone(),
            guard.revision,
        )
    };
    if !deliver_agent_notifications(state, &mut context, &mut runner, &mut bridge) {
        return;
    }
    if bridge.persist_registry_snapshot(&mut context).is_err() {
        return;
    }
    let _lane = state.control_lane.blocking_lock();
    let mut guard = state.shared();
    if guard.revision == base_revision {
        guard.context = context;
        guard.bridge = bridge;
        guard.revision = guard.revision.saturating_add(1);
        guard.cockpit_cache = None;
    }
}

fn deliver_agent_notifications<C, B>(
    state: &WebAppState<C, B>,
    context: &mut ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut B,
) -> bool
where
    C: CommandRunner,
    B: RuntimeBridge<C>,
{
    let pending = context
        .registry
        .list_tasks()
        .into_iter()
        .filter_map(|task| pending_for_task(task).map(|notification| (task.clone(), notification)))
        .collect::<Vec<_>>();
    let mut changed = false;
    for (task, notification) in pending {
        let outcome = if task.skip_interactive_agent() {
            tokio::runtime::Handle::current().block_on(
                crate::slices::web_session::deliver_agent_notification(
                    &state.task_session_directory,
                    &task,
                    &notification,
                ),
            )
        } else {
            bridge.deliver_agent_notification(context, runner, &task, &notification)
        };
        let (status, detail) = match outcome {
            Ok(status) => (status, None),
            Err(error) => (AgentNotificationDeliveryStatus::Error, Some(error)),
        };
        if let Some(task) = context.registry.get_task_mut(notification.task_id()) {
            changed |= record_delivery(
                task,
                AgentNotificationDelivery {
                    notification_id: notification.id().to_string(),
                    status,
                    detail,
                },
            );
        }
    }
    changed
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
    if req.uri().path().ends_with("/session") {
        let Some(task_handle) = handle.strip_suffix("/session") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_session(State(state), task_handle.to_string(), req).await;
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
