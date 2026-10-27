use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

pub fn map_sdk_message(message: &Value, _session_id: &str) -> RpcMapping {
    let mut events = Vec::new();
    let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
    let sub_agent = message
        .get("parent_tool_use_id")
        .is_some_and(|id| !id.is_null());
    if sub_agent && !matches!(kind, "assistant" | "user") {
        return RpcMapping {
            events,
            finished: false,
            ended_aborted: false,
        };
    }
    let (finished, ended_aborted) = match kind {
        "stream_event" => map_stream_event(message, &mut events),
        "assistant" => {
            map_synthetic_assistant_text(message, sub_agent, &mut events);
            map_assistant_tool_calls(message, sub_agent, &mut events);
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
        "ajax_plan" => {
            if let Some(entries) = ajax_plan_entries(message) {
                events.push(AgentEvent::Native(NativeEvent::Plan(entries)));
            }
            (false, false)
        }
        _ => (false, false),
    };
    RpcMapping {
        events,
        finished,
        ended_aborted,
    }
}

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

fn map_assistant_tool_calls(message: &Value, sub_agent: bool, events: &mut Vec<AgentEvent>) {
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
        if name == "TodoWrite" && !sub_agent {
            if let Some(plan) = todo_write_plan(input) {
                events.push(AgentEvent::Native(NativeEvent::Plan(plan)));
            }
        }
    }
}

fn map_synthetic_assistant_text(message: &Value, sub_agent: bool, events: &mut Vec<AgentEvent>) {
    if sub_agent {
        return;
    }
    if message.get("error").is_some_and(|e| !e.is_null()) {
        return;
    }
    let message = message.get("message");
    if message.and_then(|m| m.get("model")).and_then(Value::as_str) != Some("<synthetic>") {
        return;
    }
    let Some(blocks) = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    let text: String = blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        events.push(AgentEvent::Native(NativeEvent::AgentText(text)));
    }
}

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
fn ajax_plan_entries(message: &Value) -> Option<Vec<(String, String)>> {
    let entries = message.get("entries")?.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|entry| {
                let content = entry.get("content")?.as_str()?;
                let status = entry.get("status")?.as_str()?;
                Some((content.to_string(), status.to_string()))
            })
            .collect(),
    )
}

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
