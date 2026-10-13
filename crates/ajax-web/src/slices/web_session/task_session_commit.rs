use super::task_session::TaskSessionState;
use super::{SessionError, SessionServerEvent};

impl TaskSessionState {
    /// Commits a prompt-ledger change and transcript events in one transaction; memory is updated only after the commit succeeds.
    pub(super) fn commit_ledger_with_events(
        &mut self,
        update: impl FnOnce(&mut crate::adapters::web_session_store::prompt_ledger::PromptLedger),
        events: Vec<SessionServerEvent>,
    ) -> Result<(), SessionError> {
        let mut next = self.prompts.prompt_ledger.clone();
        update(&mut next);

        let filtered = self.filter_events_for_log(events);

        crate::adapters::web_session_store::prompt_ledger::try_persist_with_events(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            &self.qualified_handle,
            &next,
            &filtered,
        )
        .map_err(|error| {
            SessionError::persist(format!("failed to persist prompt ownership: {error}"))
        })?;

        self.prompts.prompt_ledger = next;
        self.log.append(filtered);

        Ok(())
    }
}
