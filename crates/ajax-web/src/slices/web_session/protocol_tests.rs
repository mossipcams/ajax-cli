use super::protocol::{
    parse_client_cursor, parse_tail_snapshot_flag, SessionChrome, SessionEventEnvelope,
    SessionSnapshot, SESSION_PROTOCOL_VERSION,
};
use crate::slices::web_session::SessionServerEvent;

#[test]
fn snapshot_serializes_protocol_v2_fields() {
    let snapshot = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        true,
        false,
        None,
        None,
        SessionChrome::default(),
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["type"], "snapshot");
    assert_eq!(json["protocolVersion"], SESSION_PROTOCOL_VERSION);
    assert_eq!(json["cursor"], 7);
    assert_eq!(json["model"], "composer-2.5");
    assert_eq!(json["turnState"], "busy");
    assert_eq!(json["reset"], false);
}

#[test]
fn event_envelope_wraps_payload() {
    let envelope = SessionEventEnvelope::new(
        3,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    let json = serde_json::to_value(&envelope).unwrap();
    assert_eq!(json["type"], "event");
    assert_eq!(json["protocolVersion"], SESSION_PROTOCOL_VERSION);
    assert_eq!(json["cursor"], 3);
    assert_eq!(json["payload"]["type"], "turn_end");
}

#[test]
fn parse_client_cursor_reads_query_param() {
    assert_eq!(parse_client_cursor(Some("cursor=12&model=auto")), Some(12));
    assert_eq!(parse_client_cursor(Some("model=auto")), None);
}

#[test]
fn v1_message_records_without_item_id_deserialize() {
    let value = serde_json::json!({
        "type": "message",
        "role": "agent",
        "text": "legacy"
    });
    let event: SessionServerEvent = serde_json::from_value(value).unwrap();
    assert!(matches!(
        event,
        SessionServerEvent::Message { text, item_id, .. } if text == "legacy" && item_id.is_empty()
    ));
}

#[test]
fn snapshot_transcript_field_is_omitted_when_none_and_present_when_set() {
    let base = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        false,
        true,
        None,
        None,
        SessionChrome::default(),
    );
    let omitted = serde_json::to_value(&base).unwrap();
    assert!(omitted.get("transcript").is_none());

    let empty = base.clone().with_transcript(Vec::new(), None);
    assert!(empty.transcript_from.is_none());
    let json = serde_json::to_value(&empty).unwrap();
    assert_eq!(json["transcript"], serde_json::json!([]));

    let legacy: SessionSnapshot = serde_json::from_value(serde_json::json!({
        "type": "snapshot",
        "protocolVersion": SESSION_PROTOCOL_VERSION,
        "cursor": 1,
        "model": "m",
        "turnState": "idle",
        "reset": true
    }))
    .unwrap();
    assert!(legacy.transcript.is_none());

    let envelope = SessionEventEnvelope::new(
        3,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    let mut value = serde_json::to_value(&base).unwrap();
    value["transcript"] = serde_json::json!([serde_json::to_value(&envelope).unwrap()]);
    let parsed: SessionSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.transcript, Some(vec![envelope]));
}

#[test]
fn snapshot_transcript_from_serializes_as_transcript_from() {
    let base = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        false,
        true,
        None,
        None,
        SessionChrome::default(),
    );

    // Omitted when unset.
    let omitted = serde_json::to_value(&base).unwrap();
    assert!(omitted.get("transcriptFrom").is_none());

    // Present with the tail start cursor when set.
    let envelope = SessionEventEnvelope::new(
        1703,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    let snapshot = base
        .clone()
        .with_transcript(vec![envelope.clone()], Some(1703));
    assert_eq!(snapshot.transcript_from, Some(1703));
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["transcriptFrom"], 1703);

    // Legacy payload without transcriptFrom parses to None.
    let mut value = serde_json::to_value(&base).unwrap();
    value["transcript"] = serde_json::json!([serde_json::to_value(&envelope).unwrap()]);
    let parsed: SessionSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.transcript, Some(vec![envelope]));
    assert!(parsed.transcript_from.is_none());

    // Explicit transcriptFrom round-trips.
    let mut value = serde_json::to_value(&snapshot).unwrap();
    value["transcriptFrom"] = serde_json::json!(5);
    let parsed: SessionSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.transcript_from, Some(5));
}

#[test]
fn snapshot_transcript_floor_serializes_as_transcript_floor() {
    let base = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        false,
        true,
        None,
        None,
        SessionChrome::default(),
    );

    // Omitted when unset.
    let omitted = serde_json::to_value(&base).unwrap();
    assert!(omitted.get("transcriptFloor").is_none());

    // Present with the retention floor when set, alongside transcript/transcriptFrom.
    let envelope = SessionEventEnvelope::new(
        1703,
        SessionServerEvent::TurnEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    );
    let snapshot = base
        .clone()
        .with_transcript(vec![envelope], Some(1703))
        .with_transcript_floor(5);
    assert_eq!(snapshot.transcript_floor, Some(5));
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["transcriptFloor"], 5);

    // Legacy payload without transcriptFloor parses to None.
    let legacy: SessionSnapshot = serde_json::from_value(serde_json::json!({
        "type": "snapshot",
        "protocolVersion": SESSION_PROTOCOL_VERSION,
        "cursor": 1,
        "model": "m",
        "turnState": "idle",
        "reset": true
    }))
    .unwrap();
    assert!(legacy.transcript_floor.is_none());
}

#[test]
fn tail_snapshot_flag_requires_exact_key_and_value() {
    // Opt-in: exact key and value.
    assert!(parse_tail_snapshot_flag(Some("snapshotTail=1")));

    // Combined with other query pairs.
    assert!(parse_tail_snapshot_flag(Some(
        "cursor=5&snapshotTail=1&model=x"
    )));

    // Other values do not opt in.
    assert!(!parse_tail_snapshot_flag(Some("snapshotTail=0")));
    assert!(!parse_tail_snapshot_flag(Some("snapshotTail=true")));

    // Missing value and valueless pair do not opt in.
    assert!(!parse_tail_snapshot_flag(Some("snapshotTail=")));
    assert!(!parse_tail_snapshot_flag(Some("snapshotTail")));

    // No query at all.
    assert!(!parse_tail_snapshot_flag(None));

    // A key that merely contains the name does not opt in.
    assert!(!parse_tail_snapshot_flag(Some("xsnapshotTail=1")));
}

#[test]
fn prompts_blocked_is_omitted_unless_the_durability_fault_blocks_prompts() {
    let base = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        false,
        true,
        None,
        None,
        SessionChrome::default(),
    )
    .with_transcript_error(Some("activity report failed".to_string()));
    let notice_only = serde_json::to_value(&base).unwrap();
    assert_eq!(notice_only["transcriptError"], "activity report failed");
    assert!(notice_only.get("promptsBlocked").is_none());

    let blocked = base.with_prompts_blocked(true);
    let json = serde_json::to_value(&blocked).unwrap();
    assert_eq!(json["promptsBlocked"], true);

    let mut value = json;
    value.as_object_mut().unwrap().remove("promptsBlocked");
    let parsed: SessionSnapshot = serde_json::from_value(value).unwrap();
    assert!(!parsed.prompts_blocked);
}
