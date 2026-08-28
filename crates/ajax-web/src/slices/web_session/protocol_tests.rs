use super::context_continuity::{ContextContinuity, ContextState};
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
        ContextContinuity::default(),
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["type"], "snapshot");
    assert_eq!(json["protocolVersion"], SESSION_PROTOCOL_VERSION);
    assert_eq!(json["cursor"], 7);
    assert_eq!(json["model"], "composer-2.5");
    assert_eq!(json["turnState"], "busy");
    assert_eq!(json["reset"], false);
    assert_eq!(json["contextState"], "live");
    assert_eq!(json["contextEpoch"], 0);
    assert!(json.get("contextError").is_none());
}

#[test]
fn snapshot_serializes_context_continuity_fields() {
    let snapshot = SessionSnapshot::new(
        7,
        "composer-2.5".to_string(),
        true,
        false,
        None,
        None,
        SessionChrome::default(),
        ContextContinuity {
            state: ContextState::Restored,
            epoch: 3,
            error: Some("resume timed out".to_string()),
        },
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["contextState"], "restored");
    assert_eq!(json["contextEpoch"], 3);
    assert_eq!(json["contextError"], "resume timed out");
}

#[test]
fn snapshot_omits_context_error_when_none() {
    let snapshot = SessionSnapshot::new(
        0,
        "auto".to_string(),
        false,
        false,
        None,
        None,
        SessionChrome::default(),
        ContextContinuity {
            state: ContextState::Live,
            epoch: 0,
            error: None,
        },
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["contextState"], "live");
    assert_eq!(json["contextEpoch"], 0);
    assert!(json.get("contextError").is_none());
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
