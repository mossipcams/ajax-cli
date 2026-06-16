use crate::models::{LifecycleStatus, Task};

pub(crate) fn resume_blocked_reasons(task: &Task) -> Vec<String> {
    let mut reasons = removed_reasons(task);
    if task_has_missing_resume_substrate(task) {
        reasons.push("task has missing substrate".to_string());
    }
    reasons
}

pub(crate) fn review_blocked_reasons(task: &Task) -> Vec<String> {
    let mut reasons = removed_reasons(task);
    if task_worktree_missing(task) {
        reasons.push("task worktree is missing".to_string());
    }
    reasons
}

pub(crate) fn ship_blocked_reasons(task: &Task) -> Vec<String> {
    let mut reasons = removed_reasons(task);
    if !matches!(
        task.lifecycle_status,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) {
        reasons.push("merge requires reviewable or mergeable lifecycle".to_string());
    }
    if task_has_missing_merge_substrate(task) {
        reasons.push("task has missing substrate".to_string());
    }
    reasons
}

pub(crate) fn clean_blocked_reasons(task: &Task) -> Vec<String> {
    let mut reasons = removed_reasons(task);
    if !matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged
            | LifecycleStatus::Cleanable
            | LifecycleStatus::Removing
            | LifecycleStatus::TeardownIncomplete
    ) {
        reasons.push("clean requires merged or cleanable lifecycle".to_string());
    }
    if task_has_missing_cleanup_substrate(task) {
        reasons.push("task has missing substrate".to_string());
    }
    reasons
}

pub(crate) fn remove_blocked_reasons(task: &Task) -> Vec<String> {
    removed_reasons(task)
}

fn removed_reasons(task: &Task) -> Vec<String> {
    if task.lifecycle_status == LifecycleStatus::Removed {
        vec!["task is removed".to_string()]
    } else {
        Vec::new()
    }
}

fn task_worktree_missing(task: &Task) -> bool {
    task.facts().worktree_missing
}

fn task_has_missing_resume_substrate(task: &Task) -> bool {
    task.facts().has_missing_substrate()
}

fn task_has_missing_merge_substrate(task: &Task) -> bool {
    let facts = task.facts();
    facts.worktree_missing || facts.branch_missing
}

fn task_has_missing_cleanup_substrate(task: &Task) -> bool {
    let facts = task.facts();
    facts.worktree_missing || facts.branch_missing
}

#[cfg(test)]
mod tests {
    use super::{
        clean_blocked_reasons, remove_blocked_reasons, resume_blocked_reasons,
        review_blocked_reasons, ship_blocked_reasons,
    };
    use crate::models::{AgentClient, LifecycleStatus, SideFlag, Task, TaskId};

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

    #[test]
    fn review_policy_blocks_missing_worktree_without_generic_missing_reason() {
        let mut task = task_with_status(LifecycleStatus::Active);
        task.add_side_flag(SideFlag::WorktreeMissing);

        assert_eq!(
            review_blocked_reasons(&task),
            vec!["task worktree is missing".to_string()]
        );
    }

    #[test]
    fn ship_policy_requires_reviewable_or_mergeable_lifecycle() {
        let task = task_with_status(LifecycleStatus::Active);

        assert_eq!(
            ship_blocked_reasons(&task),
            vec!["merge requires reviewable or mergeable lifecycle".to_string()]
        );
    }

    #[test]
    fn clean_policy_blocks_removed_tasks_with_missing_substrate() {
        let mut task = task_with_status(LifecycleStatus::Removed);
        task.add_side_flag(SideFlag::WorktreeMissing);

        assert_eq!(
            clean_blocked_reasons(&task),
            vec![
                "task is removed".to_string(),
                "clean requires merged or cleanable lifecycle".to_string(),
                "task has missing substrate".to_string(),
            ]
        );
    }

    #[test]
    fn resume_and_remove_policies_preserve_removed_reason() {
        let task = task_with_status(LifecycleStatus::Removed);

        assert_eq!(
            resume_blocked_reasons(&task),
            vec!["task is removed".to_string()]
        );
        assert_eq!(
            remove_blocked_reasons(&task),
            vec!["task is removed".to_string()]
        );
    }
}
