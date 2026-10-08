//! Pure mapping from Pi RPC-mode (`pi --mode rpc`) stdout records to the ACP
//! `AcpClientEvent` values consumed by the web session adapter.
//!
//! The connection loop that reads the JSONL stream (a later task) calls
//! [`map_record`] once per parsed record and folds the returned
//! [`PiRpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Records that carry no adapter meaning (`response`,
//! `extension_ui_request`, unknown types) map to an empty result and never
//! panic on missing or malformed fields.

// The Pi RPC connection loop that calls this module is added in a later task;
// it will remove this allow.
#![allow(dead_code)]

use agent_client_protocol::schema::v1::{
    Content, ContentBlock, ContentChunk, SessionNotification, SessionUpdate, TextContent, ToolCall,
    ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use serde_json::Value;

use super::client::AcpClientEvent;

/// What one Pi RPC stdout record means for the web session adapter.
pub struct PiRpcMapping {
    /// Adapter events to forward, in order. Empty for non-mappable records.
    pub events: Vec<AcpClientEvent>,
    /// The agent run has settled (`agent_settled` record).
    pub finished: bool,
    /// The run was reported aborted (`message.stopReason == "aborted"`).
    pub ended_aborted: bool,
}

/// Map one parsed Pi RPC stdout record to adapter events and run state.
///
/// `session_id` is the ACP session id the stream is bound to; every generated
/// notification carries it. The mapping is total: any record — including
/// `extension_ui_request` (`setStatus`, `setWidget`) and unknown types —
/// returns an empty result instead of panicking.
pub fn map_record(record: &Value, session_id: &str) -> PiRpcMapping {
    let mut events = Vec::new();
    let finished = match record.get("type").and_then(Value::as_str).unwrap_or("") {
        "message_update" => map_message_update(record, session_id, &mut events),
        "tool_execution_start" => {
            if let Some(tool_call) = tool_call_started(record) {
                push_update(SessionUpdate::ToolCall(tool_call), session_id, &mut events);
            }
            false
        }
        "tool_execution_update" => {
            if let Some(update) = tool_call_progress(record) {
                push_update(
                    SessionUpdate::ToolCallUpdate(update),
                    session_id,
                    &mut events,
                );
            }
            false
        }
        "tool_execution_end" => {
            push_update(
                SessionUpdate::ToolCallUpdate(tool_call_finished(record)),
                session_id,
                &mut events,
            );
            false
        }
        "agent_settled" => true,
        // `response`, `extension_ui_request` (including the `setStatus` and
        // `setWidget` methods), lifecycle records without adapter output, and
        // unknown record types produce no events.
        _ => false,
    };
    PiRpcMapping {
        events,
        finished,
        ended_aborted: record_reports_aborted(record),
    }
}

/// Map one `message_update` record's `assistantMessageEvent`, if it carries
/// text. Message starts/ends and streamed tool-call argument deltas carry no
/// adapter output.
fn map_message_update(record: &Value, session_id: &str, events: &mut Vec<AcpClientEvent>) -> bool {
    let Some(event) = record.get("assistantMessageEvent") else {
        return false;
    };
    let Some(delta) = event.get("delta").and_then(Value::as_str) else {
        return false;
    };
    // Same wire shape the ACP path already produces: one text content chunk.
    let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(delta)));
    let update = match event.get("type").and_then(Value::as_str) {
        Some("text_delta") => SessionUpdate::AgentMessageChunk(chunk),
        Some("thinking_delta") => SessionUpdate::AgentThoughtChunk(chunk),
        _ => return false,
    };
    push_update(update, session_id, events);
    false
}

fn tool_call_started(record: &Value) -> Option<ToolCall> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let name = record.get("toolName").and_then(Value::as_str)?;
    let args = record.get("args").filter(|args| args.is_object());
    let title = args
        .and_then(|args| args.get("command"))
        .and_then(Value::as_str)
        .unwrap_or(name)
        .to_string();
    Some(
        ToolCall::new(id.to_string(), title)
            .kind(ToolKind::Execute)
            .status(ToolCallStatus::InProgress)
            .raw_input(args.cloned().unwrap_or(Value::Null)),
    )
}

fn tool_call_progress(record: &Value) -> Option<ToolCallUpdate> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let texts = pi_text_blocks(record.get("partialResult")?.get("content")?.as_array()?);
    let joined = texts.join("");
    (!joined.trim().is_empty()).then(|| {
        ToolCallUpdate::new(
            id.to_string(),
            ToolCallUpdateFields::default()
                .status(ToolCallStatus::InProgress)
                .content(vec![ToolCallContent::Content(Content::new(
                    ContentBlock::Text(TextContent::new(joined)),
                ))]),
        )
    })
}

fn tool_call_finished(record: &Value) -> ToolCallUpdate {
    let id = record
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = if record
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };
    let texts = record
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| pi_text_blocks(blocks))
        .unwrap_or_default();
    ToolCallUpdate::new(
        id.to_string(),
        ToolCallUpdateFields::default()
            .status(status)
            .content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new(texts.join(""))),
            ))]),
    )
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

fn push_update(update: SessionUpdate, session_id: &str, events: &mut Vec<AcpClientEvent>) {
    events.push(AcpClientEvent::SessionUpdate(Box::new(
        SessionNotification::new(session_id.to_string(), update),
    )));
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
