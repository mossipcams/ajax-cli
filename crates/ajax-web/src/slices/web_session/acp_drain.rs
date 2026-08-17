//! Map normalized ACP client events into session wire events.

use super::{
    map_acp_client_request, map_acp_session_notification, map_acp_session_update,
    SessionServerEvent,
};
use crate::adapters::web_session_acp::AcpClientEvent;
use crate::adapters::web_session_acp::AcpStdioClient;
use serde_json::{json, Value};

pub(crate) fn drain_acp_events(client: &AcpStdioClient) -> (Vec<SessionServerEvent>, bool, bool) {
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
            AcpClientEvent::RequestFinished { result, method, .. } => {
                if method == "session/prompt" {
                    prompt_finished = true;
                }
                if let Some(mapped) = map_request_finished(method, result) {
                    events.push(mapped);
                }
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

pub(crate) fn coalesce_session_events(events: Vec<SessionServerEvent>) -> Vec<SessionServerEvent> {
    let mut coalesced = Vec::with_capacity(events.len());
    for event in events {
        let can_merge = match (&mut coalesced.last_mut(), &event) {
            (
                Some(SessionServerEvent::Message {
                    role: previous_role,
                    message_id: previous_id,
                    ..
                }),
                SessionServerEvent::Message {
                    role, message_id, ..
                },
            ) => {
                previous_role == role
                    && previous_id == message_id
                    && matches!(role.as_str(), "agent" | "thought")
            }
            _ => false,
        };
        if can_merge {
            if let (
                Some(SessionServerEvent::Message { text: previous, .. }),
                SessionServerEvent::Message { text, .. },
            ) = (coalesced.last_mut(), event)
            {
                previous.push_str(&text);
            }
        } else {
            coalesced.push(event);
        }
    }
    coalesced
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
) -> Option<SessionServerEvent> {
    match result {
        Ok(value) if method == "session/prompt" => Some(SessionServerEvent::TurnEnd {
            stop_reason: value
                .get("stopReason")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        Ok(_) => None,
        Err(message) => Some(SessionServerEvent::Error { message }),
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
