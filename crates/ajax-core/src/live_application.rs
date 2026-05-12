use std::time::SystemTime;

use crate::{
    lifecycle::{transition_lifecycle, LifecycleTransitionReason},
    live::{reduce_live_observation, LiveObservation, LiveStatusKind},
    models::{AgentRuntimeStatus, LifecycleStatus, SideFlag, Task},
};

pub fn apply_observation(task: &mut Task, observation: LiveObservation) {
    let observation = reduce_task_live_observation(task, observation);
    let refresh_activity = refreshes_activity(observation.kind);
    let has_missing_substrate_flag = has_missing_substrate_flag(task);

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
                task.agent_status = AgentRuntimeStatus::Unknown;
                task.remove_side_flag(SideFlag::AgentRunning);
            } else {
                task.agent_status = AgentRuntimeStatus::Running;
                task.add_side_flag(SideFlag::AgentRunning);
                update_live_lifecycle(task, LifecycleStatus::Active);
            }
            task.remove_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentDead);
        }
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput => {
            task.agent_status = AgentRuntimeStatus::Waiting;
            update_live_lifecycle(task, LifecycleStatus::Waiting);
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CiFailed
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            update_live_lifecycle(task, LifecycleStatus::Error);
            if observation.kind == LiveStatusKind::CiFailed {
                task.add_side_flag(SideFlag::TestsFailed);
            }
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::MergeConflict => {
            task.agent_status = AgentRuntimeStatus::Blocked;
            update_live_lifecycle(task, LifecycleStatus::Error);
            task.add_side_flag(SideFlag::Conflicted);
            task.add_side_flag(SideFlag::NeedsInput);
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::ShellIdle => {
            task.agent_status = AgentRuntimeStatus::Unknown;
            task.remove_side_flag(SideFlag::AgentRunning);
        }
        LiveStatusKind::Done => {
            task.agent_status = AgentRuntimeStatus::Done;
            update_live_lifecycle(task, LifecycleStatus::Reviewable);
            task.remove_side_flag(SideFlag::AgentRunning);
            task.remove_side_flag(SideFlag::NeedsInput);
        }
        LiveStatusKind::Unknown => {
            task.agent_status = AgentRuntimeStatus::Unknown;
            task.remove_side_flag(SideFlag::AgentRunning);
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

fn update_live_lifecycle(task: &mut Task, status: LifecycleStatus) {
    if let Err(_error) =
        transition_lifecycle(task, status, LifecycleTransitionReason::OperationResult)
    {
        // Live evidence can lag lifecycle state; invalid evidence-driven edges leave
        // the lifecycle unchanged while still allowing live status projection.
    }
}
