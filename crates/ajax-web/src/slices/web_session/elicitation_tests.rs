use super::replay::{build_attach, pending_elicitation};
use super::transcript::TranscriptLog;
use super::SessionServerEvent;

#[test]
fn pending_elicitation_cleared_after_resolved_answer() {
    let mut log = TranscriptLog::default();
    log.append(vec![
        SessionServerEvent::ElicitationRequest {
            request_id: "e1".to_string(),
            message: "Pick target".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string" }
                }
            }),
        },
        SessionServerEvent::ElicitationResolved {
            request_id: "e1".to_string(),
            action: "accept".to_string(),
        },
    ]);
    assert!(pending_elicitation(&log).is_none());
    let (snapshot, _) = build_attach(
        &log,
        "auto".to_string(),
        false,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(snapshot.pending_elicitation.is_none());
}

#[test]
fn pending_elicitation_survives_open_request_for_snapshot() {
    let mut log = TranscriptLog::default();
    log.append(vec![SessionServerEvent::ElicitationRequest {
        request_id: "e1".to_string(),
        message: "Pick target".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "enum": ["staging", "production"] }
            },
            "required": ["target"]
        }),
    }]);
    let pending = pending_elicitation(&log).expect("pending elicitation");
    assert_eq!(pending.request_id, "e1");
    assert_eq!(pending.message, "Pick target");
    let (snapshot, _) = build_attach(&log, "auto".to_string(), true, None, None, None, None, None);
    assert_eq!(
        snapshot
            .pending_elicitation
            .as_ref()
            .map(|item| item.request_id.as_str()),
        Some("e1")
    );
}
