use crate::{
    models::{
        AgentRuntimeStatus, LifecycleStatus, LiveStatusKind, SafetyClassification, SideFlag, Task,
    },
    policy::merge_safety,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiState {
    Blocked,
    Running,
    ReviewReady,
    SafeMerge,
    Cleanable,
    Idle,
    Failed,
    Archived,
}

impl UiState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Running => "running",
            Self::ReviewReady => "review ready",
            Self::SafeMerge => "safe merge",
            Self::Cleanable => "cleanable",
            Self::Idle => "idle",
            Self::Failed => "failed",
            Self::Archived => "archived",
        }
    }
}

pub fn derive_ui_state(task: &Task) -> UiState {
    if task.lifecycle_status == LifecycleStatus::Removed {
        return UiState::Archived;
    }
    if is_blocked(task) {
        return UiState::Blocked;
    }
    match task.lifecycle_status {
        LifecycleStatus::Error => UiState::Failed,
        LifecycleStatus::Mergeable => UiState::SafeMerge,
        LifecycleStatus::Cleanable => UiState::Cleanable,
        LifecycleStatus::Merged => {
            if is_clean_for_cleanup(task) {
                UiState::Cleanable
            } else {
                UiState::Idle
            }
        }
        LifecycleStatus::Reviewable => {
            if merge_safety(task).classification == SafetyClassification::Safe {
                UiState::SafeMerge
            } else {
                UiState::ReviewReady
            }
        }
        LifecycleStatus::Created
        | LifecycleStatus::Provisioning
        | LifecycleStatus::Active
        | LifecycleStatus::Waiting
        | LifecycleStatus::Orphaned => {
            if is_running(task) {
                UiState::Running
            } else {
                UiState::Idle
            }
        }
        LifecycleStatus::Removed => UiState::Archived,
    }
}

fn is_blocked(task: &Task) -> bool {
    if task.has_missing_substrate() {
        return true;
    }
    if task.has_side_flag(SideFlag::NeedsInput)
        || task.has_side_flag(SideFlag::TestsFailed)
        || task.has_side_flag(SideFlag::AgentDead)
        || task.has_side_flag(SideFlag::Conflicted)
    {
        return true;
    }
    if matches!(
        task.agent_status,
        AgentRuntimeStatus::Blocked | AgentRuntimeStatus::Dead | AgentRuntimeStatus::Waiting
    ) {
        return true;
    }
    task.live_status
        .as_ref()
        .is_some_and(|live| is_blocking_live_status(live.kind))
}

fn is_blocking_live_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval
            | LiveStatusKind::WaitingForInput
            | LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::MergeConflict
            | LiveStatusKind::CiFailed
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Blocked
    )
}

fn is_running(task: &Task) -> bool {
    if task.agent_status == AgentRuntimeStatus::Running {
        return true;
    }
    if task.has_side_flag(SideFlag::AgentRunning) {
        return true;
    }
    task.live_status.as_ref().is_some_and(|live| {
        matches!(
            live.kind,
            LiveStatusKind::AgentRunning
                | LiveStatusKind::CommandRunning
                | LiveStatusKind::TestsRunning
        )
    })
}

pub(crate) fn is_clean_for_cleanup(task: &Task) -> bool {
    if task.has_side_flag(SideFlag::Dirty)
        || task.has_side_flag(SideFlag::Conflicted)
        || task.has_side_flag(SideFlag::Unpushed)
    {
        return false;
    }
    task.git_status.as_ref().is_some_and(|git| {
        !git.dirty && !git.conflicted && !git.has_unpushed_work() && git.untracked_files == 0
    })
}

#[cfg(test)]
mod tests {
    use super::{derive_ui_state, UiState};
    use crate::{
        lifecycle::{
            mark_active, mark_cleanable, mark_error, mark_mergeable, mark_merged, mark_removed,
            mark_reviewable,
        },
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind, SideFlag,
            Task, TaskId,
        },
    };

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
            "worktrunk",
            AgentClient::Codex,
        )
    }

    fn clean_git_status() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123".to_string()),
        }
    }

    #[test]
    fn removed_lifecycle_becomes_archived_regardless_of_other_signals() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_removed(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::Dirty);

        assert_eq!(derive_ui_state(&task), UiState::Archived);
    }

    #[test]
    fn needs_input_dominates_active_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn blocker_signals_outrank_review_ready_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.add_side_flag(SideFlag::Conflicted);

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn waiting_agent_status_is_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Waiting;

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn merge_conflict_live_status_is_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "conflict",
        ));

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn missing_substrate_is_blocked_even_with_otherwise_clean_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.mark_resource_missing(SideFlag::WorktreeMissing);

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn error_lifecycle_without_blocker_is_failed() {
        let mut task = base_task();
        mark_error(&mut task).unwrap();

        assert_eq!(derive_ui_state(&task), UiState::Failed);
    }

    #[test]
    fn mergeable_lifecycle_is_safe_merge() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_mergeable(&mut task).unwrap();

        assert_eq!(derive_ui_state(&task), UiState::SafeMerge);
    }

    #[test]
    fn mergeable_lifecycle_with_blocker_is_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_mergeable(&mut task).unwrap();
        task.add_side_flag(SideFlag::Conflicted);

        assert_eq!(derive_ui_state(&task), UiState::Blocked);
    }

    #[test]
    fn cleanable_lifecycle_is_cleanable() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_cleanable(&mut task).unwrap();

        assert_eq!(derive_ui_state(&task), UiState::Cleanable);
    }

    #[test]
    fn merged_lifecycle_with_clean_git_is_cleanable() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        task.git_status = Some(clean_git_status());

        assert_eq!(derive_ui_state(&task), UiState::Cleanable);
    }

    #[test]
    fn merged_lifecycle_with_dirty_git_falls_back_to_idle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        let mut git = clean_git_status();
        git.dirty = true;
        task.git_status = Some(git);
        task.add_side_flag(SideFlag::Dirty);

        assert_eq!(derive_ui_state(&task), UiState::Idle);
    }

    #[test]
    fn reviewable_lifecycle_with_safe_merge_promotes_to_safe_merge() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        let mut git = clean_git_status();
        git.merged = false;
        task.git_status = Some(git);

        assert_eq!(derive_ui_state(&task), UiState::SafeMerge);
    }

    #[test]
    fn reviewable_lifecycle_without_blocker_is_review_ready() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();

        assert_eq!(derive_ui_state(&task), UiState::ReviewReady);
    }

    #[test]
    fn reviewable_lifecycle_with_agent_running_stays_review_ready() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);

        assert_eq!(derive_ui_state(&task), UiState::ReviewReady);
    }

    #[test]
    fn active_lifecycle_with_agent_running_is_running() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);

        assert_eq!(derive_ui_state(&task), UiState::Running);
    }

    #[test]
    fn active_lifecycle_with_tests_running_live_status_is_running() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(LiveStatusKind::TestsRunning, "tests"));

        assert_eq!(derive_ui_state(&task), UiState::Running);
    }

    #[test]
    fn active_lifecycle_without_signals_is_idle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();

        assert_eq!(derive_ui_state(&task), UiState::Idle);
    }

    #[test]
    fn ui_state_labels_are_stable_and_unique() {
        let labels = [
            UiState::Blocked,
            UiState::Running,
            UiState::ReviewReady,
            UiState::SafeMerge,
            UiState::Cleanable,
            UiState::Idle,
            UiState::Failed,
            UiState::Archived,
        ]
        .map(UiState::as_str);

        let mut sorted = labels.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }
}
