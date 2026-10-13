//! Pure mapping from Claude Agent SDK stream messages (JSON) to the native
//! [`AgentEvent`] values consumed by the web session adapter.
//!
//! The Claude client connection loop (a later packet) calls
//! [`map_sdk_message`] once per parsed message and folds the returned
//! [`RpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Messages that carry no adapter meaning (`system`,
//! `rate_limit_event`, unknown types) map to an empty result and never panic
//! on missing or malformed fields.

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::client::AcpClientEvent;
use super::rpc_session::RpcMapping;

/// Map one parsed Claude Agent SDK message to adapter events and run state.
///
/// `session_id` is retained for interface parity with the ACP mappers; native
/// events carry no session id (the session slice binds them). The mapping is
/// total: any message — including non-objects, `system` records, and unknown
/// types — returns an empty result instead of panicking.
pub fn map_sdk_message(message: &Value, _session_id: &str) -> RpcMapping {
    let mut events = Vec::new();
    // ponytail: sub-agent traffic (non-null `parent_tool_use_id`) is a known
    // gap; drop it wholesale until sub-agent sessions are supported.
    if message
        .get("parent_tool_use_id")
        .is_some_and(|id| !id.is_null())
    {
        return RpcMapping {
            events,
            finished: false,
            ended_aborted: false,
        };
    }
    let (finished, ended_aborted) = match message.get("type").and_then(Value::as_str).unwrap_or("")
    {
        "stream_event" => map_stream_event(message, &mut events),
        "assistant" => {
            map_assistant_tool_calls(message, &mut events);
            (false, false)
        }
        "user" => {
            map_user_tool_results(message, &mut events);
            (false, false)
        }
        "result" => map_result(message, &mut events),
        // `system` (init/status/hook/thinking_tokens records),
        // `rate_limit_event`, and unknown message types produce no events.
        _ => (false, false),
    };
    RpcMapping {
        events,
        finished,
        ended_aborted,
    }
}

/// Map one `stream_event` wrapper: text/thinking deltas become native text
/// events, tool-use content-block starts open a tool call. Argument
/// (`input_json_delta`) and all other delta types carry no adapter output.
fn map_stream_event(message: &Value, events: &mut Vec<AgentEvent>) -> (bool, bool) {
    if let Some(event) = message.get("event") {
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_delta") => map_delta(event, events),
            Some("content_block_start") => {
                if let Some(tool_call) = tool_call_started(event) {
                    events.push(AgentEvent::Native(tool_call));
                }
            }
            _ => {}
        }
    }
    (false, false)
}

fn map_delta(event: &Value, events: &mut Vec<AgentEvent>) {
    let delta = event.get("delta");
    let Some(delta) = delta else { return };
    match delta.get("type").and_then(Value::as_str) {
        Some("text_delta") => {
            let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
            if !text.is_empty() {
                events.push(AgentEvent::Native(NativeEvent::AgentText(text.to_string())));
            }
        }
        Some("thinking_delta") => {
            let thinking = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
            if !thinking.is_empty() {
                events.push(AgentEvent::Native(NativeEvent::AgentThought(
                    thinking.to_string(),
                )));
            }
        }
        _ => {}
    }
}

/// `content_block_start` opening a `tool_use` block: the tool call id, name, and
/// (possibly empty) initial input. Non-tool blocks produce no event. The kind
/// and status strings are exactly what the ACP mapping would emit ("read",
/// "edit", ..., "in_progress").
fn tool_call_started(event: &Value) -> Option<NativeEvent> {
    let block = event.get("content_block")?;
    if block.get("type").and_then(Value::as_str)? != "tool_use" {
        return None;
    }
    let id = block.get("id").and_then(Value::as_str)?;
    let name = block
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(NativeEvent::ToolCall {
        id: id.to_string(),
        title: name.to_string(),
        kind: tool_kind_for_name(name),
        status: "in_progress".to_string(),
        raw_input: block.get("input").cloned(),
    })
}

/// The ACP wire string for the tool kind implied by a Claude Agent SDK tool
/// name. Unrecognized names fall back to `"other"`.
fn tool_kind_for_name(name: &str) -> String {
    let kind = match name {
        "Bash" => "execute",
        "Read" => "read",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "edit",
        "Glob" | "Grep" => "search",
        "WebFetch" | "WebSearch" => "fetch",
        _ => "other",
    };
    kind.to_string()
}

/// Assistant message content blocks: with `includePartialMessages` the text and
/// thinking were already streamed as deltas (re-emitting them would duplicate
/// the output), so only `tool_use` blocks matter — they carry the complete
/// input. Text/thinking/unknown blocks produce no events.
fn map_assistant_tool_calls(message: &Value, events: &mut Vec<AgentEvent>) {
    let Some(blocks) = message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(id) = block.get("id").and_then(Value::as_str) else {
            continue;
        };
        events.push(AgentEvent::Native(NativeEvent::ToolCallUpdate {
            id: id.to_string(),
            status: "in_progress".to_string(),
            content: None,
            raw_input: block.get("input").cloned(),
        }));
    }
}

/// User message content blocks of type `tool_result` settle the matching tool
/// call; plain text user blocks (e.g. `[Request interrupted by user]`) produce
/// no events.
fn map_user_tool_results(message: &Value, events: &mut Vec<AgentEvent>) {
    let Some(blocks) = message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let status = if block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "failed".to_string()
        } else {
            "completed".to_string()
        };
        events.push(AgentEvent::Native(NativeEvent::ToolCallUpdate {
            id: id.to_string(),
            status,
            content: Some(tool_result_text(block)),
            raw_input: None,
        }));
    }
}

/// The text of one `tool_result` block: a string as is; an array of text
/// blocks joined with newlines; anything else the empty string.
fn tool_result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(blocks @ Value::Array(_)) => blocks
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `result` message: settles the run. Emits a usage event when token counts and
/// a model context window are available, then marks the run finished (aborted
/// for `error_during_execution` / `aborted_streaming`). A non-aborted error
/// result additionally emits an [`AcpClientEvent::Error`] after the usage
/// event.
fn map_result(message: &Value, events: &mut Vec<AgentEvent>) -> (bool, bool) {
    push_usage_update(message, events);
    let subtype = message
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let ended_aborted = subtype == "error_during_execution"
        && message.get("terminal_reason").and_then(Value::as_str) == Some("aborted_streaming");
    if message
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !ended_aborted
    {
        events.push(AgentEvent::Acp(AcpClientEvent::Error(format!(
            "Claude run failed: {subtype}"
        ))));
    }
    (true, ended_aborted)
}

/// Usage event for a `result` message: used tokens from `usage` and the context
/// window of the first `modelUsage` entry. Skipped when either is missing or
/// non-numeric (missing token parts count as 0).
fn push_usage_update(message: &Value, events: &mut Vec<AgentEvent>) {
    let usage = match message.get("usage") {
        Some(usage @ Value::Object(_)) => usage,
        _ => return,
    };
    let used = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ]
    .iter()
    .map(|key| usage.get(*key).and_then(Value::as_u64).unwrap_or(0))
    .sum::<u64>();
    let size = message
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|models| models.values().next())
        .and_then(|m| m.get("contextWindow"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    // A zero-usage result (e.g. an aborted turn) must not reset the live context meter to 0%.
    if used == 0 || size == 0 {
        return;
    }
    events.push(AgentEvent::Native(NativeEvent::Usage { used, size }));
}
