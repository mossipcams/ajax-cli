#![allow(dead_code)]
//! Pure mapping from Cursor SDK sidecar stream lines (JSON) to the native
//! [`AgentEvent`] values consumed by the web session adapter.
//!
//! The Cursor client connection loop (a later packet) calls
//! [`map_cursor_message`] once per parsed line and folds the returned
//! [`RpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Lines that carry no adapter meaning (`usage`,
//! `status`, `task`, `system`, `user`, `request`, unknown types) map to an
//! empty result and never panic on missing or malformed fields.

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

/// Map one parsed Cursor SDK sidecar line to adapter events and run state.
///
/// `session_id` is retained for interface parity with the other mappers;
/// native events carry no session id (the session slice binds them). The
/// mapping is total: any line — including non-objects, missing fields, and
/// unknown types — returns an empty result instead of panicking.
pub fn map_cursor_message(message: &Value, _session_id: &str) -> RpcMapping {
    let mut events = Vec::new();
    let (finished, ended_aborted) = match message.get("type").and_then(Value::as_str).unwrap_or("")
    {
        "assistant" => {
            map_assistant_text(message, &mut events);
            (false, false)
        }
        "thinking" => {
            map_thinking(message, &mut events);
            (false, false)
        }
        "tool_call" => {
            map_tool_call(message, &mut events);
            (false, false)
        }
        "result" => map_result(message, &mut events),
        // `usage` (no native turn-usage event yet — add when the shared layer
        // grows one), `status`, `task`, `system`, `user`, `request`, and unknown
        // types produce no events.
        _ => (false, false),
    };
    RpcMapping {
        events,
        finished,
        ended_aborted,
    }
}

/// Assistant content blocks: one [`NativeEvent::AgentText`] per non-empty text
/// block, in order. `tool_use` blocks are ignored — tool calls arrive as their
/// own `tool_call` lines.
fn map_assistant_text(message: &Value, events: &mut Vec<AgentEvent>) {
    let Some(blocks) = message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let text = block.get("text").and_then(Value::as_str).unwrap_or("");
        if !text.is_empty() {
            events.push(AgentEvent::Native(NativeEvent::AgentText(text.to_string())));
        }
    }
}

/// Thinking line: non-empty `text` is a thought; an empty `text` closes the
/// current thought and emits nothing.
fn map_thinking(message: &Value, events: &mut Vec<AgentEvent>) {
    let text = message.get("text").and_then(Value::as_str).unwrap_or("");
    if !text.is_empty() {
        events.push(AgentEvent::Native(NativeEvent::AgentThought(
            text.to_string(),
        )));
    }
}

/// `tool_call` line: a running call opens a tool call (or a plan, for the todo
/// tracker); a completed/errored one settles it. Lines with a missing or
/// non-string `call_id` produce nothing.
fn map_tool_call(message: &Value, events: &mut Vec<AgentEvent>) {
    let Some(id) = message.get("call_id").and_then(Value::as_str) else {
        return;
    };
    let name = message.get("name").and_then(Value::as_str).unwrap_or("");
    if is_todo_tracker(name) {
        if let Some(args) = message.get("args") {
            map_todos_plan(args, events);
        }
        return;
    }
    match message.get("status").and_then(Value::as_str).unwrap_or("") {
        "running" => {
            let args = message.get("args");
            events.push(AgentEvent::Native(NativeEvent::ToolCall {
                id: id.to_string(),
                title: tool_title(name, args),
                kind: tool_kind_for_name(name),
                status: "in_progress".to_string(),
                raw_input: args.cloned(),
            }));
        }
        "completed" | "error" => {
            let result = message.get("result");
            let failed = message.get("status").and_then(Value::as_str) == Some("error")
                || result.and_then(|r| r.get("status")).and_then(Value::as_str) == Some("error");
            events.push(AgentEvent::Native(NativeEvent::ToolCallUpdate {
                id: id.to_string(),
                status: if failed { "failed" } else { "completed" }.to_string(),
                content: tool_update_content(result),
                raw_input: message.get("args").cloned(),
            }));
        }
        _ => {}
    }
}

/// The ACP wire string for the tool kind implied by a Cursor SDK tool name.
/// Names are compared lowercased with underscores removed; unrecognized names
/// fall back to `"other"`.
fn tool_kind_for_name(name: &str) -> String {
    let normalized = normalize_tool_name(name);
    let kind = match normalized.as_str() {
        "read" => "read",
        "edit" | "write" | "delete" => "edit",
        "shell" => "execute",
        "grep" | "glob" | "ls" | "semsearch" => "search",
        _ => "other",
    };
    kind.to_string()
}

fn normalize_tool_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "")
}

/// The tool call title: the tool name, then `": "` plus the first non-empty of
/// `args.path`, `args.command`, `args.pattern`, and `args.globPattern` (string
/// values only).
fn tool_title(name: &str, args: Option<&Value>) -> String {
    let Some(args) = args.and_then(Value::as_object) else {
        return name.to_string();
    };
    for key in ["path", "command", "pattern", "globPattern"] {
        if let Some(detail) = args.get(key).and_then(Value::as_str) {
            if !detail.is_empty() {
                return format!("{name}: {detail}");
            }
        }
    }
    name.to_string()
}

/// The update content of a settled tool call: `result.value.content` when it is
/// a string; else `result.value.stdout` and `result.value.diffString` when they
/// are non-empty strings (the edit result's unified diff); else
/// `result.error.message` when it is a string; else nothing.
fn tool_update_content(result: Option<&Value>) -> Option<String> {
    let result = result?;
    if let Some(content) = result.pointer("/value/content").and_then(Value::as_str) {
        return Some(content.to_string());
    }
    if let Some(stdout) = result.pointer("/value/stdout").and_then(Value::as_str) {
        if !stdout.is_empty() {
            return Some(stdout.to_string());
        }
    }
    if let Some(diff) = result.pointer("/value/diffString").and_then(Value::as_str) {
        if !diff.is_empty() {
            return Some(diff.to_string());
        }
    }
    result
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn is_todo_tracker(name: &str) -> bool {
    normalize_tool_name(name) == "updatetodos"
}

/// `updateTodos` args: one [`NativeEvent::Plan`] entry per todo, in order. The
/// pair is `(content, status)`; `inProgress` maps to `"in_progress"`,
/// `cancelled` to `"completed"`, and other statuses pass through unchanged.
fn map_todos_plan(args: &Value, events: &mut Vec<AgentEvent>) {
    let Some(todos) = args.get("todos").and_then(Value::as_array) else {
        return;
    };
    let entries: Vec<(String, String)> = todos
        .iter()
        .map(|todo| {
            (
                todo.get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                match todo.get("status").and_then(Value::as_str).unwrap_or("") {
                    "inProgress" => "in_progress".to_string(),
                    "cancelled" => "completed".to_string(),
                    other => other.to_string(),
                },
            )
        })
        .collect();
    events.push(AgentEvent::Native(NativeEvent::Plan(entries)));
}

/// `result` line: the sidecar's single terminal line per prompt. Always settles
/// the run. A missing or non-string status is an error; `cancelled` aborts
/// silently; any other non-`finished` status emits an error event with the
/// error text, or a fallback message when absent.
fn map_result(message: &Value, events: &mut Vec<AgentEvent>) -> (bool, bool) {
    let status = message.get("status").and_then(Value::as_str);
    let ended_aborted = status == Some("cancelled");
    if status != Some("finished") && !ended_aborted {
        let text = message
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("cursor run failed");
        events.push(AgentEvent::Native(NativeEvent::Error(text.to_string())));
    }
    (true, ended_aborted)
}
