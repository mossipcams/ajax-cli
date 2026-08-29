//! ACP session run-state as task evidence.
//!
//! A provisioned chat task has no agent pane, so the supervisor's pane
//! classifier has nothing true to say about it: the dashboard, the task page,
//! the TUI and `ajax status` all read `Waiting`/`Idle` while the agent is
//! mid-turn. The ACP host is the only observer of that work, so it reports it
//! on the same contract the supervisor uses — a `LiveObservation` applied to
//! the task — rather than the browser inventing a second status.
//!
//! This does not change how status is derived. `LiveStatusKind::AgentRunning`
//! already means "Agent working" and `WaitingForApproval` already means an
//! actionable wait; this slice only supplies the evidence for tasks the pane
//! classifier cannot see.

use super::{ReportSessionActivity, SessionError};
use ajax_core::{
    agent_status::{provider_lifecycle_observation, ActivityKind, PRIMARY_RUN_ID},
    commands::CommandContext,
    live,
    models::TaskId,
    registry::Registry,
};
use std::time::SystemTime;

pub(crate) const ACTIVITY_REPORT_MAX_ATTEMPTS: usize = 3;

/// What the ACP session just became. One variant per transition the host can
/// observe first-hand; nothing here is inferred from a timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionActivity {
    /// A prompt was accepted and a turn is in flight.
    TurnStarted,
    /// The agent is blocked on the operator: permission or elicitation.
    AwaitingOperator,
    /// The turn ended normally or was cancelled.
    TurnEnded,
    /// The turn ended in a typed error.
    TurnFailed,
}

impl SessionActivity {
    fn activity_kind(self) -> ActivityKind {
        match self {
            Self::TurnStarted => ActivityKind::Working,
            Self::AwaitingOperator => ActivityKind::WaitingApproval,
            Self::TurnEnded => ActivityKind::Done,
            Self::TurnFailed => ActivityKind::Failed,
        }
    }
}

/// Apply one ACP transition to the task behind `qualified_handle`.
///
/// Only session-capable (provisioned, ACP-launchable) tasks accept this
/// evidence: an interactive tmux task is the supervisor's to observe, and two
/// producers writing one field is how a status starts oscillating.
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
        .filter(|task| task.skip_interactive_agent())
        .map(|task| task.id.clone())
        .ok_or_else(|| {
            SessionError::protocol(format!("no ACP-capable task for {qualified_handle}"))
        })?;

    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| SessionError::protocol(format!("task disappeared: {qualified_handle}")))?;

    // ProviderLifecycle → reduce_agent_status → authoritative apply. Never
    // `apply_trusted_observation` — TurnEnded → Done would mark Reviewable
    // between turns of the same launch episode.
    let observation = provider_lifecycle_observation(activity.activity_kind(), PRIMARY_RUN_ID, now);
    live::apply_provider_lifecycle_observation_at(task, observation, now);
    Ok(())
}

/// Bounded inline retries without blocking the session loop thread.
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

/// Which transitions on session events are evidence about the agent.
///
/// The host derives these from the same events it appends to JSONL, so task
/// truth and the chat transcript stay aligned even without a browser socket.
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
        // An answered ask puts the agent back to work; the turn did not end.
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
        // `error` is not only a failed turn: a refused model pick, an oversized
        // frame and a spawn complaint all arrive this way while the child keeps
        // running. Only an error during a turn says the agent stopped.
        Event::Error { .. } if turn_in_flight => Some(SessionActivity::TurnFailed),
        _ => None,
    }
}

/// Turns the outbound event stream into task evidence, one report per change.
///
/// Stateful for two reasons: `error` means "the agent stopped" only while a
/// turn is in flight, and a repeated state is not news — each report takes the
/// control lane and persists a registry snapshot, so re-reporting `Running` on
/// every answered permission would be disk traffic describing nothing.
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
    use ajax_core::models::AgentRuntimeStatus;
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

    // Without this the dashboard, task page, TUI and `ajax status` read a
    // pane-derived Waiting through an entire ACP turn: the pane classifier
    // cannot see a provisioned task's agent, and nothing else reported it.
    #[test]
    fn turn_ended_keeps_launch_attempt_open() {
        let mut context = provisioned_context();
        let task_id = TaskId::new("web/fix-login");
        {
            let task = context.registry.get_task_mut(&task_id).unwrap();
            task.agent_attempts
                .push(ajax_core::models::AgentAttempt::new(
                    task.selected_agent,
                    task.worktree_path.display().to_string(),
                ));
        }

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

        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert!(
            task.agent_attempts
                .iter()
                .any(|attempt| attempt.is_open() && attempt.status == AgentRuntimeStatus::Running),
            "TurnEnded must not close the launch episode"
        );
        assert_eq!(
            task.live_status.as_ref().map(|live| live.summary.as_str()),
            Some("done"),
            "ACP host facts use reducer summaries, not a parallel mapper"
        );
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

    // `error` carries model-pick refusals, oversized frames and spawn
    // complaints while the child keeps running. Marking the task Blocked for
    // one of those would report a stopped agent on an idle session.
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

    // Each report takes the control lane and persists a registry snapshot, so
    // an unchanged state must not be re-reported.
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

    // An interactive tmux task is the supervisor's to observe. Two producers
    // writing one field is how a status starts oscillating.
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

    // #1069 regression lives in `session_activity_directory_tests`: append_to_log
    // through TaskSessionDirectory, not a hand-rolled observe+record loop.

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

    #[test]
    fn failed_activity_report_returns_typed_persist_error() {
        use super::ReportSessionActivity;
        let report = Some(std::sync::Arc::new(
            |_qualified_handle: &str, _activity: SessionActivity| false,
        ) as ReportSessionActivity);
        let error =
            try_report_session_activity(&report, "web/fix-login", SessionActivity::TurnStarted)
                .unwrap_err();
        assert!(matches!(error, SessionError::Persist(_)), "{error}");
    }
}
