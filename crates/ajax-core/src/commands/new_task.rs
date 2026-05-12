use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{AgentAdapter, AgentLaunch, CommandSpec, GitAdapter, TmuxAdapter},
    lifecycle::mark_provisioning,
    models::{
        AgentAttempt, AgentClient, GitStatus, LifecycleStatus, SideFlag, Task, TaskId, TmuxStatus,
        WorktrunkStatus,
    },
    registry::{Registry, RegistryError},
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
}

pub fn new_task_plan<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
) -> Result<CommandPlan, CommandError> {
    let Some(repo) = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo)
    else {
        return Err(CommandError::RepoNotFound(request.repo));
    };

    let handle = slugify_title(&request.title);
    let qualified_handle = format!("{}/{}", request.repo, handle);
    if context.registry.list_tasks().into_iter().any(|task| {
        task.qualified_handle() == qualified_handle
            && task.lifecycle_status != LifecycleStatus::Removed
    }) {
        return Err(CommandError::PlanBlocked(vec![format!(
            "task already exists: {qualified_handle}"
        )]));
    }

    let branch = format!("ajax/{handle}");
    let worktree_path = ajax_worktree_path(&repo.path, &branch);
    let worktree_path_string = worktree_path.display().to_string();
    let tmux_session = format!("ajax-{}-{handle}", request.repo);
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let agent = AgentAdapter::new(&request.agent);
    let launch = agent.launch(&AgentLaunch {
        worktree_path: worktree_path_string.clone(),
        prompt: request.title.clone(),
    });
    let mut plan = CommandPlan::new(format!("create task: {}", request.title));
    plan.commands.push(git.add_worktree(
        &repo.path.display().to_string(),
        &worktree_path_string,
        &branch,
        &repo.default_branch,
    ));
    plan.commands.push(tmux.new_detached_worktrunk_session(
        &tmux_session,
        "worktrunk",
        &worktree_path_string,
    ));
    plan.commands
        .push(tmux.send_agent_command(&tmux_session, "worktrunk", &command_line(&launch)));

    Ok(plan)
}

pub fn task_from_new_request<R: Registry>(
    context: &CommandContext<R>,
    request: &NewTaskRequest,
) -> Result<Task, CommandError> {
    let Some(repo) = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo)
    else {
        return Err(CommandError::RepoNotFound(request.repo.clone()));
    };
    let handle = slugify_title(&request.title);
    let task_id = TaskId::new(format!("{}/{}", request.repo, handle));
    let branch = format!("ajax/{handle}");
    let tmux_session = format!("ajax-{}-{handle}", request.repo);
    let worktree_path = ajax_worktree_path(&repo.path, &branch);

    let mut task = Task::new(
        task_id,
        request.repo.clone(),
        handle,
        request.title.clone(),
        branch,
        repo.default_branch.clone(),
        worktree_path,
        tmux_session,
        "worktrunk",
        agent_from_name(&request.agent),
    );
    mark_provisioning(&mut task).map_err(|error| {
        CommandError::Registry(RegistryError::InvalidLifecycleTransition(error))
    })?;

    Ok(task)
}

pub fn record_new_task<R: Registry>(
    context: &mut CommandContext<R>,
    request: &NewTaskRequest,
) -> Result<Task, CommandError> {
    let task = task_from_new_request(context, request)?;
    if let Some(existing) = context.registry.get_task_mut(&task.id) {
        if existing.lifecycle_status == LifecycleStatus::Removed {
            *existing = task.clone();
            return Ok(task);
        }
    }
    context
        .registry
        .create_task(task.clone())
        .map_err(CommandError::Registry)?;

    Ok(task)
}

pub fn mark_new_task_provisioning_failed<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
) -> Result<(), CommandError> {
    context
        .registry
        .update_lifecycle(task_id, LifecycleStatus::Error)
        .map_err(CommandError::Registry)?;
    let task = context
        .registry
        .get_task_mut(task_id)
        .ok_or_else(|| CommandError::TaskNotFound(task_id.as_str().to_string()))?;
    task.add_side_flag(SideFlag::NeedsInput);

    Ok(())
}

pub fn mark_new_task_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    step_index: usize,
) -> Result<(), CommandError> {
    if step_index == 2 {
        context
            .registry
            .update_lifecycle(task_id, LifecycleStatus::Active)
            .map_err(CommandError::Registry)?;
    }

    let task = context
        .registry
        .get_task(task_id)
        .cloned()
        .ok_or_else(|| CommandError::TaskNotFound(task_id.as_str().to_string()))?;

    match step_index {
        0 => {
            context
                .registry
                .update_git_status(
                    task_id,
                    GitStatus {
                        worktree_exists: true,
                        branch_exists: true,
                        current_branch: Some(task.branch),
                        dirty: false,
                        ahead: 0,
                        behind: 0,
                        merged: false,
                        untracked_files: 0,
                        unpushed_commits: 0,
                        conflicted: false,
                        last_commit: None,
                    },
                )
                .map_err(CommandError::Registry)?;
        }
        1 => {
            context
                .registry
                .update_tmux_status(task_id, Some(TmuxStatus::present(task.tmux_session)))
                .map_err(CommandError::Registry)?;
            context
                .registry
                .update_worktrunk_status(
                    task_id,
                    Some(WorktrunkStatus::present(
                        task.worktrunk_window,
                        task.worktree_path,
                    )),
                )
                .map_err(CommandError::Registry)?;
        }
        2 => {
            let task = context
                .registry
                .get_task_mut(task_id)
                .ok_or_else(|| CommandError::TaskNotFound(task_id.as_str().to_string()))?;
            task.agent_attempts.push(AgentAttempt::new(
                task.selected_agent,
                task.worktree_path.display().to_string(),
            ));
            task.add_side_flag(SideFlag::AgentRunning);
        }
        _ => {}
    }

    Ok(())
}

fn ajax_worktree_path(repo_path: &Path, branch: &str) -> PathBuf {
    let worktree_name = branch.replace('/', "-");
    let repo_dir = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    let worktrees_dir = format!("{repo_dir}__worktrees");

    repo_path
        .parent()
        .unwrap_or(repo_path)
        .join(worktrees_dir)
        .join(worktree_name)
}

fn command_line(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/' | b'.'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_dash = false;

    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "task".to_string()
    } else {
        slug
    }
}

fn agent_from_name(name: &str) -> AgentClient {
    match name {
        "claude" => AgentClient::Claude,
        "codex" => AgentClient::Codex,
        _ => AgentClient::Other,
    }
}
