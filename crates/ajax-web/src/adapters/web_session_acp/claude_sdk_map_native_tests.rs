use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent, NativeImage};
use super::claude_sdk_map::map_sdk_message;

const SESSION_ID: &str = "acp-session-1";

fn native_event(event: &AgentEvent) -> &NativeEvent {
    match event {
        AgentEvent::Native(native) => native,
        other => panic!("expected a Native event, got {other:?}"),
    }
}

#[test]
fn synthetic_assistant_text_maps_to_agent_text_1359() {
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

fn ajax_plan_1357(entries: Value) -> Value {
    serde_json::json!({"type": "ajax_plan", "entries": entries})
}

#[test]
fn ajax_plan_maps_to_plan_event_1357() {
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

    let mapping = map_sdk_message(&ajax_plan_1357(serde_json::json!([])), SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::Plan(plan) => assert!(plan.is_empty()),
        other => panic!("expected a Plan event, got {other:?}"),
    }

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

fn tool_result_message(content: Value) -> Value {
    serde_json::json!({
        "type": "user",
        "parent_tool_use_id": null,
        "message": {
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "t-img", "content": content}]
        }
    })
}

#[test]
fn tool_result_text_and_api_image_map_to_one_update_with_both() {
    let message = tool_result_message(serde_json::json!([
        {"type": "text", "text": "screenshot taken"},
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGVsbG8="}}
    ]));
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            images,
            ..
        } => {
            assert_eq!(id, "t-img");
            assert_eq!(status, "completed");
            assert_eq!(content.as_deref(), Some("screenshot taken"));
            assert_eq!(
                images,
                &vec![NativeImage {
                    data: "aGVsbG8=".to_string(),
                    mime_type: "image/png".to_string(),
                }]
            );
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
}

#[test]
fn tool_result_mcp_shape_image_is_mapped() {
    let message = tool_result_message(serde_json::json!([
        {"type": "image", "mimeType": "image/jpeg", "data": "ZGF0YQ=="}
    ]));
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            content, images, ..
        } => {
            assert_eq!(content.as_deref(), Some(""));
            assert_eq!(
                images,
                &vec![NativeImage {
                    data: "ZGF0YQ==".to_string(),
                    mime_type: "image/jpeg".to_string(),
                }]
            );
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
}

#[test]
fn tool_result_malformed_image_blocks_are_skipped() {
    let message = tool_result_message(serde_json::json!([
        {"type": "image", "source": {"type": "base64", "media_type": "image/png"}},
        {"type": "image", "mimeType": "image/png"},
        {"type": "image", "source": {"type": "base64", "data": "aGVsbG8="}},
        {"type": "image", "mimeType": "image/png", "data": ""},
        {"type": "image"},
        {"type": "text", "text": "still here"}
    ]));
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            content, images, ..
        } => {
            assert_eq!(content.as_deref(), Some("still here"));
            assert!(images.is_empty(), "images: {images:?}");
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
}

#[test]
fn tool_result_string_content_has_no_images() {
    let message = tool_result_message(serde_json::json!("plain output"));
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            content, images, ..
        } => {
            assert_eq!(content.as_deref(), Some("plain output"));
            assert!(images.is_empty(), "images: {images:?}");
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
}
