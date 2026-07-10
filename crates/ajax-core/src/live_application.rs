use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{reduce_live_observation, LiveObservation, LiveStatusKind};
use crate::{
    lifecycle::{transition_lifecycle, LifecycleTransitionReason},
    models::{AgentRuntimeStatus, LifecycleStatus, LiveStatusClass, SideFlag, Task},
};

/// Metadata key holding the unix-seconds first sighting of an unconfirmed
/// busy→waiting observation. Persists with the registry like
/// `attention::LAST_NOTIFIED_STATUS_KEY`.
pub const WAITING_CANDIDATE_SINCE_KEY: &str = "waiting_candidate_since";
// ponytail: fixed dwell; make configurable only if a real agent needs tuning.
const WAITING_CONFIRMATION_DWELL: Duration = Duration::from_secs(4);

pub fn apply_observation(task: &mut Task, observation: LiveObservation) {
    apply_observation_at(task, observation, SystemTime::now());
}

pub fn apply_observation_at(
    task: &mut Task,
    observation: LiveObservation,
    observed_at: SystemTime,
) {
    let observation = reduce_task_live_observation(task, observation);
    if defers_unconfirmed_waiting(task, observation.kind, observed_at) {
        return;
    }
    apply_reduced_observation(task, observation, observed_at);
}

/// Waiting evidence on a busy task must persist for the dwell window before it
/// is applied: pane classification sometimes misreads a working agent as
/// waiting for one sample, and that flap must not change visible status or
/// fire notifications. Trusted wrapper/hook paths bypass this gate.
fn defers_unconfirmed_waiting(
    task: &mut Task,
    kind: LiveStatusKind,
    observed_at: SystemTime,
) -> bool {
    if kind.class() != LiveStatusClass::Waiting || !shows_running_evidence(task) {
        return false;
    }
    let observed_secs = unix_seconds(observed_at);
    let candidate_since = task
        .metadata
        .get(WAITING_CANDIDATE_SINCE_KEY)
        .and_then(|value| value.parse::<u64>().ok());
    match candidate_since {
        Some(since) if observed_secs >= since + WAITING_CONFIRMATION_DWELL.as_secs() => false,
        Some(_) => true,
        None => {
            task.metadata.insert(
                WAITING_CANDIDATE_SINCE_KEY.to_string(),
                observed_secs.to_string(),
            );
            true
        }
    }
}

pub fn has_pending_waiting_candidate(task: &Task) -> bool {
    task.metadata.contains_key(WAITING_CANDIDATE_SINCE_KEY)
}

fn shows_running_evidence(task: &Task) -> bool {
    task.live_status
        .as_ref()
        .is_some_and(|live| live.kind.class() == LiveStatusClass::Running)
        || task.agent_status == AgentRuntimeStatus::Running
        || task.has_side_flag(SideFlag::AgentRunning)
}

fn unix_seconds(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn apply_authoritative_observation(task: &mut Task, observation: LiveObservation) {
    apply_authoritative_observation_at(task, observation, SystemTime::now());
}

pub fn apply_authoritative_observation_at(
    task: &mut Task,
    observation: LiveObservation,
    observed_at: SystemTime,
) {
    apply_reduced_observation(task, observation, observed_at);
}

pub fn apply_trusted_observation(task: &mut Task, observation: LiveObservation) {
    apply_trusted_observation_at(task, observation, SystemTime::now());
}

pub fn apply_trusted_observation_at(
    task: &mut Task,
    observation: LiveObservation,
    observed_at: SystemTime,
) {
    let kind = observation.kind;
    apply_reduced_observation(task, observation, observed_at);

    let lifecycle = match kind {
        LiveStatusKind::AgentRunning
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning => Some(LifecycleStatus::Active),
        LiveStatusKind::Done => Some(LifecycleStatus::Reviewable),
        _ => None,
    };
    if let Some(lifecycle) = lifecycle {
        let _ = transition_lifecycle(task, lifecycle, LifecycleTransitionReason::OperationResult);
    }
}

/// Acknowledge operator attention on a task without changing lifecycle.
///
/// Records the acknowledgment time without erasing runtime evidence or changing
/// lifecycle. Projection and refresh compare evidence time with this timestamp.
pub fn acknowledge_attention(task: &mut Task, at: SystemTime) {
    task.record_attention_acknowledgment(at);
}

fn apply_reduced_observation(
    task: &mut Task,
    observation: LiveObservation,
    observed_at: SystemTime,
) {
    // Any applied observation resolves a pending waiting candidacy; a stale
    // candidate must never confirm a later, unrelated flap.
    task.metadata.remove(WAITING_CANDIDATE_SINCE_KEY);
    let refresh_activity = refreshes_activity(observation.kind);
    let has_missing_substrate_flag = has_missing_substrate_flag(task);
    clear_recovered_live_flags(task, observation.kind);

    match observation.kind {
        LiveStatusKind::WorktreeMissing => {
            task.mark_resource_missing(SideFlag::WorktreeMissing);
        }
        LiveStatusKind::TmuxMissing => {
            task.mark_resource_missing(SideFlag::TmuxMissing);
        }
        LiveStatusKind::TaskWindowMissing => {
            task.mark_resource_missing(SideFlag::TaskWindowMissing);
        }
        LiveStatusKind::AgentRunning
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning => {
            if has_missing_substrate_flag {
                task.agent_status = AgentRuntimeStatus::Dead;
                task.remove_side_flag(SideFlag::AgentRunning);
            } else {
                task.agent_status = AgentRuntimeStatus::Running;
                task.add_side_flag(SideFlag::AgentRunning);
            }
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentDead);
        }
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput => {
            task.agent_status = AgentRuntimeStatus::Waiting;
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::CiFailed => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            task.add_side_flag(SideFlag::TestsFailed);
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::MergeConflict => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            task.add_side_flag(SideFlag::Conflicted);
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::ShellIdle => {
            task.agent_status = if matches!(
                task.agent_status,
                AgentRuntimeStatus::Running
                    | AgentRuntimeStatus::Waiting
                    | AgentRuntimeStatus::Blocked
            ) {
                AgentRuntimeStatus::Dead
            } else {
                AgentRuntimeStatus::NotStarted
            };
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::Done => {
            task.agent_status = AgentRuntimeStatus::Done;
            task.remove_side_flag(SideFlag::AgentRunning);
            task.remove_side_flag(SideFlag::NeedsInput);
        }
        LiveStatusKind::Unknown => {
            task.remove_side_flag(SideFlag::AgentRunning);
            task.live_status = None;
            task.live_status_observed_at = None;
            return;
        }
    }

    task.live_status = Some(observation);
    task.live_status_observed_at = Some(observed_at);
    if refresh_activity {
        task.last_activity_at = SystemTime::now();
        task.remove_side_flag(SideFlag::Stale);
    }
}

fn reduce_task_live_observation(task: &Task, next: LiveObservation) -> LiveObservation {
    if recovered_from_missing_substrate(task, next.kind) {
        return next;
    }

    reduce_live_observation(task.live_status.as_ref(), next)
}

fn recovered_from_missing_substrate(task: &Task, next: LiveStatusKind) -> bool {
    task.live_status
        .as_ref()
        .is_some_and(|status| status.kind.is_missing_substrate())
        && !next.is_missing_substrate()
        && !has_missing_substrate_flag(task)
}

fn has_missing_substrate_flag(task: &Task) -> bool {
    task.side_flags().any(SideFlag::is_missing_substrate)
}

fn clear_recovered_live_flags(task: &mut Task, kind: LiveStatusKind) {
    if kind != LiveStatusKind::MergeConflict
        && !task
            .git_status
            .as_ref()
            .is_some_and(|git_status| git_status.conflicted)
    {
        task.remove_side_flag(SideFlag::Conflicted);
    }
}

fn refreshes_activity(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::ShellIdle
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::WaitingForApproval
            | LiveStatusKind::WaitingForInput
            | LiveStatusKind::Blocked
            | LiveStatusKind::RateLimited
            | LiveStatusKind::AuthRequired
            | LiveStatusKind::MergeConflict
            | LiveStatusKind::CiFailed
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Done
    )
}

#[cfg(test)]
mod tests {
    use super::{acknowledge_attention, apply_observation, apply_observation_at};
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LifecycleStatus, LiveObservation, LiveStatusKind,
        SideFlag, Task, TaskId,
    };
    use std::time::{Duration, UNIX_EPOCH};

    fn claude_active_task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Claude,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    #[rstest::rstest]
    #[case(AgentClient::Claude)]
    #[case(AgentClient::Codex)]
    fn acknowledging_waiting_is_agent_neutral_and_non_destructive(#[case] agent: AgentClient) {
        let mut task = claude_active_task();
        task.selected_agent = agent;
        let observed_at = UNIX_EPOCH + Duration::from_secs(400);
        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            observed_at,
        );
        let at = UNIX_EPOCH + Duration::from_secs(500);

        acknowledge_attention(&mut task, at);

        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert_eq!(task.live_status_observed_at, Some(observed_at));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(!task.has_side_flag(SideFlag::AgentDead));
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.attention_acknowledged_at, Some(at));
    }

    #[test]
    fn acknowledging_nonwaiting_state_does_not_erase_runtime_evidence() {
        for status in [
            LiveStatusKind::AgentRunning,
            LiveStatusKind::CommandFailed,
            LiveStatusKind::Done,
            LiveStatusKind::TmuxMissing,
        ] {
            let mut task = claude_active_task();
            apply_observation(&mut task, LiveObservation::new(status, "evidence"));
            let agent_before = task.agent_status;
            let lifecycle_before = task.lifecycle_status;
            let flags_before: Vec<SideFlag> = task.side_flags().collect();
            let live_before = task.live_status.clone();
            let at = UNIX_EPOCH + Duration::from_secs(500);

            acknowledge_attention(&mut task, at);

            assert_eq!(task.attention_acknowledged_at, Some(at), "{status:?}");
            assert_eq!(task.agent_status, agent_before, "{status:?}");
            assert_eq!(task.lifecycle_status, lifecycle_before, "{status:?}");
            assert_eq!(
                task.side_flags().collect::<Vec<_>>(),
                flags_before,
                "{status:?}"
            );
            assert_eq!(task.live_status, live_before, "{status:?}");
        }
    }

    fn active_task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    #[test]
    fn low_confidence_done_observation_does_not_mark_task_reviewable() {
        let mut task = active_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
        );

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn attention_observations_do_not_mark_task_error() {
        for status in [
            LiveStatusKind::CommandFailed,
            LiveStatusKind::AuthRequired,
            LiveStatusKind::RateLimited,
            LiveStatusKind::ContextLimit,
        ] {
            let mut task = active_task();

            apply_observation(&mut task, LiveObservation::new(status, "needs attention"));

            assert_eq!(task.lifecycle_status, LifecycleStatus::Active, "{status:?}");
            assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked, "{status:?}");
            assert!(task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
        }
    }

    #[test]
    fn waiting_observation_does_not_mark_task_waiting_lifecycle() {
        let mut task = active_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting"),
        );

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn timestamped_observation_records_and_refreshes_live_evidence_time() {
        let mut task = active_task();
        let first = UNIX_EPOCH + Duration::from_secs(100);
        let second = UNIX_EPOCH + Duration::from_secs(200);

        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting"),
            first,
        );
        assert_eq!(task.live_status_observed_at, Some(first));

        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "still waiting"),
            second,
        );
        assert_eq!(task.live_status_observed_at, Some(second));
    }

    fn busy_task_at(at: std::time::SystemTime) -> Task {
        let mut task = active_task();
        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "working"),
            at,
        );
        task
    }

    fn waiting(summary: &str) -> LiveObservation {
        LiveObservation::new(LiveStatusKind::WaitingForInput, summary)
    }

    #[test]
    fn busy_task_defers_first_waiting_observation() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));

        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );

        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.metadata.get(super::WAITING_CANDIDATE_SINCE_KEY),
            Some(&"110".to_string())
        );
    }

    #[test]
    fn waiting_confirms_after_dwell() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));
        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );

        apply_observation_at(
            &mut task,
            waiting("still waiting"),
            UNIX_EPOCH + Duration::from_secs(115),
        );

        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status_observed_at,
            Some(UNIX_EPOCH + Duration::from_secs(115))
        );
        assert!(!task
            .metadata
            .contains_key(super::WAITING_CANDIDATE_SINCE_KEY));
    }

    #[test]
    fn waiting_within_dwell_stays_deferred() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));
        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );

        apply_observation_at(
            &mut task,
            waiting("still waiting"),
            UNIX_EPOCH + Duration::from_secs(111),
        );

        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert_eq!(
            task.metadata.get(super::WAITING_CANDIDATE_SINCE_KEY),
            Some(&"110".to_string())
        );
    }

    #[test]
    fn busy_observation_clears_pending_candidate() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));
        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );
        assert!(super::has_pending_waiting_candidate(&task));

        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "working again"),
            UNIX_EPOCH + Duration::from_secs(112),
        );
        assert!(!super::has_pending_waiting_candidate(&task));

        // A much later waiting flap starts a fresh candidate instead of
        // confirming against the stale one.
        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(900),
        );
        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert_eq!(
            task.metadata.get(super::WAITING_CANDIDATE_SINCE_KEY),
            Some(&"900".to_string())
        );
    }

    #[test]
    fn non_busy_task_applies_waiting_immediately() {
        let mut task = active_task();

        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );

        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(!task
            .metadata
            .contains_key(super::WAITING_CANDIDATE_SINCE_KEY));
    }

    #[test]
    fn trusted_waiting_bypasses_gate() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));
        super::apply_trusted_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
            UNIX_EPOCH + Duration::from_secs(110),
        );
        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::Done)
        );

        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(100));
        super::apply_authoritative_observation_at(
            &mut task,
            waiting("hook waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );
        assert_eq!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn confirmed_waiting_after_acknowledgment_projects_waiting() {
        let mut task = busy_task_at(UNIX_EPOCH + Duration::from_secs(210));
        acknowledge_attention(&mut task, UNIX_EPOCH + Duration::from_secs(200));
        apply_observation_at(
            &mut task,
            waiting("waiting"),
            UNIX_EPOCH + Duration::from_secs(220),
        );
        apply_observation_at(
            &mut task,
            waiting("still waiting"),
            UNIX_EPOCH + Duration::from_secs(225),
        );

        let status = crate::ui_state::derive_operator_status(&task);

        assert_eq!(status.status, crate::ui_state::TaskStatus::Waiting);
    }

    #[test]
    fn unknown_observation_clears_live_evidence_time() {
        let mut task = active_task();
        let observed_at = UNIX_EPOCH + Duration::from_secs(100);
        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "working"),
            observed_at,
        );

        apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::Unknown, "unknown"),
            observed_at + Duration::from_secs(1),
        );

        assert_eq!(task.live_status, None);
        assert_eq!(task.live_status_observed_at, None);
    }
}
