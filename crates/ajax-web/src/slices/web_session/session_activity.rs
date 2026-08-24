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

use ajax_core::{
    commands::CommandContext,
    live,
    models::{LiveObservation, LiveStatusKind, TaskId},
    registry::Registry,
};
use std::time::SystemTime;

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
) -> Result<(), String> {
    let task_id: TaskId = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .filter(|task| task.skip_interactive_agent())
        .map(|task| task.id.clone())
        .ok_or_else(|| format!("no ACP-capable task for {qualified_handle}"))?;

    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| format!("task disappeared: {qualified_handle}"))?;

    // Authoritative: the host owns the ACP child, so this is first-hand
    // process evidence, not a guess reconciled from screen scraping.
    live::apply_authoritative_observation_at(task, activity.observation(), now);
    Ok(())
}

/// Which transitions on the outbound wire are evidence about the agent.
///
/// Read off the same stream the browser sees, so the task page and the chat
/// head cannot disagree about whether a turn is in flight. Everything else —
/// messages, tool calls, usage — is detail within a turn already reported as
/// running.
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
    pub(crate) fn observe(&mut self, event: &super::SessionServerEvent) -> Option<SessionActivity> {
        let in_flight = matches!(
            self.last,
            Some(SessionActivity::TurnStarted) | Some(SessionActivity::AwaitingOperator)
        );
        let activity = activity_for_event(event, in_flight)?;
        if self.last == Some(activity) {
            return None;
        }
        self.last = Some(activity);
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

        assert!(error.contains("no ACP-capable task"), "{error}");
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
