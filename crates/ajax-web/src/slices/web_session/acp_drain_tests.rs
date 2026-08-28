use super::acp_drain::{
    coalesce_session_events, map_acp_session_update_with_startup, map_request_finished,
    permission_response, CONNECTION_INTERRUPTED_MESSAGE,
};
use super::map_acp_session_notification;
use crate::slices::web_session::acp_usage::UsageDeduper;
use crate::slices::web_session::SessionServerEvent;
use agent_client_protocol::schema::v1::{
    ConfigOptionUpdate, ContentBlock, ContentChunk, CurrentModeUpdate, Plan, PlanEntry,
    PlanEntryPriority, PlanEntryStatus, SessionInfoUpdate, SessionNotification, SessionUpdate,
    TextContent, ToolCall, ToolCallLocation, ToolCallUpdate, UsageUpdate,
};
use serde_json::json;

#[test]
fn finished_prompt_reports_turn_end_with_stop_reason() {
    let events = map_request_finished(
        "session/prompt",
        Ok(json!({ "stopReason": "end_turn" })),
        Some(1),
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        }]
    );
}

#[test]
fn finished_prompt_with_cursor_usage_emits_turn_usage_before_turn_end() {
    let events = map_request_finished(
        "session/prompt",
        Ok(json!({
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 100,
                "outputTokens": 40,
                "totalTokens": 140
            }
        })),
        Some(9),
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![
            SessionServerEvent::TurnUsage {
                request_id: Some("9".to_string()),
                input_tokens: Some(100),
                output_tokens: Some(40),
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: Some(140),
            },
            SessionServerEvent::TurnEnd {
                stop_reason: Some("end_turn".to_string()),
            },
        ]
    );
}

#[test]
fn finished_non_prompt_request_reports_nothing() {
    assert_eq!(
        map_request_finished(
            "session/cancel",
            Ok(json!({})),
            None,
            &mut UsageDeduper::default(),
            false,
        ),
        Vec::<SessionServerEvent>::new()
    );
}

#[test]
fn failed_request_reports_error() {
    let events = map_request_finished(
        "session/prompt",
        Err("boom".to_string()),
        None,
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![SessionServerEvent::Error {
            message: "boom".to_string(),
        }]
    );
}

#[test]
fn retriable_non_cancel_prompt_failure_maps_to_host_owned_error() {
    let raw = "RetriableError: connection reset by peer";
    let events = map_request_finished(
        "session/prompt",
        Err(raw.to_string()),
        None,
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![SessionServerEvent::Error {
            message: CONNECTION_INTERRUPTED_MESSAGE.to_string(),
        }]
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, SessionServerEvent::Error { message } if message.contains("RetriableError"))),
        "raw RetriableError must not reach the operator"
    );
}

#[test]
fn retriable_non_cancel_non_prompt_failure_maps_to_host_owned_error() {
    let events = map_request_finished(
        "session/set_mode",
        Err("RetriableError: deadline exceeded".to_string()),
        None,
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![SessionServerEvent::Error {
            message: CONNECTION_INTERRUPTED_MESSAGE.to_string(),
        }]
    );
}

#[test]
fn host_requested_cancel_shaped_prompt_failures_report_turn_end_cancelled() {
    let cases = [
        "RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)",
        "Error: RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)",
        "[canceled] http/2 stream closed",
        "context canceled",
        "http/2 stream closed with error code CANCEL (0x8)",
    ];
    for message in cases {
        let events = map_request_finished(
            "session/prompt",
            Err(message.to_string()),
            None,
            &mut UsageDeduper::default(),
            true,
        );
        assert_eq!(
            events,
            vec![SessionServerEvent::TurnEnd {
                stop_reason: Some("cancelled".to_string()),
            }],
            "expected cancelled turn_end for host-requested cancel: {message}"
        );
    }
}

#[test]
fn unsolicited_cancellation_shaped_prompt_failures_report_connection_interrupted() {
    let cases = [
        "RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)",
        "Error: RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)",
        "[canceled] http/2 stream closed",
        "context canceled",
        "rpc error: code = Canceled desc = stream reset by peer",
        "http/2 stream closed with error code CANCEL (0x8)",
    ];
    for message in cases {
        let events = map_request_finished(
            "session/prompt",
            Err(message.to_string()),
            None,
            &mut UsageDeduper::default(),
            false,
        );
        assert_eq!(
            events,
            vec![SessionServerEvent::Error {
                message: CONNECTION_INTERRUPTED_MESSAGE.to_string(),
            }],
            "expected connection interrupted for unsolicited cancel-shaped abort: {message}"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. })),
            "unsolicited cancel-shaped failure must not emit turn_end: {message}"
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SessionServerEvent::Error { message } if message.contains("RetriableError")
            )),
            "raw RetriableError must not reach the operator: {message}"
        );
    }
}

#[test]
fn non_cancel_transport_prompt_failures_report_host_owned_error() {
    let cases = [
        (
            "http/2 stream closed with error code INTERNAL_ERROR (0x2)",
            "http/2 stream closed with error code INTERNAL_ERROR (0x2)",
        ),
        (
            "RetriableError: http/2 stream closed with error code REFUSED_STREAM (0x7)",
            CONNECTION_INTERRUPTED_MESSAGE,
        ),
        (
            "RetriableError: http/2 stream closed with error code INTERNAL_ERROR (0x2)",
            CONNECTION_INTERRUPTED_MESSAGE,
        ),
        (
            "transport error: request aborted by client",
            "transport error: request aborted by client",
        ),
        (
            "http/2 stream closed with error code REFUSED_STREAM (0x7)",
            "http/2 stream closed with error code REFUSED_STREAM (0x7)",
        ),
        ("http/2 stream reset by peer", "http/2 stream reset by peer"),
    ];
    for (message, expected) in cases {
        let events = map_request_finished(
            "session/prompt",
            Err(message.to_string()),
            None,
            &mut UsageDeduper::default(),
            false,
        );
        assert_eq!(
            events,
            vec![SessionServerEvent::Error {
                message: expected.to_string(),
            }],
            "expected typed error for non-cancel transport failure: {message}"
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. })),
            "non-cancel transport failure must not emit turn_end: {message}"
        );
    }
}

#[test]
fn genuine_prompt_failures_still_report_error() {
    for message in ["boom", "model refused", "invalid prompt payload"] {
        let events = map_request_finished(
            "session/prompt",
            Err(message.to_string()),
            None,
            &mut UsageDeduper::default(),
            false,
        );
        assert_eq!(
            events,
            vec![SessionServerEvent::Error {
                message: message.to_string(),
            }],
            "expected error for genuine prompt failure: {message}"
        );
    }
}

#[test]
fn cancellation_shaped_non_prompt_failure_still_reports_error() {
    let events = map_request_finished(
        "session/cancel",
        Err("[canceled] http/2 stream closed".to_string()),
        None,
        &mut UsageDeduper::default(),
        false,
    );
    assert_eq!(
        events,
        vec![SessionServerEvent::Error {
            message: "[canceled] http/2 stream closed".to_string(),
        }]
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
            content_blocks: Vec::new(),
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
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionServerEvent::Status { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionServerEvent::Artifact { kind: actual, .. } if actual == "session_info")));
    assert!(!events.iter().any(
        |event| matches!(event, SessionServerEvent::Artifact { kind, .. } if kind == "config")
    ));
}

#[test]
fn consecutive_agent_chunks_are_coalesced_before_persistence() {
    let events = coalesce_session_events(vec![
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "hel".to_string(),
            content_blocks: Vec::new(),
            item_id: String::new(),
            message_id: None,
        },
        SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "lo".to_string(),
            content_blocks: Vec::new(),
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
                content_blocks: Vec::new(),
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
            content_blocks: Vec::new(),
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
