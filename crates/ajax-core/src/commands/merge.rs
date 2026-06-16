use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::GitAdapter,
    capability_policy,
    live::LiveStatusKind,
    models::{LifecycleStatus, LiveObservation, SideFlag, Task, TaskCondition, TaskConditionKind},
    registry::Registry,
};

use super::lookup::find_task;

pub fn merge_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("merge task: {qualified_handle}"));
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
    ) {
        let preflight_reasons = merge_preflight_blocked_reasons(task);
        if !preflight_reasons.is_empty() {
            plan.blocked_reasons = preflight_reasons;
            return Ok(plan);
        }
    }
    let reasons = capability_policy::ship_blocked_reasons(task);
    if !reasons.is_empty() {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let repo_path = super::lookup::task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    plan.requires_confirmation = task.side_flags().next().is_some();
    plan.commands
        .push(git.switch_branch(&repo_path, &task.base_branch));
    plan.commands
        .push(git.merge_branch(&repo_path, &task.branch));

    Ok(plan)
}

pub fn mark_task_merged<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    context
        .registry
        .update_lifecycle(&task.id, LifecycleStatus::Merged)
        .map_err(CommandError::Registry)?;
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.remove_side_flag(SideFlag::Conflicted);
        task.clear_condition(TaskConditionKind::MergeFailed);
        if task.live_status.as_ref().is_some_and(|status| {
            status.kind == LiveStatusKind::CommandFailed && status.summary == "merge failed"
        }) {
            task.live_status = None;
        }
    }
    Ok(())
}

pub fn mark_task_merge_failed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    conflicted: bool,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        if conflicted {
            task.add_side_flag(SideFlag::Conflicted);
            task.record_condition(TaskCondition::merge_failed(std::time::SystemTime::now()));
        } else {
            task.clear_condition(TaskConditionKind::MergeFailed);
        }
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "merge failed",
        ));
    }
    Ok(())
}

fn merge_preflight_blocked_reasons(task: &Task) -> Vec<String> {
    let facts = task.facts();
    let mut reasons = Vec::new();
    if facts.dirty || facts.conflicted {
        reasons.push("merge requires clean worktree evidence".to_string());
    }
    if facts.branch_missing {
        reasons.push("task branch is missing".to_string());
    }
    reasons
}
