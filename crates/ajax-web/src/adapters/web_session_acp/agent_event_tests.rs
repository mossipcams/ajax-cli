//! Unit tests for `agent_event`: variant construction and the `AgentEvent`
//! wrapper over both ACP and native sources.

use super::agent_event::{AgentEvent, NativeEvent};
use super::AcpClientEvent;

#[test]
fn text_and_thought_variants_carry_streamed_chunks() {
    let text = NativeEvent::AgentText("hello".to_string());
    match &text {
        NativeEvent::AgentText(value) => assert_eq!(value, "hello"),
        other => panic!("expected AgentText, got {other:?}"),
    }

    let thought = NativeEvent::AgentThought("thinking...".to_string());
    match &thought {
        NativeEvent::AgentThought(value) => assert_eq!(value, "thinking..."),
        other => panic!("expected AgentThought, got {other:?}"),
    }
}

#[test]
fn tool_call_variants_carry_fields() {
    let call = NativeEvent::ToolCall {
        id: "t1".to_string(),
        title: "ls -la".to_string(),
        kind: "execute".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(serde_json::json!({"command": "ls -la"})),
    };
    match &call {
        NativeEvent::ToolCall {
            id,
            title,
            kind,
            status,
            raw_input,
        } => {
            assert_eq!(id, "t1");
            assert_eq!(title, "ls -la");
            assert_eq!(kind, "execute");
            assert_eq!(status, "in_progress");
            assert_eq!(
                raw_input.as_ref(),
                Some(&serde_json::json!({"command": "ls -la"}))
            );
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }

    let update = NativeEvent::ToolCallUpdate {
        id: "t1".to_string(),
        status: "completed".to_string(),
        content: Some("done".to_string()),
    };
    match &update {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
        } => {
            assert_eq!(id, "t1");
            assert_eq!(status, "completed");
            assert_eq!(content.as_deref(), Some("done"));
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn plan_and_usage_variants_carry_fields() {
    let plan = NativeEvent::Plan(vec![
        ("step a".to_string(), "in_progress".to_string()),
        ("step b".to_string(), "pending".to_string()),
    ]);
    match &plan {
        NativeEvent::Plan(entries) => assert_eq!(entries.len(), 2),
        other => panic!("expected Plan, got {other:?}"),
    }

    let usage = NativeEvent::Usage {
        used: 1200,
        size: 4000,
    };
    match &usage {
        NativeEvent::Usage { used, size } => {
            assert_eq!(*used, 1200);
            assert_eq!(*size, 4000);
        }
        other => panic!("expected Usage, got {other:?}"),
    }
}

#[test]
fn agent_event_wraps_both_sources() {
    let native = AgentEvent::Native(NativeEvent::AgentText("hello".to_string()));
    assert!(matches!(
        native,
        AgentEvent::Native(NativeEvent::AgentText(_))
    ));

    let acp = AgentEvent::Acp(AcpClientEvent::Error("boom".to_string()));
    assert!(matches!(acp, AgentEvent::Acp(AcpClientEvent::Error(_))));
}
