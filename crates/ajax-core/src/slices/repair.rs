use crate::{
    adapters::{CommandOutput, CommandRunner},
    models::{LifecycleStatus, OperatorAction, Task},
    recommended::{available_built_in_decision, blocked_built_in_decision, TaskActionDecision},
    registry::Registry,
    use_cases::{CommandContext, CommandError, CommandPlan, OpenMode},
};

pub fn decision(task: &Task) -> TaskActionDecision {
    if task.lifecycle_status == LifecycleStatus::Removed {
        return blocked_built_in_decision(OperatorAction::Repair, "task is removed", false);
    }
    let facts = task.facts();
    if facts.worktree_missing || facts.branch_missing {
        return blocked_built_in_decision(
            OperatorAction::Repair,
            "task has missing substrate",
            false,
        );
    }
    if facts.has_missing_substrate() || task.runtime_projection.observation_error.is_some() {
        available_built_in_decision(
            OperatorAction::Repair,
            crate::recommended::primary_blocker_reason(task).unwrap_or("repair"),
            false,
        )
    } else {
        available_built_in_decision(OperatorAction::Repair, "repair", false)
    }
}

pub fn plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    super::task_action::plan_repair(context, qualified_handle, open_mode)
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    super::task_action::execute_repair(context, qualified_handle, plan, confirmed, runner)
}

#[cfg(test)]
mod tests {
    use super::decision;
    use crate::{
        models::{AgentClient, GitStatus, OperatorAction, Task, TaskId},
        recommended::{ActionAvailability, TaskActionDecision, TaskActionId},
    };

    fn task() -> Task {
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

    #[test]
    fn repair_decision_blocks_missing_branch_without_side_flag() {
        let mut task = task();
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: false,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });

        assert_eq!(
            decision(&task),
            TaskActionDecision {
                id: TaskActionId::BuiltIn(OperatorAction::Repair),
                availability: ActionAvailability::Blocked,
                reason: "task has missing substrate".to_string(),
                requires_confirmation: false,
            }
        );
    }
}
