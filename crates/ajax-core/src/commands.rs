mod check;
mod context;
mod diff;
mod doctor;
mod lookup;
mod merge;
mod new_task;
mod open;
mod orphan_gc;
mod projection;
mod task_state;
mod task_window;
mod teardown;

pub use crate::adapters::DoctorEnvironment;
pub(crate) use check::check_task_plan_after_worktree_recreate;
pub use check::{
    check_task_plan, mark_task_check_failed, mark_task_check_started, mark_task_check_succeeded,
};
pub use context::{BranchAdoptionPlan, CommandContext, CommandError, CommandPlan, OpenMode};
pub use diff::diff_task_plan;
pub use doctor::{doctor, doctor_with_environment};
pub use merge::{mark_task_merge_failed, mark_task_merged, merge_task_plan};
pub use new_task::{
    is_agent_send_keys_command, is_git_worktree_add_command, is_task_window_new_session_command,
    mark_new_task_provisioning_failed, mark_new_task_provisioning_step_completed,
    mark_new_task_step_completed, new_task_plan, new_task_plan_with_observation, record_new_task,
    start_provisioning_step_for_command, start_task_identity, task_from_new_request,
    AgentStartMode, NewTaskRequest, StartPlanObservation, StartProvisioningStep,
};
pub use open::{mark_task_opened, mark_task_opened_at, open_task_plan};
pub use orphan_gc::{
    append_orphan_gc_to_plan, classify_orphans, collect_orphan_gc_commands, orphan_gc_commands,
    OrphanGcMode, OrphanGcTarget,
};
pub use task_window::{
    mark_task_window_repaired, task_window_repair_plan, task_window_repair_plan_with_open_mode,
};
pub use teardown::{
    clean_task_plan, drop_op_label, ensure_cleanup_git_status,
    format_drop_remaining_resources_detail, format_drop_teardown_incomplete_message,
    is_delete_branch_substrate_command, is_fast_worktree_remove_command, mark_drop_agent_stopped,
    mark_task_cleanup_step_completed, mark_task_force_removed, mark_task_removed,
    mark_task_removing, mark_task_teardown_incomplete, observe_drop_resources,
    observe_drop_resources_with_cache, plan_drop_from_observation,
    plan_drop_from_observation_for_task, remove_task_plan, sweep_cleanup_candidates,
    sweep_cleanup_plan, sweep_trash_commands, DropObservation, DropOp, RepoDropObservationCache,
    ResourceState, DROP_TEARDOWN_ORDER,
};

use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter},
    analysis::git_evidence::interpret_git_status,
    config::Config,
    models::{GitStatus, LifecycleStatus, SideFlag, Task},
    output::{
        CockpitProjection, CockpitResponse, CockpitView, InboxResponse, InspectResponse,
        NextResponse, RepoSummary, ReposResponse, TasksResponse,
    },
    registry::Registry,
};
use lookup::find_task;
use projection::{
    cockpit_projection as build_cockpit_projection, cockpit_summary, count_active_tasks,
    count_attention_items, count_lifecycle, inbox_from_cards, is_cockpit_menu_task,
    is_visible_task, task_card, task_summary,
};
use std::{collections::BTreeSet, path::Path, time::Duration, time::SystemTime};

const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn list_repos<R: Registry>(context: &CommandContext<R>) -> ReposResponse {
    let all_tasks = context.registry.list_tasks();
    list_repos_from_tasks(&context.config, all_tasks.as_slice())
}

fn list_repos_from_tasks(config: &Config, all_tasks: &[&Task]) -> ReposResponse {
    let repos = config
        .repos
        .iter()
        .map(|repo| {
            let repo_tasks: Vec<&Task> = all_tasks
                .iter()
                .copied()
                .filter(|task| task.repo == repo.name && is_visible_task(task))
                .collect();

            RepoSummary {
                name: repo.name.clone(),
                path: repo.path.display().to_string(),
                active_tasks: count_active_tasks(&repo_tasks),
                attention_items: count_attention_items(&repo_tasks),
                reviewable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Reviewable),
                cleanable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Cleanable),
            }
        })
        .collect();

    ReposResponse { repos }
}

pub fn list_tasks<R: Registry>(context: &CommandContext<R>, repo: Option<&str>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    list_tasks_from_tasks(all_tasks.as_slice(), repo)
}

fn list_tasks_from_tasks(tasks: &[&Task], repo: Option<&str>) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| repo.is_none_or(|repo_name| task.repo == repo_name))
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

pub fn review_queue<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    review_queue_from_tasks(all_tasks.as_slice())
}

fn review_queue_from_tasks(tasks: &[&Task]) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| {
            matches!(
                task.lifecycle_status,
                LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
            )
        })
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

pub fn inspect_task<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<InspectResponse, CommandError> {
    let Some(task) = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
    else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    Ok(InspectResponse {
        task: task_summary(task),
        branch: task.branch.clone(),
        worktree_path: task.worktree_path.display().to_string(),
        tmux_session: task.tmux_session.clone(),
        flags: task
            .side_flags()
            .map(|flag| format!("{flag:?}"))
            .collect::<Vec<_>>(),
    })
}

pub fn inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    inbox_from_tasks(tasks.as_slice())
}

pub fn cockpit_inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    inbox_from_tasks(tasks.as_slice())
}

fn inbox_from_tasks(tasks: &[&Task]) -> InboxResponse {
    let cards = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .map(task_card)
        .collect::<Vec<_>>();
    inbox_from_cards(&cards)
}

pub fn next<R: Registry>(context: &CommandContext<R>) -> NextResponse {
    NextResponse {
        item: inbox(context).items.into_iter().next(),
    }
}

pub fn status<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    list_tasks(context, None)
}

pub fn cockpit<R: Registry>(context: &CommandContext<R>) -> CockpitResponse {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let tasks = list_tasks_from_tasks(all_tasks.as_slice(), None);
    let review = review_queue_from_tasks(all_tasks.as_slice());
    let inbox = inbox_from_tasks(all_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks, &review);
    let next = NextResponse {
        item: inbox.items.first().cloned(),
    };

    CockpitResponse {
        summary,
        repos,
        tasks,
        review,
        inbox,
        next,
    }
}

pub fn cockpit_projection<R: Registry>(context: &CommandContext<R>) -> CockpitProjection {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review);
    build_cockpit_projection(all_tasks.as_slice(), summary)
}

pub fn cockpit_view<R: Registry>(context: &CommandContext<R>) -> CockpitView {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review);
    let projection = build_cockpit_projection(all_tasks.as_slice(), summary);
    let inbox = inbox_from_cards(&projection.cards);

    CockpitView {
        repos,
        cards: projection.cards,
        inbox,
    }
}

pub fn mark_stale_tasks<R: Registry>(context: &mut CommandContext<R>, now: SystemTime) -> u32 {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let mut tasks_changed = 0;

    for task_id in &task_ids {
        if let Some(task) = context.registry.get_task_mut(task_id) {
            let Ok(inactive_for) = now.duration_since(task.last_activity_at) else {
                continue;
            };

            if inactive_for >= STALE_AFTER && !task.has_side_flag(SideFlag::Stale) {
                task.add_side_flag(SideFlag::Stale);
                tasks_changed += 1;
            }
        }
    }

    tasks_changed
}

pub fn refresh_git_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    runner: &mut impl CommandRunner,
    merged: bool,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let git = GitAdapter::new("git");
    let output = runner
        .run(&git.status(&task.worktree_path.display().to_string()))
        .map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: None,
        }));
    }

    let Some(git_status) = interpret_git_status(&output.stdout, task.git_status.as_ref(), merged)
    else {
        return Ok(());
    };
    context
        .registry
        .update_git_status(&task.id, git_status)
        .map_err(CommandError::Registry)?;

    Ok(())
}

pub fn refresh_git_substrate_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CommandError> {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .filter(|task| {
            task.git_status.is_some()
                || task.has_side_flag(crate::models::SideFlag::WorktreeMissing)
                || task.has_side_flag(crate::models::SideFlag::BranchMissing)
        })
        .cloned()
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return Ok(false);
    }

    let git = GitAdapter::new("git");
    let mut updates = Vec::new();

    for repo in &context.config.repos {
        let repo_tasks = tasks
            .iter()
            .filter(|task| task.repo == repo.name)
            .collect::<Vec<_>>();
        if repo_tasks.is_empty() {
            continue;
        }

        let repo_path = repo.path.display().to_string();
        let worktrees_output = run_successful_command(runner, &git.list_worktrees(&repo_path))?;
        if worktrees_output.trim().is_empty() {
            continue;
        }
        let branches_output = run_successful_command(runner, &git.list_branches(&repo_path))?;
        let worktrees = GitAdapter::parse_worktrees(&worktrees_output);
        let branches = GitAdapter::parse_branches(&branches_output)
            .into_iter()
            .collect::<BTreeSet<_>>();

        for task in repo_tasks {
            let path_worktree = worktrees
                .iter()
                .find(|worktree| Path::new(&worktree.path) == task.worktree_path.as_path());
            let worktree_exists = path_worktree.is_some();
            let branch_exists = branches.contains(&task.branch);
            let current_branch = path_worktree.and_then(|worktree| worktree.branch.clone());
            let git_status = substrate_git_status(
                task.git_status.as_ref(),
                worktree_exists,
                branch_exists,
                current_branch,
            );

            if task.git_status.as_ref() != Some(&git_status) {
                updates.push((task.id.clone(), git_status));
            }
        }
    }

    let changed = !updates.is_empty();
    for (task_id, git_status) in updates {
        context
            .registry
            .update_git_status(&task_id, git_status)
            .map_err(CommandError::Registry)?;
    }

    Ok(changed)
}

pub fn mark_task_git_substrate_missing<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<bool, CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let git_status = substrate_git_status(task.git_status.as_ref(), false, false, None);
    if task.git_status.as_ref() == Some(&git_status) {
        return Ok(false);
    }

    context
        .registry
        .update_git_status(&task.id, git_status)
        .map_err(CommandError::Registry)?;

    Ok(true)
}

fn run_successful_command(
    runner: &mut impl CommandRunner,
    command: &CommandSpec,
) -> Result<String, CommandError> {
    let output = runner.run(command).map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: command.program.clone(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: command.cwd.clone(),
        }));
    }

    Ok(output.stdout)
}

fn substrate_git_status(
    previous: Option<&GitStatus>,
    worktree_exists: bool,
    branch_exists: bool,
    current_branch: Option<String>,
) -> GitStatus {
    let mut status = previous.cloned().unwrap_or(GitStatus {
        worktree_exists,
        branch_exists,
        current_branch: current_branch.clone(),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    status.worktree_exists = worktree_exists;
    status.branch_exists = branch_exists;
    status.current_branch = current_branch;

    if !worktree_exists {
        status.dirty = false;
        status.ahead = 0;
        status.behind = 0;
        status.untracked_files = 0;
        status.unpushed_commits = 0;
        status.conflicted = false;
        status.last_commit = None;
    }

    status
}

pub fn execute_plan(
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<Vec<CommandOutput>, CommandError> {
    crate::task_operations::kernel::execute_external_plan(plan, confirmed, runner)
}

#[cfg(test)]
mod tests;
