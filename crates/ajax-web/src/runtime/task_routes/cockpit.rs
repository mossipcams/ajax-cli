use crate::adapters::http::Response;
use crate::runtime::bridge::RuntimeBridge;
use crate::runtime::state::{CockpitCacheEntry, WebAppState};
use crate::{
    adapters::http::{
        json_response, json_value_response, response_from_web_error, web_error_response,
    },
    slices::{cockpit, web_session::owned_session_handles},
    WebError,
};
use ajax_core::{
    adapters::CommandRunner,
    agent_notification::{
        pending_for_task, record_delivery, AgentNotificationDelivery,
        AgentNotificationDeliveryStatus,
    },
    registry::{InMemoryRegistry, Registry},
    runtime_refresh::RefreshTier,
};
use axum::{
    extract::{rejection::JsonRejection, Json, Path as AxumPath, Request as AxumRequest, State},
    http::{header, HeaderMap},
    response::Response as AxumResponse,
};
use std::{sync::Arc, time::Instant};

use super::live::{axum_task_session, axum_task_stt, axum_task_terminal, origin_authority};

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

#[derive(serde::Deserialize)]
pub(crate) struct PresenceRequest {
    visible: bool,
}

pub(crate) async fn axum_presence<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: Result<Json<PresenceRequest>, JsonRejection>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let Json(request) = match body {
        Ok(request) => request,
        Err(error) => {
            return json_value_response(
                400,
                serde_json::json!({ "ok": false, "error": error.body_text() }),
            );
        }
    };
    if request.visible {
        state.mark_browser_cockpit_seen();
    } else {
        state.clear_browser_cockpit_seen();
    }
    json_value_response(200, serde_json::json!({ "ok": true }))
}

pub(crate) async fn axum_cockpit<C, B>(
    State(state): State<WebAppState<C, B>>,
    headers: HeaderMap,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if request_marks_foreground_presence(&headers) {
        state.mark_browser_cockpit_seen();
    }
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    tokio::task::spawn_blocking(move || match state.refresh_flight.try_lock() {
        Ok(_flight) => refresh_cockpit_and_cache_locked(&state, RefreshTier::Live, false),
        Err(_) => current_projection_response(&state),
    })
    .await
    .unwrap_or_else(|error| {
        web_error_response(WebError::CommandFailed(format!(
            "cockpit refresh worker failed: {error}"
        )))
    })
}

/// The current shared projection, served without running or applying a refresh.
fn current_projection_response<C, B>(state: &WebAppState<C, B>) -> AxumResponse {
    let guard = state.shared();
    match serde_json::to_value(cockpit::browser_cockpit_view(&guard.context)) {
        Ok(value) => json_value_response(200, value),
        Err(error) => web_error_response(WebError::JsonSerialization(error.to_string())),
    }
}

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
        let _flight = state.refresh_flight.blocking_lock();
        refresh_cockpit_and_cache_locked(&state, tier, deliver_notifications)
    })
    .await
    .unwrap_or_else(|error| {
        web_error_response(WebError::CommandFailed(format!(
            "cockpit refresh worker failed: {error}"
        )))
    })
}

/// Runs one refresh. The caller holds `refresh_flight`. The probe runs on a
/// clone without `control_lane`; the apply (revision check, persist, swap, cache
/// fill) takes `control_lane` with a blocking lock and releases it. Deliveries
/// then run without the lane and are recorded under it by `deliver_and_record`.
/// Operations hold `refresh_flight` before the lane, so no operation can hold
/// the lane while this refresh is in flight; only brief session writes can, and
/// a probe whose base revision moved because of one is discarded and redone by
/// the next poll.
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
    let probe = bridge.refresh_cockpit(&mut context, &mut runner, tier, deliver_notifications);

    let lane = state.control_lane.blocking_lock();
    if state.shared().revision != base_revision {
        drop(lane);
        return current_projection_response(state);
    }
    let applied = match probe {
        Ok(true) => bridge.persist_registry_snapshot(&mut context),
        Ok(false) => Ok(()),
        Err(error) => Err(error),
    }
    .and_then(|()| cockpit_response(&context));
    let response = match applied {
        Ok(response) => response,
        Err(error) => {
            drop(lane);
            return web_error_response(error);
        }
    };
    {
        let mut guard = state.shared();
        if guard.revision == base_revision {
            let previously_owned = owned_session_handles(&guard.context);
            let still_owned = owned_session_handles(&context);
            let owned_task_removed = previously_owned
                .iter()
                .any(|handle| !still_owned.contains(handle));
            guard.context = context;
            guard.bridge = bridge;
            if owned_task_removed {
                state.cleanup_unowned_sessions();
            }
            guard.cockpit_cache = Some(CockpitCacheEntry {
                response: response.clone(),
                cached_at: Instant::now(),
                revision: guard.revision,
            });
        }
    }
    drop(lane);
    deliver_and_record(state, deliver_notifications)
        .unwrap_or_else(|| response.into_axum_response())
}

fn cockpit_response(
    context: &ajax_core::commands::CommandContext<InMemoryRegistry>,
) -> Result<Response, WebError> {
    json_response(
        200,
        serde_json::to_value(cockpit::browser_cockpit_view(context))
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    )
}

/// Delivers pending agent notifications and, when `deliver_notifications`, web
/// pushes, on a snapshot of the applied state. The caller holds `refresh_flight`
/// but not the control lane, and nothing is persisted here. Any change is then
/// recorded under the lane: swapped in and the revision bumped when the shared
/// revision is unchanged, otherwise merge-persisted so the next probe reloads
/// the delivered marks. Returns the response when deliveries changed state.
fn deliver_and_record<C, B>(
    state: &WebAppState<C, B>,
    deliver_notifications: bool,
) -> Option<AxumResponse>
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let (mut context, mut runner, mut bridge, base_revision) = {
        let guard = state.shared();
        let pending = guard
            .context
            .registry
            .list_tasks()
            .into_iter()
            .any(|task| pending_for_task(task).is_some());
        if !pending && !deliver_notifications {
            return None;
        }
        (
            guard.context.clone(),
            guard.runner.clone(),
            guard.bridge.clone(),
            guard.revision,
        )
    };
    let notified = deliver_agent_notifications(state, &mut context, &mut runner, &mut bridge);
    let pushed = deliver_notifications
        && crate::slices::push::deliver_attention_pushes(&mut context, &state.push);
    if !notified && !pushed {
        return None;
    }

    let lane = state.control_lane.blocking_lock();
    let persisted = bridge.persist_registry_snapshot(&mut context);
    if let Err(error) = &persisted {
        tracing::error!(
            target: "ajax_web",
            %error,
            "delivered agent notifications are recorded in memory but not on disk; they may be redelivered after restart"
        );
    }
    let mut guard = state.shared();
    let swapped = guard.revision == base_revision;
    if swapped {
        guard.context = context;
        guard.runner = runner;
        guard.bridge = bridge;
        guard.revision = guard.revision.saturating_add(1);
        guard.cockpit_cache = None;
    }
    let response = cockpit_response(&guard.context);
    if swapped {
        if let Ok(response) = &response {
            guard.cockpit_cache = Some(CockpitCacheEntry {
                response: response.clone(),
                cached_at: Instant::now(),
                revision: guard.revision,
            });
        }
    }
    drop(guard);
    drop(lane);
    Some(match response {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    })
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
    if req.uri().path().ends_with("/session/transcript") {
        let Some(task_handle) = handle.strip_suffix("/session/transcript") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_session_transcript(State(state), task_handle.to_string(), req).await;
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

fn transcript_origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(origin) = origin.to_str().ok() else {
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

pub(crate) async fn axum_task_session_transcript<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if !transcript_origin_allowed(req.headers()) {
        return json_value_response(
            403,
            serde_json::json!({ "ok": false, "error": "origin forbidden" }),
        );
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

    let query = req.uri().query().unwrap_or("");
    let before = match parse_usize_param(query, "before") {
        QueryParam::Missing => None,
        QueryParam::Value(value) => Some(value),
        QueryParam::Invalid => return invalid_query_param_response("before"),
    };
    let limit = match parse_usize_param(query, "limit") {
        QueryParam::Missing => None,
        QueryParam::Value(value) => Some(value.clamp(1, 1000)),
        QueryParam::Invalid => return invalid_query_param_response("limit"),
    };

    let directory = Arc::clone(&state.task_session_directory);
    let page = match tokio::task::spawn_blocking(move || {
        directory.retained_transcript_page(&handle, before, limit)
    })
    .await
    {
        Ok(page) => page,
        Err(error) => {
            return web_error_response(WebError::CommandFailed(format!(
                "transcript page worker failed: {error}"
            )));
        }
    };

    json_value_response(
        200,
        serde_json::json!({
            "protocolVersion": 2,
            "events": page.events,
            "from": page.from,
            "floor": page.floor,
        }),
    )
}

enum QueryParam {
    Missing,
    Value(usize),
    Invalid,
}

fn parse_usize_param(query: &str, key: &str) -> QueryParam {
    let Some(raw) = query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    }) else {
        return QueryParam::Missing;
    };
    match raw.parse::<usize>() {
        Ok(value) => QueryParam::Value(value),
        Err(_) => QueryParam::Invalid,
    }
}

fn invalid_query_param_response(key: &str) -> AxumResponse {
    json_value_response(
        400,
        serde_json::json!({ "ok": false, "error": format!("invalid {key} parameter") }),
    )
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
