use crate::{
    models::{
        AgentRuntimeStatus, LifecycleStatus, LiveStatusKind, SafetyClassification, SideFlag, Task,
    },
    policy::merge_safety,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiState {
    Blocked,
    NeedsInput,
    Running,
    ReviewReady,
    SafeMerge,
    Cleanable,
    Idle,
    Failed,
    Archived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorStatusKind {
    Blocked,
    NeedsInput,
    Running,
    ReviewReady,
    SafeMerge,
    Cleanable,
    Idle,
    Failed,
    ObservationFailed,
    Archived,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorStatus {
    pub kind: OperatorStatusKind,
    pub label: String,
}

impl OperatorStatus {
    fn new(kind: OperatorStatusKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
        }
    }

    pub const fn ui_state(&self) -> UiState {
        match self.kind {
            OperatorStatusKind::Blocked => UiState::Blocked,
            OperatorStatusKind::NeedsInput => UiState::NeedsInput,
            OperatorStatusKind::Running => UiState::Running,
            OperatorStatusKind::ReviewReady => UiState::ReviewReady,
            OperatorStatusKind::SafeMerge => UiState::SafeMerge,
            OperatorStatusKind::Cleanable => UiState::Cleanable,
            OperatorStatusKind::Idle => UiState::Idle,
            OperatorStatusKind::Failed | OperatorStatusKind::ObservationFailed => UiState::Failed,
            OperatorStatusKind::Archived => UiState::Archived,
        }
    }
}

impl UiState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::NeedsInput => "needs input",
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
    derive_operator_status(task).ui_state()
}

pub fn derive_operator_status(task: &Task) -> OperatorStatus {
    if task.lifecycle_status == LifecycleStatus::Removed {
        return OperatorStatus::new(OperatorStatusKind::Archived, "archived");
    }
    if task.lifecycle_status == LifecycleStatus::Removing {
        return OperatorStatus::new(OperatorStatusKind::Cleanable, "removing");
    }
    if task.lifecycle_status == LifecycleStatus::TeardownIncomplete {
        return OperatorStatus::new(OperatorStatusKind::Failed, "teardown incomplete");
    }
    if let Some(error) = task.runtime_projection.observation_error.as_deref() {
        return OperatorStatus::new(
            OperatorStatusKind::ObservationFailed,
            format!("status unavailable: {error}"),
        );
    }
    if let Some(label) = missing_substrate_label(task) {
        return OperatorStatus::new(OperatorStatusKind::Failed, label);
    }
    if is_blocked(task) {
        return OperatorStatus::new(
            OperatorStatusKind::Blocked,
            live_summary(task).unwrap_or_else(|| "blocked".to_string()),
        );
    }
    if needs_input(task) {
        return OperatorStatus::new(
            OperatorStatusKind::NeedsInput,
            live_summary(task).unwrap_or_else(|| "needs input".to_string()),
        );
    }
    if is_failed(task) {
        return OperatorStatus::new(
            OperatorStatusKind::Failed,
            live_summary(task).unwrap_or_else(|| "failed".to_string()),
        );
    }
    match task.lifecycle_status {
        LifecycleStatus::Error => OperatorStatus::new(OperatorStatusKind::Failed, "failed"),
        LifecycleStatus::TeardownIncomplete => {
            OperatorStatus::new(OperatorStatusKind::Failed, "teardown incomplete")
        }
        LifecycleStatus::Mergeable => {
            OperatorStatus::new(OperatorStatusKind::SafeMerge, "safe merge")
        }
        LifecycleStatus::Removing => OperatorStatus::new(OperatorStatusKind::Cleanable, "removing"),
        LifecycleStatus::Cleanable => {
            OperatorStatus::new(OperatorStatusKind::Cleanable, "cleanable")
        }
        LifecycleStatus::Merged => {
            if is_clean_for_cleanup(task) {
                OperatorStatus::new(OperatorStatusKind::Cleanable, "cleanable")
            } else {
                OperatorStatus::new(OperatorStatusKind::Idle, "merged")
            }
        }
        LifecycleStatus::Reviewable => {
            if merge_safety(task).classification == SafetyClassification::Safe {
                OperatorStatus::new(OperatorStatusKind::SafeMerge, "safe merge")
            } else {
                OperatorStatus::new(OperatorStatusKind::ReviewReady, "review ready")
            }
        }
        LifecycleStatus::Created
        | LifecycleStatus::Provisioning
        | LifecycleStatus::Active
        | LifecycleStatus::Waiting
        | LifecycleStatus::Orphaned => {
            if is_running(task) {
                OperatorStatus::new(
                    OperatorStatusKind::Running,
                    live_summary(task).unwrap_or_else(|| "running".to_string()),
                )
            } else {
                OperatorStatus::new(OperatorStatusKind::Idle, "idle")
            }
        }
        LifecycleStatus::Removed => OperatorStatus::new(OperatorStatusKind::Archived, "archived"),
    }
}

fn live_summary(task: &Task) -> Option<String> {
    task.live_status
        .as_ref()
        .filter(|live| live.kind != LiveStatusKind::Unknown)
        .map(|live| live.summary.clone())
}

fn missing_substrate_label(task: &Task) -> Option<&'static str> {
    if task.has_side_flag(SideFlag::WorktreeMissing)
        || task.runtime_projection.health == crate::models::RuntimeHealth::MissingWorktree
    {
        return Some("worktree missing");
    }
    if task.has_side_flag(SideFlag::BranchMissing) {
        return Some("branch missing");
    }
    if task.has_side_flag(SideFlag::TmuxMissing)
        || task.runtime_projection.health == crate::models::RuntimeHealth::MissingSession
    {
        return Some("tmux session missing");
    }
    if task.has_side_flag(SideFlag::WorktrunkMissing)
        || matches!(
            task.runtime_projection.health,
            crate::models::RuntimeHealth::MissingTaskWindow
                | crate::models::RuntimeHealth::WrongTaskWindowPath
        )
    {
        return Some("task window missing");
    }
    None
}

fn is_blocked(task: &Task) -> bool {
    if task.has_side_flag(SideFlag::TestsFailed) || task.has_side_flag(SideFlag::Conflicted) {
        return true;
    }
    task.live_status
        .as_ref()
        .is_some_and(|live| is_blocking_live_status(live.kind))
}

fn is_blocking_live_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::MergeConflict | LiveStatusKind::CiFailed
    )
}

fn needs_input(task: &Task) -> bool {
    if task.has_side_flag(SideFlag::NeedsInput) {
        return true;
    }
    if matches!(
        task.agent_status,
        AgentRuntimeStatus::Waiting | AgentRuntimeStatus::Blocked
    ) {
        return true;
    }
    task.live_status
        .as_ref()
        .is_some_and(|live| is_input_live_status(live.kind))
}

fn is_input_live_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval
            | LiveStatusKind::WaitingForInput
            | LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
    )
}

fn is_failed(task: &Task) -> bool {
    if task.has_missing_substrate() || task.has_side_flag(SideFlag::AgentDead) {
        return true;
    }
    if task.agent_status == AgentRuntimeStatus::Dead {
        return true;
    }
    task.live_status
        .as_ref()
        .is_some_and(|live| is_failed_live_status(live.kind))
}

fn is_failed_live_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::CommandFailed | LiveStatusKind::Blocked
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
            AgentClient, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind,
            RuntimeObservationSource, SideFlag, Task, TaskId,
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

        assert_eq!(derive_ui_state(&task), UiState::NeedsInput);
    }

    #[test]
    fn needs_input_is_distinct_from_blocked() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);

        assert_eq!(derive_ui_state(&task), UiState::NeedsInput);
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
    fn waiting_agent_status_needs_input() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Waiting;

        assert_eq!(derive_ui_state(&task), UiState::NeedsInput);
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
    fn missing_substrate_is_failed_even_with_otherwise_clean_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.mark_resource_missing(SideFlag::WorktreeMissing);

        assert_eq!(derive_ui_state(&task), UiState::Failed);
    }

    #[test]
    fn runtime_probe_failure_is_failed_without_changing_lifecycle() {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        assert_eq!(derive_ui_state(&task), UiState::Failed);
        assert_eq!(
            task.lifecycle_status,
            crate::models::LifecycleStatus::Active
        );
    }

    #[test]
    fn waiting_live_statuses_need_input_instead_of_blocking() {
        for live_status in [
            LiveStatusKind::WaitingForApproval,
            LiveStatusKind::WaitingForInput,
        ] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "waiting"));

            assert_eq!(
                derive_ui_state(&task),
                UiState::NeedsInput,
                "{live_status:?}"
            );
        }
    }

    #[test]
    fn only_failed_ci_and_merge_conflicts_are_blocked() {
        for live_status in [LiveStatusKind::CiFailed, LiveStatusKind::MergeConflict] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "blocked"));

            assert_eq!(derive_ui_state(&task), UiState::Blocked, "{live_status:?}");
        }

        for live_status in [
            LiveStatusKind::AuthRequired,
            LiveStatusKind::RateLimited,
            LiveStatusKind::ContextLimit,
            LiveStatusKind::CommandFailed,
            LiveStatusKind::Blocked,
        ] {
            let mut task = base_task();
            mark_active(&mut task).unwrap();
            task.live_status = Some(LiveObservation::new(live_status, "attention"));

            assert_ne!(derive_ui_state(&task), UiState::Blocked, "{live_status:?}");
        }
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
