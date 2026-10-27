#![allow(dead_code)]

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

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
        _ => (false, false),
    };
    RpcMapping {
        events,
        finished,
        ended_aborted,
    }
}

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

fn map_thinking(message: &Value, events: &mut Vec<AgentEvent>) {
    let text = message.get("text").and_then(Value::as_str).unwrap_or("");
    if !text.is_empty() {
        events.push(AgentEvent::Native(NativeEvent::AgentThought(
            text.to_string(),
        )));
    }
}

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
