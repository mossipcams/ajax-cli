use super::acp_usage::{
    map_prompt_result_usage, parse_turn_usage, turn_usage_dedup_key, turn_usage_event, UsageDeduper,
};
use super::SessionServerEvent;
use serde_json::json;

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
