#[path = "live_application.rs"]
mod application;
pub use crate::models::{AgentClient, LiveObservation, LiveStatusKind};
pub use application::{
    acknowledge_attention, apply_authoritative_observation, apply_authoritative_observation_at,
    apply_observation, apply_observation_at, apply_trusted_observation,
    apply_trusted_observation_at,
};

pub fn reduce_live_observation(
    current: Option<&LiveObservation>,
    next: LiveObservation,
) -> LiveObservation {
    let Some(current) = current else {
        return next;
    };

    if next.kind.is_missing_substrate() {
        return next;
    }

    if current.kind.is_missing_substrate() {
        return current.clone();
    }

    if should_keep_current_status(current.kind, next.kind) {
        return current.clone();
    }

    next
}

fn should_keep_current_status(current: LiveStatusKind, next: LiveStatusKind) -> bool {
    if current == LiveStatusKind::Done {
        return is_passive_observation(next);
    }

    if current == LiveStatusKind::TestsRunning {
        return matches!(
            next,
            LiveStatusKind::AgentRunning | LiveStatusKind::CommandRunning
        );
    }

    if is_waiting_status(current) {
        return is_passive_observation(next);
    }

    if is_failure_status(current) {
        return is_passive_observation(next);
    }

    false
}

fn is_waiting_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput
    )
}

fn is_failure_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CiFailed
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Blocked
            | LiveStatusKind::MergeConflict
    )
}

fn is_passive_observation(kind: LiveStatusKind) -> bool {
    matches!(kind, LiveStatusKind::ShellIdle | LiveStatusKind::Unknown)
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
    };

    use super::{apply_observation, apply_observation_at};

    fn base_task() -> Task {
        Task::new(
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
        )
    }

    #[test]
    fn missing_resource_observations_clear_agent_running() {
        for status in [
            LiveStatusKind::WorktreeMissing,
            LiveStatusKind::TmuxMissing,
            LiveStatusKind::TaskWindowMissing,
        ] {
            let mut task = base_task();
            task.agent_status = AgentRuntimeStatus::Running;
            task.add_side_flag(SideFlag::AgentRunning);

            apply_observation(&mut task, LiveObservation::new(status, "resource missing"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Dead, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
        }
    }

    #[test]
    fn running_observation_does_not_override_missing_resources() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::WorktreeMissing);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn recovered_missing_resource_can_accept_new_live_status() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WorktreeMissing, "worktree missing"),
        );
        task.remove_side_flag(SideFlag::WorktreeMissing);
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn done_observation_is_not_downgraded_by_shell_idle() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn waiting_observation_is_not_downgraded_by_passive_terminal_evidence() {
        for status in [LiveStatusKind::ShellIdle, LiveStatusKind::Unknown] {
            let mut task = base_task();

            apply_observation(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
            );
            apply_observation(&mut task, LiveObservation::new(status, "passive evidence"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting, "{status:?}");
            assert!(task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(LiveStatusKind::WaitingForApproval),
                "{status:?}"
            );
        }
    }

    #[test]
    fn waiting_for_approval_is_cleared_by_resumed_activity() {
        use std::time::{Duration, UNIX_EPOCH};
        for status in [
            LiveStatusKind::AgentRunning,
            LiveStatusKind::CommandRunning,
            LiveStatusKind::TestsRunning,
        ] {
            let mut task = base_task();

            apply_observation_at(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
                UNIX_EPOCH + Duration::from_secs(100),
            );
            apply_observation_at(
                &mut task,
                LiveObservation::new(status, "activity resumed"),
                UNIX_EPOCH + Duration::from_secs(110),
            );

            assert_eq!(task.agent_status, AgentRuntimeStatus::Running, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert!(task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(status),
                "{status:?}"
            );
        }
    }

    #[test]
    fn done_yields_to_new_busy_evidence() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(LiveStatusKind::Done, "done")),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn failure_yields_to_new_busy_evidence() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(
                LiveStatusKind::CommandFailed,
                "command failed",
            )),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn done_keeps_over_shell_idle_and_unknown() {
        for passive in [LiveStatusKind::ShellIdle, LiveStatusKind::Unknown] {
            let reduced = super::reduce_live_observation(
                Some(&LiveObservation::new(LiveStatusKind::Done, "done")),
                LiveObservation::new(passive, "passive"),
            );

            assert_eq!(reduced.kind, LiveStatusKind::Done, "{passive:?}");
        }
    }

    #[test]
    fn failed_observation_yields_to_later_busy_output() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandFailed, "command failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandRunning, "command running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandRunning)
        );
    }

    #[test]
    fn blocked_observation_yields_to_later_busy_output() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(
                LiveStatusKind::Blocked,
                "manual intervention required",
            )),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn merge_conflict_flag_is_cleared_by_later_input_prompt() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "merge conflict needs attention",
            ),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn live_lifecycle_updates_ignore_invalid_transition_edges() {
        let mut task = base_task();
        task.lifecycle_status = crate::models::LifecycleStatus::Error;

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        );

        assert_eq!(task.lifecycle_status, crate::models::LifecycleStatus::Error);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn ci_failed_observation_marks_task_blocked_and_tests_failed() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CiFailed)
        );

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn active_live_observation_refreshes_activity_and_clears_stale() {
        let mut task = base_task();
        task.last_activity_at = std::time::SystemTime::UNIX_EPOCH;
        task.add_side_flag(SideFlag::Stale);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert!(!task.has_side_flag(SideFlag::Stale));
        assert!(task.last_activity_at > std::time::SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn reduce_live_observation_does_not_mutate_lifecycle_or_substrate() {
        let task = base_task();
        let lifecycle_before = task.lifecycle_status;
        let git_before = task.git_status.clone();
        let tmux_before = task.tmux_status.clone();
        let task_before = task.task_window_status.clone();

        let reduced = super::reduce_live_observation(
            task.live_status.as_ref(),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
        assert_eq!(task.lifecycle_status, lifecycle_before);
        assert_eq!(task.git_status, git_before);
        assert_eq!(task.tmux_status, tmux_before);
        assert_eq!(task.task_window_status, task_before);
    }
}
