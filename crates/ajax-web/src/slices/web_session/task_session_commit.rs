use super::task_session::TaskSessionState;
use super::{SessionError, SessionServerEvent};

impl TaskSessionState {
    pub(super) fn commit_ledger_with_events(
        &mut self,
        update: impl FnOnce(&mut crate::adapters::web_session_store::prompt_ledger::PromptLedger),
        events: Vec<SessionServerEvent>,
    ) -> Result<(), SessionError> {
        let mut next = self.prompts.prompt_ledger.clone();
        update(&mut next);

        let mut filtered = self.unpersisted.clone();
        filtered.extend(self.filter_events_for_log(events));

        let cursors = self.log.superseded_cursors_for(&filtered);
        let rewrites: Vec<(usize, SessionServerEvent)> = cursors
            .iter()
            .filter_map(|&c| self.log.marker_for(c).map(|m| (c, m)))
            .collect();
        crate::adapters::web_session_store::prompt_ledger::try_persist_with_events_and_rewrites(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            &self.qualified_handle,
            &next,
            &filtered,
            &rewrites,
        )
        .map_err(|error| {
            SessionError::persist(format!("failed to persist prompt ownership: {error}"))
        })?;

        self.prompts.prompt_ledger = next;
        self.unpersisted.clear();
        self.evidence.clear_transcript_durability_fault();
        self.report_committed_events(&filtered);
        self.log.mark_superseded(&cursors);
        self.log.append(filtered);

        Ok(())
    }
}
