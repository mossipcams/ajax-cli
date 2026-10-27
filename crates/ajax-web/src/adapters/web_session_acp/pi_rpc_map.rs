use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

pub fn map_record(record: &Value, _session_id: &str) -> RpcMapping {
    let mut events = Vec::new();
    let finished = match record.get("type").and_then(Value::as_str).unwrap_or("") {
        "message_update" => map_message_update(record, &mut events),
        "tool_execution_start" => {
            if let Some(tool_call) = tool_call_started(record) {
                events.push(AgentEvent::Native(tool_call));
            }
            false
        }
        "tool_execution_update" => {
            if let Some(update) = tool_call_progress(record) {
                events.push(AgentEvent::Native(update));
            }
            false
        }
        "tool_execution_end" => {
            events.push(AgentEvent::Native(tool_call_finished(record)));
            false
        }
        "compaction_end" => {
            if record.get("reason").and_then(Value::as_str) == Some("manual")
                && record.get("aborted").and_then(Value::as_bool) != Some(true)
                && string_field(record, "errorMessage").is_none()
            {
                events.push(AgentEvent::Native(NativeEvent::AgentText(
                    manual_compaction_text(record),
                )));
            }
            false
        }
        "agent_settled" => true,
        "extension_ui_request" => {
            if let Some(event) = dialog_request(record).or_else(|| notify_event(record)) {
                events.push(AgentEvent::Native(event));
            }
            false
        }
        _ => false,
    };
    if let Some(PiRunError::Failed(text) | PiRunError::Notice(text)) = run_error(record) {
        events.push(AgentEvent::Native(NativeEvent::Error(text)));
    }
    RpcMapping {
        events,
        finished,
        ended_aborted: record_reports_aborted(record),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PiRunError {
    Pending(String),
    Clear,
    Failed(String),
    Notice(String),
}

pub(super) fn run_error(record: &Value) -> Option<PiRunError> {
    match record.get("type").and_then(Value::as_str)? {
        "message_end" => {
            let message = record.get("message")?;
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            match message.get("stopReason").and_then(Value::as_str) {
                Some("error") => Some(PiRunError::Pending(
                    string_field(message, "errorMessage")
                        .unwrap_or_else(|| "assistant message ended with an error".to_string()),
                )),
                _ => Some(PiRunError::Clear),
            }
        }
        "auto_retry_start" => Some(PiRunError::Clear),
        "auto_retry_end" => {
            if record.get("success").and_then(Value::as_bool) == Some(false) {
                Some(PiRunError::Failed(
                    string_field(record, "finalError")
                        .unwrap_or_else(|| "automatic retry failed".to_string()),
                ))
            } else {
                Some(PiRunError::Clear)
            }
        }
        "compaction_start" => (record.get("reason").and_then(Value::as_str) == Some("overflow"))
            .then_some(PiRunError::Clear),
        "compaction_end" => {
            if record.get("aborted").and_then(Value::as_bool) == Some(true)
                || record.get("reason").and_then(Value::as_str) == Some("manual")
            {
                return None;
            }
            string_field(record, "errorMessage").map(PiRunError::Failed)
        }
        "extension_error" => {
            let path = string_field(record, "extensionPath")
                .unwrap_or_else(|| "unknown extension".to_string());
            let event =
                string_field(record, "event").unwrap_or_else(|| "unknown event".to_string());
            let error =
                string_field(record, "error").unwrap_or_else(|| "unknown error".to_string());
            Some(PiRunError::Notice(format!(
                "Pi extension {path} failed during {event}: {error}"
            )))
        }
        _ => None,
    }
}

fn dialog_request(record: &Value) -> Option<NativeEvent> {
    let request_id = record.get("id")?.as_str()?.to_string();
    match record.get("method")?.as_str()? {
        "confirm" => {
            let text = string_field(record, "message").or_else(|| string_field(record, "title"))?;
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: text.clone(),
                schema: json!({
                    "type": "object",
                    "properties": { "confirmed": { "type": "boolean", "title": text } },
                    "required": ["confirmed"]
                }),
            })
        }
        "select" => {
            let title = string_field(record, "title")?;
            let options = record.get("options").and_then(Value::as_array)?;
            let enum_values: Vec<Value> = options
                .iter()
                .filter_map(Value::as_str)
                .map(Value::from)
                .collect();
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title.clone(),
                schema: json!({
                    "type": "object",
                    "properties": { "value": { "type": "string", "title": title, "enum": enum_values } },
                    "required": ["value"]
                }),
            })
        }
        "input" => {
            let title = string_field(record, "title")?;
            let mut schema = json!({
                "type": "object",
                "properties": { "value": { "type": "string", "title": title } },
                "required": ["value"]
            });
            if let Some(placeholder) = string_field(record, "placeholder") {
                schema["properties"]["value"]["description"] = json!(placeholder);
            }
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title,
                schema,
            })
        }
        "editor" => {
            let title = string_field(record, "title")?;
            let mut schema = json!({
                "type": "object",
                "properties": { "value": { "type": "string", "title": title } },
                "required": ["value"]
            });
            if let Some(prefill) = record.get("prefill") {
                schema["properties"]["value"]["default"] = prefill.clone();
            }
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title,
                schema,
            })
        }
        _ => None,
    }
}

fn notify_event(record: &Value) -> Option<NativeEvent> {
    if record.get("method")?.as_str()? != "notify" {
        return None;
    }
    let message = string_field(record, "message")?;
    if message.trim().is_empty() {
        return None;
    }
    Some(
        if record.get("notifyType").and_then(Value::as_str) == Some("error") {
            NativeEvent::Error(message)
        } else {
            NativeEvent::AgentText(format!("{message}\n"))
        },
    )
}

fn string_field(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(Value::as_str).map(str::to_string)
}

fn map_message_update(record: &Value, events: &mut Vec<AgentEvent>) -> bool {
    let Some(event) = record.get("assistantMessageEvent") else {
        return false;
    };
    let Some(delta) = event.get("delta").and_then(Value::as_str) else {
        return false;
    };
    let update = match event.get("type").and_then(Value::as_str) {
        Some("text_delta") => NativeEvent::AgentText(delta.to_string()),
        Some("thinking_delta") => NativeEvent::AgentThought(delta.to_string()),
        _ => return false,
    };
    events.push(AgentEvent::Native(update));
    false
}

fn tool_call_started(record: &Value) -> Option<NativeEvent> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let name = record.get("toolName").and_then(Value::as_str)?;
    let args = record.get("args").filter(|args| args.is_object());
    let title = match args.and_then(tool_call_detail) {
        Some(detail) => format!("{name}: {detail}"),
        None => name.to_string(),
    };
    Some(NativeEvent::ToolCall {
        id: id.to_string(),
        title,
        kind: tool_kind(name),
        status: "in_progress".to_string(),
        raw_input: Some(args.cloned().unwrap_or(Value::Null)),
    })
}

fn tool_kind(tool_name: &str) -> String {
    match tool_name.to_ascii_lowercase().as_str() {
        "read" => "read",
        "edit" | "write" => "edit",
        "grep" | "find" | "ls" => "search",
        "bash" => "execute",
        _ => "other",
    }
    .to_string()
}

fn tool_call_detail(args: &Value) -> Option<String> {
    for key in ["command", "pattern", "path"] {
        if let Some(detail) = args.get(key).and_then(Value::as_str) {
            if !detail.trim().is_empty() {
                return Some(detail.to_string());
            }
        }
    }
    None
}

fn tool_call_progress(record: &Value) -> Option<NativeEvent> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let texts = pi_text_blocks(record.get("partialResult")?.get("content")?.as_array()?);
    let joined = texts.join("");
    (!joined.trim().is_empty()).then(|| NativeEvent::ToolCallUpdate {
        id: id.to_string(),
        status: "in_progress".to_string(),
        content: Some(joined),
        raw_input: None,
    })
}

fn tool_call_finished(record: &Value) -> NativeEvent {
    let id = record
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = if record
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "failed".to_string()
    } else {
        "completed".to_string()
    };
    let texts = record
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| pi_text_blocks(blocks))
        .unwrap_or_default();
    NativeEvent::ToolCallUpdate {
        id: id.to_string(),
        status,
        content: Some(texts.join("")),
        raw_input: None,
    }
}

fn record_reports_aborted(record: &Value) -> bool {
    matches!(
        record
            .get("message")
            .and_then(|m| m.get("stopReason"))
            .and_then(Value::as_str),
        Some("aborted")
    )
}

fn manual_compaction_text(record: &Value) -> String {
    let result = record.get("result");
    let before = result
        .and_then(|r| r.get("tokensBefore"))
        .and_then(Value::as_u64);
    let after = result
        .and_then(|r| r.get("estimatedTokensAfter"))
        .and_then(Value::as_u64);
    match (before, after) {
        (Some(before), Some(after)) => {
            format!("Context compacted: {before} \u{2192} {after} tokens.\n")
        }
        _ => "Context compacted.\n".to_string(),
    }
}

fn pi_text_blocks(blocks: &[Value]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}
