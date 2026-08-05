use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{agent_launch_spec, AgentLaunch, CommandSpec, GitAdapter, TmuxAdapter},
    config::WorktreePlacement,
    lifecycle::mark_provisioning,
    models::{
        AgentAttempt, AgentClient, GitStatus, LifecycleStatus, RuntimeObservationSource, SideFlag,
        Task, TaskId, TaskOperationKind, TaskWindowStatus, TmuxStatus,
    },
    registry::{Registry, RegistryError},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const HUSKY_GUARD: &str =
    "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi";
pub const DEFAULT_TASK_WINDOW_NAME: &str = "task";
pub const ORIGIN_FETCH_FRESH_FOR: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartPlanObservation {
    pub origin_fetch_age: Option<Duration>,
    pub target_branch_exists: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartProvisioningStep {
    WorktreeCreated,
    TaskSessionCreated,
    AgentCommandSent,
}

pub fn new_task_plan<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
) -> Result<CommandPlan, CommandError> {
    new_task_plan_with_observation(
        context,
        request,
        &StartPlanObservation {
            origin_fetch_age: None,
            target_branch_exists: false,
        },
    )
}

pub fn new_task_plan_with_observation<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
    observation: &StartPlanObservation,
) -> Result<CommandPlan, CommandError> {
    validate_managed_repo_name(&request.repo)?;
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
    let worktree_path = ajax_worktree_path(
        &context.runtime_paths.worktree_placement,
        &repo.path,
        &request.repo,
        &branch,
        &handle,
    );
    let worktree_path_string = worktree_path.display().to_string();

    if worktree_path.exists() {
        return Err(CommandError::PlanBlocked(vec![format!(
            "worktree path already exists: {}",
            worktree_path.display()
        )]));
    }
    if observation.target_branch_exists {
        return Err(CommandError::PlanBlocked(vec![format!(
            "branch already exists: {branch}"
        )]));
    }
    if let Some(task) = context.registry.list_tasks().into_iter().find(|task| {
        task.lifecycle_status != LifecycleStatus::Removed && task.worktree_path == worktree_path
    }) {
        return Err(CommandError::PlanBlocked(vec![format!(
            "worktree path already claimed by task {}: {}",
            task.qualified_handle(),
            worktree_path_string
        )]));
    }
    if let Some(task) = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.lifecycle_status != LifecycleStatus::Removed && task.branch == branch)
    {
        return Err(CommandError::PlanBlocked(vec![format!(
            "branch already claimed by task {}: {branch}",
            task.qualified_handle()
        )]));
    }

    let tmux_session = format!("ajax-{}-{handle}", request.repo);
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let selected_agent = agent_from_name(&request.agent);
    let agent_launch = agent_launch_spec(
        &request.agent,
        selected_agent,
        &AgentLaunch {
            worktree_path: worktree_path_string.clone(),
            prompt: String::new(),
        },
    );
    let launch = agent_runtime_command(
        &qualified_handle,
        &context.runtime_paths.cache_dir.join("agent-runtime"),
        agent_launch,
    );
    let repo_path = repo.path.display().to_string();
    let mut plan = CommandPlan::new(format!("create task: {}", request.title));
    if observation
        .origin_fetch_age
        .is_none_or(|age| age >= ORIGIN_FETCH_FRESH_FOR)
    {
        plan.commands
            .push(git.fetch_origin_branch(&repo_path, &repo.default_branch));
    }
    plan.commands.push(git.add_worktree(
        &repo_path,
        &worktree_path_string,
        &branch,
        &format!("origin/{}", repo.default_branch),
    ));
    plan.commands.push(tmux.new_detached_task_session(
        &tmux_session,
        DEFAULT_TASK_WINDOW_NAME,
        &worktree_path_string,
    ));
    let agent_launch_line =
        fold_setup_into_agent_launch(repo.bootstrap.as_deref(), &command_line(&launch));
    plan.commands.push(tmux.send_agent_command(
        &tmux_session,
        DEFAULT_TASK_WINDOW_NAME,
        &agent_launch_line,
    ));

    Ok(plan)
}

pub fn task_from_new_request<R: Registry>(
    context: &CommandContext<R>,
    request: &NewTaskRequest,
) -> Result<Task, CommandError> {
    validate_managed_repo_name(&request.repo)?;
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
    let worktree_path = ajax_worktree_path(
        &context.runtime_paths.worktree_placement,
        &repo.path,
        &request.repo,
        &branch,
        &handle,
    );

    let mut task = Task::new(
        task_id,
        request.repo.clone(),
        handle,
        request.title.clone(),
        branch,
        repo.default_branch.clone(),
        worktree_path,
        tmux_session,
        DEFAULT_TASK_WINDOW_NAME,
        agent_from_name(&request.agent),
    );
    mark_provisioning(&mut task).map_err(|error| {
        CommandError::Registry(RegistryError::InvalidLifecycleTransition(error))
    })?;

    Ok(task)
}

pub fn start_task_identity(repo: &str, title: &str) -> TaskId {
    TaskId::new(format!("{repo}/{}", slugify_title(title)))
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
    let failed_step = next_incomplete_start_step(context, task_id);
    let task = context
        .registry
        .get_task_mut(task_id)
        .ok_or_else(|| CommandError::TaskNotFound(task_id.as_str().to_string()))?;
    task.add_side_flag(SideFlag::NeedsInput);
    task.metadata
        .insert("start_failed_step".to_string(), failed_step.to_string());
    task.metadata.insert(
        "operator_recommendation".to_string(),
        "retry ajax start after checking the failed provisioning step".to_string(),
    );

    Ok(())
}

fn next_incomplete_start_step<R: Registry>(
    context: &CommandContext<R>,
    task_id: &TaskId,
) -> &'static str {
    let completed = context
        .registry
        .step_receipts_for_task(task_id)
        .into_iter()
        .filter(|receipt| receipt.operation == TaskOperationKind::Start)
        .map(|receipt| receipt.step_key.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    if !completed.contains("worktree_created") {
        "worktree_created"
    } else if !completed.contains("task_session_created") {
        "task_session_created"
    } else if !completed.contains("agent_command_sent") {
        "agent_command_sent"
    } else {
        "open_task"
    }
}

pub fn mark_new_task_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    plan: &CommandPlan,
    command_index: usize,
) -> Result<(), CommandError> {
    let Some(step) = plan
        .commands
        .get(command_index)
        .and_then(start_provisioning_step_for_command)
    else {
        return Ok(());
    };
    mark_new_task_provisioning_step_completed(context, task_id, step)
}

pub fn mark_new_task_provisioning_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    step: StartProvisioningStep,
) -> Result<(), CommandError> {
    if step == StartProvisioningStep::AgentCommandSent {
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

    match step {
        StartProvisioningStep::WorktreeCreated => {
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
        StartProvisioningStep::TaskSessionCreated => {
            context
                .registry
                .update_tmux_status(task_id, Some(TmuxStatus::present(task.tmux_session)))
                .map_err(CommandError::Registry)?;
            context
                .registry
                .update_task_window_status(
                    task_id,
                    Some(TaskWindowStatus::present(
                        task.task_window,
                        task.worktree_path,
                    )),
                )
                .map_err(CommandError::Registry)?;
            if let Some(task) = context.registry.get_task_mut(task_id) {
                task.refresh_runtime_projection_from_source(
                    RuntimeObservationSource::CommandResult,
                );
            }
        }
        StartProvisioningStep::AgentCommandSent => {
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
    }

    Ok(())
}

pub fn is_git_worktree_add_command(command: &CommandSpec) -> bool {
    command.program == "git"
        && command
            .args
            .windows(2)
            .any(|window| window == ["worktree", "add"])
}

pub fn is_task_window_new_session_command(command: &CommandSpec) -> bool {
    command.program == "tmux" && command.args.first().is_some_and(|arg| arg == "new-session")
}

pub fn is_agent_send_keys_command(command: &CommandSpec) -> bool {
    command.program == "tmux" && command.args.first().is_some_and(|arg| arg == "send-keys")
}

pub fn start_provisioning_step_for_command(command: &CommandSpec) -> Option<StartProvisioningStep> {
    if is_git_worktree_add_command(command) {
        Some(StartProvisioningStep::WorktreeCreated)
    } else if is_task_window_new_session_command(command) {
        Some(StartProvisioningStep::TaskSessionCreated)
    } else if is_agent_send_keys_command(command) {
        Some(StartProvisioningStep::AgentCommandSent)
    } else {
        None
    }
}

fn ajax_worktree_path(
    placement: &WorktreePlacement,
    repo_path: &Path,
    repo_name: &str,
    branch: &str,
    handle: &str,
) -> PathBuf {
    match placement {
        WorktreePlacement::LegacySibling => legacy_ajax_worktree_path(repo_path, branch),
        WorktreePlacement::Root(root) => root
            .join(rooted_repo_dir(repo_name, repo_path))
            .join(handle),
    }
}

fn legacy_ajax_worktree_path(repo_path: &Path, branch: &str) -> PathBuf {
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

fn rooted_repo_dir(repo_name: &str, repo_path: &Path) -> String {
    let slug = slugify_title(repo_name);
    format!("{slug}-{:08x}", short_path_hash(repo_path))
}

fn short_path_hash(path: &Path) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn command_line(command: &CommandSpec) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn agent_runtime_command(
    task_id: &str,
    state_root: &Path,
    agent_command: CommandSpec,
) -> CommandSpec {
    let mut args = vec![
        "__agent-runtime".to_string(),
        "--task-id".to_string(),
        task_id.to_string(),
        "--state-root".to_string(),
        state_root.display().to_string(),
        "--".to_string(),
        agent_command.program,
    ];
    args.extend(agent_command.args);
    CommandSpec {
        program: "ajax-cli".to_string(),
        args,
        cwd: agent_command.cwd,
        mode: agent_command.mode,
        timeout: agent_command.timeout,
    }
}

fn fold_setup_into_agent_launch(bootstrap: Option<&str>, agent_line: &str) -> String {
    match bootstrap {
        Some(bootstrap) => format!("{HUSKY_GUARD}; {bootstrap} && {agent_line}"),
        None => format!("{HUSKY_GUARD}; {agent_line}"),
    }
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
    match name.to_ascii_lowercase().as_str() {
        "claude" => AgentClient::Claude,
        "codex" => AgentClient::Codex,
        "cursor" => AgentClient::Cursor,
        "pi" => AgentClient::Pi,
        _ => AgentClient::Other,
    }
}

fn validate_managed_repo_name(repo: &str) -> Result<(), CommandError> {
    if repo.is_empty() || repo.contains('/') || repo.contains('\\') || repo.contains("..") {
        return Err(CommandError::PlanBlocked(vec![format!(
            "invalid repo name: {repo}"
        )]));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
