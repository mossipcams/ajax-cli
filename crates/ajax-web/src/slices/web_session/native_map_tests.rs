//! Equivalence tests for `native_map`: every [`NativeEvent`] variant must map to
//! exactly the same [`SessionServerEvent`] values that `acp_map` produces for
//! the equivalent ACP session update.

use agent_client_protocol::schema::v1::{
    Content, ContentBlock, ContentChunk, Plan as AcpPlan, PlanEntry as AcpPlanEntry,
    PlanEntryPriority, PlanEntryStatus, SessionNotification, SessionUpdate, TextContent,
    ToolCall as AcpToolCall, ToolCallStatus as AcpToolCallStatus, ToolKind as AcpToolKind,
    UsageUpdate as AcpUsageUpdate,
};

use super::acp_map::map_acp_session_notification;
use super::native_map::map_native_event;
use crate::adapters::web_session_acp::NativeEvent;

/// The `acp_map` output for one typed ACP session update, via the same typed
/// mapping (`map_acp_session_notification`) the ACP pipeline uses.
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
