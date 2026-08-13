//! Orchestration-chat session WebSocket and models catalog handlers.

use crate::{
    adapters::{
        cursor_session,
        http::{json_value_response, text_axum_response},
    },
    runtime::state::WebAppState,
    runtime::task_routes::live::websocket_origin_allowed,
};
use ajax_core::adapters::CommandRunner;
use axum::{
    extract::{ws::WebSocketUpgrade, FromRequestParts, Request as AxumRequest, State},
    http::header,
    response::Response as AxumResponse,
};
use std::sync::Arc;

use crate::runtime::bridge::RuntimeBridge;

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

    let plan = {
        let guard = state.shared();
        match crate::slices::web_session::prepare_task_session(&guard.context, &handle) {
            Ok(plan) => plan,
            Err(crate::slices::web_session::SessionRouteError::TaskNotFound) => {
                return json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                );
            }
            Err(crate::slices::web_session::SessionRouteError::NotCursor) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "session chat requires cursor orchestration" }),
                );
            }
            Err(crate::slices::web_session::SessionRouteError::WorktreeMissing) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "worktree missing" }),
                );
            }
        }
    };

    let model = model_from_query(req.uri().query());
    let state_dir = Arc::clone(&state.state_dir);
    let session_host = Arc::clone(&state.session_host);
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        cursor_session::attach_session_socket(socket, plan, model, state_dir, session_host).await;
    })
}

pub(crate) async fn axum_session_models<C, B>(
    State(_state): State<WebAppState<C, B>>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    match cursor_session::list_cursor_models().await {
        Ok(models) => json_value_response(200, serde_json::json!({ "models": models })),
        Err(error) => json_value_response(500, serde_json::json!({ "ok": false, "error": error })),
    }
}

/// Parse `model=` from the session WS query. Empty, absent, or invalid values
/// fall back to `"auto"`.
pub(crate) fn model_from_query(query: Option<&str>) -> String {
    let Some(query) = query else {
        return "auto".to_string();
    };
    for pair in query.split('&') {
        if let Some(rest) = pair.strip_prefix("model=") {
            if rest.is_empty() {
                return "auto".to_string();
            }
            if rest.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
            }) {
                return rest.to_string();
            }
            return "auto".to_string();
        }
    }
    "auto".to_string()
}

#[cfg(test)]
mod tests {
    use super::model_from_query;

    #[test]
    fn model_from_query_defaults_to_auto() {
        assert_eq!(model_from_query(None), "auto");
        assert_eq!(model_from_query(Some("")), "auto");
        assert_eq!(model_from_query(Some("foo=bar")), "auto");
        assert_eq!(model_from_query(Some("model=")), "auto");
    }

    #[test]
    fn model_from_query_accepts_catalog_ids() {
        assert_eq!(model_from_query(Some("model=composer-2.5")), "composer-2.5");
        assert_eq!(model_from_query(Some("model=auto")), "auto");
    }

    #[test]
    fn model_from_query_rejects_junk() {
        assert_eq!(model_from_query(Some("model=bad/model")), "auto");
        assert_eq!(model_from_query(Some("model=bad model")), "auto");
    }
}
