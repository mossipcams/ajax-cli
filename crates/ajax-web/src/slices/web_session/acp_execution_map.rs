//! Translate web session events into neutral ACP execution facts.

use ajax_core::acp_execution_state::{
    AcpExecutionEvent, ProviderState, ToolCallFields, ToolKind, ToolStatus,
};

use super::SessionServerEvent;
pub(super) fn execution_event(
    event: &super::SessionServerEvent,
    turn_in_flight: bool,
) -> AcpExecutionEvent {
    match event {
        SessionServerEvent::PromptAccepted { .. } => AcpExecutionEvent::PromptAccepted,
        SessionServerEvent::ToolCall {
            call_id,
            title,
            kind,
            status,
            ..
        } => AcpExecutionEvent::ToolCallUpdate {
            tool_call_id: call_id.clone(),
            fields: ToolCallFields {
                kind: map_kind(kind),
                status: map_status(status),
                command: None,
                title: if title.is_empty() {
                    None
                } else {
                    Some(title.clone())
                },
            },
        },
        SessionServerEvent::PermissionRequest { request_id, .. } => {
            AcpExecutionEvent::PermissionRequested {
                request_id: request_id.clone(),
            }
        }
        SessionServerEvent::ElicitationRequest { request_id, .. } => {
            AcpExecutionEvent::InputRequested {
                request_id: request_id.clone(),
            }
        }
        SessionServerEvent::PermissionResolved { request_id, .. }
        | SessionServerEvent::ElicitationResolved { request_id, .. } => {
            AcpExecutionEvent::RequestResolved {
                request_id: request_id.clone(),
            }
        }
        SessionServerEvent::TurnEnd { stop_reason } => {
            match stop_reason.as_deref().map(str::to_ascii_lowercase) {
                Some(ref reason) if *reason == "error" => AcpExecutionEvent::TurnFailed,
                Some(ref reason) if *reason == "cancelled" || *reason == "canceled" => {
                    AcpExecutionEvent::TurnCancelled
                }
                _ => AcpExecutionEvent::TurnEnded,
            }
        }
        SessionServerEvent::Error { .. } => {
            if turn_in_flight {
                AcpExecutionEvent::TurnFailed
            } else {
                AcpExecutionEvent::Unknown
            }
        }
        SessionServerEvent::Message { .. } => AcpExecutionEvent::MessageChunk,
        SessionServerEvent::Status { state, .. } => {
            match state.trim().to_ascii_lowercase().as_str() {
                "running" => AcpExecutionEvent::StateUpdate(ProviderState::Running),
                "requires_action" | "waiting" => {
                    AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction)
                }
                "idle" => AcpExecutionEvent::StateUpdate(ProviderState::Idle),
                _ => AcpExecutionEvent::Unknown,
            }
        }
        _ => AcpExecutionEvent::Unknown,
    }
}

fn map_kind(kind: &str) -> Option<ToolKind> {
    if kind.is_empty() {
        None
    } else if kind.eq_ignore_ascii_case("execute") {
        Some(ToolKind::Execute)
    } else {
        Some(ToolKind::Other)
    }
}

fn map_status(status: &str) -> Option<ToolStatus> {
    match status.to_ascii_lowercase().as_str() {
        "pending" => Some(ToolStatus::Pending),
        "in_progress" => Some(ToolStatus::Running),
        "completed" => Some(ToolStatus::Completed),
        "failed" => Some(ToolStatus::Failed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event: SessionServerEvent) -> &'static SessionServerEvent {
        Box::leak(Box::new(event))
    }

    #[test]
    fn prompt_accepted_maps_to_prompt_accepted() {
        let e = event(SessionServerEvent::PromptAccepted {
            client_message_id: "m1".into(),
        });
        assert_eq!(execution_event(e, false), AcpExecutionEvent::PromptAccepted);
    }

    #[test]
    fn tool_call_kind_execute_and_status_mapping() {
        let e = event(SessionServerEvent::ToolCall {
            call_id: "c1".into(),
            title: "run".into(),
            kind: "EXECUTE".into(),
            status: "IN_PROGRESS".into(),
            locations: vec![],
            content: vec![],
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::ToolCallUpdate {
                tool_call_id: "c1".to_string(),
                fields: ToolCallFields {
                    kind: Some(ToolKind::Execute),
                    status: Some(ToolStatus::Running),
                    command: None,
                    title: Some("run".to_string()),
                },
            }
        );

        let e = event(SessionServerEvent::ToolCall {
            call_id: "c2".into(),
            title: "".into(),
            kind: "".into(),
            status: "mystery".into(),
            locations: vec![],
            content: vec![],
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::ToolCallUpdate {
                tool_call_id: "c2".to_string(),
                fields: ToolCallFields {
                    kind: None,
                    status: None,
                    command: None,
                    title: None
                },
            }
        );

        let e = event(SessionServerEvent::ToolCall {
            call_id: "c3".into(),
            title: "t".into(),
            kind: "edit".into(),
            status: "failed".into(),
            locations: vec![],
            content: vec![],
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::ToolCallUpdate {
                tool_call_id: "c3".to_string(),
                fields: ToolCallFields {
                    kind: Some(ToolKind::Other),
                    status: Some(ToolStatus::Failed),
                    command: None,
                    title: Some("t".to_string()),
                },
            }
        );
    }

    #[test]
    fn permission_request_maps_to_permission_requested() {
        let e = event(SessionServerEvent::PermissionRequest {
            request_id: "p1".into(),
            title: None,
            detail: None,
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::PermissionRequested {
                request_id: "p1".to_string()
            }
        );
    }

    #[test]
    fn elicitation_request_maps_to_input_requested() {
        let e = event(SessionServerEvent::ElicitationRequest {
            request_id: "e1".into(),
            message: "m".into(),
            schema: serde_json::json!({}),
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::InputRequested {
                request_id: "e1".to_string()
            }
        );
    }

    #[test]
    fn resolved_requests_map_to_request_resolved() {
        let e = event(SessionServerEvent::PermissionResolved {
            request_id: "p2".into(),
            approved: true,
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::RequestResolved {
                request_id: "p2".to_string()
            }
        );

        let e = event(SessionServerEvent::ElicitationResolved {
            request_id: "e2".into(),
            action: "ok".into(),
        });
        assert_eq!(
            execution_event(e, false),
            AcpExecutionEvent::RequestResolved {
                request_id: "e2".to_string()
            }
        );
    }

    #[test]
    fn turn_end_stop_reasons_map_to_outcomes() {
        for (reason, expected) in [
            ("error", AcpExecutionEvent::TurnFailed),
            ("Cancelled", AcpExecutionEvent::TurnCancelled),
            ("canceled", AcpExecutionEvent::TurnCancelled),
            ("end_turn", AcpExecutionEvent::TurnEnded),
        ] {
            let e = event(SessionServerEvent::TurnEnd {
                stop_reason: Some(reason.into()),
            });
            assert_eq!(execution_event(e, false), expected, "{reason}");
        }

        let e = event(SessionServerEvent::TurnEnd { stop_reason: None });
        assert_eq!(execution_event(e, false), AcpExecutionEvent::TurnEnded);
    }

    #[test]
    fn error_maps_on_turn_in_flight() {
        let e = event(SessionServerEvent::Error {
            message: "boom".into(),
        });
        assert_eq!(execution_event(e, true), AcpExecutionEvent::TurnFailed);
        assert_eq!(execution_event(e, false), AcpExecutionEvent::Unknown);
    }

    #[test]
    fn message_maps_to_message_chunk() {
        let e = event(SessionServerEvent::Message {
            role: "assistant".into(),
            text: "hi".into(),
            content_blocks: vec![],
            item_id: "i1".into(),
            message_id: None,
        });
        assert_eq!(execution_event(e, false), AcpExecutionEvent::MessageChunk);
    }

    #[test]
    fn status_state_maps_to_state_update() {
        let e = event(SessionServerEvent::Status {
            state: "  Running ".into(),
            detail: None,
        });
        assert_eq!(
            execution_event(e, true),
            AcpExecutionEvent::StateUpdate(ProviderState::Running)
        );

        let e = event(SessionServerEvent::Status {
            state: "REQUIRES_ACTION".into(),
            detail: None,
        });
        assert_eq!(
            execution_event(e, true),
            AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction)
        );

        let e = event(SessionServerEvent::Status {
            state: "waiting".into(),
            detail: None,
        });
        assert_eq!(
            execution_event(e, true),
            AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction)
        );

        let e = event(SessionServerEvent::Status {
            state: "idle".into(),
            detail: None,
        });
        assert_eq!(
            execution_event(e, true),
            AcpExecutionEvent::StateUpdate(ProviderState::Idle)
        );

        let e = event(SessionServerEvent::Status {
            state: "unknown".into(),
            detail: None,
        });
        assert_eq!(execution_event(e, true), AcpExecutionEvent::Unknown);

        let e = event(SessionServerEvent::Status {
            state: "".into(),
            detail: None,
        });
        assert_eq!(execution_event(e, true), AcpExecutionEvent::Unknown);
    }
    #[test]
    fn other_variants_map_to_unknown() {
        let e = event(SessionServerEvent::Ready {
            model: "auto".into(),
            busy: false,
        });
        assert_eq!(execution_event(e, true), AcpExecutionEvent::Unknown);

        let e = event(SessionServerEvent::UsageReset);
        assert_eq!(execution_event(e, true), AcpExecutionEvent::Unknown);
    }
}
