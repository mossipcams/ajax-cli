//! Versioned WebSocket envelopes for orchestration chat (protocol v2).

use super::SessionServerEvent;
use crate::adapters::web_session_acp::ConfigOptionDescriptor;
use serde::{Deserialize, Serialize};

pub const SESSION_PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPermission {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Attach state sent once per logical attach or generation change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    pub cursor: usize,
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "sessionConfigOptions"
    )]
    pub session_config_options: Option<Vec<ConfigOptionDescriptor>>,
    #[serde(rename = "turnState")]
    pub turn_state: String,
    pub reset: bool,
    #[serde(rename = "pendingPermission", skip_serializing_if = "Option::is_none")]
    pub pending_permission: Option<PendingPermission>,
}

impl SessionSnapshot {
    pub fn new(
        cursor: usize,
        model: String,
        busy: bool,
        reset: bool,
        pending_permission: Option<PendingPermission>,
        session_config_options: Option<Vec<ConfigOptionDescriptor>>,
    ) -> Self {
        Self {
            kind: "snapshot".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            cursor,
            model,
            session_config_options,
            turn_state: if busy { "busy" } else { "idle" }.to_string(),
            reset,
            pending_permission,
        }
    }
}

/// One persisted transcript row with its absolute cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEventEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    pub cursor: usize,
    pub payload: SessionServerEvent,
}

impl SessionEventEnvelope {
    pub fn new(cursor: usize, payload: SessionServerEvent) -> Self {
        Self {
            kind: "event".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            cursor,
            payload,
        }
    }
}

pub fn parse_client_cursor(query: Option<&str>) -> Option<usize> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key != "cursor" {
            return None;
        }
        value.parse().ok()
    })
}
