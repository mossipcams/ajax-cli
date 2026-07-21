use crate::models::{LifecycleStatus, SideFlag, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskOperation {
    Create,
    Open,
    Trunk,
    Check,
    Diff,
    Merge,
    Clean,
    Remove,
    Refresh,
    Recover,
}

impl TaskOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Open => "open",
            Self::Trunk => "trunk",
            Self::Check => "check",
            Self::Diff => "diff",
            Self::Merge => "merge",
            Self::Clean => "clean",
            Self::Remove => "remove",
            Self::Refresh => "refresh",
            Self::Recover => "recover",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationEligibility {
    Allowed,
    Blocked(Vec<String>),
}

impl OperationEligibility {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

pub fn task_operation_eligibility(task: &Task, operation: TaskOperation) -> OperationEligibility {
    let mut reasons = Vec::new();

    if task.lifecycle_status == LifecycleStatus::Removed {
        reasons.push("task is removed".to_string());
    }
    if operation == TaskOperation::Merge
        && !matches!(
            task.lifecycle_status,
            LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
        )
    {
        reasons.push("merge requires reviewable or mergeable lifecycle".to_string());
    }
    if operation == TaskOperation::Clean
        && !matches!(
            task.lifecycle_status,
            LifecycleStatus::Merged
                | LifecycleStatus::Cleanable
                | LifecycleStatus::Removing
                | LifecycleStatus::TeardownIncomplete
        )
    {
        reasons.push("clean requires merged or cleanable lifecycle".to_string());
    }
    if matches!(operation, TaskOperation::Check | TaskOperation::Diff)
        && (task.has_side_flag(SideFlag::WorktreeMissing)
            || task
                .git_status
                .as_ref()
                .is_some_and(|status| !status.worktree_exists))
    {
        reasons.push("task worktree is missing".to_string());
    }
    if missing_substrate_blocks_operation(task, operation) {
        reasons.push("task has missing substrate".to_string());
    }
    // Remove must stay allowed on checkout mismatch so Drop can tear down a
    // drifted worktree; Merge/Clean stay blocked until the checkout matches.
    if matches!(operation, TaskOperation::Merge | TaskOperation::Clean)
        && task.has_checkout_mismatch()
    {
        if let Some(explanation) = task.checkout_mismatch_explanation() {
            reasons.push(explanation);
        }
    }

    if reasons.is_empty() {
        OperationEligibility::Allowed
    } else {
        OperationEligibility::Blocked(reasons)
    }
}

fn missing_substrate_blocks_operation(task: &Task, operation: TaskOperation) -> bool {
    match operation {
        TaskOperation::Check | TaskOperation::Diff | TaskOperation::Remove => false,
        TaskOperation::Merge => task.has_missing_git_substrate(),
        _ => task.has_missing_substrate(),
    }
}

#[cfg(test)]
mod tests {
    use super::{task_operation_eligibility, OperationEligibility, TaskOperation};
    use crate::models::{AgentClient, LifecycleStatus, SideFlag, Task, TaskId};
    use rstest::rstest;

    fn task_with_status(status: LifecycleStatus) -> Task {
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
        task.lifecycle_status = status;
        task
    }

    #[rstest]
    #[case(TaskOperation::Create, "create")]
    #[case(TaskOperation::Open, "open")]
    #[case(TaskOperation::Trunk, "trunk")]
    #[case(TaskOperation::Check, "check")]
    #[case(TaskOperation::Diff, "diff")]
    #[case(TaskOperation::Merge, "merge")]
    #[case(TaskOperation::Clean, "clean")]
    #[case(TaskOperation::Remove, "remove")]
    #[case(TaskOperation::Refresh, "refresh")]
    #[case(TaskOperation::Recover, "recover")]
    fn task_operation_labels_are_stable(#[case] operation: TaskOperation, #[case] label: &str) {
        assert_eq!(operation.as_str(), label);
        assert_eq!(format!("{operation:?}").to_ascii_lowercase(), label);
    }

    #[test]
    fn operation_eligibility_returns_allowed_for_visible_task() {
        let task = task_with_status(LifecycleStatus::Active);

        let eligibility = task_operation_eligibility(&task, TaskOperation::Open);

        assert_eq!(eligibility, OperationEligibility::Allowed);
        assert!(eligibility.is_allowed());
    }

    #[test]
    fn operation_eligibility_returns_blocked_with_reasons() {
        let mut task = task_with_status(LifecycleStatus::Removed);
        task.add_side_flag(SideFlag::WorktreeMissing);

        let eligibility = task_operation_eligibility(&task, TaskOperation::Open);

        assert_eq!(
            eligibility,
            OperationEligibility::Blocked(vec![
                "task is removed".to_string(),
                "task has missing substrate".to_string(),
            ])
        );
        assert!(!eligibility.is_allowed());
    }

    fn task_with_git_checkout(status: LifecycleStatus, current_branch: Option<&str>) -> Task {
        let mut task = task_with_status(status);
        task.git_status = Some(crate::models::GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: current_branch.map(str::to_string),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: status == LifecycleStatus::Cleanable,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task
    }

    #[test]
    fn branch_sensitive_checkout_mismatch_operations_are_blocked_with_details() {
        let named_mismatch =
            task_with_git_checkout(LifecycleStatus::Reviewable, Some("fix/pane-stuck"));
        let detached_mismatch = task_with_git_checkout(LifecycleStatus::Cleanable, None);
        let named_detail = "Worktree on fix/pane-stuck; expected ajax/fix-login";
        let detached_detail = "Worktree detached; expected ajax/fix-login";

        for operation in [
            TaskOperation::Open,
            TaskOperation::Check,
            TaskOperation::Diff,
        ] {
            assert_eq!(
                task_operation_eligibility(&named_mismatch, operation),
                OperationEligibility::Allowed,
                "{operation:?} should stay available for named mismatch"
            );
            assert_eq!(
                task_operation_eligibility(&detached_mismatch, operation),
                OperationEligibility::Allowed,
                "{operation:?} should stay available for detached mismatch"
            );
        }

        assert_eq!(
            task_operation_eligibility(&named_mismatch, TaskOperation::Merge),
            OperationEligibility::Blocked(vec![named_detail.to_string()])
        );
        assert_eq!(
            task_operation_eligibility(&detached_mismatch, TaskOperation::Clean),
            OperationEligibility::Blocked(vec![detached_detail.to_string()])
        );
        assert_eq!(
            task_operation_eligibility(&named_mismatch, TaskOperation::Remove),
            OperationEligibility::Allowed,
            "Remove/Drop must stay allowed so mismatched worktrees can be torn down"
        );
        assert_eq!(
            task_operation_eligibility(&detached_mismatch, TaskOperation::Remove),
            OperationEligibility::Allowed,
        );

        let mut missing_worktree = task_with_git_checkout(LifecycleStatus::Cleanable, None);
        missing_worktree
            .git_status
            .as_mut()
            .unwrap()
            .worktree_exists = false;
        missing_worktree.add_side_flag(SideFlag::WorktreeMissing);
        let clean_eligibility = task_operation_eligibility(&missing_worktree, TaskOperation::Clean);
        assert!(
            matches!(clean_eligibility, OperationEligibility::Blocked(ref reasons)
                if reasons.iter().any(|reason| reason == "task has missing substrate")
                    && !reasons.iter().any(|reason| reason.contains("expected ajax/fix-login")))
        );

        let mut mismatch_with_missing_tmux =
            task_with_git_checkout(LifecycleStatus::Reviewable, Some("fix/pane-stuck"));
        mismatch_with_missing_tmux.add_side_flag(SideFlag::TmuxMissing);
        assert_eq!(
            task_operation_eligibility(&mismatch_with_missing_tmux, TaskOperation::Merge),
            OperationEligibility::Blocked(vec![named_detail.to_string()])
        );
    }
}
