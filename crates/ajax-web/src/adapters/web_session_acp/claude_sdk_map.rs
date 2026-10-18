//! Pure mapping from Claude Agent SDK stream messages (JSON) to the native
//! [`AgentEvent`] values consumed by the web session adapter.
//!
//! The Claude client connection loop (a later packet) calls
//! [`map_sdk_message`] once per parsed message and folds the returned
//! [`RpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Messages that carry no adapter meaning (`system`,
//! `rate_limit_event`, malformed `elicitation_request` records, unknown types)
//! map to an empty result and never panic
//! on missing or malformed fields.

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
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
        "elicitation_request" => {
            if let (Some(id), Some(schema)) = (
                message.get("id").and_then(Value::as_str),
                message.get("schema").filter(|s| s.is_object()),
            ) {
                events.push(AgentEvent::Native(NativeEvent::ElicitationRequest {
                    request_id: id.to_string(),
                    message: message
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    schema: schema.clone(),
                }));
            }
            (false, false)
        }
        // `system` (init/status/hook/thinking_tokens records),
        // `rate_limit_event`, malformed `elicitation_request` records, and
        // unknown message types produce no events.
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
        "TodoWrite" => "think",
        _ => "other",
    };
    kind.to_string()
}

/// Assistant message content blocks: with `includePartialMessages` the text and
/// thinking were already streamed as deltas (re-emitting them would duplicate
/// the output), so only `tool_use` blocks matter — they carry the complete
/// input. Each block emits a [`NativeEvent::ToolCall`] with a descriptive
/// title, kind, and the complete raw input, upgrading the row opened at
/// `content_block_start`. A `TodoWrite` block additionally emits a
/// [`NativeEvent::Plan`] carrying one (content, status) pair per todo.
/// Text/thinking/unknown blocks produce no events.
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
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let input = block.get("input");
        events.push(AgentEvent::Native(NativeEvent::ToolCall {
            id: id.to_string(),
            title: tool_title(name, input),
            kind: tool_kind_for_name(name),
            status: "in_progress".to_string(),
            raw_input: input.cloned(),
        }));
        if name == "TodoWrite" {
            if let Some(plan) = todo_write_plan(input) {
                events.push(AgentEvent::Native(NativeEvent::Plan(plan)));
            }
        }
    }
}

/// The (content, status) pairs of a `TodoWrite` block's input `todos` array,
/// in order. Missing or non-array todos yield no plan; each todo contributes
/// one pair, passing its `content` and `status` strings through.
fn todo_write_plan(input: Option<&Value>) -> Option<Vec<(String, String)>> {
    let todos = input?.get("todos")?.as_array()?;
    Some(
        todos
            .iter()
            .map(|todo| {
                (
                    todo.get("content")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    todo.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .collect(),
    )
}

/// The descriptive tool-call title for one assistant `tool_use` block: the bare
/// name, suffixed with `: detail` when the input carries a recognizable target
/// (e.g. `Bash: ls -la`). Tools without a mapped detail field keep the bare
/// name.
fn tool_title(name: &str, input: Option<&Value>) -> String {
    let key = match name {
        "Bash" => "command",
        "Read" | "Write" | "Edit" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        "Glob" | "Grep" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Task" => "description",
        _ => "",
    };
    let detail = input
        .and_then(|input| input.get(key))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty());
    match detail {
        Some(detail) => format!("{name}: {detail}"),
        None => name.to_string(),
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

/// `result` message: settles the run. Marks the run finished (aborted for
/// `error_during_execution` / `aborted_streaming`). A non-aborted error result
/// additionally emits an [`NativeEvent::Error`]. API errors can use subtype
/// `success` with `is_error: true`; their `result` text carries the provider's
/// explanation (including quota resets). Context fill is not taken from here:
/// the `usage` field sums the whole turn and `modelUsage` is cumulative, so
/// both are wrong for the live meter; it comes from the client's
/// run-finished hook instead.
fn map_result(message: &Value, events: &mut Vec<AgentEvent>) -> (bool, bool) {
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
        let detail = message
            .get("result")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .unwrap_or(subtype);
        events.push(AgentEvent::Native(NativeEvent::Error(format!(
            "Claude run failed: {detail}"
        ))));
    }
    (true, ended_aborted)
}
