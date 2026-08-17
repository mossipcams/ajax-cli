use super::acp_drain::{
    coalesce_session_events, map_acp_session_update_with_startup, map_request_finished,
    permission_response,
};
use super::map_acp_session_notification;
use crate::slices::web_session::SessionServerEvent;
use agent_client_protocol::schema::v1::{
    ConfigOptionUpdate, ContentBlock, ContentChunk, CurrentModeUpdate, Plan, PlanEntry,
    PlanEntryPriority, PlanEntryStatus, SessionInfoUpdate, SessionNotification, SessionUpdate,
    TextContent, ToolCall, ToolCallLocation, ToolCallUpdate, UsageUpdate,
};
use serde_json::json;

#[test]
fn finished_prompt_reports_turn_end_with_stop_reason() {
    let event = map_request_finished("session/prompt", Ok(json!({ "stopReason": "end_turn" })));
    assert_eq!(
        event,
        Some(SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        })
    );
}

#[test]
fn finished_non_prompt_request_reports_nothing() {
    assert_eq!(map_request_finished("session/cancel", Ok(json!({}))), None);
}

#[test]
fn failed_request_reports_error() {
    let event = map_request_finished("session/prompt", Err("boom".to_string()));
    assert_eq!(
        event,
        Some(SessionServerEvent::Error {
            message: "boom".to_string(),
        })
    );
}

#[test]
fn drain_maps_session_update_notifications() {
    let update = SessionNotification::new(
        "sess",
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            "hello",
        )))),
    );
    let events = map_acp_session_notification(&update);
    assert_eq!(
        events,
        vec![SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "hello".to_string(),
            item_id: String::new(),
            message_id: None,
        }]
    );
}

#[test]
fn typed_mapper_covers_stable_acp_updates() {
    let notifications = vec![
        SessionNotification::new(
            "sess",
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("thinking"),
            ))),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::ToolCall(
                ToolCall::new("call-1", "Read file")
                    .locations(vec![ToolCallLocation::new("/tmp/file")]),
            ),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("call-1", Default::default())),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
                "Patch the bug",
                PlanEntryPriority::Medium,
                PlanEntryStatus::InProgress,
            )])),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new("default")),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(Vec::new())),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()),
        ),
        SessionNotification::new(
            "sess",
            SessionUpdate::UsageUpdate(UsageUpdate::new(10, 100)),
        ),
    ];
    let events: Vec<_> = notifications
        .iter()
        .flat_map(map_acp_session_notification)
        .collect();

    assert!(events.iter().any(|event| matches!(
        event,
        SessionServerEvent::Message { role, text, .. }
            if role == "thought" && text == "thinking"
    )));
    assert!(events.iter().any(
        |event| matches!(event, SessionServerEvent::ToolCall { call_id, .. } if call_id == "call-1")
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        SessionServerEvent::Plan { entries }
            if entries.first().map(|entry| entry.status.as_str()) == Some("in_progress")
    )));
    assert!(events
        .iter()
        .any(|event| matches!(event, SessionServerEvent::Usage { used, size } if *used == 10 && *size == 100)));
    for kind in ["config", "session_info"] {
        assert!(events
            .iter()
            .any(|event| matches!(event, SessionServerEvent::Artifact { kind: actual, body: Some(_) , .. } if actual == kind)));
    }
}

#[test]
fn consecutive_agent_chunks_are_coalesced_before_persistence() {
    let events = coalesce_session_events(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "hel".to_string(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "lo".to_string(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::TurnEnd { stop_reason: None },
    ]);

    assert_eq!(
        events,
        vec![
            SessionServerEvent::Message {
                role: "agent".to_string(),
                text: "hello".to_string(),
                item_id: "i1".to_string(),
                message_id: None,
            },
            SessionServerEvent::TurnEnd { stop_reason: None },
        ]
    );
}

#[test]
fn pi_startup_info_is_a_note_instead_of_agent_prose() {
    let startup = "pi v0.80.10 ---\nContext\n/repo/AGENTS.md";
    let update = json!({
        "sessionId": "sess",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": startup }
        }
    });

    assert_eq!(
        map_acp_session_update_with_startup(&update, Some(startup)),
        vec![SessionServerEvent::Message {
            role: "note".to_string(),
            text: startup.to_string(),
            item_id: String::new(),
            message_id: None,
        }]
    );
}

#[test]
fn permission_response_matches_779_shape() {
    assert_eq!(
        permission_response(true, Some("because")),
        json!({ "approved": true, "reason": "because" })
    );
}
