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
            LifecycleStatus::Merged | LifecycleStatus::Cleanable
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
    if task.has_missing_substrate()
        && !matches!(operation, TaskOperation::Check | TaskOperation::Diff)
    {
        reasons.push("task has missing substrate".to_string());
    }

    if reasons.is_empty() {
        OperationEligibility::Allowed
    } else {
        OperationEligibility::Blocked(reasons)
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
            "worktrunk",
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
}
