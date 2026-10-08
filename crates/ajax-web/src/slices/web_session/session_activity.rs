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

use super::{acp_execution_map, ReportSessionActivity, SessionError};
use ajax_core::{
    acp_execution_state::{AcpExecutionEvent, RootTurnState},
    adapters::acp_launch_for_agent,
    agent_status::{reduce_agent_status, ParentPhase, ReduceInput},
    commands::CommandContext,
    live,
    models::{LiveObservation, LiveStatusKind, TaskId},
    registry::Registry,
};
use std::time::SystemTime;

/// The ACP session's run id as seen by the status reducer.
const ROOT_RUN_ID: &str = "acp-session";

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
    /// A shell command from the turn is executing.
    CommandRunning,
    /// The turn's test run is in progress.
    TestsRunning,
    /// The agent is waiting for the operator to type an answer.
    AwaitingInput,
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
            Self::CommandRunning => {
                LiveObservation::new(LiveStatusKind::CommandRunning, "Running command")
            }
            Self::TestsRunning => {
                LiveObservation::new(LiveStatusKind::TestsRunning, "Running tests")
            }
            Self::AwaitingInput => {
                LiveObservation::new(LiveStatusKind::WaitingForInput, "Waiting for input")
            }
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

    // Authoritative: the host owns the ACP child, so this is first-hand
    // process evidence, not a guess reconciled from screen scraping.
    live::apply_authoritative_observation_at(task, activity.observation(), now);
    Ok(())
}

pub(crate) const ACTIVITY_REPORT_MAX_ATTEMPTS: usize = 3;

/// Bounded retries without sleeping on the per-session command loop.
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

/// Turns the outbound event stream into task evidence, one report per change.
///
/// Stateful for two reasons: `error` means "the agent stopped" only while a
/// turn is in flight, and a repeated state is not news — each report takes the
/// control lane and persists a registry snapshot, so re-reporting `Running` on
/// every answered permission would be disk traffic describing nothing.
#[derive(Debug, Default)]
pub(crate) struct SessionActivityReporter {
    state: ajax_core::acp_execution_state::AcpExecutionState,
    last: Option<SessionActivity>,
}

impl SessionActivityReporter {
    pub(crate) fn activity_for_event(
        &mut self,
        event: &super::SessionServerEvent,
    ) -> Option<SessionActivity> {
        let in_flight = self.state.turn() == RootTurnState::Running;
        let now = SystemTime::now();
        self.state
            .apply(acp_execution_map::execution_event(event, in_flight), now);

        // `Unknown` never overwrites prior live evidence: with no turn in
        // flight there is nothing true to report.
        let projection = reduce_agent_status(ReduceInput {
            now,
            primary_run_id: ROOT_RUN_ID.to_string(),
            process_liveness: None,
            observations: &self.state.observations(ROOT_RUN_ID, now),
        });

        if projection.phase == ParentPhase::Unknown {
            return None;
        }

        let activity = match projection.live.kind {
            LiveStatusKind::AgentRunning => SessionActivity::TurnStarted,
            LiveStatusKind::CommandRunning => SessionActivity::CommandRunning,
            LiveStatusKind::TestsRunning => SessionActivity::TestsRunning,
            LiveStatusKind::WaitingForApproval => SessionActivity::AwaitingOperator,
            LiveStatusKind::WaitingForInput => SessionActivity::AwaitingInput,
            LiveStatusKind::Done => SessionActivity::TurnEnded,
            LiveStatusKind::Blocked | LiveStatusKind::CommandFailed => SessionActivity::TurnFailed,
            _ => return None,
        };
        if self.last == Some(activity) {
            return None;
        }
        Some(activity)
    }

    pub(crate) fn commit(&mut self, activity: SessionActivity) {
        // A recovered in-flight turn (task_session_exit records `TurnStarted`
        // on recovery) must still count a later error as a failed turn.
        if activity == SessionActivity::TurnStarted && self.state.turn() != RootTurnState::Running {
            self.state
                .apply(AcpExecutionEvent::PromptAccepted, SystemTime::now());
        }
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

    // Without this the dashboard, task page, TUI and `ajax status` read a
    // pane-derived Waiting through an entire ACP turn: the pane classifier
    // cannot see a provisioned task's agent, and nothing else reported it.
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

    fn prompt() -> SessionServerEvent {
        SessionServerEvent::PromptAccepted {
            client_message_id: "c1".to_string(),
        }
    }

    fn tool(call_id: &str, title: &str, kind: &str) -> SessionServerEvent {
        SessionServerEvent::ToolCall {
            call_id: call_id.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            status: "in_progress".to_string(),
            locations: Vec::new(),
            content: Vec::new(),
        }
    }

    #[test]
    fn a_running_execute_tool_is_command_running() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&tool("c1", "ls -la", "execute")),
            Some(SessionActivity::CommandRunning)
        );
    }

    #[test]
    fn a_running_test_tool_is_tests_running() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&tool("c1", "cargo test", "execute")),
            Some(SessionActivity::TestsRunning)
        );
    }

    #[test]
    fn an_elicitation_is_a_wait_for_input() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::ElicitationRequest {
                request_id: "e1".to_string(),
                message: "Which option?".to_string(),
                schema: serde_json::Value::Null,
            }),
            Some(SessionActivity::AwaitingInput)
        );
    }

    #[test]
    fn a_permission_over_a_command_waits_and_the_answer_resumes_it() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&tool("c1", "ls -la", "execute")),
            Some(SessionActivity::CommandRunning)
        );
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
            Some(SessionActivity::CommandRunning)
        );
    }

    #[test]
    fn a_cancelled_turn_ends_not_fails() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::TurnEnd {
                stop_reason: Some("cancelled".to_string()),
            }),
            Some(SessionActivity::TurnEnded)
        );
    }

    #[test]
    fn off_turn_events_report_nothing() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&SessionServerEvent::Message {
                role: "assistant".to_string(),
                text: "hi".to_string(),
                content_blocks: Vec::new(),
                item_id: String::new(),
                message_id: None,
            }),
            None
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::Error {
                message: "spawn failed".to_string(),
            }),
            None
        );
    }

    #[test]
    fn a_repeated_prompt_acceptance_reports_once() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(reporter.observe(&prompt()), None);
    }

    #[test]
    fn an_error_after_a_recovered_turn_fails_it() {
        let mut reporter = reporter();
        reporter.commit(SessionActivity::TurnStarted);
        assert_eq!(
            reporter.observe(&SessionServerEvent::Error {
                message: "boom".to_string(),
            }),
            Some(SessionActivity::TurnFailed)
        );
    }

    #[test]
    fn command_test_and_input_evidence_read_as_running_or_waiting() {
        let mut context = provisioned_context();
        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::CommandRunning,
            SystemTime::now(),
        )
        .expect("recorded");

        assert_eq!(status_of(&context), TaskStatus::Running);

        let mut context = provisioned_context();
        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::TestsRunning,
            SystemTime::now(),
        )
        .expect("recorded");

        assert_eq!(status_of(&context), TaskStatus::Running);

        let mut context = provisioned_context();
        record_session_activity(
            &mut context,
            "web/fix-login",
            SessionActivity::AwaitingInput,
            SystemTime::now(),
        )
        .expect("recorded");

        assert_eq!(status_of(&context), TaskStatus::Waiting);
    }

    #[test]
    fn a_running_status_starts_the_turn() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::Status {
                state: "running".to_string(),
                detail: None,
            }),
            Some(SessionActivity::TurnStarted)
        );
    }

    #[test]
    fn an_uncorrelated_requires_action_never_invents_a_wait() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::Status {
                state: "requires_action".to_string(),
                detail: None,
            }),
            None
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::PermissionRequest {
                request_id: "p1".to_string(),
                title: None,
                detail: None,
            }),
            Some(SessionActivity::AwaitingOperator)
        );
    }

    #[test]
    fn an_idle_status_never_marks_the_turn_done() {
        let mut reporter = reporter();
        assert_eq!(
            reporter.observe(&prompt()),
            Some(SessionActivity::TurnStarted)
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::Status {
                state: "idle".to_string(),
                detail: None,
            }),
            None
        );
        assert_eq!(
            reporter.observe(&SessionServerEvent::TurnEnd {
                stop_reason: Some("end_turn".to_string()),
            }),
            Some(SessionActivity::TurnEnded)
        );
    }

    #[test]
    fn an_off_turn_idle_status_reports_nothing() {
        assert_eq!(
            reporter().observe(&SessionServerEvent::Status {
                state: "idle".to_string(),
                detail: None,
            }),
            None
        );
    }
}
