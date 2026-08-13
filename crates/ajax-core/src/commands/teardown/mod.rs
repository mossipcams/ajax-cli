use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{CommandRunner, CommandSpec, GitAdapter, TmuxAdapter},
    lifecycle::force_mark_removed,
    models::{LifecycleStatus, SafetyClassification, SideFlag, Task, TaskWindowStatus, TmuxStatus},
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    policy::cleanup_safety,
    registry::{Registry, RegistryError, RegistryEventKind},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::lookup::{find_task, task_repo_path, update_task_lifecycle};

pub fn mark_task_cleanup_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    command: &CommandSpec,
) -> Result<bool, CommandError> {
    let task = find_task(context, qualified_handle)?.clone();

    if command.program == "tmux"
        && command
            .args
            .first()
            .is_some_and(|arg| arg == "kill-session")
        && command.args.iter().any(|arg| arg == &task.tmux_session)
    {
        context
            .registry
            .update_tmux_status(
                &task.id,
                Some(TmuxStatus {
                    exists: false,
                    session_name: task.tmux_session.clone(),
                }),
            )
            .map_err(CommandError::Registry)?;
        context
            .registry
            .update_task_window_status(
                &task.id,
                Some(TaskWindowStatus::missing(
                    task.task_window.clone(),
                    task.worktree_path.clone(),
                )),
            )
            .map_err(CommandError::Registry)?;
        return Ok(true);
    }

    if is_fast_worktree_remove_command(command)
        && command
            .args
            .get(4)
            .is_some_and(|arg| arg == &task.worktree_path.display().to_string())
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.worktree_exists = false;
            git_status.dirty = false;
            git_status.untracked_files = 0;
            git_status.conflicted = false;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::WorktreeMissing);
            task.remove_side_flag(SideFlag::Dirty);
            task.remove_side_flag(SideFlag::Conflicted);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "worktree")
        && command.args.iter().any(|arg| arg == "remove")
        && command
            .args
            .iter()
            .any(|arg| arg == &task.worktree_path.display().to_string())
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.worktree_exists = false;
            git_status.dirty = false;
            git_status.untracked_files = 0;
            git_status.conflicted = false;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::WorktreeMissing);
            task.remove_side_flag(SideFlag::Dirty);
            task.remove_side_flag(SideFlag::Conflicted);
        }
        return Ok(true);
    }

    if is_delete_branch_substrate_command(command)
        && command.args.get(4).is_some_and(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "push")
        && command.args.iter().any(|arg| arg == "--delete")
        && command.args.iter().any(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "branch")
        && (command.args.iter().any(|arg| arg == "-d")
            || command.args.iter().any(|arg| arg == "-D"))
        && command.args.iter().any(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    Ok(false)
}

pub fn is_fast_worktree_remove_command(command: &CommandSpec) -> bool {
    command.program == "sh"
        && command.args.first().is_some_and(|arg| arg == "-c")
        && command
            .args
            .get(2)
            .is_some_and(|arg| arg == "ajax-fast-worktree-remove")
}

pub fn is_delete_branch_substrate_command(command: &CommandSpec) -> bool {
    command.program == "sh"
        && command.args.first().is_some_and(|arg| arg == "-c")
        && command
            .args
            .get(2)
            .is_some_and(|arg| arg == "ajax-delete-branch")
}

pub fn clean_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("clean task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Clean)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let safety = cleanup_safety(task);

    match safety.classification {
        SafetyClassification::Safe => {
            plan.commands = native_cleanup_commands(context, task)?;
        }
        SafetyClassification::NeedsConfirmation | SafetyClassification::Dangerous => {
            plan.requires_confirmation = true;
            plan.commands = native_cleanup_commands(context, task)?;
        }
        SafetyClassification::Blocked => {
            plan.blocked_reasons = safety.reasons;
        }
    }

    Ok(plan)
}

pub fn remove_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("remove task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Remove)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    plan.requires_confirmation = true;
    plan.commands = native_remove_commands(context, task)?;

    Ok(plan)
}

pub fn ensure_cleanup_git_status<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    runner: &mut impl CommandRunner,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let merged = task.lifecycle_status == LifecycleStatus::Merged
        || task.lifecycle_status == LifecycleStatus::Cleanable
        || task.git_status.as_ref().is_some_and(|status| status.merged);
    super::refresh_git_evidence(context, qualified_handle, runner, merged)
}

pub fn mark_task_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Removed)
}

pub fn mark_task_removing<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Removing)
}

pub fn mark_task_force_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    let Some(task) = context.registry.get_task_mut(&task_id) else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    force_mark_removed(task).map_err(|error| {
        CommandError::Registry(RegistryError::InvalidLifecycleTransition(error))
    })?;
    task.last_activity_at = SystemTime::now();
    task.remove_side_flag(SideFlag::Stale);
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::LifecycleChanged,
            "lifecycle changed to Removed",
        )
        .map_err(CommandError::Registry)
}

pub fn sweep_cleanup_plan<R: Registry>(context: &CommandContext<R>) -> CommandPlan {
    let mut plan = CommandPlan::new("sweep cleanup");

    plan.commands = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| super::projection::is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .filter_map(|task| native_cleanup_commands(context, task).ok())
        .flatten()
        .collect();
    plan.commands.extend(sweep_trash_commands(context));

    plan
}

pub fn sweep_cleanup_candidates<R: Registry>(context: &CommandContext<R>) -> Vec<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| super::projection::is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .map(Task::qualified_handle)
        .collect()
}

pub fn sweep_trash_commands<R: Registry>(context: &CommandContext<R>) -> Vec<CommandSpec> {
    worktree_roots(context)
        .into_iter()
        .map(|worktree_root| sweep_trash_command(&worktree_root))
        .collect()
}

fn native_cleanup_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, false)
}

fn native_remove_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, true)
}

fn worktree_roots<R: Registry>(context: &CommandContext<R>) -> Vec<PathBuf> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter_map(|task| task.worktree_path.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sweep_trash_command(worktree_root: &Path) -> CommandSpec {
    let trash_dir = worktree_root.join(".ajax-trash").display().to_string();
    CommandSpec::new(
        "sh",
        [
            "-c",
            "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi",
            "ajax-trash-sweep",
            &trash_dir,
        ],
    )
}
fn native_teardown_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
    force: bool,
) -> Result<Vec<CommandSpec>, CommandError> {
    let repo_path = task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let mut commands = Vec::new();

    if task
        .git_status
        .as_ref()
        .is_none_or(|status| status.worktree_exists)
    {
        let worktree_path = task.worktree_path.display().to_string();
        let needs_force = force
            || task.git_status.as_ref().is_some_and(|status| {
                status.dirty
                    || status.untracked_files > 0
                    || status.conflicted
                    || task.has_side_flag(SideFlag::Dirty)
                    || task.has_side_flag(SideFlag::Conflicted)
            });
        let command = if needs_force {
            git.force_remove_worktree(&repo_path, &worktree_path)
        } else {
            git.remove_worktree(&repo_path, &worktree_path)
        };
        commands.push(command);
    }
    if task
        .git_status
        .as_ref()
        .is_none_or(|status| status.branch_exists)
    {
        let needs_force = force
            || task
                .git_status
                .as_ref()
                .is_some_and(|status| !status.merged);
        let command = git.delete_branch_substrate(&repo_path, &task.branch, needs_force);
        commands.push(command);
    }
    if task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
    {
        commands.push(tmux.kill_session(&task.tmux_session));
    }

    Ok(commands)
}

mod drop_observation;

pub use drop_observation::{
    drop_op_label, format_drop_remaining_resources_detail, format_drop_teardown_incomplete_message,
    mark_drop_agent_stopped, mark_task_teardown_incomplete, observe_drop_resources,
    observe_drop_resources_with_cache, plan_drop_from_observation,
    plan_drop_from_observation_for_task, DropObservation, DropOp, RepoDropObservationCache,
    ResourceState, DROP_TEARDOWN_ORDER,
};

#[cfg(test)]
mod tests;
