//! Task evidence reporting and transcript durability faults for one session slot.

use super::session_activity::{activity_report_transcript_error, try_report_session_activity};
use super::{
    ReportSessionActivity, SessionActivity, SessionActivityReporter, SessionError,
    SessionServerEvent,
};

pub(super) struct SessionEvidence {
    /// Dedupes ACP run-state transitions for task evidence for this slot.
    pub activity_reporter: SessionActivityReporter,
    /// Activity that failed to persist on a prior append; retried before new events.
    pub pending_activity_report: Option<SessionActivity>,
    pub report_activity: Option<ReportSessionActivity>,
    pub activity_report_fault: Option<String>,
    /// Set when task activity report fails; next collect_outbound emits transcriptError.
    pub pending_activity_report_error_snapshot: bool,
    /// Dedupes identical spawn-class transcript errors across reconnect ([#1040]).
    pub last_logged_spawn_error_id: Option<String>,
    /// Set when transcript append fails; blocks new prompts until operator reset.
    pub transcript_durability_fault: Option<String>,
    /// Set when transcript append fails; next collect_outbound emits transcriptError.
    pub pending_transcript_error_snapshot: bool,
}

impl SessionEvidence {
    pub(super) fn should_skip_duplicate_spawn_error(
        &mut self,
        generation: u64,
        event: &SessionServerEvent,
    ) -> bool {
        let SessionServerEvent::Error { message } = event else {
            return false;
        };
        let Some(id) = SessionError::spawn_error_id(generation, message) else {
            return false;
        };
        if self.last_logged_spawn_error_id.as_deref() == Some(id.as_str()) {
            return true;
        }
        self.last_logged_spawn_error_id = Some(id);
        false
    }

    pub(super) fn note_activity_report_failure(&mut self, error: &SessionError) {
        let message = activity_report_transcript_error(error);
        self.activity_report_fault = Some(message);
        self.pending_activity_report_error_snapshot = true;
    }

    pub(super) fn flush_pending_activity_report(&mut self, qualified_handle: &str) {
        let Some(pending) = self.pending_activity_report else {
            return;
        };
        match try_report_session_activity(&self.report_activity, qualified_handle, pending) {
            Ok(()) => {
                self.activity_reporter.commit(pending);
                self.pending_activity_report = None;
                self.activity_report_fault = None;
            }
            Err(error) => self.note_activity_report_failure(&error),
        }
    }

    pub(super) fn report_activity_for_event(
        &mut self,
        qualified_handle: &str,
        event: &SessionServerEvent,
    ) {
        let Some(activity) = self.activity_reporter.activity_for_event(event) else {
            return;
        };
        match try_report_session_activity(&self.report_activity, qualified_handle, activity) {
            Ok(()) => {
                self.activity_reporter.commit(activity);
                self.activity_report_fault = None;
            }
            Err(error) => {
                self.note_activity_report_failure(&error);
                self.pending_activity_report = Some(activity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::session_activity::SessionActivity;
    use super::*;

    #[test]
    fn activity_report_failure_does_not_set_transcript_durability_fault() {
        let mut evidence = SessionEvidence {
            activity_reporter: SessionActivityReporter::default(),
            pending_activity_report: None,
            report_activity: Some(std::sync::Arc::new(
                |_qualified_handle: &str, _activity: SessionActivity| false,
            )),
            activity_report_fault: None,
            pending_activity_report_error_snapshot: false,
            last_logged_spawn_error_id: None,
            transcript_durability_fault: None,
            pending_transcript_error_snapshot: false,
        };
        let error = SessionError::persist("task activity report failed");
        evidence.note_activity_report_failure(&error);
        assert!(evidence.activity_report_fault.is_some());
        assert!(evidence.pending_activity_report_error_snapshot);
        assert!(evidence.transcript_durability_fault.is_none());
        assert!(!evidence.pending_transcript_error_snapshot);
    }

    #[test]
    fn successful_flush_clears_activity_report_fault() {
        let mut evidence = SessionEvidence {
            activity_reporter: SessionActivityReporter::default(),
            pending_activity_report: Some(SessionActivity::TurnEnded),
            report_activity: Some(std::sync::Arc::new(
                |_qualified_handle: &str, _activity: SessionActivity| true,
            )),
            activity_report_fault: Some("stale fault".to_string()),
            pending_activity_report_error_snapshot: true,
            last_logged_spawn_error_id: None,
            transcript_durability_fault: None,
            pending_transcript_error_snapshot: false,
        };
        evidence.flush_pending_activity_report("web/fix-login");
        assert!(evidence.activity_report_fault.is_none());
        assert!(evidence.pending_activity_report.is_none());
    }
}
