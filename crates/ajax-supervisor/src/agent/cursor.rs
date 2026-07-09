use std::sync::Arc;

use ajax_core::{events::AgentEvent, live::classify_pane, models::LiveStatusKind};
use serde_json::Value;

use crate::process_observer::{ProcessProtocol, StdoutParser};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorAdapter {
    program: String,
}

impl CursorAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn stream_json_args(&self, prompt: &str) -> Vec<String> {
        vec![
            "agent".to_string(),
            "--print".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            prompt.to_string(),
        ]
    }

    pub fn parse_json_line(&self, line: &str) -> Option<AgentEvent> {
        parse_cursor_json_line(line)
    }
}

impl ProcessProtocol for CursorAdapter {
    fn process_name(&self) -> &str {
        "cursor"
    }

    fn program(&self) -> &str {
        self.program()
    }

    fn args(&self, prompt: &str) -> Vec<String> {
        self.stream_json_args(prompt)
    }

    fn stdout_parser(&self) -> StdoutParser {
        Arc::new(parse_cursor_json_line)
    }
}

fn parse_cursor_json_line(line: &str) -> Option<AgentEvent> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let event_type = value.get("type")?.as_str()?.to_ascii_lowercase();

    match event_type.as_str() {
        "system" => parse_system_event(&value),
        "thinking" => Some(AgentEvent::Thinking),
        "tool_call" => parse_tool_call_event(&value),
        "assistant" => parse_assistant_event(&value),
        "result" => parse_result_event(&value),
        "request" => parse_request_event(&value),
        "status" => parse_status_event(&value),
        _ => None,
    }
}

fn parse_system_event(value: &Value) -> Option<AgentEvent> {
    if value.get("subtype").and_then(Value::as_str) == Some("init") {
        Some(AgentEvent::Started {
            agent: "cursor".to_string(),
        })
    } else {
        None
    }
}

fn parse_tool_call_event(value: &Value) -> Option<AgentEvent> {
    if let Some(status) = value.get("status").and_then(Value::as_str) {
        return match status {
            "running" | "in_progress" => Some(AgentEvent::ToolCall {
                name: sdk_tool_name(value),
            }),
            "error" | "failed" => Some(AgentEvent::Failed {
                message: tool_failure_message(value, &sdk_tool_name(value)),
            }),
            "completed" => None,
            _ => None,
        };
    }

    match value.get("subtype").and_then(Value::as_str) {
        Some("started") => value
            .get("tool_call")
            .map(|tool_call| AgentEvent::ToolCall {
                name: tool_call_name(tool_call),
            }),
        Some("completed") => value.get("tool_call").and_then(completed_tool_call_event),
        _ => None,
    }
}

fn parse_assistant_event(value: &Value) -> Option<AgentEvent> {
    agent_event_from_text(&assistant_text(value)?)
}

fn parse_result_event(value: &Value) -> Option<AgentEvent> {
    if value.get("is_error").and_then(Value::as_bool) == Some(true)
        || value.get("subtype").and_then(Value::as_str) == Some("error")
    {
        let message = value
            .get("result")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| value.get("message").and_then(Value::as_str))
            .unwrap_or("cursor reported failure");
        return Some(AgentEvent::Failed {
            message: message.to_string(),
        });
    }

    if value.get("subtype").and_then(Value::as_str) == Some("success")
        || value.get("is_error").and_then(Value::as_bool) == Some(false)
    {
        return Some(AgentEvent::Completed);
    }

    None
}

fn parse_request_event(value: &Value) -> Option<AgentEvent> {
    let prompt = value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.get("prompt").and_then(Value::as_str))
        .or_else(|| value.get("text").and_then(Value::as_str))
        .unwrap_or("waiting for operator input")
        .to_string();

    if mentions_approval(&prompt) {
        return Some(AgentEvent::WaitingForApproval {
            command: extract_shell_command(&prompt),
        });
    }

    Some(AgentEvent::WaitingForInput { prompt })
}

fn parse_status_event(value: &Value) -> Option<AgentEvent> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();

    match status.as_str() {
        "RUNNING" | "CREATING" => Some(AgentEvent::Thinking),
        "FINISHED" => Some(AgentEvent::Completed),
        "ERROR" | "CANCELLED" | "EXPIRED" => Some(AgentEvent::Failed {
            message: value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("cursor run failed")
                .to_string(),
        }),
        _ => None,
    }
}

fn agent_event_from_text(text: &str) -> Option<AgentEvent> {
    if mentions_approval(text) {
        return Some(AgentEvent::WaitingForApproval {
            command: extract_shell_command(text),
        });
    }

    if text.trim_end().ends_with('?') {
        return Some(AgentEvent::WaitingForInput {
            prompt: text.to_string(),
        });
    }

    match classify_pane(text).kind {
        LiveStatusKind::WaitingForApproval => Some(AgentEvent::WaitingForApproval {
            command: extract_shell_command(text),
        }),
        LiveStatusKind::WaitingForInput => Some(AgentEvent::WaitingForInput {
            prompt: text.to_string(),
        }),
        LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked
        | LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CiFailed
        | LiveStatusKind::MergeConflict => Some(AgentEvent::Failed {
            message: text.to_string(),
        }),
        LiveStatusKind::Done => Some(AgentEvent::Completed),
        _ => Some(AgentEvent::Message {
            text: text.to_string(),
        }),
    }
}

fn completed_tool_call_event(tool_call: &Value) -> Option<AgentEvent> {
    if let Some(message) = nested_tool_failure_message(tool_call) {
        return Some(AgentEvent::Failed { message });
    }

    None
}

fn assistant_text(value: &Value) -> Option<String> {
    let content = value.get("message")?.get("content")?.as_array()?;
    let text = content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            block.get("text").and_then(Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn sdk_tool_name(value: &Value) -> String {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("tool");

    if let Some(command) = value
        .get("args")
        .and_then(|args| args.get("command").or_else(|| args.get("cmd")))
        .and_then(Value::as_str)
    {
        return format!("{name}: {command}");
    }

    if let Some(path) = value
        .get("args")
        .and_then(|args| args.get("path"))
        .and_then(Value::as_str)
    {
        return format!("{name} {path}");
    }

    name.to_string()
}

fn tool_failure_message(value: &Value, tool_name: &str) -> String {
    value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get("message"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("tool failed: {tool_name}"))
}

fn tool_call_name(tool_call: &Value) -> String {
    if let Some(read) = tool_call.get("readToolCall") {
        if let Some(path) = read
            .get("args")
            .and_then(|args| args.get("path"))
            .and_then(Value::as_str)
        {
            return format!("read {path}");
        }
        return "read".to_string();
    }

    if let Some(write) = tool_call.get("writeToolCall") {
        if let Some(path) = write
            .get("args")
            .and_then(|args| args.get("path"))
            .and_then(Value::as_str)
        {
            return format!("write {path}");
        }
        return "write".to_string();
    }

    if let Some(edit) = tool_call.get("editToolCall") {
        if let Some(path) = edit
            .get("args")
            .and_then(|args| args.get("path"))
            .and_then(Value::as_str)
        {
            return format!("edit {path}");
        }
        return "edit".to_string();
    }

    if let Some(shell) = tool_call
        .get("bashToolCall")
        .or_else(|| tool_call.get("shellToolCall"))
        .or_else(|| tool_call.get("runTerminalCommandToolCall"))
    {
        if let Some(command) = shell
            .get("args")
            .and_then(|args| args.get("command").or_else(|| args.get("cmd")))
            .and_then(Value::as_str)
        {
            return format!("shell: {command}");
        }
        return "shell".to_string();
    }

    if let Some(function) = tool_call.get("function") {
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            return name.to_string();
        }
    }

    tool_call
        .as_object()
        .and_then(|fields| fields.keys().next())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "tool".to_string())
}

fn nested_tool_failure_message(tool_call: &Value) -> Option<String> {
    for key in tool_call.as_object()?.keys() {
        let item = tool_call.get(key)?;
        if item.get("error").and_then(Value::as_str).is_some() {
            return item
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if let Some(error) = item
            .get("result")
            .and_then(|result| result.get("error"))
            .and_then(Value::as_str)
        {
            return Some(error.to_string());
        }
        if item.get("status").and_then(Value::as_str) == Some("failed") {
            return Some(format!("tool failed: {key}"));
        }
    }

    None
}

fn extract_shell_command(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with("shell:"))
        .map(|line| line.trim_start_matches("shell:").trim().to_string())
        .filter(|command| !command.is_empty())
}

fn mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("approval required")
        || lower.contains("requires approval")
        || lower.contains("waiting for approval")
        || lower.contains("allow command")
        || lower.contains("proceed?")
        || lower.contains("approve to proceed")
}

#[cfg(test)]
mod tests {
    use ajax_core::events::AgentEvent;
    use rstest::rstest;

    use crate::process_observer::ProcessProtocol;

    use super::CursorAdapter;

    #[test]
    fn cursor_adapter_builds_stream_json_arguments() {
        let adapter = CursorAdapter::new("cursor");

        assert_eq!(
            adapter.stream_json_args("fix tests"),
            vec![
                "agent",
                "--print",
                "--output-format",
                "stream-json",
                "fix tests"
            ]
        );
    }

    #[test]
    fn cursor_adapter_process_protocol_uses_stream_json_and_stdout_parser() {
        let adapter = CursorAdapter::new("cursor");

        assert_eq!(adapter.process_name(), "cursor");
        assert_eq!(
            (adapter.stdout_parser())(r#"{"type":"system","subtype":"init","session_id":"abc"}"#),
            Some(AgentEvent::Started {
                agent: "cursor".to_string()
            })
        );
    }

    #[rstest]
    #[case(
        r#"{"type":"system","subtype":"init","session_id":"abc"}"#,
        AgentEvent::Started { agent: "cursor".to_string() }
    )]
    #[case(r#"{"type":"thinking","text":"planning"}"#, AgentEvent::Thinking)]
    #[case(
        r#"{"type":"tool_call","subtype":"started","call_id":"1","tool_call":{"readToolCall":{"args":{"path":"README.md"}}}}"#,
        AgentEvent::ToolCall { name: "read README.md".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","subtype":"started","call_id":"2","tool_call":{"bashToolCall":{"args":{"command":"cargo test"}}}}"#,
        AgentEvent::ToolCall { name: "shell: cargo test".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","call_id":"3","name":"grep","status":"running"}"#,
        AgentEvent::ToolCall { name: "grep".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","call_id":"3","name":"shell","status":"running","args":{"command":"cargo nextest run --all-features"}}"#,
        AgentEvent::ToolCall { name: "shell: cargo nextest run --all-features".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","call_id":"4","name":"grep","status":"error","error":"denied"}"#,
        AgentEvent::Failed { message: "denied".to_string() }
    )]
    #[case(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Plan ready. Approve to proceed."}]}}"#,
        AgentEvent::WaitingForApproval { command: None }
    )]
    #[case(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Which branch should I use?"}]}}"#,
        AgentEvent::WaitingForInput { prompt: "Which branch should I use?".to_string() }
    )]
    #[case(
        r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#,
        AgentEvent::Completed
    )]
    #[case(
        r#"{"type":"result","subtype":"error","is_error":true,"result":"auth failed"}"#,
        AgentEvent::Failed { message: "auth failed".to_string() }
    )]
    #[case(
        r#"{"type":"request","request_id":"req-1","message":"Allow command?"}"#,
        AgentEvent::WaitingForApproval { command: None }
    )]
    #[case(r#"{"type":"status","status":"FINISHED"}"#, AgentEvent::Completed)]
    #[case(
        r#"{"type":"status","status":"ERROR","message":"run failed"}"#,
        AgentEvent::Failed { message: "run failed".to_string() }
    )]
    fn cursor_stream_json_events_map_to_agent_events(
        #[case] line: &str,
        #[case] expected: AgentEvent,
    ) {
        let adapter = CursorAdapter::new("cursor");

        assert_eq!(adapter.parse_json_line(line), Some(expected));
    }

    #[rstest]
    #[case("")]
    #[case("not json")]
    #[case(r#"{"type":"user"}"#)]
    #[case(r#"{"type":"tool_call","subtype":"completed"}"#)]
    #[case(r#"{"type":"tool_call","call_id":"1","name":"grep","status":"completed"}"#)]
    fn cursor_json_lines_without_notifications_are_ignored(#[case] line: &str) {
        let adapter = CursorAdapter::new("cursor");

        assert_eq!(adapter.parse_json_line(line), None);
    }

    #[test]
    fn cursor_tool_call_completion_failure_maps_to_agent_failed() {
        let adapter = CursorAdapter::new("cursor");
        let line = r#"{"type":"tool_call","subtype":"completed","call_id":"1","tool_call":{"bashToolCall":{"args":{"command":"cargo test"},"result":{"error":"command declined"}}}}"#;

        assert_eq!(
            adapter.parse_json_line(line),
            Some(AgentEvent::Failed {
                message: "command declined".to_string(),
            })
        );
    }
}
