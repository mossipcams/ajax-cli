use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{CommandRunner, CommandSpec, GitAdapter, TmuxAdapter},
    lifecycle::force_mark_removed,
    models::{LifecycleStatus, SafetyClassification, SideFlag, Task, TmuxStatus, WorktrunkStatus},
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    policy::cleanup_safety,
    registry::{Registry, RegistryError, RegistryEventKind},
};
use std::time::SystemTime;

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
            .update_worktrunk_status(
                &task.id,
                Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                }),
            )
            .map_err(CommandError::Registry)?;
        let task = context
            .registry
            .get_task_mut(&task.id)
            .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
        task.remove_side_flag(SideFlag::TmuxMissing);
        task.remove_side_flag(SideFlag::WorktrunkMissing);
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

fn native_cleanup_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, TeardownMode::Policy)
}

fn native_remove_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, TeardownMode::Force)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TeardownMode {
    Policy,
    Force,
}

fn native_teardown_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
    mode: TeardownMode,
) -> Result<Vec<CommandSpec>, CommandError> {
    let repo_path = task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let mut commands = Vec::new();

    if task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
    {
        commands.push(tmux.kill_session(&task.tmux_session));
    }
    if task
        .git_status
        .as_ref()
        .is_none_or(|status| status.worktree_exists)
    {
        let worktree_path = task.worktree_path.display().to_string();
        let needs_force = mode == TeardownMode::Force
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
        let needs_force = mode == TeardownMode::Force
            || task
                .git_status
                .as_ref()
                .is_some_and(|status| !status.merged);
        let command = if needs_force {
            git.force_delete_branch(&repo_path, &task.branch)
        } else {
            git.delete_branch(&repo_path, &task.branch)
        };
        commands.push(command);
    }

    Ok(commands)
}
