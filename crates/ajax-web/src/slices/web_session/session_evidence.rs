use super::session_activity::{activity_report_transcript_error, try_report_session_activity};
use super::task_session_directory::LiveActivitySlot;
use super::{
    ReportSessionActivity, SessionActivity, SessionActivityReporter, SessionError,
    SessionServerEvent,
};
use std::time::{Duration, Instant};

const ACTIVITY_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(1);
const ACTIVITY_RETRY_MAX_DELAY: Duration = Duration::from_secs(60);

pub(super) struct SessionEvidence {
    pub activity_reporter: SessionActivityReporter,
    pub pending_activity_report: Option<SessionActivity>,
    pub report_activity: Option<ReportSessionActivity>,
    pub activity_report_fault: Option<String>,
    pub pending_activity_report_error_snapshot: bool,
    pub last_logged_spawn_error_id: Option<String>,
    pub transcript_durability_fault: Option<String>,
    pub pending_transcript_error_snapshot: bool,
    pub transcript_corruption: Option<String>,
    pub live_activity: Option<LiveActivitySlot>,
    pub activity_retry: Option<(Instant, Duration)>,
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

    pub(super) fn clear_transcript_durability_fault(&mut self) {
        if self.transcript_durability_fault.take().is_some() {
            self.pending_transcript_error_snapshot = true;
        }
    }

    pub(super) fn transcript_error(&self) -> Option<String> {
        self.transcript_durability_fault
            .clone()
            .or_else(|| self.activity_report_fault.clone())
            .or_else(|| self.transcript_corruption.clone())
    }

    pub(super) fn prompts_blocked(&self) -> bool {
        self.transcript_durability_fault.is_some()
    }

    fn clear_activity_report_fault(&mut self) {
        if self.activity_report_fault.take().is_some() {
            self.pending_transcript_error_snapshot = true;
        }
    }

    pub(super) fn note_activity_report_failure(&mut self, error: &SessionError) {
        let message = activity_report_transcript_error(error);
        if self.activity_report_fault.as_deref() != Some(message.as_str()) {
            self.pending_activity_report_error_snapshot = true;
        }
        self.activity_report_fault = Some(message);
    }

    pub(super) fn flush_pending_activity_report(&mut self, qualified_handle: &str) {
        let Some(pending) = self.pending_activity_report else {
            return;
        };
        match try_report_session_activity(&self.report_activity, qualified_handle, pending) {
            Ok(()) => {
                self.activity_reporter.commit(pending);
                self.pending_activity_report = None;
                self.clear_activity_report_fault();
            }
            Err(error) => self.note_activity_report_failure(&error),
        }
    }

    pub(super) fn retry_pending_activity_report(&mut self, qualified_handle: &str, now: Instant) {
        if self.pending_activity_report.is_none() {
            self.activity_retry = None;
            return;
        }
        if self.activity_retry.is_some_and(|(next, _)| now < next) {
            return;
        }
        self.flush_pending_activity_report(qualified_handle);
        if self.pending_activity_report.is_none() {
            self.activity_retry = None;
            return;
        }
        let delay = match self.activity_retry {
            Some((_, previous)) => (previous * 2).min(ACTIVITY_RETRY_MAX_DELAY),
            None => ACTIVITY_RETRY_INITIAL_DELAY,
        };
        self.activity_retry = Some((now + delay, delay));
    }

    #[cfg(test)]
    pub(super) fn report_activity_for_event(
        &mut self,
        qualified_handle: &str,
        event: &SessionServerEvent,
    ) {
        self.report_activity_for_event_with_probe(qualified_handle, event, &mut || Some(false));
    }

    pub(super) fn report_activity_for_event_with_probe(
        &mut self,
        qualified_handle: &str,
        event: &SessionServerEvent,
        probe: &mut dyn FnMut() -> Option<bool>,
    ) {
        let Some(activity) = self
            .activity_reporter
            .activity_for_event_with_probe(event, probe)
        else {
            return;
        };
        self.report(qualified_handle, activity);
    }

    pub(super) fn report(&mut self, qualified_handle: &str, activity: SessionActivity) {
        if let Some(live) = &self.live_activity {
            live.publish(qualified_handle, activity);
        }
        match try_report_session_activity(&self.report_activity, qualified_handle, activity) {
            Ok(()) => {
                self.activity_reporter.commit(activity);
                self.pending_activity_report = None;
                self.clear_activity_report_fault();
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

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
            live_activity: None,
            activity_retry: None,
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
            live_activity: None,
            activity_retry: None,
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
            live_activity: None,
            activity_retry: None,
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
        evidence.retry_pending_activity_report("web/fix-login", Instant::now());

        assert_eq!(
            *reported.lock().unwrap(),
            vec![SessionActivity::TurnEnded],
            "a turn start that was never delivered must not land after the turn end"
        );
        assert!(evidence.pending_activity_report.is_none());
    }

    fn idle_evidence() -> SessionEvidence {
        SessionEvidence {
            activity_reporter: SessionActivityReporter::default(),
            pending_activity_report: None,
            report_activity: Some(std::sync::Arc::new(
                |_qualified_handle: &str, _activity: SessionActivity| true,
            )),
            activity_report_fault: None,
            pending_activity_report_error_snapshot: false,
            last_logged_spawn_error_id: None,
            transcript_durability_fault: None,
            pending_transcript_error_snapshot: false,
            transcript_corruption: None,
            live_activity: None,
            activity_retry: None,
        }
    }

    #[test]
    fn prompts_blocked_is_set_only_for_the_durability_fault() {
        let mut evidence = idle_evidence();
        assert!(!evidence.prompts_blocked());

        evidence.transcript_corruption = Some("stored session is unreadable".to_string());
        assert!(evidence.transcript_error().is_some());
        assert!(!evidence.prompts_blocked());

        evidence.activity_report_fault = Some("activity report failed".to_string());
        assert!(!evidence.prompts_blocked());

        evidence.note_transcript_durability_fault("append failed".to_string());
        assert!(evidence.prompts_blocked());
    }

    #[test]
    fn clearing_the_activity_report_fault_republishes_the_transcript_error_snapshot() {
        let mut evidence = idle_evidence();
        evidence.note_activity_report_failure(&SessionError::persist("activity report failed"));
        evidence.pending_activity_report_error_snapshot = false;
        evidence.pending_transcript_error_snapshot = false;
        evidence.pending_activity_report = Some(SessionActivity::TurnEnded);

        evidence.flush_pending_activity_report("web/fix-login");

        assert!(evidence.transcript_error().is_none());
        assert!(evidence.pending_transcript_error_snapshot);
    }

    fn counting_failing_evidence(calls: &std::sync::Arc<AtomicUsize>) -> SessionEvidence {
        let counter = std::sync::Arc::clone(calls);
        SessionEvidence {
            report_activity: Some(std::sync::Arc::new(
                move |_qualified_handle: &str, _activity: SessionActivity| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    false
                },
            )),
            pending_activity_report: Some(SessionActivity::TurnEnded),
            ..idle_evidence()
        }
    }

    #[test]
    fn persistent_registry_failure_backs_off_and_resends_snapshot_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut evidence = counting_failing_evidence(&calls);
        let start = Instant::now();

        let mut retry_attempts = 0;
        for step in 0..100u32 {
            let now = start + Duration::from_millis(100 * u64::from(step));
            let before = calls.load(Ordering::SeqCst);
            evidence.retry_pending_activity_report("web/fix-login", now);
            if calls.load(Ordering::SeqCst) > before {
                retry_attempts += 1;
            }
            if step == 0 {
                assert!(evidence.pending_activity_report_error_snapshot);
                evidence.pending_activity_report_error_snapshot = false;
            } else {
                assert!(!evidence.pending_activity_report_error_snapshot);
            }
        }

        let report_calls = calls.load(Ordering::SeqCst);
        assert!(report_calls >= 3, "first retry must attempt the report");
        assert!(
            report_calls <= 12,
            "backoff must bound attempts over 10s, got {report_calls}"
        );
        assert_eq!(retry_attempts, 4, "retries at t=0, 1s, 3s and 7s");
        assert!(evidence.pending_activity_report.is_some());
    }

    #[test]
    fn recovered_registry_clears_pending_report_and_backoff() {
        let calls = Arc::new(AtomicUsize::new(0));
        let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let counter = Arc::clone(&calls);
        let gate = Arc::clone(&healthy);
        let mut evidence = SessionEvidence {
            report_activity: Some(Arc::new(
                move |_qualified_handle: &str, _activity: SessionActivity| {
                    counter.fetch_add(1, Ordering::SeqCst);
                    gate.load(Ordering::SeqCst)
                },
            )),
            pending_activity_report: Some(SessionActivity::TurnEnded),
            ..idle_evidence()
        };
        let start = Instant::now();

        evidence.retry_pending_activity_report("web/fix-login", start);
        assert!(evidence.pending_activity_report.is_some());
        assert!(evidence.activity_retry.is_some());

        healthy.store(true, Ordering::SeqCst);
        let calls_before_skipped_retry = calls.load(Ordering::SeqCst);
        evidence.retry_pending_activity_report("web/fix-login", start + Duration::from_millis(500));
        assert_eq!(calls.load(Ordering::SeqCst), calls_before_skipped_retry);
        assert!(evidence.pending_activity_report.is_some());

        evidence.retry_pending_activity_report("web/fix-login", start + Duration::from_secs(1));
        assert!(evidence.pending_activity_report.is_none());
        assert!(evidence.activity_retry.is_none());
        assert!(evidence.activity_report_fault.is_none());

        let calls_after_recovery = calls.load(Ordering::SeqCst);
        evidence.retry_pending_activity_report("web/fix-login", start + Duration::from_secs(2));
        assert_eq!(calls.load(Ordering::SeqCst), calls_after_recovery);
    }
}
