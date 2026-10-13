//! Records the live agent child's identity on the run row when a new client is installed.

use super::task_session::{agent_client_name, TaskSessionState};
use crate::adapters::web_session_store;

impl TaskSessionState {
    /// Best-effort: write this host's boot id and the live child's pid plus
    /// start-time token to the run row so a later host can find orphans.
    /// Recording must never fail an attach or respawn; any error is only logged.
    pub(super) fn record_child_identity(&self) {
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
                    tracing::warn!(
                        target: "ajax_web",
                        handle = %self.qualified_handle,
                        pid,
                        "child start-time token unreadable; not recording child identity"
                    );
                    None
                }
            },
        };
        if let Err(error) = web_session_store::record_child_identity(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            &self.qualified_handle,
            agent_client_name(self.agent),
            child,
            self.ownership.boot_id(),
        ) {
            tracing::warn!(
                target: "ajax_web",
                handle = %self.qualified_handle,
                %error,
                "failed to record chat child identity on the run row"
            );
        }
    }
}
