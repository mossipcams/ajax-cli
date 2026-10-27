use agent_client_protocol::schema::v1::{
    Content, ContentBlock, ContentChunk, Plan as AcpPlan, PlanEntry as AcpPlanEntry,
    PlanEntryPriority, PlanEntryStatus, SessionNotification, SessionUpdate, TextContent,
    ToolCall as AcpToolCall, ToolCallStatus as AcpToolCallStatus, ToolKind as AcpToolKind,
    UsageUpdate as AcpUsageUpdate,
};

use super::acp_map::map_acp_session_notification;
use super::native_map::map_native_event;
use crate::adapters::web_session_acp::NativeEvent;

fn acp_events(update: &SessionUpdate) -> Vec<super::SessionServerEvent> {
    let notification = SessionNotification::new("s1".to_string(), update.clone());
    map_acp_session_notification(&notification)
}

#[test]
fn agent_text_matches_acp_message_chunk() {
    assert_eq!(
        map_native_event(&NativeEvent::AgentText("hello".to_string())),
        acp_events(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("hello"))
        ))),
    );
}

#[test]
fn agent_thought_matches_acp_thought_chunk() {
    assert_eq!(
        map_native_event(&NativeEvent::AgentThought("thinking...".to_string())),
        acp_events(&SessionUpdate::AgentThoughtChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("thinking..."))
        ))),
    );
}

#[test]
fn empty_chunks_match_acp_empty_output() {
    assert_eq!(
        map_native_event(&NativeEvent::AgentText(String::new())),
        acp_events(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(""))
        )))
    );
    assert!(map_native_event(&NativeEvent::AgentThought(String::new())).is_empty());
}

#[test]
fn tool_call_with_raw_input_matches_acp_tool_call() {
    let raw_input = serde_json::json!({"command": "ls -la"});
    assert_eq!(
        map_native_event(&NativeEvent::ToolCall {
            id: "t1".to_string(),
            title: "ls -la".to_string(),
            kind: "execute".to_string(),
            status: "in_progress".to_string(),
            raw_input: Some(raw_input.clone()),
        }),
        acp_events(&SessionUpdate::ToolCall(
            AcpToolCall::new("t1", "ls -la")
                .kind(AcpToolKind::Execute)
                .status(AcpToolCallStatus::InProgress)
                .raw_input(raw_input),
        )),
    );
}

#[test]
fn tool_call_without_raw_input_matches_acp_tool_call() {
    assert_eq!(
        map_native_event(&NativeEvent::ToolCall {
            id: "t2".to_string(),
            title: "Read file".to_string(),
            kind: "read".to_string(),
            status: "pending".to_string(),
            raw_input: None,
        }),
        acp_events(&SessionUpdate::ToolCall(
            AcpToolCall::new("t2", "Read file")
                .kind(AcpToolKind::Read)
                .status(AcpToolCallStatus::Pending),
        )),
    );
}

#[test]
fn tool_call_update_with_content_matches_acp_tool_call_update() {
    assert_eq!(
        map_native_event(&NativeEvent::ToolCallUpdate {
            id: "t1".to_string(),
            status: "completed".to_string(),
            content: Some("done".to_string()),
            raw_input: None,
        }),
        acp_events(&SessionUpdate::ToolCallUpdate(
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "t1",
                agent_client_protocol::schema::v1::ToolCallUpdateFields::default()
                    .status(AcpToolCallStatus::Completed)
                    .content(vec![
                        agent_client_protocol::schema::v1::ToolCallContent::Content(Content::new(
                            ContentBlock::Text(TextContent::new("done"))
                        ),)
                    ]),
            )
        )),
    );
}

#[test]
fn tool_call_update_without_content_matches_acp_tool_call_update() {
    assert_eq!(
        map_native_event(&NativeEvent::ToolCallUpdate {
            id: "t1".to_string(),
            status: "failed".to_string(),
            content: None,
            raw_input: None,
        }),
        acp_events(&SessionUpdate::ToolCallUpdate(
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "t1",
                agent_client_protocol::schema::v1::ToolCallUpdateFields::default()
                    .status(AcpToolCallStatus::Failed),
            )
        )),
    );
}

#[test]
fn tool_call_update_with_raw_input_matches_acp_tool_call_update() {
    let raw_input = serde_json::json!({"file_path": "/tmp/a.txt"});
    assert_eq!(
        map_native_event(&NativeEvent::ToolCallUpdate {
            id: "t3".to_string(),
            status: "in_progress".to_string(),
            content: None,
            raw_input: Some(raw_input.clone()),
        }),
        acp_events(&SessionUpdate::ToolCallUpdate(
            agent_client_protocol::schema::v1::ToolCallUpdate::new(
                "t3",
                agent_client_protocol::schema::v1::ToolCallUpdateFields::default()
                    .status(AcpToolCallStatus::InProgress)
                    .raw_input(raw_input),
            )
        )),
    );
}
#[test]
fn plan_matches_acp_plan() {
    assert_eq!(
        map_native_event(&NativeEvent::Plan(vec![
            ("step a".to_string(), "in_progress".to_string()),
            ("  step b  ".to_string(), "pending".to_string()),
        ])),
        acp_events(&SessionUpdate::Plan(AcpPlan::new(vec![
            AcpPlanEntry::new(
                "step a",
                PlanEntryPriority::Medium,
                PlanEntryStatus::InProgress,
            ),
            AcpPlanEntry::new(
                "  step b  ",
                PlanEntryPriority::Medium,
                PlanEntryStatus::Pending
            ),
        ]))),
    );
}

#[test]
fn usage_matches_acp_usage_update() {
    assert_eq!(
        map_native_event(&NativeEvent::Usage {
            used: 1200,
            size: 4000
        }),
        acp_events(&SessionUpdate::UsageUpdate(AcpUsageUpdate::new(1200, 4000))),
    );
}

#[test]
fn zero_size_usage_matches_acp_empty_output() {
    assert_eq!(
        map_native_event(&NativeEvent::Usage { used: 5, size: 0 }),
        acp_events(&SessionUpdate::UsageUpdate(AcpUsageUpdate::new(5, 0))),
    );
}

fn tool_call_content(events: Vec<super::SessionServerEvent>) -> Vec<super::ToolContent> {
    match events.as_slice() {
        [super::SessionServerEvent::ToolCall { content, .. }] => content.clone(),
        other => panic!("expected exactly one ToolCall event, got {other:?}"),
    }
}

#[test]
fn claude_edit_input_produces_diff_on_tool_call() {
    let raw_input = serde_json::json!({
        "file_path": "/tmp/a.txt",
        "old_string": "foo",
        "new_string": "bar"
    });
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCall {
        id: "e1".to_string(),
        title: "Edit /tmp/a.txt".to_string(),
        kind: "edit".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(raw_input),
    }));
    assert_eq!(
        content,
        vec![super::ToolContent::Diff {
            path: "/tmp/a.txt".to_string(),
            old_text: Some("foo".to_string()),
            new_text: "bar".to_string(),
        }]
    );
}

#[test]
fn claude_multiedit_input_produces_one_diff_per_edit() {
    let raw_input = serde_json::json!({
        "file_path": "/tmp/a.txt",
        "edits": [
            { "old_string": "a", "new_string": "b" },
            { "old_text": "c", "newText": "d" }
        ]
    });
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCall {
        id: "e2".to_string(),
        title: "Edit /tmp/a.txt".to_string(),
        kind: "edit".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(raw_input),
    }));
    assert_eq!(
        content,
        vec![
            super::ToolContent::Diff {
                path: "/tmp/a.txt".to_string(),
                old_text: Some("a".to_string()),
                new_text: "b".to_string(),
            },
            super::ToolContent::Diff {
                path: "/tmp/a.txt".to_string(),
                old_text: Some("c".to_string()),
                new_text: "d".to_string(),
            }
        ]
    );
}

#[test]
fn pi_edit_input_produces_diff_on_tool_call() {
    let raw_input = serde_json::json!({
        "file_path": "/tmp/b.rs",
        "oldText": "let x = 1;",
        "newText": "let x = 2;"
    });
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCall {
        id: "e3".to_string(),
        title: "edit /tmp/b.rs".to_string(),
        kind: "edit".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(raw_input),
    }));
    assert_eq!(
        content,
        vec![super::ToolContent::Diff {
            path: "/tmp/b.rs".to_string(),
            old_text: Some("let x = 1;".to_string()),
            new_text: "let x = 2;".to_string(),
        }]
    );
}

#[test]
fn write_style_input_produces_diff_without_old_text() {
    let raw_input = serde_json::json!({
        "file_path": "/tmp/new.txt",
        "content": "fresh file"
    });
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCall {
        id: "e4".to_string(),
        title: "Write /tmp/new.txt".to_string(),
        kind: "edit".to_string(),
        status: "in_progress".to_string(),
        raw_input: Some(raw_input),
    }));
    assert_eq!(
        content,
        vec![super::ToolContent::Diff {
            path: "/tmp/new.txt".to_string(),
            old_text: None,
            new_text: "fresh file".to_string(),
        }]
    );
}

#[test]
fn non_edit_inputs_produce_no_diffs() {
    for raw_input in [
        serde_json::json!({ "command": "ls -la" }),
        serde_json::json!({ "path": "/tmp/a.txt" }),
    ] {
        let content = tool_call_content(map_native_event(&NativeEvent::ToolCall {
            id: "e5".to_string(),
            title: String::new(),
            kind: String::new(),
            status: "in_progress".to_string(),
            raw_input: Some(raw_input),
        }));
        assert!(content.is_empty(), "expected no diffs, got {content:?}");
    }
}

#[test]
fn tool_call_update_with_edit_raw_input_lists_diff_before_text() {
    let raw_input = serde_json::json!({
        "file_path": "/tmp/a.txt",
        "old_string": "foo",
        "new_string": "bar"
    });
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCallUpdate {
        id: "e1".to_string(),
        status: "completed".to_string(),
        content: Some("done".to_string()),
        raw_input: Some(raw_input),
    }));
    assert_eq!(
        content,
        vec![
            super::ToolContent::Diff {
                path: "/tmp/a.txt".to_string(),
                old_text: Some("foo".to_string()),
                new_text: "bar".to_string(),
            },
            super::ToolContent::Text {
                text: "done".to_string()
            }
        ]
    );
}

#[test]
fn tool_call_update_without_raw_input_produces_only_text() {
    let content = tool_call_content(map_native_event(&NativeEvent::ToolCallUpdate {
        id: "e1".to_string(),
        status: "completed".to_string(),
        content: Some("done".to_string()),
        raw_input: None,
    }));
    assert_eq!(
        content,
        vec![super::ToolContent::Text {
            text: "done".to_string()
        }]
    );
}
