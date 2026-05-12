use std::sync::Arc;

use ajax_core::events::AgentEvent;
use serde_json::Value;

use crate::process_observer::{ProcessProtocol, StdoutParser};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexAdapter {
    program: String,
}

impl CodexAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn exec_json_args(&self, prompt: &str) -> Vec<String> {
        vec!["exec".to_string(), "--json".to_string(), prompt.to_string()]
    }

    pub fn parse_json_line(&self, line: &str) -> Option<AgentEvent> {
        parse_codex_json_line(line)
    }
}

impl ProcessProtocol for CodexAdapter {
    fn process_name(&self) -> &str {
        "codex"
    }

    fn program(&self) -> &str {
        self.program()
    }

    fn args(&self, prompt: &str) -> Vec<String> {
        self.exec_json_args(prompt)
    }

    fn parse_stdout_line(&self, line: &str) -> Option<AgentEvent> {
        self.parse_json_line(line)
    }

    fn stdout_parser(&self) -> StdoutParser {
        Arc::new(parse_codex_json_line)
    }
}

fn parse_codex_json_line(line: &str) -> Option<AgentEvent> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let event_type = value
        .get("type")
        .or_else(|| value.get("event"))
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = value
        .get("message")
        .or_else(|| value.get("text"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if is_current_codex_event(&event_type) {
        return parse_current_codex_event(&value, &event_type, text);
    }

    if event_type.contains("approval") || mentions_approval(text) {
        return Some(AgentEvent::WaitingForApproval {
            command: string_field(&value, &["command", "cmd"]),
        });
    }

    if event_type.contains("tool") {
        return Some(AgentEvent::ToolCall {
            name: string_field(&value, &["name", "tool"]).unwrap_or_else(|| "tool".to_string()),
        });
    }

    if event_type.contains("start") {
        return Some(AgentEvent::Started {
            agent: "codex".to_string(),
        });
    }

    if event_type.contains("complete") || event_type == "done" {
        return Some(AgentEvent::Completed);
    }

    if event_type.contains("error") || event_type.contains("failed") {
        return Some(AgentEvent::Failed {
            message: if text.is_empty() {
                "codex reported failure".to_string()
            } else {
                text.to_string()
            },
        });
    }

    if !text.is_empty() {
        return Some(AgentEvent::Message {
            text: text.to_string(),
        });
    }

    None
}

fn is_current_codex_event(event_type: &str) -> bool {
    event_type == "error"
        || event_type.starts_with("thread.")
        || event_type.starts_with("turn.")
        || event_type.starts_with("item.")
}

fn parse_current_codex_event(value: &Value, event_type: &str, text: &str) -> Option<AgentEvent> {
    match event_type {
        "thread.started" => Some(AgentEvent::Started {
            agent: "codex".to_string(),
        }),
        "turn.started" => Some(AgentEvent::Thinking),
        "turn.completed" => Some(AgentEvent::Completed),
        "turn.failed" => Some(AgentEvent::Failed {
            message: event_error_message(value, text, "turn failed"),
        }),
        "error" => Some(AgentEvent::Failed {
            message: event_error_message(value, text, "codex reported failure"),
        }),
        "item.started" => value.get("item").and_then(parse_started_item),
        "item.updated" => value.get("item").and_then(parse_updated_item),
        "item.completed" => value.get("item").and_then(parse_completed_item),
        _ => None,
    }
}

fn parse_started_item(item: &Value) -> Option<AgentEvent> {
    match item_type(item)? {
        "command_execution" => Some(AgentEvent::ToolCall {
            name: string_field(item, &["command"]).unwrap_or_else(|| "command".to_string()),
        }),
        "mcp_tool_call" => Some(AgentEvent::ToolCall {
            name: mcp_tool_name(item),
        }),
        "web_search" => Some(AgentEvent::ToolCall {
            name: "web_search".to_string(),
        }),
        "collab_tool_call" => Some(AgentEvent::ToolCall {
            name: string_field(item, &["tool"]).unwrap_or_else(|| "collab_tool".to_string()),
        }),
        "file_change" => Some(AgentEvent::ToolCall {
            name: "file_change".to_string(),
        }),
        "todo_list" | "reasoning" => Some(AgentEvent::Thinking),
        _ => None,
    }
}

fn parse_updated_item(item: &Value) -> Option<AgentEvent> {
    match item_type(item)? {
        "command_execution" => command_execution_event(item),
        "file_change" => file_change_event(item),
        "mcp_tool_call" => mcp_tool_call_event(item),
        "collab_tool_call" => collab_tool_call_event(item),
        "todo_list" | "reasoning" => Some(AgentEvent::Thinking),
        _ => None,
    }
}

fn parse_completed_item(item: &Value) -> Option<AgentEvent> {
    match item_type(item)? {
        "agent_message" => string_field(item, &["text", "message", "content"])
            .map(|text| AgentEvent::Message { text }),
        "command_execution" => command_execution_event(item),
        "file_change" => file_change_event(item),
        "mcp_tool_call" => mcp_tool_call_event(item),
        "collab_tool_call" => collab_tool_call_event(item),
        "error" => Some(AgentEvent::Failed {
            message: event_error_message(item, "", "codex reported failure"),
        }),
        "reasoning" | "todo_list" | "web_search" => None,
        _ => None,
    }
}

fn command_execution_event(item: &Value) -> Option<AgentEvent> {
    let command = string_field(item, &["command"]).unwrap_or_else(|| "command".to_string());
    match item_status(item)? {
        "failed" => Some(AgentEvent::Failed {
            message: format!("command failed: {command}"),
        }),
        "declined" => Some(AgentEvent::Failed {
            message: format!("command declined: {command}"),
        }),
        "in_progress" => Some(AgentEvent::ToolCall { name: command }),
        "completed" => None,
        _ => None,
    }
}

fn file_change_event(item: &Value) -> Option<AgentEvent> {
    match item_status(item)? {
        "failed" => Some(AgentEvent::Failed {
            message: "file change failed".to_string(),
        }),
        "in_progress" => Some(AgentEvent::ToolCall {
            name: "file_change".to_string(),
        }),
        "completed" => None,
        _ => None,
    }
}

fn mcp_tool_call_event(item: &Value) -> Option<AgentEvent> {
    let name = mcp_tool_name(item);
    match item_status(item)? {
        "failed" => {
            let error = item_error_message(item);
            Some(AgentEvent::Failed {
                message: error.map_or_else(
                    || format!("mcp tool failed: {name}"),
                    |error| format!("mcp tool failed: {name}: {error}"),
                ),
            })
        }
        "in_progress" => Some(AgentEvent::ToolCall { name }),
        "completed" => None,
        _ => None,
    }
}

fn collab_tool_call_event(item: &Value) -> Option<AgentEvent> {
    let name = string_field(item, &["tool"]).unwrap_or_else(|| "collab_tool".to_string());
    match item_status(item)? {
        "failed" => Some(AgentEvent::Failed {
            message: format!("collab tool failed: {name}"),
        }),
        "in_progress" => Some(AgentEvent::ToolCall { name }),
        "completed" => None,
        _ => None,
    }
}

fn item_type(item: &Value) -> Option<&str> {
    item.get("type").and_then(Value::as_str)
}

fn item_status(item: &Value) -> Option<&str> {
    item.get("status").and_then(Value::as_str)
}

fn mcp_tool_name(item: &Value) -> String {
    let server = string_field(item, &["server"]);
    let tool = string_field(item, &["tool"]);
    match (server, tool) {
        (Some(server), Some(tool)) => format!("{server}.{tool}"),
        (Some(server), None) => server,
        (None, Some(tool)) => tool,
        (None, None) => "mcp_tool".to_string(),
    }
}

fn event_error_message(value: &Value, text: &str, fallback: &str) -> String {
    if let Some(message) = item_error_message(value) {
        return message;
    }

    if !text.is_empty() {
        return text.to_string();
    }

    fallback.to_string()
}

fn item_error_message(value: &Value) -> Option<String> {
    string_field(value, &["message"]).or_else(|| {
        value
            .get("error")
            .and_then(|error| string_field(error, &["message"]))
    })
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(Value::as_str)
        .map(str::to_string)
}

fn mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("approval required")
        || lower.contains("requires approval")
        || lower.contains("waiting for approval")
        || lower.contains("allow command")
        || lower.contains("proceed?")
}

#[cfg(test)]
mod tests {
    use ajax_core::events::AgentEvent;
    use rstest::rstest;

    use crate::process_observer::ProcessProtocol;

    use super::CodexAdapter;

    #[test]
    fn codex_adapter_builds_exec_json_arguments() {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            adapter.exec_json_args("fix tests"),
            vec!["exec", "--json", "fix tests"]
        );
    }

    #[test]
    fn codex_adapter_process_protocol_uses_json_exec_and_stdout_parser() {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            ProcessProtocol::args(&adapter, "fix tests"),
            vec!["exec", "--json", "fix tests"]
        );
        assert_eq!(
            ProcessProtocol::parse_stdout_line(&adapter, r#"{"type":"turn.started"}"#),
            Some(AgentEvent::Thinking)
        );
        assert_eq!(
            (adapter.stdout_parser())(r#"{"type":"turn.completed"}"#),
            Some(AgentEvent::Completed)
        );
    }

    #[rstest]
    #[case(
        r#"{"type":"thread.started","thread_id":"thread_1"}"#,
        AgentEvent::Started { agent: "codex".to_string() }
    )]
    #[case(r#"{"type":"turn.started"}"#, AgentEvent::Thinking)]
    #[case(
        r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"cargo test","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "cargo test".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Plan ready. Approve to proceed."}}"#,
        AgentEvent::Message { text: "Plan ready. Approve to proceed.".to_string() }
    )]
    #[case(
        r#"{"type":"turn.failed","message":"model stopped"}"#,
        AgentEvent::Failed { message: "model stopped".to_string() }
    )]
    #[case(
        r#"{"type":"turn.failed"}"#,
        AgentEvent::Failed { message: "turn failed".to_string() }
    )]
    #[case(
        r#"{"type":"error","error":{"message":"bad request"}}"#,
        AgentEvent::Failed { message: "bad request".to_string() }
    )]
    #[case(
        r#"{"type":"error"}"#,
        AgentEvent::Failed { message: "codex reported failure".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"mcp_tool_call","server":"github","tool":"search"}}"#,
        AgentEvent::ToolCall { name: "github.search".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"mcp_tool_call","server":"github"}}"#,
        AgentEvent::ToolCall { name: "github".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"mcp_tool_call","tool":"search"}}"#,
        AgentEvent::ToolCall { name: "search".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"mcp_tool_call"}}"#,
        AgentEvent::ToolCall { name: "mcp_tool".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"web_search"}}"#,
        AgentEvent::ToolCall { name: "web_search".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"collab_tool_call","tool":"apply_patch"}}"#,
        AgentEvent::ToolCall { name: "apply_patch".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"file_change"}}"#,
        AgentEvent::ToolCall { name: "file_change".to_string() }
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"reasoning"}}"#,
        AgentEvent::Thinking
    )]
    #[case(
        r#"{"type":"item.started","item":{"type":"todo_list"}}"#,
        AgentEvent::Thinking
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"command_execution","command":"cargo check","status":"failed"}}"#,
        AgentEvent::Failed { message: "command failed: cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"command_execution","command":"cargo check","status":"declined"}}"#,
        AgentEvent::Failed { message: "command declined: cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"command_execution","command":"cargo check","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"file_change","status":"failed"}}"#,
        AgentEvent::Failed { message: "file change failed".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"file_change","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "file_change".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"mcp_tool_call","server":"github","tool":"search","status":"failed","error":{"message":"rate limited"}}}"#,
        AgentEvent::Failed { message: "mcp tool failed: github.search: rate limited".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"mcp_tool_call","tool":"search","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "search".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"collab_tool_call","tool":"apply_patch","status":"failed"}}"#,
        AgentEvent::Failed { message: "collab tool failed: apply_patch".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"collab_tool_call","tool":"apply_patch","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "apply_patch".to_string() }
    )]
    #[case(
        r#"{"type":"item.updated","item":{"type":"reasoning","status":"in_progress"}}"#,
        AgentEvent::Thinking
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"command_execution","command":"cargo check","status":"failed"}}"#,
        AgentEvent::Failed { message: "command failed: cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"command_execution","command":"cargo check","status":"declined"}}"#,
        AgentEvent::Failed { message: "command declined: cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"command_execution","command":"cargo check","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "cargo check".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"file_change","status":"failed"}}"#,
        AgentEvent::Failed { message: "file change failed".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"file_change","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "file_change".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"mcp_tool_call","server":"github","tool":"search","status":"failed"}}"#,
        AgentEvent::Failed { message: "mcp tool failed: github.search".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"search","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "search".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"collab_tool_call","tool":"apply_patch","status":"failed"}}"#,
        AgentEvent::Failed { message: "collab tool failed: apply_patch".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"collab_tool_call","tool":"apply_patch","status":"in_progress"}}"#,
        AgentEvent::ToolCall { name: "apply_patch".to_string() }
    )]
    #[case(
        r#"{"type":"item.completed","item":{"type":"error","message":"stream failed"}}"#,
        AgentEvent::Failed { message: "stream failed".to_string() }
    )]
    #[case(r#"{"type":"turn.completed"}"#, AgentEvent::Completed)]
    fn current_codex_json_events_map_to_agent_events(
        #[case] line: &str,
        #[case] expected: AgentEvent,
    ) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(adapter.parse_json_line(line), Some(expected));
    }

    #[rstest]
    #[case(r#"{"type":"item.completed","item":{"type":"command_execution","command":"cargo check","status":"completed"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"file_change","status":"completed"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"search","status":"completed"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"collab_tool_call","tool":"apply_patch","status":"completed"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"reasoning"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"todo_list"}}"#)]
    #[case(r#"{"type":"item.completed","item":{"type":"web_search"}}"#)]
    fn completed_codex_items_without_visible_status_are_ignored(#[case] line: &str) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(adapter.parse_json_line(line), None);
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    #[case("not json")]
    #[case(r#"{"type":"unknown"}"#)]
    fn codex_json_lines_without_event_or_message_are_ignored(#[case] line: &str) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(adapter.parse_json_line(line), None);
    }

    #[rstest]
    #[case(r#"{"type":"task.complete"}"#)]
    #[case(r#"{"type":"done"}"#)]
    fn legacy_codex_completion_events_are_completed(#[case] line: &str) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(adapter.parse_json_line(line), Some(AgentEvent::Completed));
    }

    #[rstest]
    #[case(
        r#"{"type":"worker_failed","message":"process failed"}"#,
        "process failed"
    )]
    #[case(r#"{"type":"fatal_error","message":"bad request"}"#, "bad request")]
    #[case(r#"{"type":"worker_failed"}"#, "codex reported failure")]
    fn legacy_codex_failure_events_report_failures(#[case] line: &str, #[case] message: &str) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            adapter.parse_json_line(line),
            Some(AgentEvent::Failed {
                message: message.to_string(),
            })
        );
    }

    #[rstest]
    #[case(r#"{"message":"plain legacy message"}"#)]
    #[case(r#"{"content":"plain legacy message"}"#)]
    fn legacy_codex_message_text_is_emitted(#[case] line: &str) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            adapter.parse_json_line(line),
            Some(AgentEvent::Message {
                text: "plain legacy message".to_string(),
            })
        );
    }

    #[rstest]
    #[case(
        r#"{"type":"message","message":"Approval required to run cargo test","command":"cargo test"}"#,
        Some("cargo test")
    )]
    #[case(
        r#"{"type":"message","message":"This requires approval before continuing"}"#,
        None
    )]
    #[case(r#"{"type":"message","message":"Waiting for approval"}"#, None)]
    #[case(r#"{"type":"message","message":"Allow command?"}"#, None)]
    #[case(r#"{"type":"message","message":"Proceed?"}"#, None)]
    fn approval_messages_request_operator_approval(
        #[case] line: &str,
        #[case] expected_command: Option<&str>,
    ) {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            adapter.parse_json_line(line),
            Some(AgentEvent::WaitingForApproval {
                command: expected_command.map(str::to_string),
            })
        );
    }
}
