//! Task-scoped ACP host sessions keyed by qualified handle.

use super::client::{AcpClientEvent, AcpStdioClient};
use crate::slices::web_session::{
    map_acp_client_request, map_acp_session_update, SessionServerEvent,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

pub struct WebSessionHub {
    sessions: Mutex<HashMap<String, Arc<Mutex<AcpStdioClient>>>>,
}

impl Default for WebSessionHub {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl WebSessionHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(
        &self,
        qualified_handle: &str,
        worktree_path: &Path,
    ) -> Result<Arc<Mutex<AcpStdioClient>>, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(client) = sessions.get(qualified_handle) {
            return Ok(Arc::clone(client));
        }
        let client = Arc::new(Mutex::new(AcpStdioClient::spawn(worktree_path)?));
        sessions.insert(qualified_handle.to_string(), Arc::clone(&client));
        Ok(client)
    }

    pub fn release(&self, handle: &str) {
        self.sessions.lock().unwrap().remove(handle);
    }
}

pub fn drain_acp_events(client: &AcpStdioClient) -> Vec<SessionServerEvent> {
    let mut events = Vec::new();
    while let Some(event) = client.poll_event() {
        match event {
            AcpClientEvent::SessionUpdate(params) => {
                events.extend(map_acp_session_update(&params));
            }
            AcpClientEvent::ClientRequest { id, method, params } => {
                if let Some(mut mapped) = map_acp_client_request(&method, &params) {
                    if let SessionServerEvent::PermissionRequest { request_id, .. } = &mut mapped {
                        // Prefer the JSON-RPC request id so permission replies match.
                        if let Some(rpc_id) = id.as_str() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_u64() {
                            *request_id = rpc_id.to_string();
                        } else if let Some(rpc_id) = id.as_i64() {
                            *request_id = rpc_id.to_string();
                        }
                    }
                    events.push(mapped);
                }
            }
            AcpClientEvent::Error(message) => {
                events.push(SessionServerEvent::Error { message });
            }
            AcpClientEvent::Exited => {
                events.push(SessionServerEvent::Error {
                    message: "ACP process exited".to_string(),
                });
            }
        }
    }
    events
}

pub fn permission_response(approved: bool, reason: Option<&str>) -> Value {
    json!({
        "approved": approved,
        "reason": reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_release_drops_cached_session() {
        let hub = WebSessionHub::new();
        hub.release("web/fix-login");
        assert!(hub.sessions.lock().unwrap().is_empty());
    }

    #[test]
    fn drain_maps_session_update_notifications() {
        let update = serde_json::json!({
            "sessionId": "sess",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hello" }
            }
        });
        let events = map_acp_session_update(&update);
        assert_eq!(events.len(), 1);
    }
}
