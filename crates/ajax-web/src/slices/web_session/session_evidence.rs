use super::session_activity::{activity_report_transcript_error, try_report_session_activity};
use super::{
    ReportSessionActivity, SessionActivity, SessionActivityReporter, SessionError,
    SessionServerEvent,
};

pub(super) struct SessionEvidence {
    pub activity_reporter: SessionActivityReporter,
    pub pending_activity_report: Option<SessionActivity>,
    pub report_activity: Option<ReportSessionActivity>,
    pub activity_report_fault: Option<String>,
    pub pending_activity_report_error_snapshot: bool,
    pub last_logged_spawn_error_id: Option<String>,
    pub transcript_durability_fault: Option<String>,
    pub pending_transcript_error_snapshot: bool,
    /// Stored rows that could not be read back; shown, but does not block prompts.
    pub transcript_corruption: Option<String>,
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

    pub(super) fn note_transcript_durability_fault(&mut self, reason: String) {
        self.transcript_durability_fault = Some(reason);
        self.pending_transcript_error_snapshot = true;
    }

    pub(super) fn transcript_error(&self) -> Option<String> {
        self.transcript_durability_fault
            .clone()
            .or_else(|| self.activity_report_fault.clone())
            .or_else(|| self.transcript_corruption.clone())
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

    pub(super) fn retry_pending_activity_report(&mut self, qualified_handle: &str) {
        self.flush_pending_activity_report(qualified_handle);
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
                self.pending_activity_report = None;
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
            transcript_corruption: None,
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
            transcript_corruption: None,
        };
        evidence.flush_pending_activity_report("web/fix-login");
        assert!(evidence.activity_report_fault.is_none());
        assert!(evidence.pending_activity_report.is_none());
    }

    #[test]
    fn issue_1136_deferred_turn_start_is_not_replayed_over_a_reported_turn_end() {
        use crate::slices::web_session::session_activity::ACTIVITY_REPORT_MAX_ATTEMPTS;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let busy_calls = Arc::new(AtomicUsize::new(2 * ACTIVITY_REPORT_MAX_ATTEMPTS));
        let reported = Arc::new(Mutex::new(Vec::new()));
        let lane = Arc::clone(&busy_calls);
        let log = Arc::clone(&reported);
        let mut evidence = SessionEvidence {
            activity_reporter: SessionActivityReporter::default(),
            pending_activity_report: None,
            report_activity: Some(Arc::new(move |_handle: &str, activity: SessionActivity| {
                if lane
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                        left.checked_sub(1)
                    })
                    .is_ok()
                {
                    return false;
                }
                log.lock().unwrap().push(activity);
                true
            })),
            activity_report_fault: None,
            pending_activity_report_error_snapshot: false,
            last_logged_spawn_error_id: None,
            transcript_durability_fault: None,
            pending_transcript_error_snapshot: false,
            transcript_corruption: None,
        };

        evidence.report_activity_for_event(
            "web/fix-login",
            &SessionServerEvent::PromptAccepted {
                client_message_id: "prompt-1".to_string(),
            },
        );
        assert_eq!(
            evidence.pending_activity_report,
            Some(SessionActivity::TurnStarted)
        );

        evidence.flush_pending_activity_report("web/fix-login");
        evidence.report_activity_for_event(
            "web/fix-login",
            &SessionServerEvent::TurnEnd { stop_reason: None },
        );
        evidence.retry_pending_activity_report("web/fix-login");

        assert_eq!(
            *reported.lock().unwrap(),
            vec![SessionActivity::TurnEnded],
            "a turn start that was never delivered must not land after the turn end"
        );
        assert!(evidence.pending_activity_report.is_none());
    }
}
