//! Pure mapping from Pi RPC-mode (`pi --mode rpc`) stdout records to the native
//! [`AgentEvent`] values consumed by the web session adapter.
//!
//! The connection loop that reads the JSONL stream (a later task) calls
//! [`map_record`] once per parsed record and folds the returned
//! [`RpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Records that carry no adapter meaning (`response`,
//! `extension_ui_request`, unknown types) map to an empty result and never
//! panic on missing or malformed fields.

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

/// Map one parsed Pi RPC stdout record to adapter events and run state.
///
/// `session_id` is retained for interface parity with the ACP mappers; native
/// events carry no session id (the session slice binds them). The mapping is
/// total: any record — including `extension_ui_request` (`setStatus`,
/// `setWidget`) and unknown types — returns an empty result instead of
/// panicking.
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
        "agent_settled" => true,
        // `response`, `extension_ui_request` (including the `setStatus` and
        // `setWidget` methods), lifecycle records without adapter output, and
        // unknown record types produce no events.
        _ => false,
    };
    RpcMapping {
        events,
        finished,
        ended_aborted: record_reports_aborted(record),
    }
}

/// Map one `message_update` record's `assistantMessageEvent`, if it carries
/// text. Message starts/ends and streamed tool-call argument deltas carry no
/// adapter output.
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
    let title = args
        .and_then(|args| args.get("command"))
        .and_then(Value::as_str)
        .unwrap_or(name)
        .to_string();
    Some(NativeEvent::ToolCall {
        id: id.to_string(),
        title,
        kind: "execute".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(args.cloned().unwrap_or(Value::Null)),
    })
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

/// `message_end` / `turn_end` record reporting an aborted assistant message.
fn record_reports_aborted(record: &Value) -> bool {
    matches!(
        record
            .get("message")
            .and_then(|m| m.get("stopReason"))
            .and_then(Value::as_str),
        Some("aborted")
    )
}

/// The `text` values of Pi content blocks, in order. Blocks without a string
/// `text` field are skipped so unexpected block shapes never fail the mapping.
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
