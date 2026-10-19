//! Native-event mapping tests for [`super::claude_sdk_map`].

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::claude_sdk_map::map_sdk_message;

const SESSION_ID: &str = "acp-session-1";

/// The native event of an event that must be `AgentEvent::Native`, failing on
/// anything else (an ACP-wrapped event would fail the test here).
fn native_event(event: &AgentEvent) -> &NativeEvent {
    match event {
        AgentEvent::Native(native) => native,
        other => panic!("expected a Native event, got {other:?}"),
    }
}

#[test]
fn synthetic_assistant_text_maps_to_agent_text_1359() {
    // #1359: local slash-command output (e.g. `/context`) arrives as one full
    // assistant message whose `message.model` is `"<synthetic>"`, with text
    // blocks and no preceding stream deltas; the blocks join into exactly one
    // AgentText event.
    let message = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {
            "model": "<synthetic>",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "## Context Usage"},
                {"type": "text", "text": "| Section | Tokens |"}
            ]
        }
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::AgentText(text) => {
            assert_eq!(text, "## Context Usage\n| Section | Tokens |");
        }
        other => panic!("expected an AgentText event, got {other:?}"),
    }
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);

    // A real-model assistant message must not re-emit its text: it already
    // arrived via stream_event deltas, and a second copy would duplicate it.
    let real_model = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {
            "model": "claude-sonnet-4-6",
            "role": "assistant",
            "content": [{"type": "text", "text": "already streamed"}]
        }
    });
    let mapping = map_sdk_message(&real_model, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);

    // A sub-agent assistant message emits no AgentText.
    let sub_agent = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": "toolu_task",
        "message": {
            "model": "<synthetic>",
            "role": "assistant",
            "content": [{"type": "text", "text": "sub-agent output"}]
        }
    });
    let mapping = map_sdk_message(&sub_agent, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);

    // A synthetic message carrying only a tool_use block emits no AgentText;
    // the tool_use mapping itself is unchanged.
    let tool_only = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {
            "model": "<synthetic>",
            "role": "assistant",
            "content": [{"type": "tool_use", "id": "t-syn", "name": "Bash", "input": {"command": "ls"}}]
        }
    });
    let mapping = map_sdk_message(&tool_only, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall { id, title, .. } => {
            assert_eq!(id, "t-syn");
            assert_eq!(title, "Bash: ls");
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }

    // A synthetic message with a top-level `error` (e.g. rate-limit) emits no
    // AgentText: the following `result` message already surfaces it as an
    // Error, and re-emitting its text would duplicate it.
    let errored = serde_json::json!({
        "type": "assistant",
        "error": "rate_limit",
        "parent_tool_use_id": null,
        "message": {
            "model": "<synthetic>",
            "role": "assistant",
            "content": [{"type": "text", "text": "You've hit your session limit"}]
        }
    });
    let mapping = map_sdk_message(&errored, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);
}

/// One Claude sidecar `ajax_plan` line (#1357).
fn ajax_plan_1357(entries: Value) -> Value {
    serde_json::json!({"type": "ajax_plan", "entries": entries})
}

#[test]
fn ajax_plan_maps_to_plan_event_1357() {
    // #1357: a sidecar `ajax_plan` line carries the full ordered task list and
    // maps to exactly one Plan event without settling the run.
    let message = ajax_plan_1357(serde_json::json!([
        {"content": "alpha", "status": "completed"},
        {"content": "beta", "status": "pending"}
    ]));
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::Plan(plan) => {
            assert_eq!(
                plan,
                &vec![
                    ("alpha".to_string(), "completed".to_string()),
                    ("beta".to_string(), "pending".to_string())
                ]
            );
        }
        other => panic!("expected a Plan event, got {other:?}"),
    }
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);

    // An empty entries array (all tasks deleted) clears the panel.
    let mapping = map_sdk_message(&ajax_plan_1357(serde_json::json!([])), SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::Plan(plan) => assert!(plan.is_empty()),
        other => panic!("expected a Plan event, got {other:?}"),
    }

    // Entries missing a string `content` or `status` are skipped; the valid
    // entry survives in order.
    let mapping = map_sdk_message(
        &ajax_plan_1357(serde_json::json!([
            {"content": "gamma"},
            {"status": "pending"},
            {"content": 1, "status": "pending"},
            {"content": "delta", "status": "in_progress"}
        ])),
        SESSION_ID,
    );
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::Plan(plan) => {
            assert_eq!(
                plan,
                &vec![("delta".to_string(), "in_progress".to_string())]
            );
        }
        other => panic!("expected a Plan event, got {other:?}"),
    }

    // A missing or non-array `entries` field emits nothing.
    let missing = serde_json::json!({"type": "ajax_plan"});
    let mapping = map_sdk_message(&missing, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);

    let not_array = serde_json::json!({
        "type": "ajax_plan",
        "entries": {"content": "x", "status": "y"}
    });
    let mapping = map_sdk_message(&not_array, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);
}
