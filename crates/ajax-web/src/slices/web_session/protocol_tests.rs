use super::protocol::{
    parse_client_cursor, SessionChrome, SessionEventEnvelope, SessionSnapshot,
    SESSION_PROTOCOL_VERSION,
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

    let empty = base.clone().with_transcript(Vec::new());
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
