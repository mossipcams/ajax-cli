use super::acp_usage::{
    map_prompt_result_usage, parse_turn_usage, turn_usage_dedup_key, turn_usage_event, UsageDeduper,
};
use super::SessionServerEvent;
use serde_json::json;

#[test]
fn usage_reset_round_trips_without_payload_fields() {
    let wire = json!({ "type": "usage_reset" });
    assert_eq!(
        serde_json::to_value(SessionServerEvent::UsageReset).unwrap(),
        wire
    );
    assert_eq!(
        serde_json::from_value::<SessionServerEvent>(wire).unwrap(),
        SessionServerEvent::UsageReset
    );
}

#[test]
fn context_resets_append_usage_reset_after_the_host_note() {
    use super::test_support::{fake_acp_fixture, scratch_dir, BlockingSessionDirectory};
    use crate::adapters::web_session_acp::with_test_acp_program;
    use crate::adapters::web_session_store;
    use ajax_core::models::AgentClient;

    for switch_harness in [false, true] {
        let dir = scratch_dir("usage-reset");
        let handle = "web/usage-reset";
        let directory = BlockingSessionDirectory::new(dir.clone());
        with_test_acp_program(&fake_acp_fixture(), || {
            directory
                .acquire(handle, &dir, "auto", AgentClient::Cursor)
                .unwrap();
            directory.record(
                handle,
                SessionServerEvent::Usage {
                    used: 90,
                    size: 100,
                },
            );
            let generation = directory.generation(handle);
            let note = if switch_harness {
                directory
                    .runtime_handle()
                    .block_on(directory.inner().reset_harness_context(
                        handle,
                        &dir,
                        AgentClient::Codex,
                        "auto",
                    ))
                    .unwrap();
                "Client switched harness. Context reset."
            } else {
                directory
                    .runtime_handle()
                    .block_on(directory.inner().clear_context(handle, &dir))
                    .unwrap();
                "Context cleared."
            };

            let stored = web_session_store::load::<SessionServerEvent>(&dir, handle);
            let note_index = stored.events.iter().position(|event| matches!(
                event, SessionServerEvent::Message { role, text, .. } if role == "note" && text == note
            )).expect("reset note persisted");
            assert_eq!(
                stored.events.get(note_index + 1),
                Some(&SessionServerEvent::UsageReset)
            );
            let outbound = directory.collect_outbound(handle, 0, generation);
            assert!(outbound
                .events
                .iter()
                .any(|event| event.payload == SessionServerEvent::UsageReset));
            assert!(!outbound
                .events
                .iter()
                .any(|event| matches!(event.payload, SessionServerEvent::Usage { .. })));
        });
        drop(directory);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn attach_replay_keeps_only_usage_after_the_latest_reset_with_absolute_cursors() {
    use super::protocol::SessionChrome;
    use super::replay::build_attach;
    use super::test_support::note;
    use super::transcript::TranscriptLog;

    let turn_usage = turn_usage_event(
        parse_turn_usage(&json!({ "inputTokens": 7 })).unwrap(),
        None,
    );
    let events = vec![
        note("history"),
        SessionServerEvent::Usage {
            used: 90,
            size: 100,
        },
        turn_usage.clone(),
        SessionServerEvent::UsageReset,
        SessionServerEvent::Usage {
            used: 80,
            size: 100,
        },
        turn_usage.clone(),
        note("latest reset"),
        SessionServerEvent::UsageReset,
        SessionServerEvent::Usage { used: 5, size: 100 },
        turn_usage,
    ];
    // Exercise full and incremental replay, including a trimmed log's cursor offset.
    for dropped in [0, 20] {
        let log = TranscriptLog::from_events(events.clone(), dropped);
        for client_cursor in [None, Some(dropped + 2), Some(dropped + 8)] {
            let (snapshot, replayed) = build_attach(
                &log,
                "auto".into(),
                false,
                client_cursor,
                SessionChrome::default(),
            );
            let expected_indices = [0, 3, 6, 7, 8, 9]
                .into_iter()
                .filter(|index| dropped + index >= client_cursor.unwrap_or(0))
                .collect::<Vec<_>>();
            assert_eq!(snapshot.cursor, dropped + events.len());
            assert_eq!(
                replayed.iter().map(|row| row.cursor).collect::<Vec<_>>(),
                expected_indices
                    .iter()
                    .map(|index| dropped + index)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                replayed
                    .iter()
                    .map(|row| row.payload.clone())
                    .collect::<Vec<_>>(),
                expected_indices
                    .iter()
                    .map(|index| events[*index].clone())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn attach_replay_preserves_usage_without_a_reset() {
    use super::protocol::SessionChrome;
    use super::replay::build_attach;
    use super::transcript::TranscriptLog;

    let events = vec![
        SessionServerEvent::Usage {
            used: 90,
            size: 100,
        },
        turn_usage_event(
            parse_turn_usage(&json!({ "inputTokens": 7 })).unwrap(),
            None,
        ),
    ];
    let log = TranscriptLog::from_events(events.clone(), 0);
    let (snapshot, replayed) =
        build_attach(&log, "auto".into(), false, None, SessionChrome::default());
    assert_eq!(snapshot.cursor, events.len());
    assert_eq!(
        replayed
            .into_iter()
            .map(|row| row.payload)
            .collect::<Vec<_>>(),
        events
    );
}

#[test]
fn parse_cursor_camel_case_usage_fields() {
    let usage = parse_turn_usage(&json!({
        "inputTokens": 1200,
        "outputTokens": 340,
        "cacheReadTokens": 800,
        "cacheWriteTokens": 50,
        "totalTokens": 2390
    }))
    .expect("usage");

    assert_eq!(
        usage,
        super::acp_usage::NormalizedTurnUsage {
            input_tokens: Some(1200),
            output_tokens: Some(340),
            cache_read_tokens: Some(800),
            cache_write_tokens: Some(50),
            total_tokens: Some(2390),
        }
    );
}

#[test]
fn parse_cached_camel_case_cache_aliases() {
    let usage = parse_turn_usage(&json!({
        "inputTokens": 1,
        "outputTokens": 2,
        "cachedReadTokens": 30,
        "cachedWriteTokens": 40
    }))
    .expect("usage");

    assert_eq!(usage.cache_read_tokens, Some(30));
    assert_eq!(usage.cache_write_tokens, Some(40));
    assert_eq!(usage.total_tokens, Some(73));
}

#[test]
fn dedup_runs_only_after_successful_parse() {
    let result = json!({
        "stopReason": "end_turn",
        "usage": {
            "requestId": "req-1",
            "cost": { "total": 0.01 }
        }
    });
    let mut deduper = UsageDeduper::default();
    assert!(map_prompt_result_usage(&result, Some(1), &mut deduper).is_none());

    let valid = json!({
        "stopReason": "end_turn",
        "usage": {
            "requestId": "req-1",
            "inputTokens": 10,
            "outputTokens": 5,
            "totalTokens": 15
        }
    });
    assert!(map_prompt_result_usage(&valid, Some(1), &mut deduper).is_some());
}

#[test]
fn parse_snake_case_and_cache_aliases() {
    let usage = parse_turn_usage(&json!({
        "input_tokens": 10,
        "output_tokens": 20,
        "cached_read_tokens": 3,
        "cached_write_tokens": 4,
        "total_tokens": 37
    }))
    .expect("usage");

    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.output_tokens, Some(20));
    assert_eq!(usage.cache_read_tokens, Some(3));
    assert_eq!(usage.cache_write_tokens, Some(4));
    assert_eq!(usage.total_tokens, Some(37));
}

#[test]
fn missing_total_is_summed_from_present_parts() {
    let usage = parse_turn_usage(&json!({
        "inputTokens": 100,
        "outputTokens": 25,
        "cacheReadTokens": 5
    }))
    .expect("usage");

    assert_eq!(usage.total_tokens, Some(130));
}

#[test]
fn absent_usage_object_emits_nothing() {
    assert!(parse_turn_usage(&json!({})).is_none());
    assert!(map_prompt_result_usage(
        &json!({ "stopReason": "end_turn" }),
        Some(7),
        &mut UsageDeduper::default()
    )
    .is_none());
}

#[test]
fn duplicate_usage_is_dropped_by_request_id() {
    let result = json!({
        "stopReason": "end_turn",
        "usage": {
            "requestId": "req-1",
            "inputTokens": 10,
            "outputTokens": 5,
            "totalTokens": 15
        }
    });
    let mut deduper = UsageDeduper::default();
    assert!(map_prompt_result_usage(&result, Some(99), &mut deduper).is_some());
    assert!(map_prompt_result_usage(&result, Some(100), &mut deduper).is_none());
}

#[test]
fn duplicate_usage_is_dropped_by_generation_id_alias() {
    let usage = json!({ "generationId": "gen-2", "inputTokens": 1, "outputTokens": 2 });
    let mut deduper = UsageDeduper::default();
    assert!(parse_turn_usage(&usage).is_some());
    let key = turn_usage_dedup_key(&usage, None);
    assert_eq!(key, "gen-2");
    assert!(deduper.should_emit(&key));
    assert!(!deduper.should_emit(&key));
}

#[test]
fn turn_usage_event_omits_missing_fields_instead_of_zero() {
    let event = turn_usage_event(
        super::acp_usage::NormalizedTurnUsage {
            input_tokens: Some(12),
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_tokens: Some(12),
        },
        None,
    );
    let json = serde_json::to_value(event).expect("json");
    assert_eq!(json.get("inputTokens"), Some(&json!(12)));
    assert_eq!(json.get("outputTokens"), None);
    assert_eq!(json.get("cacheReadTokens"), None);
    assert_eq!(json.get("totalTokens"), Some(&json!(12)));
}

#[test]
fn providers_without_usage_stay_absent_on_the_wire() {
    let mut deduper = UsageDeduper::default();
    assert!(
        map_prompt_result_usage(&json!({ "stopReason": "cancelled" }), Some(1), &mut deduper,)
            .is_none()
    );
    assert!(parse_turn_usage(&json!({ "cost": { "total": 0.01 } })).is_none());
}

#[test]
fn standard_context_usage_update_is_not_turn_usage() {
    use super::map_acp_session_update;
    let events = map_acp_session_update(&json!({
        "update": { "sessionUpdate": "usage_update", "used": 100, "size": 200000 }
    }));
    assert_eq!(
        events,
        vec![SessionServerEvent::Usage {
            used: 100,
            size: 200000,
        }]
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionServerEvent::TurnUsage { .. })));
}
#[test]
fn prompt_result_maps_to_turn_usage_wire_event() {
    let event = map_prompt_result_usage(
        &json!({
            "stopReason": "end_turn",
            "usage": { "inputTokens": 3, "outputTokens": 4, "totalTokens": 7 }
        }),
        Some(42),
        &mut UsageDeduper::default(),
    )
    .expect("event");

    assert_eq!(
        event,
        SessionServerEvent::TurnUsage {
            request_id: Some("42".to_string()),
            input_tokens: Some(3),
            output_tokens: Some(4),
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_tokens: Some(7),
        }
    );
}
