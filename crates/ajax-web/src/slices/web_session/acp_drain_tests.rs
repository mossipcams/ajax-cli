use super::acp_drain::{
    coalesce_session_events, drain_acp_events, map_acp_session_update_with_startup,
    map_request_finished, permission_response, PromptTerminalOutcome,
    CONNECTION_INTERRUPTED_MESSAGE,
};
use super::map_acp_session_notification;
use crate::adapters::web_session_acp::{
    AcpClientEvent, AcpEventSource, AgentEvent, AvailableCommandDescriptor, NativeEvent,
};
use crate::slices::web_session::acp_usage::UsageDeduper;
use crate::slices::web_session::SessionServerEvent;
use agent_client_protocol::schema::v1::{
    AvailableCommand, ConfigOptionUpdate, ContentBlock, ContentChunk, CurrentModeUpdate, Plan,
    PlanEntry, PlanEntryPriority, PlanEntryStatus, SessionInfoUpdate, SessionNotification,
    SessionUpdate, TextContent, ToolCall, ToolCallLocation, ToolCallUpdate, UsageUpdate,
};
use serde_json::json;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

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
    let notifications = [
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

struct FakeAcpSource {
    new_result: serde_json::Value,
    events: RefCell<VecDeque<AgentEvent>>,
}

impl AcpEventSource for FakeAcpSource {
    fn session_new_result(&self) -> &serde_json::Value {
        &self.new_result
    }

    fn poll_event(&self) -> Option<AgentEvent> {
        self.events.borrow_mut().pop_front()
    }
}

fn drain_one(event: AgentEvent) -> super::acp_drain::AcpDrainOutcome {
    let client = FakeAcpSource {
        new_result: json!({}),
        events: RefCell::new(VecDeque::from([event])),
    };
    drain_acp_events(&client, &mut UsageDeduper::default())
}

fn assert_outcomes_match(
    native: super::acp_drain::AcpDrainOutcome,
    acp: super::acp_drain::AcpDrainOutcome,
) {
    assert_eq!(native.events, acp.events);
    assert_eq!(native.host_exited, acp.host_exited);
    assert_eq!(native.prompt_terminals.len(), acp.prompt_terminals.len());
    for (nat, a) in native.prompt_terminals.iter().zip(&acp.prompt_terminals) {
        assert_eq!(nat.request_id, a.request_id);
        assert_eq!(nat.outcome, a.outcome);
        assert_eq!(nat.events, a.events);
    }
}

#[test]
fn native_turn_finished_matches_acp_request_finished() {
    let native = drain_one(AgentEvent::Native(NativeEvent::TurnFinished {
        request_id: 7,
        result: Ok("end_turn".to_string()),
    }));
    let acp = drain_one(AgentEvent::Acp(AcpClientEvent::RequestFinished {
        id: 7,
        method: "session/prompt",
        result: Ok(json!({ "stopReason": "end_turn" })),
    }));
    assert_outcomes_match(native.clone(), acp);
    assert_eq!(native.prompt_terminals.len(), 1);
    let terminal = &native.prompt_terminals[0];
    assert_eq!(terminal.request_id, 7u64);
    assert_eq!(terminal.outcome, PromptTerminalOutcome::Success);
    assert_eq!(
        terminal.events,
        vec![SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string())
        }]
    );
}

#[test]
fn native_turn_rejection_matches_acp() {
    let native = drain_one(AgentEvent::Native(NativeEvent::TurnFinished {
        request_id: 7,
        result: Err("boom".to_string()),
    }));
    let acp = drain_one(AgentEvent::Acp(AcpClientEvent::RequestFinished {
        id: 7,
        method: "session/prompt",
        result: Err("boom".to_string()),
    }));
    assert_outcomes_match(native.clone(), acp);
    assert_eq!(native.prompt_terminals.len(), 1);
    assert_eq!(
        native.prompt_terminals[0].outcome,
        PromptTerminalOutcome::Failed
    );
}

#[test]
fn native_error_matches_acp_error() {
    let native = drain_one(AgentEvent::Native(NativeEvent::Error("x".to_string())));
    let acp = drain_one(AgentEvent::Acp(AcpClientEvent::Error("x".to_string())));
    assert_outcomes_match(native, acp);
}

#[test]
fn claude_error_result_shows_one_error_and_failed_terminal_1275() {
    // The error text is folded into the failed TurnFinished (#1275), so the drain
    // must show it once and settle the prompt as Failed, not as a success.
    let outcome = drain_one(AgentEvent::Native(NativeEvent::TurnFinished {
        request_id: 7,
        result: Err("Claude run failed: API Error: overloaded".to_string()),
    }));

    let error_count = |events: &[SessionServerEvent]| {
        events
            .iter()
            .filter(|event| matches!(event, SessionServerEvent::Error { .. }))
            .count()
    };
    assert_eq!(outcome.prompt_terminals.len(), 1);
    let terminal = &outcome.prompt_terminals[0];
    assert_eq!(terminal.outcome, PromptTerminalOutcome::Failed);
    assert_eq!(
        error_count(&outcome.events) + error_count(&terminal.events),
        1,
        "the error must surface exactly once: {outcome:?}"
    );
    assert_eq!(
        terminal.events,
        vec![SessionServerEvent::Error {
            message: "Claude run failed: API Error: overloaded".to_string()
        }]
    );
}

#[test]
fn native_exit_matches_acp_exit() {
    let native = drain_one(AgentEvent::Native(NativeEvent::Exited));
    let acp = drain_one(AgentEvent::Acp(AcpClientEvent::Exited));
    assert_eq!(native.host_exited, acp.host_exited);
    assert!(native.host_exited);
}

#[test]
fn native_exit_reports_agent_process_exited_1336() {
    let native = drain_one(AgentEvent::Native(NativeEvent::Exited));
    assert!(
        native.host_exited,
        "native exit must mark the host as exited"
    );
    assert_eq!(
        native.events,
        vec![SessionServerEvent::Error {
            message: "Agent process exited".to_string()
        }],
        "a native harness exit must be reported as an agent exit, not an ACP one"
    );

    let acp = drain_one(AgentEvent::Acp(AcpClientEvent::Exited));
    assert!(
        acp.host_exited,
        "an ACP host exit must mark the host as exited"
    );
    assert_eq!(
        acp.events,
        vec![SessionServerEvent::Error {
            message: "ACP process exited".to_string()
        }],
        "the ACP host exit text is unchanged"
    );
}

/// A source whose handshake advertised one slash command, yielded on the
/// first drain only, with no further events unless queued.
struct HandshakeCommandSource {
    events: RefCell<VecDeque<AgentEvent>>,
    commands_taken: Cell<bool>,
}

impl HandshakeCommandSource {
    fn fresh(events: VecDeque<AgentEvent>) -> Self {
        HandshakeCommandSource {
            events: RefCell::new(events),
            commands_taken: Cell::new(false),
        }
    }
}

impl AcpEventSource for HandshakeCommandSource {
    fn session_new_result(&self) -> &serde_json::Value {
        static NEW_RESULT: serde_json::Value = serde_json::Value::Null;
        &NEW_RESULT
    }

    fn poll_event(&self) -> Option<AgentEvent> {
        self.events.borrow_mut().pop_front()
    }

    fn take_available_commands(&self) -> Vec<AvailableCommandDescriptor> {
        if self.commands_taken.replace(true) {
            return Vec::new();
        }
        vec![AvailableCommandDescriptor {
            name: "compact".to_string(),
            description: "Compact the conversation".to_string(),
            input_hint: None,
        }]
    }
}

#[test]
fn handshake_available_commands_set_session_state_without_events() {
    let source = HandshakeCommandSource::fresh(VecDeque::new());
    let outcome = drain_acp_events(&source, &mut UsageDeduper::default());

    assert_eq!(
        outcome.session_available_commands,
        Some(vec![AvailableCommandDescriptor {
            name: "compact".to_string(),
            description: "Compact the conversation".to_string(),
            input_hint: None,
        }]),
        "handshake-advertised commands fill session_available_commands"
    );
    assert!(outcome.events.is_empty());
}

#[test]
fn acp_available_commands_update_replaces_the_handshake_list() {
    let source = HandshakeCommandSource::fresh(VecDeque::from([AgentEvent::Acp(
        AcpClientEvent::AvailableCommandsUpdated {
            available_commands: vec![AvailableCommand::new("extensions", "")],
        },
    )]));
    let outcome = drain_acp_events(&source, &mut UsageDeduper::default());

    assert_eq!(
        outcome.session_available_commands,
        Some(vec![AvailableCommandDescriptor {
            name: "extensions".to_string(),
            description: String::new(),
            input_hint: None,
        }]),
        "the ACP AvailableCommandsUpdated update replaces the handshake list, not merges"
    );
    assert!(outcome.events.is_empty());
}

#[test]
fn handshake_commands_are_delivered_once_across_drains() {
    let source = HandshakeCommandSource::fresh(VecDeque::new());
    let mut deduper = UsageDeduper::default();

    let first = drain_acp_events(&source, &mut deduper);
    assert_eq!(
        first.session_available_commands,
        Some(vec![AvailableCommandDescriptor {
            name: "compact".to_string(),
            description: "Compact the conversation".to_string(),
            input_hint: None,
        }]),
        "the first drain delivers the handshake commands"
    );

    let second = drain_acp_events(&source, &mut deduper);
    assert_eq!(
        second.session_available_commands, None,
        "a second drain must not re-deliver the handshake command snapshot"
    );
    assert!(second.events.is_empty());
}
