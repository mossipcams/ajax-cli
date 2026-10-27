use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::cursor_sdk_map::map_cursor_message;
use super::rpc_session::RpcMapping;

const SESSION_ID: &str = "cursor-session-1";

fn map(message: &Value) -> RpcMapping {
    map_cursor_message(message, SESSION_ID)
}

fn native_event(event: &AgentEvent) -> &NativeEvent {
    match event {
        AgentEvent::Native(native) => native,
        other => panic!("expected a Native event, got {other:?}"),
    }
}

#[test]
fn assistant_text_blocks_map_in_order_and_skip_empty() {
    let message = serde_json::json!({
        "type": "assistant",
        "agent_id": "a1",
        "run_id": "r1",
        "message": {
            "role": "assistant",
            "content": [
                {"type": "text", "text": "first"},
                {"type": "text", "text": ""},
                {"type": "tool_use", "id": "x", "name": "read"},
                {"type": "text", "text": "second"}
            ]
        }
    });
    let mapping = map(&message);
    assert!(!mapping.finished);
    let texts: Vec<&str> = mapping
        .events
        .iter()
        .map(native_event)
        .map(|event| match event {
            NativeEvent::AgentText(text) => text.as_str(),
            other => panic!("unexpected event: {other:?}"),
        })
        .collect();
    assert_eq!(texts, vec!["first", "second"]);
}

#[test]
fn thinking_delta_maps_to_thought_and_empty_closes_silently() {
    let delta = map(&serde_json::json!({
        "type": "thinking",
        "text": "pondering",
        "thinking_duration_ms": 1
    }));
    assert_eq!(delta.events.len(), 1);
    match native_event(&delta.events[0]) {
        NativeEvent::AgentThought(text) => assert_eq!(text, "pondering"),
        other => panic!("expected an AgentThought, got {other:?}"),
    }

    let close = map(&serde_json::json!({"type": "thinking", "text": ""}));
    assert!(close.events.is_empty());
}

#[test]
fn running_tool_call_opens_with_verbatim_newline_id() {
    let message = serde_json::json!({
        "type": "tool_call",
        "call_id": "call-1\nfc_1",
        "name": "Read",
        "status": "running",
        "args": {"path": "/tmp/file.txt"}
    });
    let mapping = map(&message);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall {
            id, kind, status, ..
        } => {
            assert_eq!(id, "call-1\nfc_1");
            assert_eq!(kind, "read");
            assert_eq!(status, "in_progress");
        }
        other => panic!("expected a ToolCall, got {other:?}"),
    }
}

#[test]
fn tool_kind_and_title_table() {
    let cases: Vec<(&str, Value, &str, &str)> = vec![
        (
            "Read",
            serde_json::json!({"path": "f.rs"}),
            "read",
            "Read: f.rs",
        ),
        (
            "Edit",
            serde_json::json!({"path": "f.rs"}),
            "edit",
            "Edit: f.rs",
        ),
        ("Write", serde_json::json!({"path": ""}), "edit", "Write"),
        ("Delete", Value::Null, "edit", "Delete"),
        (
            "Shell",
            serde_json::json!({"command": "ls -la"}),
            "execute",
            "Shell: ls -la",
        ),
        (
            "Grep",
            serde_json::json!({"pattern": "fn main"}),
            "search",
            "Grep: fn main",
        ),
        (
            "Glob",
            serde_json::json!({"globPattern": "*.rs"}),
            "search",
            "Glob: *.rs",
        ),
        ("Ls", Value::Null, "search", "Ls"),
        ("SemSearch", Value::Null, "search", "SemSearch"),
        (
            "MysteryTool",
            serde_json::json!({"path": "/x"}),
            "other",
            "MysteryTool: /x",
        ),
    ];
    for (name, args, kind, title) in cases {
        let message = serde_json::json!({
            "type": "tool_call",
            "call_id": "c1",
            "name": name,
            "status": "running",
            "args": &args
        });
        match native_event(&map(&message).events[0]) {
            NativeEvent::ToolCall {
                kind: got_kind,
                title: got_title,
                ..
            } => {
                assert_eq!(got_kind.as_str(), kind, "kind for {name}");
                assert_eq!(got_title.as_str(), title, "title for {name}");
            }
            other => panic!("expected a ToolCall for {name}, got {other:?}"),
        }
    }
}

#[test]
fn completed_tool_call_update_carries_value_content() {
    let message = serde_json::json!({
        "type": "tool_call",
        "call_id": "c1",
        "name": "Read",
        "status": "completed",
        "args": {"path": "/tmp/file.txt"},
        "result": {"status": "success", "value": {"content": "file body"}}
    });
    let mapping = map(&message);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            raw_input,
        } => {
            assert_eq!(id, "c1");
            assert_eq!(status, "completed");
            assert_eq!(content.as_deref(), Some("file body"));
            assert!(raw_input.is_some());
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn error_tool_call_update_fails_with_stdout_content() {
    let message = serde_json::json!({
        "type": "tool_call",
        "call_id": "c2",
        "name": "Shell",
        "status": "error",
        "args": {"command": "false"},
        "result": {"status": "success", "value": {"stdout": ""}}
    });
    let mapping = map(&message);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            ..
        } => {
            assert_eq!(id, "c2");
            assert_eq!(status, "failed");
            assert_eq!(content.as_deref(), None);
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }

    let stdout = serde_json::json!({
        "type": "tool_call",
        "call_id": "c3",
        "name": "Shell",
        "status": "completed",
        "args": {"command": "echo hi"},
        "result": {"status": "success", "value": {"stdout": "hi\n"}}
    });
    match native_event(&map(&stdout).events[0]) {
        NativeEvent::ToolCallUpdate {
            status, content, ..
        } => {
            assert_eq!(status, "completed");
            assert_eq!(content.as_deref(), Some("hi\n"));
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }

    let error_message = serde_json::json!({
        "type": "tool_call",
        "call_id": "c4",
        "name": "Shell",
        "status": "completed",
        "args": {"command": "nope"},
        "result": {"status": "error", "error": {"message": "boom"}}
    });
    match native_event(&map(&error_message).events[0]) {
        NativeEvent::ToolCallUpdate {
            status, content, ..
        } => {
            assert_eq!(status, "failed");
            assert_eq!(content.as_deref(), Some("boom"));
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn update_todos_maps_to_plan_on_every_status() {
    let args = serde_json::json!({
        "todos": [
            {"content": "do the thing", "status": "inProgress"},
            {"content": "later thing", "status": "pending"},
            {"content": "done thing", "status": "completed"},
            {"content": "dropped thing", "status": "cancelled"}
        ]
    });
    for status in ["running", "completed"] {
        let message = serde_json::json!({
            "type": "tool_call",
            "call_id": "c5",
            "name": "updateTodos",
            "status": status,
            "args": args
        });
        let mapping = map(&message);
        assert_eq!(mapping.events.len(), 1, "one Plan per {status} line");
        match native_event(&mapping.events[0]) {
            NativeEvent::Plan(entries) => assert_eq!(
                entries.as_slice(),
                [
                    ("do the thing".to_string(), "in_progress".to_string()),
                    ("later thing".to_string(), "pending".to_string()),
                    ("done thing".to_string(), "completed".to_string()),
                    ("dropped thing".to_string(), "completed".to_string())
                ]
            ),
            other => panic!("expected a Plan, got {other:?}"),
        }
    }
}

#[test]
fn tool_call_with_missing_or_non_string_id_maps_to_empty() {
    for message in [
        serde_json::json!({"type": "tool_call", "name": "Read", "status": "running"}),
        serde_json::json!({"type": "tool_call", "call_id": 42, "name": "Read", "status": "running"}),
    ] {
        let mapping = map(&message);
        assert!(mapping.events.is_empty(), "{message:?} must map to nothing");
        assert!(!mapping.finished);
    }
}

#[test]
fn result_finished_finishes_without_events() {
    let mapping = map(&serde_json::json!({
        "type": "result",
        "status": "finished",
        "usage": {"inputTokens": 1, "outputTokens": 2, "totalTokens": 3}
    }));
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert!(mapping.events.is_empty());
}

#[test]
fn result_cancelled_finishes_aborted_without_events() {
    let mapping = map(&serde_json::json!({"type": "result", "status": "cancelled"}));
    assert!(mapping.finished);
    assert!(mapping.ended_aborted);
    assert!(mapping.events.is_empty());
}

#[test]
fn result_error_finishes_with_an_error_event() {
    let mapping = map(&serde_json::json!({
        "type": "result",
        "status": "error",
        "error": "run exploded"
    }));
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert_eq!(mapping.events.len(), 1, "expected exactly the error event");
    match &mapping.events[0] {
        AgentEvent::Native(NativeEvent::Error(text)) => assert_eq!(text, "run exploded"),
        other => panic!("expected an Error event, got {other:?}"),
    }

    let fallback = map(&serde_json::json!({"type": "result", "status": "error"}));
    match &fallback.events[0] {
        AgentEvent::Native(NativeEvent::Error(text)) => assert_eq!(text, "cursor run failed"),
        other => panic!("expected an Error event, got {other:?}"),
    }

    let unknown = map(&serde_json::json!({"type": "result", "status": "weird"}));
    assert!(unknown.finished);
    assert_eq!(
        unknown.events.len(),
        1,
        "unrecognized status must be treated as an error"
    );
}
#[test]
fn result_without_a_status_is_an_error() {
    let mapping = map(&serde_json::json!({"type": "result"}));
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert_eq!(mapping.events.len(), 1, "expected exactly the error event");
    match &mapping.events[0] {
        AgentEvent::Native(NativeEvent::Error(text)) => assert_eq!(text, "cursor run failed"),
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn result_with_a_non_string_status_is_an_error() {
    let mapping = map(&serde_json::json!({
        "type": "result",
        "status": 7,
        "error": "boom"
    }));
    assert!(mapping.finished);
    assert_eq!(mapping.events.len(), 1, "expected exactly the error event");
    match &mapping.events[0] {
        AgentEvent::Native(NativeEvent::Error(text)) => assert_eq!(text, "boom"),
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn non_event_lines_map_to_empty() {
    let lines = [
        serde_json::json!({"type": "usage", "usage": {"inputTokens": 1}}),
        serde_json::json!({"type": "status", "status": "RUNNING", "message": ""}),
        serde_json::json!({"type": "task"}),
        serde_json::json!({"type": "system"}),
        serde_json::json!({"type": "user"}),
        serde_json::json!({"type": "request"}),
        serde_json::json!({"type": "totally_unknown"}),
    ];
    for message in lines {
        let mapping = map(&message);
        assert!(mapping.events.is_empty(), "{message:?} must map to nothing");
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn total_on_non_object_and_malformed_inputs() {
    let inputs = [
        Value::Null,
        Value::String("a line".to_string()),
        serde_json::json!({}),
    ];
    for message in inputs {
        let mapping = map(&message);
        assert!(mapping.events.is_empty(), "{message:?} must map to nothing");
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn completed_tool_call_update_carries_value_diff_string() {
    let message = serde_json::json!({
        "type": "tool_call",
        "call_id": "c5",
        "name": "Edit",
        "status": "completed",
        "args": {"path": "/tmp/file.txt"},
        "result": {
            "status": "success",
            "value": {"diffString": "--- a\n+++ b\n-old\n+new"}
        }
    });
    match native_event(&map(&message).events[0]) {
        NativeEvent::ToolCallUpdate {
            status, content, ..
        } => {
            assert_eq!(status, "completed");
            assert_eq!(content.as_deref(), Some("--- a\n+++ b\n-old\n+new"));
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn content_wins_over_diff_string() {
    let message = serde_json::json!({
        "type": "tool_call",
        "call_id": "c6",
        "name": "Edit",
        "status": "completed",
        "args": {"path": "/tmp/file.txt"},
        "result": {
            "status": "success",
            "value": {"content": "edited", "diffString": "--- a\n+++ b"}
        }
    });
    match native_event(&map(&message).events[0]) {
        NativeEvent::ToolCallUpdate { content, .. } => {
            assert_eq!(content.as_deref(), Some("edited"));
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}
