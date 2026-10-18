//! Records the live agent child's identity on the run row when a new client is installed.

use super::task_session::{agent_client_name, TaskSessionState};
use super::SessionError;
use crate::adapters::web_session_store;

impl TaskSessionState {
    /// Write this host's boot id and the live child's pid plus start-time token
    /// to the run row so a later host can find orphans. A live child whose
    /// identity cannot be recorded must fail the attach.
    pub(super) fn record_child_identity(&self) -> Result<(), SessionError> {
        let child = match self
            .acp
            .client
            .as_ref()
            .and_then(|client| client.child_pid())
        {
            None => None,
            Some(pid) => match super::process_identity::start_token(i64::from(pid)) {
                Some(token) => Some((i64::from(pid), token)),
                None => {
                    return Err(SessionError::persist(format!(
                        "child identity could not be recorded: start-time token unreadable for pid {pid}"
                    )));
                }
            },
        };
        web_session_store::record_child_identity(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            &self.qualified_handle,
            agent_client_name(self.agent),
            child,
            self.ownership.boot_id(),
        )
        .map_err(|error| {
            SessionError::persist(format!("child identity could not be recorded: {error}"))
        })
    }
}
