use std::sync::Arc;

use ajax_core::events::AgentEvent;
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

    fn parse_stdout_line(&self, line: &str) -> Option<AgentEvent> {
        self.parse_json_line(line)
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
        "tool_call" => parse_tool_call_event(&value),
        "assistant" => parse_assistant_event(&value),
        "result" => parse_result_event(&value),
        "request" => parse_request_event(&value),
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
    if value.get("subtype").and_then(Value::as_str) != Some("started") {
        return None;
    }

    let tool_call = value.get("tool_call")?;
    Some(AgentEvent::ToolCall {
        name: tool_call_name(tool_call),
    })
}

fn parse_assistant_event(value: &Value) -> Option<AgentEvent> {
    let text = assistant_text(value)?;
    if mentions_approval(&text) {
        return Some(AgentEvent::WaitingForApproval { command: None });
    }

    Some(AgentEvent::Message { text })
}

fn parse_result_event(value: &Value) -> Option<AgentEvent> {
    if value.get("is_error").and_then(Value::as_bool) == Some(true) {
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

    Some(AgentEvent::Completed)
}

fn parse_request_event(value: &Value) -> Option<AgentEvent> {
    let prompt = value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.get("prompt").and_then(Value::as_str))
        .unwrap_or("waiting for operator input")
        .to_string();

    if mentions_approval(&prompt) {
        Some(AgentEvent::WaitingForApproval { command: None })
    } else {
        Some(AgentEvent::WaitingForInput { prompt })
    }
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

    if let Some(shell) = tool_call
        .get("shellToolCall")
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

fn mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("approval required")
        || lower.contains("requires approval")
        || lower.contains("waiting for approval")
        || lower.contains("allow command")
        || lower.contains("proceed?")
        || lower.contains("permission")
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
            ProcessProtocol::args(&adapter, "fix tests"),
            vec![
                "agent",
                "--print",
                "--output-format",
                "stream-json",
                "fix tests"
            ]
        );
        assert_eq!(
            ProcessProtocol::parse_stdout_line(
                &adapter,
                r#"{"type":"system","subtype":"init","session_id":"abc"}"#
            ),
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
    #[case(
        r#"{"type":"tool_call","subtype":"started","call_id":"1","tool_call":{"readToolCall":{"args":{"path":"README.md"}}}}"#,
        AgentEvent::ToolCall { name: "read README.md".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","subtype":"started","call_id":"2","tool_call":{"writeToolCall":{"args":{"path":"summary.txt"}}}}"#,
        AgentEvent::ToolCall { name: "write summary.txt".to_string() }
    )]
    #[case(
        r#"{"type":"tool_call","subtype":"started","call_id":"3","tool_call":{"function":{"name":"grep","arguments":"{}"}}}"#,
        AgentEvent::ToolCall { name: "grep".to_string() }
    )]
    #[case(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Plan ready. Approve to proceed."}]}}"#,
        AgentEvent::Message { text: "Plan ready. Approve to proceed.".to_string() }
    )]
    #[case(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Approval required to run cargo test"}]}}"#,
        AgentEvent::WaitingForApproval { command: None }
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
    #[case(
        r#"{"type":"request","request_id":"req-2","prompt":"Which branch should I use?"}"#,
        AgentEvent::WaitingForInput { prompt: "Which branch should I use?".to_string() }
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
    fn cursor_json_lines_without_notifications_are_ignored(#[case] line: &str) {
        let adapter = CursorAdapter::new("cursor");

        assert_eq!(adapter.parse_json_line(line), None);
    }
}
