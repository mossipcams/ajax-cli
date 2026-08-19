//! Map normalized ACP client events into session wire events.

use super::acp_usage::UsageDeduper;
use super::{
    map_acp_client_request, map_acp_session_notification, map_acp_session_update,
    normalize::StreamNormalizer, SessionServerEvent,
};
use crate::adapters::web_session_acp::AcpClientEvent;
use crate::adapters::web_session_acp::AcpStdioClient;
use serde_json::{json, Value};

pub(crate) fn drain_acp_events(
    client: &AcpStdioClient,
    deduper: &mut UsageDeduper,
) -> (Vec<SessionServerEvent>, bool, bool) {
    let mut events = Vec::new();
    let mut host_exited = false;
    let mut prompt_finished = false;
    let startup_info = client
        .session_new_result()
        .pointer("/_meta/piAcp/startupInfo")
        .and_then(Value::as_str);
    while let Some(event) = client.poll_event() {
        match event {
            AcpClientEvent::SessionUpdate(params) => {
                let mut mapped = map_acp_session_notification(&params);
                for event in &mut mapped {
                    if let SessionServerEvent::Message { role, text, .. } = event {
                        if role == "agent" && startup_info == Some(text.as_str()) {
                            *role = "note".to_string();
                        }
                    }
                }
                events.extend(mapped);
            }
            AcpClientEvent::UnknownSessionUpdate(params) => {
                events.extend(map_acp_session_update_with_startup(&params, startup_info));
            }
            AcpClientEvent::ClientRequest { id, method, params } => {
                if let Some(mut mapped) = map_acp_client_request(&method, &params) {
                    if let SessionServerEvent::PermissionRequest { request_id, .. } = &mut mapped {
                        if let Some(rpc_id) = id.as_str() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_u64() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_i64() {
                            *request_id = rpc_id.to_string();
                        }
                    }
                    events.push(mapped);
                }
            }
            AcpClientEvent::RequestFinished {
                result, method, id, ..
            } => {
                if method == "session/prompt" {
                    prompt_finished = true;
                }
                events.extend(map_request_finished(method, result, Some(id), deduper));
            }
            AcpClientEvent::Error(message) => {
                events.push(SessionServerEvent::Error { message });
            }
            AcpClientEvent::Exited => {
                host_exited = true;
                events.push(SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                });
            }
        }
    }
    (events, host_exited, prompt_finished)
}

pub(crate) fn normalize_session_events(
    normalizer: &mut StreamNormalizer,
    events: Vec<SessionServerEvent>,
) -> Vec<SessionServerEvent> {
    normalizer.normalize_batch(events)
}

/// Legacy name kept for tests that assert delta coalescing behavior moved to normalize.
#[cfg(test)]
pub(crate) fn coalesce_session_events(events: Vec<SessionServerEvent>) -> Vec<SessionServerEvent> {
    normalize_session_events(&mut StreamNormalizer::default(), events)
}

pub(crate) fn map_acp_session_update_with_startup(
    params: &Value,
    startup_info: Option<&str>,
) -> Vec<SessionServerEvent> {
    let mut events = map_acp_session_update(params);
    for event in &mut events {
        if let SessionServerEvent::Message { role, text, .. } = event {
            if role == "agent" && startup_info == Some(text.as_str()) {
                *role = "note".to_string();
            }
        }
    }
    events
}

/// A finished `session/prompt` is the only signal the browser gets that the
/// agent stopped working, so it must reach the client even when the turn
/// succeeded. Other completed requests carry nothing the chat can show.
pub(crate) fn map_request_finished(
    method: &'static str,
    result: Result<Value, String>,
    request_id: Option<u64>,
    deduper: &mut UsageDeduper,
) -> Vec<SessionServerEvent> {
    match result {
        Ok(value) if method == "session/prompt" => {
            let mut events = Vec::new();
            if let Some(usage) =
                super::acp_usage::map_prompt_result_usage(&value, request_id, deduper)
            {
                events.push(usage);
            }
            events.push(SessionServerEvent::TurnEnd {
                stop_reason: value
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
            events
        }
        Ok(_) => Vec::new(),
        Err(message) => vec![SessionServerEvent::Error { message }],
    }
}

pub(crate) fn permission_response(approved: bool, reason: Option<&str>) -> Value {
    json!({
        "approved": approved,
        "reason": reason,
    })
}

pub(crate) fn parse_json_rpc_id(raw: &str) -> Value {
    if let Ok(n) = raw.parse::<u64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Value::Number(n.into());
    }
    Value::String(raw.to_string())
}
