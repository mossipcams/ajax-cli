//! Unit tests for [`super::claude_sdk_map`], replaying trimmed real Claude
//! Agent SDK transcripts from `tests/fixtures/claude_sdk/`.

use std::path::PathBuf;

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::claude_sdk_map::map_sdk_message;
use super::rpc_session::RpcMapping;

const SESSION_ID: &str = "acp-session-1";

/// Load and parse one trimmed transcript fixture.
fn fixture(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude_sdk")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read fixture {path:?}: {error}");
    });
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line must be valid JSON"))
        .collect()
}

/// Map every record of one transcript, keeping the per-message mappings so a
/// test can reason about which message produced which events.
fn replay(records: &[Value]) -> Vec<RpcMapping> {
    records
        .iter()
        .map(|record| map_sdk_message(record, SESSION_ID))
        .collect()
}

/// All events of all mappings, flattened in order.
fn all_events(mappings: &[RpcMapping]) -> Vec<&AgentEvent> {
    mappings
        .iter()
        .flat_map(|mapping| mapping.events.iter())
        .collect()
}

/// The native event of an event that must be `AgentEvent::Native`, failing on
/// anything else (an ACP-wrapped event would fail the test here).
fn native_event(event: &AgentEvent) -> &NativeEvent {
    match event {
        AgentEvent::Native(native) => native,
        other => panic!("expected a Native event, got {other:?}"),
    }
}

#[test]
fn prompt_transcript_maps_text_usage_and_finishes_once() {
    let records = fixture("prompt.jsonl");
    let mappings = replay(&records);
    let events = all_events(&mappings);

    // Every streamed chunk event must be a text/thought native event; the text
    // is emitted exactly once (the assistant message re-emits nothing), and no
    // chunk is empty.
    let mut message_text: Vec<&str> = Vec::new();
    for event in &events {
        match native_event(event) {
            NativeEvent::AgentText(text) => message_text.push(text),
            NativeEvent::AgentThought(thinking) => assert!(!thinking.is_empty()),
            NativeEvent::Usage { .. } => {}
            other => panic!("unexpected event in prompt transcript: {other:?}"),
        }
    }
    let ok_chunks = message_text.iter().filter(|text| **text == "ok").count();
    assert_eq!(
        ok_chunks, 1,
        "expected exactly one 'ok' text chunk, got {message_text:?}"
    );

    // #1334: the result message's `usage` is a whole-turn sum and must not
    // become the context meter; no mapping in the transcript emits a Usage event.
    assert!(
        !events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Usage { .. })),
        "no Usage event may come from SDK messages: {events:?}"
    );

    assert!(
        mappings.iter().all(|mapping| !mapping.ended_aborted),
        "prompt run must not be aborted"
    );
}

#[test]
fn toolcall_transcript_opens_updates_and_completes_a_tool_call() {
    let records = fixture("toolcall.jsonl");
    let mappings = replay(&records);
    let events = all_events(&mappings);

    // The content_block_start opens an Edit tool call in progress.
    let tool_call_position = events
        .iter()
        .position(|event| matches!(native_event(event), NativeEvent::ToolCall { .. }))
        .expect("expected a ToolCall event");
    match native_event(events[tool_call_position]) {
        NativeEvent::ToolCall { kind, status, .. } => {
            assert_eq!(*kind, "edit");
            assert_eq!(*status, "in_progress");
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }

    // #1339: the later assistant tool_use message re-emits a ToolCall carrying
    // the complete raw input and a descriptive title, upgrading the opened row.
    let _assistant_tool_call_position: usize = events[tool_call_position + 1..]
        .iter()
        .position(|event| match native_event(event) {
            NativeEvent::ToolCall {
                title, raw_input, ..
            } => {
                matches!(raw_input,
                    Some(Value::Object(object)) if object.contains_key("file_path"))
                    && title.starts_with("Write: ")
            }
            _ => false,
        })
        .map(|offset| tool_call_position + 1 + offset)
        .expect("expected a later ToolCall with a file_path raw_input");

    // The tool_result settles the call as completed with non-empty text.
    let completed_position = events[tool_call_position + 1..]
        .iter()
        .position(|event| matches!(native_event(event), NativeEvent::ToolCallUpdate { status, .. } if *status == "completed"))
        .map(|offset| tool_call_position + 1 + offset)
        .expect("expected a completed ToolCallUpdate");
    assert!(matches!(
        native_event(events[completed_position]),
        NativeEvent::ToolCallUpdate {
            content: Some(content),
            ..
        } if !content.is_empty()
    ));

    // #1334: no Usage event comes from the transcript (the meter is fed by the
    // client's run-finished hook, not the result message).
    assert!(
        !events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Usage { .. })),
        "no Usage event may come from SDK messages: {events:?}"
    );

    let finished_count = mappings.iter().filter(|mapping| mapping.finished).count();
    assert_eq!(finished_count, 1, "expected exactly one finished mapping");
}

#[test]
fn interrupt_transcript_aborts_then_restarts() {
    let records = fixture("interrupt.jsonl");
    let mappings = replay(&records);
    let finished: Vec<&RpcMapping> = mappings.iter().filter(|mapping| mapping.finished).collect();
    assert_eq!(finished.len(), 2, "expected exactly two finished mappings");

    // The interrupted run is aborted and must not emit a zero usage event.
    assert!(finished[0].ended_aborted, "first run must be aborted");
    assert!(
        !finished[0]
            .events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Usage { .. })),
        "aborted zero-usage run must not emit a Usage event"
    );

    // The restarted run finishes cleanly; no Usage event comes from either
    // result message (context fill is fetched by the client's hook, #1334).
    assert!(!finished[1].ended_aborted, "second run must not be aborted");
    assert!(
        !finished[1]
            .events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Usage { .. })),
        "the restarted run's result must not emit a Usage event"
    );

    // The '[Request interrupted by user]' user message produces no events.
    let interrupt_records: Vec<&Value> = records
        .iter()
        .filter(|record| record.to_string().contains("[Request interrupted by user]"))
        .collect();
    assert!(
        !interrupt_records.is_empty(),
        "fixture must contain the interrupt marker"
    );
    for record in interrupt_records {
        let mapping = map_sdk_message(record, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "interrupt user message must produce no events"
        );
    }
}

#[test]
fn sub_agent_messages_map_to_empty() {
    let message = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": "toolu_123",
        "message": {"role": "assistant", "content": [{"type": "text", "text": "hi"}]}
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.events.is_empty());
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

/// A sub-agent (Task) assistant message carrying one `tool_use` block.
fn sub_agent_tool_use_1342(parent: &str, id: &str, name: &str, input: Value) -> Value {
    serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": parent,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": name, "input": input}
        ]}
    })
}

/// A sub-agent user message carrying one `tool_result` block.
fn sub_agent_tool_result_1342(parent: &str, id: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "user",
        "parent_tool_use_id": parent,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": text}
        ]}
    })
}

#[test]
fn sub_agent_tool_use_opens_a_tool_call_1342() {
    let message = sub_agent_tool_use_1342(
        "toolu_task",
        "t-sub-read",
        "Read",
        serde_json::json!({"file_path": "/work/a.rs"}),
    );
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall {
            id,
            title,
            status,
            raw_input,
            ..
        } => {
            assert_eq!(id, "t-sub-read");
            assert_eq!(title, "Read: /work/a.rs");
            assert_eq!(*status, "in_progress");
            assert!(raw_input.is_some(), "raw input must be carried");
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn sub_agent_tool_result_completes_its_tool_call_1342() {
    let message = sub_agent_tool_result_1342("toolu_task", "t-sub-read", "fn main() {}");
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1, "events: {:?}", mapping.events);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            ..
        } => {
            assert_eq!(id, "t-sub-read");
            assert_eq!(*status, "completed");
            assert_eq!(content.as_deref(), Some("fn main() {}"));
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn sub_agent_text_stream_event_maps_to_empty_1342() {
    let message = serde_json::json!({
        "type": "stream_event",
        "parent_tool_use_id": "toolu_task",
        "event": {
            "type": "content_block_delta",
            "delta": {"type": "text_delta", "text": "sub-agent prose"}
        }
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn sub_agent_result_maps_to_empty_and_never_finishes_1342() {
    let message = serde_json::json!({
        "type": "result",
        "parent_tool_use_id": "toolu_task",
        "subtype": "error_during_execution",
        "is_error": true,
        "result": "sub-agent failed"
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.events.is_empty(), "events: {:?}", mapping.events);
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn interleaved_sub_agent_tool_calls_map_independently_1342() {
    let mappings: Vec<RpcMapping> = [
        sub_agent_tool_use_1342(
            "toolu_task",
            "t-a",
            "Grep",
            serde_json::json!({"pattern": "alpha"}),
        ),
        sub_agent_tool_use_1342(
            "toolu_task",
            "t-b",
            "Bash",
            serde_json::json!({"command": "ls"}),
        ),
        sub_agent_tool_result_1342("toolu_task", "t-b", "listing"),
        sub_agent_tool_result_1342("toolu_task", "t-a", "match"),
    ]
    .iter()
    .map(|message| map_sdk_message(message, SESSION_ID))
    .collect();
    let events = all_events(&mappings);
    let ids: Vec<(&str, &str)> = events
        .iter()
        .map(|event| match native_event(event) {
            NativeEvent::ToolCall { id, status, .. } => (id.as_str(), status.as_str()),
            NativeEvent::ToolCallUpdate { id, status, .. } => (id.as_str(), status.as_str()),
            other => panic!("unexpected event: {other:?}"),
        })
        .collect();
    assert_eq!(
        ids,
        vec![
            ("t-a", "in_progress"),
            ("t-b", "in_progress"),
            ("t-b", "completed"),
            ("t-a", "completed"),
        ]
    );
    assert!(mappings.iter().all(|mapping| !mapping.finished));
}

#[test]
fn sub_agent_todowrite_does_not_replace_the_plan_1342() {
    let sub_agent_todo = sub_agent_tool_use_1342(
        "toolu_task",
        "t-sub-todo",
        "TodoWrite",
        serde_json::json!({"todos": [
            {"content": "sub step", "status": "in_progress", "activeForm": "Doing sub step"}
        ]}),
    );
    let mapping = map_sdk_message(&sub_agent_todo, SESSION_ID);
    assert!(
        !mapping
            .events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Plan(_))),
        "a sub-agent TodoWrite must not emit a Plan event: {:?}",
        mapping.events
    );
    // The tool row itself still shows, like any other sub-agent tool call.
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall { id, title, .. } => {
            assert_eq!(id, "t-sub-todo");
            assert_eq!(title, "TodoWrite");
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }

    // The main loop's TodoWrite still replaces the plan (#1338 unchanged).
    let main_todo = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t-main-todo", "name": "TodoWrite",
             "input": {"todos": [
                 {"content": "main step", "status": "pending", "activeForm": "Doing main step"}
             ]}}
        ]}
    });
    let mapping = map_sdk_message(&main_todo, SESSION_ID);
    assert!(
        mapping.events.iter().any(|event| matches!(
            native_event(event),
            NativeEvent::Plan(entries)
                if entries == &vec![("main step".to_string(), "pending".to_string())]
        )),
        "a main-loop TodoWrite must still emit its Plan event: {:?}",
        mapping.events
    );
}

#[test]
fn main_loop_messages_map_unchanged_1342() {
    let tool_use = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t-main", "name": "Bash", "input": {"command": "ls"}}
        ]}
    });
    let mapping = map_sdk_message(&tool_use, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall { id, title, .. } => {
            assert_eq!(id, "t-main");
            assert_eq!(title, "Bash: ls");
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }

    let text_delta = serde_json::json!({
        "type": "stream_event",
        "parent_tool_use_id": null,
        "event": {
            "type": "content_block_delta",
            "delta": {"type": "text_delta", "text": "main prose"}
        }
    });
    let mapping = map_sdk_message(&text_delta, SESSION_ID);
    assert!(matches!(
        mapping.events.as_slice(),
        [AgentEvent::Native(NativeEvent::AgentText(text))] if text == "main prose"
    ));

    let result = serde_json::json!({
        "type": "result",
        "parent_tool_use_id": null,
        "subtype": "success",
        "is_error": false,
        "stop_reason": "end_turn"
    });
    let mapping = map_sdk_message(&result, SESSION_ID);
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert!(mapping.events.is_empty());
}

#[test]
fn elicitation_request_maps_to_a_native_elicitation_event() {
    let expected_schema = serde_json::json!({"questions": [{"id": "q1"}]});
    let message = serde_json::json!({
        "type": "elicitation_request",
        "id": "req-1234",
        "message": "Confirm the action?",
        "schema": expected_schema
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
    match native_event(&mapping.events[0]) {
        NativeEvent::ElicitationRequest {
            request_id,
            message,
            schema,
        } => {
            assert_eq!(request_id, "req-1234");
            assert_eq!(message, "Confirm the action?");
            assert_eq!(*schema, expected_schema);
        }
        other => panic!("expected an ElicitationRequest event, got {other:?}"),
    }
}

#[test]
fn malformed_elicitation_requests_map_to_empty() {
    for message in [
        // Missing id.
        serde_json::json!({
            "type": "elicitation_request",
            "message": "hi",
            "schema": {"questions": []}
        }),
        // Non-object schema.
        serde_json::json!({
            "type": "elicitation_request",
            "id": "req-9",
            "message": "hi",
            "schema": [1, 2]
        }),
    ] {
        let mapping = map_sdk_message(&message, SESSION_ID);
        assert!(mapping.events.is_empty());
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn tool_result_status_and_content() {
    let failed = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": "boom"}
        ]}
    });
    let mapping = map_sdk_message(&failed, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            raw_input,
        } => {
            assert_eq!(*id, "t1");
            assert_eq!(status.as_str(), "failed");
            assert_eq!(content.as_deref(), Some("boom"));
            assert!(raw_input.is_none());
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }

    let joined = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t2",
             "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}
        ]}
    });
    let mapping = map_sdk_message(&joined, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCallUpdate {
            id,
            status,
            content,
            ..
        } => {
            assert_eq!(*id, "t2");
            assert_eq!(status.as_str(), "completed");
            assert_eq!(content.as_deref(), Some("a\nb"));
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn meaningless_and_non_object_messages_map_to_empty() {
    let messages: Vec<Value> = vec![
        serde_json::json!({"type": "unknown_type"}),
        serde_json::json!({"type": "system", "subtype": "init", "cwd": "/work/path"}),
        serde_json::json!({"type": "rate_limit_event"}),
        Value::String("not an object".to_string()),
        Value::Number(42.into()),
        Value::Null,
    ];
    for message in messages {
        let mapping = map_sdk_message(&message, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "expected no events for {message:?}"
        );
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn non_aborted_error_result_finishes_with_an_error_event() {
    let message = serde_json::json!({
        "type": "result",
        "subtype": "error_max_turns",
        "is_error": true,
        "stop_reason": null
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert_eq!(mapping.events.len(), 1, "expected exactly the error event");
    match &mapping.events[0] {
        AgentEvent::Native(NativeEvent::Error(text)) => assert!(text.contains("error_max_turns")),
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn result_without_usage_finishes_silently() {
    let message = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "stop_reason": "end_turn"
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert!(mapping.events.is_empty());
}

#[test]
fn rate_limited_turn_displays_claude_session_limit_text() {
    // #1287: native transcript d78a4640, line 89, supplies the assistant error.
    // SDK 0.3.296's result path carries its text in SDKResultSuccess.result
    // with is_error=true; Ajax event 60 previously reduced this to "success".
    let assistant = serde_json::json!({
        "type": "assistant",
        "error": "rate_limit",
        "parent_tool_use_id": null,
        "message": {"role": "assistant", "model": "<synthetic>",
        "stop_reason": "stop_sequence", "content": [
            {"type": "text", "text": "You've hit your session limit · resets 6:30pm (America/Chicago)"}
        ]}
    });
    let assistant_mapping = map_sdk_message(&assistant, SESSION_ID);
    assert!(
        assistant_mapping.events.is_empty(),
        "the synthetic assistant message must not emit a duplicate or an error"
    );
    assert!(!assistant_mapping.finished);
    assert!(!assistant_mapping.ended_aborted);

    let result = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": true,
        "result": "You've hit your session limit · resets 6:30pm (America/Chicago)",
        "usage": {
            "input_tokens": 3,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 459829,
            "output_tokens": 0
        },
        "modelUsage": {"claude-sonnet-4-6": {"contextWindow": 1000000}},
        "stop_reason": null
    });
    let mapping = map_sdk_message(&result, SESSION_ID);

    // Existing turn-end behavior is preserved: the result finishes the turn,
    // not aborted, emitting exactly the error event. Its summed `usage` must
    // not become a Usage meter event (#1334).
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert_eq!(
        mapping.events.len(),
        1,
        "expected exactly the error event, no usage event"
    );
    match native_event(&mapping.events[0]) {
        NativeEvent::Error(text) => {
            assert_eq!(
                text,
                "Claude run failed: You've hit your session limit · resets 6:30pm (America/Chicago)"
            );
            assert!(
                text.contains("You've hit your session limit"),
                "the error must display Claude's session-limit text, got: {text}"
            );
            assert!(
                !text.trim_end().ends_with("success"),
                "the error must not reduce to the subtype, got: {text}"
            );
        }
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn error_result_without_usable_text_falls_back_to_subtype() {
    for result in [
        Value::Null,
        serde_json::json!(42),
        serde_json::json!(" \n\t"),
    ] {
        let mapping = map_sdk_message(
            &serde_json::json!({
                "type": "result", "subtype": "error_max_turns",
                "is_error": true, "result": result
            }),
            SESSION_ID,
        );
        assert!(mapping.finished);
        assert!(!mapping.ended_aborted);
        assert_eq!(mapping.events.len(), 1);
        assert!(matches!(
            native_event(&mapping.events[0]),
            NativeEvent::Error(text) if text == "Claude run failed: error_max_turns"
        ));
    }
}

#[test]
fn result_text_does_not_emit_an_error_on_success_or_cancellation() {
    for (subtype, is_error, terminal_reason, aborted) in [
        ("success", false, "completed", false),
        ("error_during_execution", true, "aborted_streaming", true),
    ] {
        let mapping = map_sdk_message(
            &serde_json::json!({
                "type": "result", "subtype": subtype, "is_error": is_error,
                "terminal_reason": terminal_reason, "result": "provider text"
            }),
            SESSION_ID,
        );
        assert!(mapping.finished);
        assert_eq!(mapping.ended_aborted, aborted);
        assert!(mapping.events.is_empty());
    }
}

#[test]
fn result_with_summed_usage_emits_no_usage_event_1334() {
    // #1334: a `result` message whose whole-turn summed `usage` is large and
    // whose cumulative `modelUsage` window is present must not emit a Usage
    // event; the live context meter comes from the client's run-finished hook.
    let message = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 3,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 459829,
            "output_tokens": 1200
        },
        "modelUsage": {"claude-sonnet-4-6": {"contextWindow": 1000000}}
    });
    let mapping = map_sdk_message(&message, SESSION_ID);

    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert!(
        !mapping
            .events
            .iter()
            .any(|event| matches!(native_event(event), NativeEvent::Usage { .. })),
        "a summed-usage result must not emit a Usage event: {:?}",
        mapping.events
    );
}

#[test]
fn todo_write_assistant_message_emits_plan_1338() {
    let message = serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_todo_1", "name": "TodoWrite",
             "input": {"todos": [
                 {"content": "first step",  "status": "completed",   "activeForm": "Finishing first step"},
                 {"content": "second step", "status": "in_progress", "activeForm": "Doing second step"},
                 {"content": "third step",  "status": "pending",     "activeForm": "Doing third step"}
             ]}}
        ]}
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert_eq!(
        mapping.events.len(),
        2,
        "expected the tool row plus a plan: {:?}",
        mapping.events
    );

    // The TodoWrite row itself keeps today's behavior: an in-progress call with
    // the complete raw input (bare name title — no detail field for it).
    match native_event(&mapping.events[0]) {
        NativeEvent::ToolCall {
            id,
            title,
            kind,
            status,
            raw_input,
        } => {
            assert_eq!(id, "toolu_todo_1");
            assert_eq!(title, "TodoWrite");
            assert_eq!(*kind, "think");
            assert_eq!(*status, "in_progress");
            let todos = raw_input
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|object| object.get("todos"))
                .cloned();
            assert!(matches!(todos, Some(Value::Array(items)) if items.len() == 3));
        }
        other => panic!("expected a ToolCall event, got {other:?}"),
    }

    // #1338: additionally emits a Plan with one (content, status) pair per todo,
    // in order.
    match native_event(&mapping.events[1]) {
        NativeEvent::Plan(entries) => assert_eq!(
            entries,
            &vec![
                ("first step".to_string(), "completed".to_string()),
                ("second step".to_string(), "in_progress".to_string()),
                ("third step".to_string(), "pending".to_string())
            ]
        ),
        other => panic!("expected a Plan event, got {other:?}"),
    }
}

#[test]
fn assistant_tool_calls_carry_descriptive_titles_1339() {
    let message = serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t-bash",  "name": "Bash",     "input": {"command": "ls -la /work"}},
            {"type": "tool_use", "id": "t-read",  "name": "Read",     "input": {"file_path": "/work/src/main.rs"}},
            {"type": "tool_use", "id": "t-grep",  "name": "Grep",     "input": {"pattern": "fn main"}},
            {"type": "tool_use", "id": "t-fetch", "name": "WebFetch", "input": {"url": "https://example.com/docs"}},
            {"type": "tool_use", "id": "t-task",  "name": "Task",
             "input": {"description": "Explore layout", "prompt": "list the crates"}}
        ]}
    });
    let mapping = map_sdk_message(&message, SESSION_ID);

    // Each tool_use block emits exactly one ToolCall; the titles are descriptive
    // and the kinds match today's mapping.
    assert_eq!(
        mapping.events.len(),
        5,
        "expected one ToolCall per block: {:?}",
        mapping.events
    );
    let expected = [
        ("t-bash", "Bash: ls -la /work", "execute"),
        ("t-read", "Read: /work/src/main.rs", "read"),
        ("t-grep", "Grep: fn main", "search"),
        ("t-fetch", "WebFetch: https://example.com/docs", "fetch"),
        ("t-task", "Task: Explore layout", "other"),
    ];
    for (event, (id, title, kind)) in mapping.events.iter().zip(expected) {
        match native_event(event) {
            NativeEvent::ToolCall {
                id: row_id,
                title: row_title,
                kind: row_kind,
                status,
                raw_input,
            } => {
                assert_eq!(row_id, id);
                assert_eq!(row_title, title);
                assert_eq!(row_kind, kind);
                assert_eq!(*status, "in_progress");
                assert!(raw_input.is_some(), "raw input must be carried: {event:?}");
            }
            other => panic!("expected a ToolCall event, got {other:?}"),
        }
    }

    // Bare name when the input carries no mapped detail field (e.g. Task with
    // no description).
    let bare = serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "t-bare", "name": "Task", "input": {"prompt": "go"}}
        ]}
    });
    let bare_mapping = map_sdk_message(&bare, SESSION_ID);
    assert_eq!(bare_mapping.events.len(), 1);
    match native_event(&bare_mapping.events[0]) {
        NativeEvent::ToolCall { title, .. } => assert_eq!(title, "Task"),
        other => panic!("expected a ToolCall event, got {other:?}"),
    }
}
