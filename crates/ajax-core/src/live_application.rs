use std::time::SystemTime;

use crate::{
    lifecycle::{transition_lifecycle, LifecycleTransitionReason},
    live::{reduce_live_observation, LiveObservation, LiveStatusKind},
    models::{AgentRuntimeStatus, LifecycleStatus, SideFlag, Task},
};

pub fn apply_observation(task: &mut Task, observation: LiveObservation) {
    let observation = reduce_task_live_observation(task, observation);
    apply_reduced_observation(task, observation);
}

pub fn apply_authoritative_observation(task: &mut Task, observation: LiveObservation) {
    apply_reduced_observation(task, observation);
}

pub fn apply_trusted_observation(task: &mut Task, observation: LiveObservation) {
    let kind = observation.kind;
    apply_reduced_observation(task, observation);

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

fn apply_reduced_observation(task: &mut Task, observation: LiveObservation) {
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
        LiveStatusKind::WorktrunkMissing => {
            task.mark_resource_missing(SideFlag::WorktrunkMissing);
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
            return;
        }
    }

    task.live_status = Some(observation);
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
    use super::apply_observation;
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LifecycleStatus, LiveObservation, LiveStatusKind,
        SideFlag, Task, TaskId,
    };

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
            "worktrunk",
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
}
