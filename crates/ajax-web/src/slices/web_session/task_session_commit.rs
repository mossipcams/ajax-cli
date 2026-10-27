use super::task_session::TaskSessionState;
use super::task_session_exit::try_finalize_active_prompt;
use super::{SessionError, SessionServerEvent};
use std::time::{Duration, Instant};

const BACKLOG_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(2);
const BACKLOG_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct BacklogRetryBackoff {
    delay: Option<Duration>,
    next_attempt: Option<Instant>,
}

impl BacklogRetryBackoff {
    pub(super) fn is_due(&self, now: Instant) -> bool {
        self.next_attempt.is_none_or(|next| now >= next)
    }

    pub(super) fn record_failure(&mut self, now: Instant) {
        let delay = self.delay.map_or(BACKLOG_RETRY_INITIAL_DELAY, |delay| {
            (delay * 2).min(BACKLOG_RETRY_MAX_DELAY)
        });
        self.delay = Some(delay);
        self.next_attempt = Some(now + delay);
    }

    pub(super) fn record_failure_if_unscheduled(&mut self, now: Instant) {
        if self.next_attempt.is_none() {
            self.record_failure(now);
        }
    }

    pub(super) fn record_success(&mut self) {
        *self = Self::default();
    }
}

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
        self.report_committed_events(&filtered);
        self.log.mark_superseded(&cursors);
        self.log.append(filtered);
        self.backlog_retry.record_success();

        Ok(())
    }

    fn active_terminal_commits_backlog(&self) -> bool {
        self.prompts
            .active_prompt
            .as_ref()
            .is_some_and(|active| active.terminal.is_some())
    }

    pub(super) fn retry_unpersisted_backlog(&mut self) {
        if self.unpersisted.is_empty() || self.active_terminal_commits_backlog() {
            return;
        }
        let now = Instant::now();
        if !self.backlog_retry.is_due(now) {
            return;
        }
        if self.commit_with_backlog(Vec::new()).is_err() {
            self.backlog_retry.record_failure(now);
        }
    }

    pub(super) fn flush_unpersisted_backlog(&mut self) {
        if self.unpersisted.is_empty() {
            return;
        }
        if self.active_terminal_commits_backlog() {
            try_finalize_active_prompt(self);
        } else {
            let _ = self.commit_with_backlog(Vec::new());
        }
    }

    pub(super) fn warn_if_backlog_dropped(&self) {
        if !self.unpersisted.is_empty() {
            tracing::warn!(
                handle = %self.qualified_handle,
                dropped = self.unpersisted.len(),
                "transcript backlog dropped at shutdown"
            );
        }
    }
}
