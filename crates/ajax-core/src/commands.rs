use crate::{
    adapters::{
        CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter, TmuxAdapter,
        WorkmuxAdapter, WorkmuxNewTask,
    },
    attention::derive_attention_items,
    config::Config,
    live::{apply_observation, classify_pane},
    models::{AgentClient, LifecycleStatus, SafetyClassification, SideFlag, Task, TaskId},
    output::{
        CockpitResponse, DoctorCheck, DoctorResponse, InboxResponse, InspectResponse, NextResponse,
        ReconcileResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse,
    },
    policy::cleanup_safety,
    reconcile::{reconcile_task, reconcile_task_filesystem, ReconciliationInput},
    registry::{Registry, RegistryError},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub struct CommandContext<R> {
    pub config: Config,
    pub registry: R,
}

impl<R> CommandContext<R> {
    pub fn new(config: Config, registry: R) -> Self {
        Self { config, registry }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandError {
    TaskNotFound(String),
    RepoNotFound(String),
    ConfirmationRequired,
    PlanBlocked(Vec<String>),
    CommandRun(CommandRunError),
    Registry(RegistryError),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CommandPlan {
    pub title: String,
    pub commands: Vec<CommandSpec>,
    pub requires_confirmation: bool,
    pub blocked_reasons: Vec<String>,
}

impl CommandPlan {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            commands: Vec::new(),
            requires_confirmation: false,
            blocked_reasons: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenMode {
    Attach,
    SwitchClient,
}

pub fn list_repos<R: Registry>(context: &CommandContext<R>) -> ReposResponse {
    let repos = context
        .config
        .repos
        .iter()
        .map(|repo| {
            let repo_tasks: Vec<&Task> = context
                .registry
                .list_tasks()
                .into_iter()
                .filter(|task| task.repo == repo.name)
                .collect();

            RepoSummary {
                name: repo.name.clone(),
                path: repo.path.display().to_string(),
                active_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Active),
                reviewable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Reviewable),
                cleanable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Cleanable),
                broken_tasks: repo_tasks
                    .iter()
                    .filter(|task| {
                        task.has_side_flag(SideFlag::TmuxMissing)
                            || task.has_side_flag(SideFlag::WorktreeMissing)
                            || task.has_side_flag(SideFlag::WorktrunkMissing)
                            || task.has_side_flag(SideFlag::BranchMissing)
                    })
                    .count() as u32,
            }
        })
        .collect();

    ReposResponse { repos }
}

pub fn list_tasks<R: Registry>(context: &CommandContext<R>, repo: Option<&str>) -> TasksResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| repo.is_none_or(|repo_name| task.repo == repo_name))
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

pub fn review_queue<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
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
        .cloned()
        .collect::<Vec<_>>();

    InboxResponse {
        items: derive_attention_items(&tasks),
    }
}

pub fn next<R: Registry>(context: &CommandContext<R>) -> NextResponse {
    NextResponse {
        item: inbox(context).items.into_iter().next(),
    }
}

pub fn doctor<R: Registry>(context: &CommandContext<R>) -> DoctorResponse {
    DoctorResponse {
        checks: vec![
            DoctorCheck {
                name: "config".to_string(),
                ok: true,
                message: format!("{} repo(s) configured", context.config.repos.len()),
            },
            DoctorCheck {
                name: "registry".to_string(),
                ok: true,
                message: format!("{} task(s) tracked", context.registry.list_tasks().len()),
            },
        ],
    }
}

pub fn status<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    list_tasks(context, None)
}

pub fn cockpit<R: Registry>(context: &CommandContext<R>) -> CockpitResponse {
    CockpitResponse {
        repos: list_repos(context),
        tasks: list_tasks(context, None),
        review: review_queue(context),
        inbox: inbox(context),
    }
}

pub fn reconcile_filesystem<R: Registry>(context: &mut CommandContext<R>) -> ReconcileResponse {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let mut tasks_changed = 0;

    for task_id in &task_ids {
        if let Some(task) = context.registry.get_task_mut(task_id) {
            let already_missing = task.has_side_flag(SideFlag::WorktreeMissing);
            reconcile_task_filesystem(task);
            if !already_missing && task.has_side_flag(SideFlag::WorktreeMissing) {
                tasks_changed += 1;
            }
        }
    }
    tasks_changed += mark_stale_tasks(context, SystemTime::now());

    ReconcileResponse {
        tasks_checked: task_ids.len() as u32,
        tasks_changed,
    }
}

pub fn reconcile_external<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<ReconcileResponse, CommandError> {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    if task_ids.is_empty() {
        return Ok(ReconcileResponse {
            tasks_checked: 0,
            tasks_changed: 0,
        });
    }

    let tmux = TmuxAdapter::new("tmux");
    let git = GitAdapter::new("git");
    let sessions_output = runner
        .run(&tmux.list_sessions())
        .map_err(CommandError::CommandRun)?;
    let mut tasks_changed = 0;

    for task_id in &task_ids {
        let Some(task_snapshot) = context.registry.get_task(task_id).cloned() else {
            continue;
        };
        let worktree_path = task_snapshot.worktree_path.display().to_string();
        let git_output = runner
            .run(&git.status(&worktree_path))
            .map_err(CommandError::CommandRun)?;
        let git_status = if git_output.status_code == 0 {
            let status = GitAdapter::parse_status(&git_output.stdout, false);
            let merged = if status.branch_exists {
                runner
                    .run(&git.merge_base_is_ancestor(
                        &worktree_path,
                        &task_snapshot.branch,
                        &task_snapshot.base_branch,
                    ))
                    .map_err(CommandError::CommandRun)?
                    .status_code
                    == 0
            } else {
                false
            };
            GitAdapter::parse_status(&git_output.stdout, merged)
        } else {
            missing_git_status()
        };
        let tmux_status =
            TmuxAdapter::parse_session_status(&task_snapshot.tmux_session, &sessions_output.stdout);
        let windows_output = if tmux_status.exists {
            runner
                .run(&tmux.list_windows(&task_snapshot.tmux_session))
                .map_err(CommandError::CommandRun)?
                .stdout
        } else {
            String::new()
        };
        let worktrunk_status = TmuxAdapter::parse_worktrunk_status(
            &task_snapshot.worktrunk_window,
            &worktree_path,
            &windows_output,
        );
        let live_observation = if !git_status.worktree_exists {
            Some(crate::models::LiveObservation::new(
                crate::models::LiveStatusKind::WorktreeMissing,
                "worktree missing",
            ))
        } else if !tmux_status.exists {
            Some(crate::models::LiveObservation::new(
                crate::models::LiveStatusKind::TmuxMissing,
                "tmux session missing",
            ))
        } else if !worktrunk_status.exists || !worktrunk_status.points_at_expected_path {
            Some(crate::models::LiveObservation::new(
                crate::models::LiveStatusKind::WorktrunkMissing,
                "worktrunk missing or pointed at the wrong path",
            ))
        } else {
            let pane_output = runner
                .run(
                    &tmux
                        .capture_pane(&task_snapshot.tmux_session, &task_snapshot.worktrunk_window),
                )
                .map_err(CommandError::CommandRun)?;
            Some(classify_pane(&pane_output.stdout))
        };

        if let Some(task) = context.registry.get_task_mut(task_id) {
            let before = task.clone();
            reconcile_task(
                task,
                ReconciliationInput {
                    git_status: Some(git_status),
                    tmux_status: Some(tmux_status),
                    worktrunk_status: Some(worktrunk_status),
                },
            );
            if let Some(observation) = live_observation {
                apply_observation(task, observation);
            }

            if task != &before {
                tasks_changed += 1;
            }
        }
    }
    tasks_changed += mark_stale_tasks(context, SystemTime::now());

    Ok(ReconcileResponse {
        tasks_checked: task_ids.len() as u32,
        tasks_changed,
    })
}

pub fn mark_stale_tasks<R: Registry>(context: &mut CommandContext<R>, now: SystemTime) -> u32 {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
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

    let workmux = WorkmuxAdapter::new("workmux");
    let branch = format!("ajax/{}", slugify_title(&request.title));
    let mut plan = CommandPlan::new(format!("create task: {}", request.title));
    plan.commands.push(workmux.add_task(&WorkmuxNewTask {
        repo_path: repo.path.display().to_string(),
        branch,
        title: request.title,
        agent: request.agent,
    }));

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
    let worktree_path = repo.path.join(".ajax-worktrees").join(&handle);

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
    task.lifecycle_status = LifecycleStatus::Provisioning;

    Ok(task)
}

pub fn record_new_task<R: Registry>(
    context: &mut CommandContext<R>,
    request: &NewTaskRequest,
) -> Result<Task, CommandError> {
    let task = task_from_new_request(context, request)?;
    context
        .registry
        .create_task(task.clone())
        .map_err(CommandError::Registry)?;

    Ok(task)
}

pub fn open_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let workmux = WorkmuxAdapter::new("workmux");
    let tmux = TmuxAdapter::new("tmux");
    let mut plan = CommandPlan::new(format!("open task: {qualified_handle}"));

    plan.commands.push(command_in_task_repo(
        context,
        task,
        workmux.open_task(&task.branch),
    )?);
    match mode {
        OpenMode::Attach => plan.commands.push(tmux.attach_session(&task.tmux_session)),
        OpenMode::SwitchClient => plan.commands.push(tmux.switch_client(&task.tmux_session)),
    }

    Ok(plan)
}

pub fn mark_task_opened<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Active)
}

pub fn merge_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let workmux = WorkmuxAdapter::new("workmux");
    let mut plan = CommandPlan::new(format!("merge task: {qualified_handle}"));

    plan.requires_confirmation = task.side_flags().next().is_some();
    plan.commands.push(command_in_task_repo(
        context,
        task,
        workmux.merge_task(&task.branch),
    )?);

    Ok(plan)
}

pub fn mark_task_merged<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Merged)
}

pub fn clean_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let workmux = WorkmuxAdapter::new("workmux");
    let safety = cleanup_safety(task);
    let mut plan = CommandPlan::new(format!("clean task: {qualified_handle}"));

    match safety.classification {
        SafetyClassification::Safe => plan.commands.push(command_in_task_repo(
            context,
            task,
            workmux.remove_task(&task.branch),
        )?),
        SafetyClassification::NeedsConfirmation | SafetyClassification::Dangerous => {
            plan.requires_confirmation = true;
            plan.commands.push(command_in_task_repo(
                context,
                task,
                workmux.remove_task(&task.branch),
            )?);
        }
        SafetyClassification::Blocked => {
            plan.blocked_reasons = safety.reasons;
        }
    }

    Ok(plan)
}

pub fn check_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let Some(test_command) = context
        .config
        .test_commands
        .iter()
        .find(|test_command| test_command.repo == task.repo)
    else {
        return Err(CommandError::PlanBlocked(vec![format!(
            "no test command configured for repo {}",
            task.repo
        )]));
    };
    let mut plan = CommandPlan::new(format!("check task: {qualified_handle}"));
    plan.commands.push(
        CommandSpec::new("sh", ["-lc", test_command.command.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

pub fn diff_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("diff task: {qualified_handle}"));
    let range = format!("{}...{}", task.base_branch, task.branch);
    plan.commands.push(
        CommandSpec::new("git", ["diff", "--stat", range.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

pub fn mark_task_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Removed)
}

pub fn sweep_cleanup_plan<R: Registry>(context: &CommandContext<R>) -> CommandPlan {
    let workmux = WorkmuxAdapter::new("workmux");
    let mut plan = CommandPlan::new("sweep cleanup");

    plan.commands = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .map(|task| {
            let command = workmux.remove_task(&task.branch);
            match task_repo_path(context, task) {
                Some(repo_path) => command.with_cwd(repo_path),
                None => command,
            }
        })
        .collect();

    plan
}

pub fn sweep_cleanup_candidates<R: Registry>(context: &CommandContext<R>) -> Vec<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .map(Task::qualified_handle)
        .collect()
}

pub fn repair_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let tmux = TmuxAdapter::new("tmux");
    let mut plan = CommandPlan::new(format!("repair task: {qualified_handle}"));

    if task.has_side_flag(SideFlag::WorktrunkMissing) {
        plan.commands.push(tmux.ensure_worktrunk(
            &task.tmux_session,
            &task.worktrunk_window,
            &task.worktree_path.display().to_string(),
        ));
    }

    Ok(plan)
}

pub fn trunk_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let tmux = TmuxAdapter::new("tmux");
    let mut plan = CommandPlan::new(format!("open worktrunk: {qualified_handle}"));

    plan.commands.push(tmux.ensure_worktrunk(
        &task.tmux_session,
        &task.worktrunk_window,
        &task.worktree_path.display().to_string(),
    ));

    Ok(plan)
}

pub fn execute_plan(
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<Vec<CommandOutput>, CommandError> {
    if !plan.blocked_reasons.is_empty() {
        return Err(CommandError::PlanBlocked(plan.blocked_reasons.clone()));
    }

    if plan.requires_confirmation && !confirmed {
        return Err(CommandError::ConfirmationRequired);
    }

    let mut outputs = Vec::new();

    for command in &plan.commands {
        let output = runner.run(command).map_err(CommandError::CommandRun)?;
        if output.status_code != 0 {
            return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr,
                cwd: command.cwd.clone(),
            }));
        }
        outputs.push(output);
    }

    Ok(outputs)
}

fn count_lifecycle(tasks: &[&Task], status: LifecycleStatus) -> u32 {
    tasks
        .iter()
        .filter(|task| task.lifecycle_status == status)
        .count() as u32
}

fn task_summary(task: &Task) -> TaskSummary {
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        needs_attention: task.side_flags().next().is_some(),
        live_status: task.live_status.clone(),
    }
}

fn find_task<'a, R: Registry>(
    context: &'a CommandContext<R>,
    qualified_handle: &str,
) -> Result<&'a Task, CommandError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))
}

fn command_in_task_repo<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
    command: CommandSpec,
) -> Result<CommandSpec, CommandError> {
    task_repo_path(context, task)
        .map(|repo_path| command.with_cwd(repo_path))
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))
}

fn task_repo_path<R: Registry>(context: &CommandContext<R>, task: &Task) -> Option<String> {
    context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == task.repo)
        .map(|repo| repo.path.display().to_string())
}

fn update_task_lifecycle<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    status: LifecycleStatus,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    context
        .registry
        .update_lifecycle(&task_id, status)
        .map_err(CommandError::Registry)
}

fn missing_git_status() -> crate::models::GitStatus {
    crate::models::GitStatus {
        worktree_exists: false,
        branch_exists: false,
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    }
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

#[cfg(test)]
mod tests {
    use super::{
        check_task_plan, clean_task_plan, diff_task_plan, doctor, inbox, inspect_task, list_repos,
        list_tasks, mark_stale_tasks, merge_task_plan, new_task_plan, next, open_task_plan,
        reconcile_external, reconcile_filesystem, repair_task_plan, review_queue, status,
        sweep_cleanup_plan, task_from_new_request, CommandContext, CommandError, NewTaskRequest,
        OpenMode,
    };
    use crate::{
        adapters::{
            CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
            RecordingCommandRunner,
        },
        config::{Config, ManagedRepo, TestCommand},
        live::LiveStatusKind,
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, SideFlag, Task, TaskId,
        },
        registry::{InMemoryRegistry, Registry},
    };

    fn context_with_tasks() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![
                ManagedRepo::new("web", "/Users/matt/projects/web", "main"),
                ManagedRepo::new("api", "/Users/matt/projects/api", "main"),
            ],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
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
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.add_side_flag(SideFlag::NeedsInput);
        registry.create_task(task).unwrap();

        CommandContext::new(config, registry)
    }

    fn context_with_cleanable_task() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_tasks();
        let task_id = TaskId::new("task-1");
        let task = context.registry.get_task(&task_id).cloned().unwrap();
        let mut cleanable = task;
        cleanable.lifecycle_status = LifecycleStatus::Cleanable;
        cleanable.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        context.registry = InMemoryRegistry::default();
        context.registry.create_task(cleanable).unwrap();
        context
    }

    fn context_with_test_command() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_tasks();
        context.config.test_commands = vec![TestCommand::new("web", "cargo test")];
        context
    }

    #[derive(Default)]
    struct QueuedRunner {
        outputs: std::collections::VecDeque<CommandOutput>,
        commands: Vec<CommandSpec>,
    }

    impl QueuedRunner {
        fn new(outputs: Vec<CommandOutput>) -> Self {
            Self {
                outputs: outputs.into(),
                commands: Vec::new(),
            }
        }
    }

    impl CommandRunner for QueuedRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            self.outputs
                .pop_front()
                .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
        }
    }

    fn output(status_code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            status_code,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    #[test]
    fn repos_include_task_counts_by_repo() {
        let context = context_with_tasks();

        let response = list_repos(&context);

        assert_eq!(response.repos.len(), 2);
        assert_eq!(response.repos[0].name, "web");
        assert_eq!(response.repos[0].reviewable_tasks, 1);
        assert_eq!(response.repos[1].name, "api");
        assert_eq!(response.repos[1].active_tasks, 0);
    }

    #[test]
    fn tasks_can_be_filtered_by_repo() {
        let context = context_with_tasks();

        let all_tasks = list_tasks(&context, None);
        let web_tasks = list_tasks(&context, Some("web"));
        let api_tasks = list_tasks(&context, Some("api"));

        assert_eq!(all_tasks.tasks.len(), 1);
        assert_eq!(web_tasks.tasks.len(), 1);
        assert_eq!(api_tasks.tasks.len(), 0);
    }

    #[test]
    fn review_queue_lists_reviewable_and_mergeable_tasks() {
        let mut context = context_with_tasks();
        let mut mergeable = Task::new(
            TaskId::new("task-2"),
            "api",
            "add-cache",
            "Add cache",
            "ajax/add-cache",
            "main",
            "/tmp/worktrees/api-add-cache",
            "ajax-api-add-cache",
            "worktrunk",
            AgentClient::Claude,
        );
        mergeable.lifecycle_status = LifecycleStatus::Mergeable;
        context.registry.create_task(mergeable).unwrap();

        let response = review_queue(&context);

        assert_eq!(response.tasks.len(), 2);
        assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
        assert_eq!(response.tasks[1].qualified_handle, "api/add-cache");
    }

    #[test]
    fn inspect_returns_task_details_by_qualified_handle() {
        let context = context_with_tasks();

        let response = inspect_task(&context, "web/fix-login").unwrap();

        assert_eq!(response.task.qualified_handle, "web/fix-login");
        assert_eq!(response.branch, "ajax/fix-login");
        assert_eq!(response.tmux_session, "ajax-web-fix-login");
        assert_eq!(response.flags, vec!["NeedsInput"]);
    }

    #[test]
    fn inspect_reports_missing_tasks() {
        let context = context_with_tasks();

        let error = inspect_task(&context, "web/missing").unwrap_err();

        assert_eq!(error, CommandError::TaskNotFound("web/missing".to_string()));
    }

    #[test]
    fn inbox_returns_attention_items_from_side_flags() {
        let context = context_with_tasks();

        let response = inbox(&context);

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].task_handle, "web/fix-login");
        assert_eq!(response.items[0].reason, "agent needs input");
        assert_eq!(response.items[0].priority, 10);
        assert_eq!(response.items[0].recommended_action, "open task");
    }

    #[test]
    fn next_returns_first_attention_item() {
        let context = context_with_tasks();

        let response = next(&context);

        let item = response.item.unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.reason, "agent needs input");
    }

    #[test]
    fn doctor_and_status_return_basic_health() {
        let context = context_with_tasks();

        let doctor = doctor(&context);
        let status = status(&context);

        assert!(doctor.checks.iter().all(|check| check.ok));
        assert_eq!(status.tasks.len(), 1);
    }

    #[test]
    fn reconcile_filesystem_marks_missing_worktrees_in_registry() {
        let mut context = context_with_tasks();

        let response = reconcile_filesystem(&mut context);

        assert_eq!(response.tasks_checked, 1);
        assert_eq!(response.tasks_changed, 1);
        assert!(context
            .registry
            .list_tasks()
            .iter()
            .any(|task| task.has_side_flag(SideFlag::WorktreeMissing)));
    }

    #[test]
    fn stale_reconciliation_marks_inactive_old_tasks() {
        let mut context = context_with_tasks();
        let old_activity = std::time::SystemTime::UNIX_EPOCH;
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .last_activity_at = old_activity;

        let changed = mark_stale_tasks(
            &mut context,
            old_activity + std::time::Duration::from_secs(8 * 24 * 60 * 60),
        );

        assert_eq!(changed, 1);
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::Stale));
    }

    #[test]
    fn reconcile_external_updates_task_from_git_and_tmux_discovery() {
        let mut context = context_with_tasks();
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "## ajax/fix-login...origin/ajax/fix-login [ahead 1]\n M src/main.rs\n",
            ),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "codex is working on your task\n"),
        ]);

        let response = reconcile_external(&mut context, &mut runner).unwrap();

        assert_eq!(response.tasks_checked, 1);
        assert_eq!(response.tasks_changed, 1);
        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "status",
                        "--porcelain=v1",
                        "--branch"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "merge-base",
                        "--is-ancestor",
                        "ajax/fix-login",
                        "main"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "list-windows",
                        "-t",
                        "ajax-web-fix-login",
                        "-F",
                        "#{window_name}\t#{pane_current_path}"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "capture-pane",
                        "-p",
                        "-t",
                        "ajax-web-fix-login:worktrunk",
                        "-S",
                        "-200"
                    ]
                ),
            ]
        );

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::Unpushed));
        assert!(!task.has_side_flag(SideFlag::TmuxMissing));
        assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
        assert!(task.git_status.as_ref().unwrap().dirty);
        assert!(task.tmux_status.as_ref().unwrap().exists);
        assert!(
            task.worktrunk_status
                .as_ref()
                .unwrap()
                .points_at_expected_path
        );
        assert!(!task.git_status.as_ref().unwrap().merged);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn reconcile_external_marks_live_approval_waiting_from_worktrunk_pane() {
        let mut context = context_with_tasks();
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(1, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "Allow command `npm test`? y/n\n"),
        ]);

        let response = reconcile_external(&mut context, &mut runner).unwrap();

        assert_eq!(response.tasks_checked, 1);
        assert_eq!(response.tasks_changed, 1);
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForApproval)
        );
    }

    #[test]
    fn reconcile_external_marks_missing_resources_when_git_and_tmux_are_absent() {
        let mut context = context_with_tasks();
        let mut runner = QueuedRunner::new(vec![
            output(0, "other-session\n"),
            output(128, "fatal: not a git repository\n"),
        ]);

        let response = reconcile_external(&mut context, &mut runner).unwrap();

        assert_eq!(response.tasks_checked, 1);
        assert_eq!(response.tasks_changed, 1);
        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/tmp/worktrees/web-fix-login",
                        "status",
                        "--porcelain=v1",
                        "--branch"
                    ]
                ),
            ]
        );

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert!(task.has_side_flag(SideFlag::WorktrunkMissing));
    }

    #[test]
    fn reconcile_external_marks_branch_merged_from_merge_base() {
        let mut context = context_with_tasks();
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(0, "## ajax/fix-login...origin/ajax/fix-login\n"),
            output(0, ""),
            output(0, "worktrunk\t/tmp/worktrees/web-fix-login\n"),
            output(0, "task complete\n"),
        ]);

        reconcile_external(&mut context, &mut runner).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.git_status.as_ref().unwrap().merged);
    }

    #[test]
    fn new_task_plan_validates_repo_and_calls_workmux_add() {
        let context = context_with_tasks();

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert!(!plan.requires_confirmation);
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new(
                "workmux",
                [
                    "add",
                    "ajax/fix-login",
                    "--prompt",
                    "fix login",
                    "--agent",
                    "codex"
                ]
            )
            .with_cwd("/Users/matt/projects/web")]
        );
    }

    #[test]
    fn new_task_plan_rejects_unknown_repo() {
        let context = context_with_tasks();

        let error = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "missing".to_string(),
                title: "fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(error, CommandError::RepoNotFound("missing".to_string()));
    }

    #[test]
    fn new_task_request_creates_provisional_task_record() {
        let context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
        };

        let task = task_from_new_request(&context, &request).unwrap();

        assert_eq!(task.id.as_str(), "web/fix-login");
        assert_eq!(task.handle, "fix-login");
        assert_eq!(task.branch, "ajax/fix-login");
        assert_eq!(task.tmux_session, "ajax-web-fix-login");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web/.ajax-worktrees/fix-login"
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
        assert_eq!(task.selected_agent, AgentClient::Codex);
    }

    #[test]
    fn new_task_request_slugifies_blank_titles_to_task() {
        let context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "!!!".to_string(),
            agent: "claude".to_string(),
        };

        let task = task_from_new_request(&context, &request).unwrap();

        assert_eq!(task.handle, "task");
        assert_eq!(task.selected_agent, AgentClient::Claude);
    }

    #[test]
    fn record_new_task_adds_provisional_task_to_registry() {
        let mut context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "api".to_string(),
            title: "Add cache".to_string(),
            agent: "codex".to_string(),
        };

        let task = super::record_new_task(&mut context, &request).unwrap();

        assert_eq!(task.qualified_handle(), "api/add-cache");
        assert!(context
            .registry
            .list_tasks()
            .iter()
            .any(|task| task.qualified_handle() == "api/add-cache"));
    }

    #[test]
    fn open_task_plan_routes_to_workmux_then_tmux() {
        let context = context_with_tasks();

        let outside_tmux = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
        let inside_tmux =
            open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

        assert_eq!(
            outside_tmux.commands,
            vec![
                CommandSpec::new("workmux", ["open", "ajax/fix-login"])
                    .with_cwd("/Users/matt/projects/web"),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio),
            ]
        );
        assert_eq!(
            inside_tmux.commands,
            vec![
                CommandSpec::new("workmux", ["open", "ajax/fix-login"])
                    .with_cwd("/Users/matt/projects/web"),
                CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio),
            ]
        );
    }

    #[test]
    fn check_task_plan_runs_configured_command_in_task_worktree() {
        let context = context_with_test_command();

        let plan = check_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "check task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("sh", ["-lc", "cargo test"])
                .with_cwd("/tmp/worktrees/web-fix-login")]
        );
    }

    #[test]
    fn diff_task_plan_summarizes_branch_diff_in_task_worktree() {
        let context = context_with_tasks();

        let plan = diff_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "diff task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("git", ["diff", "--stat", "main...ajax/fix-login"])
                    .with_cwd("/tmp/worktrees/web-fix-login")
            ]
        );
    }

    #[test]
    fn lifecycle_transitions_update_registry_status() {
        let mut context = context_with_tasks();

        super::mark_task_opened(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Active
        );

        super::mark_task_merged(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Merged
        );

        super::mark_task_removed(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
    }

    #[test]
    fn merge_plan_requires_confirmation_when_task_needs_attention() {
        let context = context_with_tasks();

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("workmux", ["merge", "ajax/fix-login"])
                .with_cwd("/Users/matt/projects/web")]
        );
    }

    #[test]
    fn clean_plan_uses_policy_and_workmux_remove() {
        let context = context_with_cleanable_task();

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("workmux", ["remove", "ajax/fix-login"])
                .with_cwd("/Users/matt/projects/web")]
        );
    }

    #[test]
    fn sweep_cleanup_plans_only_safe_candidates() {
        let context = context_with_cleanable_task();

        let plan = sweep_cleanup_plan(&context);

        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("workmux", ["remove", "ajax/fix-login"])
                .with_cwd("/Users/matt/projects/web")]
        );
    }

    #[test]
    fn repair_task_plan_recreates_missing_worktrunk() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .cloned()
            .unwrap();
        let mut broken = task;
        broken.add_side_flag(SideFlag::WorktrunkMissing);
        context.registry = InMemoryRegistry::default();
        context.registry.create_task(broken).unwrap();

        let plan = repair_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "worktrunk",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            )]
        );
    }

    #[test]
    fn execute_plan_runs_safe_commands() {
        let context = context_with_tasks();
        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
        let mut runner = RecordingCommandRunner::default();

        let outputs = super::execute_plan(&plan, false, &mut runner).unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(runner.commands(), plan.commands.as_slice());
    }

    #[test]
    fn execute_plan_requires_confirmation_for_risky_commands() {
        let context = context_with_tasks();
        let plan = merge_task_plan(&context, "web/fix-login").unwrap();
        let mut runner = RecordingCommandRunner::default();

        let error = super::execute_plan(&plan, false, &mut runner).unwrap_err();

        assert_eq!(error, CommandError::ConfirmationRequired);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn execute_plan_refuses_blocked_commands() {
        let mut runner = RecordingCommandRunner::default();
        let mut plan = super::CommandPlan::new("blocked");
        plan.blocked_reasons.push("worktree is missing".to_string());
        plan.commands
            .push(CommandSpec::new("workmux", ["remove", "web/fix-login"]));

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::PlanBlocked(vec!["worktree is missing".to_string()])
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn execute_plan_rejects_nonzero_command_outputs() {
        let mut runner = QueuedRunner::new(vec![output(2, "nope")]);
        let mut plan = super::CommandPlan::new("failing");
        plan.commands
            .push(CommandSpec::new("workmux", ["merge", "web/fix-login"]));

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "workmux".to_string(),
                status_code: 2,
                stderr: String::new(),
                cwd: None,
            })
        );
    }

    #[test]
    fn execute_plan_reports_nonzero_command_cwd() {
        let mut runner = QueuedRunner::new(vec![output(1, "Error: Not in a git repository\n")]);
        let mut plan = super::CommandPlan::new("failing");
        plan.commands.push(
            CommandSpec::new("workmux", ["add", "ajax/task-123"])
                .with_cwd("/Users/matt/Desktop/Projects/autodoctor"),
        );

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "workmux".to_string(),
                status_code: 1,
                stderr: String::new(),
                cwd: Some("/Users/matt/Desktop/Projects/autodoctor".to_string()),
            })
        );
    }
}
