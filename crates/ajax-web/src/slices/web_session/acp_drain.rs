//! Map normalized ACP client events into session wire events.

use super::acp_usage::UsageDeduper;
use super::{
    map_acp_client_request, map_acp_session_notification, map_acp_session_update,
    normalize::StreamNormalizer, SessionServerEvent,
};
use crate::adapters::web_session_acp::{
    available_command_descriptors, config_option_descriptors, AcpClientEvent, AcpStdioClient,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptTerminalOutcome {
    Success,
    Cancelled,
    Failed,
}

pub(crate) fn classify_prompt_terminal(result: &Result<Value, String>) -> PromptTerminalOutcome {
    match result {
        Ok(_) => PromptTerminalOutcome::Success,
        Err(message) if is_cancellation_shaped_prompt_error(message) => {
            PromptTerminalOutcome::Cancelled
        }
        Err(_) => PromptTerminalOutcome::Failed,
    }
}

/// Request-correlated terminal result for one `session/prompt` RPC.
#[derive(Debug, Clone)]
pub(crate) struct PromptTerminal {
    pub request_id: u64,
    pub outcome: PromptTerminalOutcome,
    pub events: Vec<SessionServerEvent>,
}

pub(crate) struct AcpDrainOutcome {
    pub events: Vec<SessionServerEvent>,
    pub host_exited: bool,
    pub prompt_terminals: Vec<PromptTerminal>,
    pub applied_model: Option<String>,
    pub session_config_options:
        Option<Vec<crate::adapters::web_session_acp::ConfigOptionDescriptor>>,
    pub session_available_commands:
        Option<Vec<crate::adapters::web_session_acp::AvailableCommandDescriptor>>,
    pub session_title_update: Option<Option<String>>,
}

pub(crate) fn drain_acp_events(
    client: &AcpStdioClient,
    deduper: &mut UsageDeduper,
) -> AcpDrainOutcome {
    let mut events = Vec::new();
    let mut host_exited = false;
    let mut prompt_terminals = Vec::new();
    let mut applied_model = None;
    let mut session_config_options = None;
    let mut session_available_commands = None;
    let mut session_title_update = None;
    let startup_info = client
        .session_new_result()
        .pointer("/_meta/piAcp/startupInfo")
        .and_then(Value::as_str);
    while let Some(event) = client.poll_event() {
        match event {
            AcpClientEvent::ConfigOptionsUpdated {
                applied_model: model,
                config_options,
            } => {
                applied_model = Some(model);
                session_config_options = Some(config_option_descriptors(&config_options));
            }
            AcpClientEvent::AvailableCommandsUpdated { available_commands } => {
                session_available_commands =
                    Some(available_command_descriptors(&available_commands));
            }
            AcpClientEvent::SessionInfoUpdated { title } => {
                session_title_update = Some(title);
            }
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
            AcpClientEvent::ElicitationRequest {
                request_id,
                message,
                schema,
            } => {
                events.push(SessionServerEvent::ElicitationRequest {
                    request_id,
                    message,
                    schema,
                });
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
                let mapped = map_request_finished(method, result.clone(), Some(id), deduper);
                if method == "session/prompt" {
                    prompt_terminals.push(PromptTerminal {
                        request_id: id,
                        outcome: classify_prompt_terminal(&result),
                        events: mapped,
                    });
                } else {
                    events.extend(mapped);
                }
            }
            AcpClientEvent::Error(message) => {
                events.push(SessionServerEvent::Error {
                    message: map_operator_visible_acp_error(&message),
                });
            }
            AcpClientEvent::Exited => {
                host_exited = true;
                events.push(SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                });
            }
        }
    }
    AcpDrainOutcome {
        events,
        host_exited,
        prompt_terminals,
        applied_model,
        session_config_options,
        session_available_commands,
        session_title_update,
    }
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

pub(crate) const CONNECTION_INTERRUPTED_MESSAGE: &str =
    "The connection was interrupted. Try sending again.";

/// Cursor/ACP often finish a cancelled in-flight `session/prompt` as a transport
/// RetriableError instead of a normal result with `stopReason: "cancelled"`.
/// Match the cancel family only: `canceled`/`cancelled` (including harness
/// `[canceled]`/`[cancelled]` tags, `context canceled`, gRPC `Canceled`), and
/// HTTP/2 `error code cancel` / `CANCEL (0x8)`. Untagged stream close/reset and
/// other RST codes are transport failures, not operator cancellation.
fn is_cancellation_shaped_prompt_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("canceled") || lower.contains("cancelled") {
        return true;
    }
    lower.contains("error code cancel") || lower.contains("cancel (0x8)")
}

fn is_retriable_transport_error(message: &str) -> bool {
    message.starts_with("RetriableError:")
}

fn map_operator_visible_acp_error(message: &str) -> String {
    if is_retriable_transport_error(message) && !is_cancellation_shaped_prompt_error(message) {
        CONNECTION_INTERRUPTED_MESSAGE.to_string()
    } else {
        message.to_string()
    }
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
        Err(message)
            if method == "session/prompt" && is_cancellation_shaped_prompt_error(&message) =>
        {
            vec![SessionServerEvent::TurnEnd {
                stop_reason: Some("cancelled".to_string()),
            }]
        }
        Err(message) => vec![SessionServerEvent::Error {
            message: map_operator_visible_acp_error(&message),
        }],
    }
}

pub(crate) fn permission_response(approved: bool, reason: Option<&str>) -> Value {
    json!({
        "approved": approved,
        "reason": reason,
    })
}

#[cfg(test)]
mod tests;

pub(crate) fn parse_json_rpc_id(raw: &str) -> Value {
    if let Ok(n) = raw.parse::<u64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = raw.parse::<i64>() {
        return Value::Number(n.into());
    }
    Value::String(raw.to_string())
}
