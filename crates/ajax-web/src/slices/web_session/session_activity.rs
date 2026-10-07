use super::{ReportSessionActivity, SessionError};
use ajax_core::{
    adapters::acp_launch_for_agent,
    commands::CommandContext,
    live,
    models::{LiveObservation, LiveStatusKind, TaskId},
    registry::Registry,
};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionActivity {
    TurnStarted,
    AwaitingOperator,
    TurnEnded,
    TurnFailed,
}

impl SessionActivity {
    fn observation(self) -> LiveObservation {
        match self {
            Self::TurnStarted => {
                LiveObservation::new(LiveStatusKind::AgentRunning, "Agent working")
            }
            Self::AwaitingOperator => {
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "Waiting for approval")
            }
            Self::TurnEnded => LiveObservation::new(LiveStatusKind::Done, "Response ready"),
            Self::TurnFailed => LiveObservation::new(LiveStatusKind::Blocked, "Agent stopped"),
        }
    }
}

pub fn record_session_activity<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    activity: SessionActivity,
    now: SystemTime,
) -> Result<(), SessionError> {
    let task_id: TaskId = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .filter(|task| {
            task.skip_interactive_agent() && acp_launch_for_agent(task.selected_agent).is_some()
        })
        .map(|task| task.id.clone())
        .ok_or_else(|| {
            SessionError::protocol(format!("no ACP-capable task for {qualified_handle}"))
        })?;

    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| SessionError::protocol(format!("task disappeared: {qualified_handle}")))?;

    live::apply_authoritative_observation_at(task, activity.observation(), now);
    Ok(())
}

pub(crate) const ACTIVITY_REPORT_MAX_ATTEMPTS: usize = 3;

pub(crate) fn try_report_session_activity(
    report: &Option<ReportSessionActivity>,
    qualified_handle: &str,
    activity: SessionActivity,
) -> Result<(), SessionError> {
    let Some(report) = report else {
        return Ok(());
    };
    for _ in 0..ACTIVITY_REPORT_MAX_ATTEMPTS {
        if report(qualified_handle, activity) {
            return Ok(());
        }
    }
    Err(SessionError::persist(format!(
        "task activity report failed after {ACTIVITY_REPORT_MAX_ATTEMPTS} attempts ({activity:?})"
    )))
}

pub(crate) fn activity_report_transcript_error(error: &SessionError) -> String {
    format!("task activity report failed: {error}")
}

fn activity_for_event(
    event: &super::SessionServerEvent,
    turn_in_flight: bool,
) -> Option<SessionActivity> {
    use super::SessionServerEvent as Event;
    match event {
        Event::PromptAccepted { .. } => Some(SessionActivity::TurnStarted),
        Event::PermissionRequest { .. } | Event::ElicitationRequest { .. } => {
            Some(SessionActivity::AwaitingOperator)
        }
        Event::PermissionResolved { .. } | Event::ElicitationResolved { .. } => {
            Some(SessionActivity::TurnStarted)
        }
        Event::TurnEnd { stop_reason } => Some(
            if stop_reason
                .as_deref()
                .map(str::to_ascii_lowercase)
                .as_deref()
                == Some("error")
            {
                SessionActivity::TurnFailed
            } else {
                SessionActivity::TurnEnded
            },
        ),
        Event::Error { .. } if turn_in_flight => Some(SessionActivity::TurnFailed),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub(crate) struct SessionActivityReporter {
    last: Option<SessionActivity>,
}

impl SessionActivityReporter {
    pub(crate) fn activity_for_event(
        &self,
        event: &super::SessionServerEvent,
    ) -> Option<SessionActivity> {
        let in_flight = matches!(
            self.last,
            Some(SessionActivity::TurnStarted) | Some(SessionActivity::AwaitingOperator)
        );
        let activity = activity_for_event(event, in_flight)?;
        if self.last == Some(activity) {
            return None;
        }
        Some(activity)
    }

    pub(crate) fn commit(&mut self, activity: SessionActivity) {
        self.last = Some(activity);
    }

    #[cfg(test)]
    fn observe(&mut self, event: &super::SessionServerEvent) -> Option<SessionActivity> {
        let activity = self.activity_for_event(event)?;
        self.commit(activity);
        Some(activity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slices::web_session::SessionServerEvent;
    use crate::test_support;
    use ajax_core::ui_state::{derive_operator_status, TaskStatus};

    fn provisioned_context(
    ) -> ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry> {
        let mut task = test_support::fix_login_task();
        task.set_skip_interactive_agent(true);
        test_support::context_with_tasks(&["web"], vec![task])
    }

    fn status_of(
        context: &ajax_core::commands::CommandContext<ajax_core::registry::InMemoryRegistry>,
    ) -> TaskStatus {
        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .expect("task");
        derive_operator_status(task).status
    }

    #[test]
    fn a_turn_in_flight_makes_the_task_read_as_running() {
        let mut context = provisioned_context();

        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::TurnStarted,
            SystemTime::now(),
        )
        .expect("recorded");

        assert_eq!(status_of(&context), TaskStatus::Running);
    }

    #[test]
    fn an_ask_makes_the_task_read_as_waiting() {
        let mut context = provisioned_context();

        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::AwaitingOperator,
            SystemTime::now(),
        )
        .expect("recorded");

        assert_eq!(status_of(&context), TaskStatus::Waiting);
    }

    #[test]
    fn the_turn_ending_clears_the_running_state() {
        let mut context = provisioned_context();
        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::TurnStarted,
            SystemTime::now(),
        )
        .expect("started");

        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::TurnEnded,
            SystemTime::now(),
        )
        .expect("ended");

        assert_ne!(status_of(&context), TaskStatus::Running);
    }

    #[test]
    fn an_error_outside_a_turn_is_not_a_stopped_agent() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::Error {
                message: "model not advertised".to_string(),
            }),
            None
        );
    }

    #[test]
    fn an_error_during_a_turn_stops_the_agent() {
        let mut reporter = reporter();
        reporter.observe(&SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        });

        assert_eq!(
            reporter.observe(&SessionServerEvent::Error {
                message: "ACP process exited".to_string(),
            }),
            Some(SessionActivity::TurnFailed)
        );
    }

    #[test]
    fn an_unchanged_state_reports_once() {
        let mut reporter = reporter();
        let accepted = SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        };

        assert_eq!(
            reporter.observe(&accepted),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(reporter.observe(&accepted), None);
    }

    #[test]
    fn an_interactive_task_refuses_acp_evidence() {
        let mut context =
            test_support::context_with_tasks(&["web"], vec![test_support::fix_login_task()]);

        let error = record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::TurnStarted,
            SystemTime::now(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("no ACP-capable task"), "{error}");
    }

    fn reporter() -> SessionActivityReporter {
        SessionActivityReporter::default()
    }

    #[test]
    fn prompt_acceptance_is_the_turn_starting() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::PromptAccepted {
                client_message_id: "c1".to_string(),
            }),
            Some(SessionActivity::TurnStarted)
        );
    }

    #[test]
    fn an_ask_is_an_actionable_wait_and_its_answer_resumes_work() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&SessionServerEvent::PermissionRequest {
                request_id: "p1".to_string(),
                title: None,
                detail: None,
            }),
            Some(SessionActivity::AwaitingOperator)
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::PermissionResolved {
                request_id: "p1".to_string(),
                approved: true,
            }),
            Some(SessionActivity::TurnStarted)
        );
    }

    #[test]
    fn a_turn_ends_done_and_an_errored_turn_ends_blocked() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::TurnEnd {
                stop_reason: Some("end_turn".to_string()),
            }),
            Some(SessionActivity::TurnEnded)
        );
        assert_eq!(
            reporter().observe(&SessionServerEvent::TurnEnd {
                stop_reason: Some("Error".to_string()),
            }),
            Some(SessionActivity::TurnFailed)
        );
    }

    #[test]
    fn detail_inside_a_turn_reports_nothing() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::ToolCall {
                call_id: "c1".to_string(),
                title: "Read".to_string(),
                kind: "read".to_string(),
                status: "in_progress".to_string(),
                locations: Vec::new(),
                content: Vec::new(),
            }),
            None
        );
    }
}
