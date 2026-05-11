use crate::{
    adapters::{
        AgentAdapter, AgentLaunch, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
        GitAdapter, TmuxAdapter,
    },
    attention::derive_attention_items,
    config::Config,
    live::LiveStatusKind,
    models::{
        AgentClient, LifecycleStatus, LiveObservation, RecommendedAction, SafetyClassification,
        SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
    },
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    output::{
        CockpitResponse, CockpitSummary, DoctorCheck, DoctorResponse, InboxResponse,
        InspectResponse, NextResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse,
    },
    policy::cleanup_safety,
    registry::{Registry, RegistryError, RegistryEventKind},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const REQUIRED_TOOLS: [&str; 3] = ["git", "tmux", "codex"];

pub struct CommandContext<R> {
    pub config: Config,
    pub registry: R,
}

impl<R> CommandContext<R> {
    pub fn new(config: Config, registry: R) -> Self {
        Self { config, registry }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DoctorEnvironment {
    available_tools: BTreeSet<String>,
    existing_paths: Option<BTreeSet<PathBuf>>,
}

impl DoctorEnvironment {
    pub fn from_available_tools<I, T>(tools: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        Self {
            available_tools: tools.into_iter().map(Into::into).collect(),
            existing_paths: None,
        }
    }

    pub fn from_path() -> Self {
        let Some(path) = std::env::var_os("PATH") else {
            return Self::default();
        };
        let available_tools = REQUIRED_TOOLS
            .iter()
            .copied()
            .filter(|tool| {
                std::env::split_paths(&path).any(|directory| directory.join(tool).is_file())
            })
            .map(str::to_string)
            .collect();

        Self {
            available_tools,
            existing_paths: None,
        }
    }

    pub fn with_existing_paths<I, T>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<PathBuf>,
    {
        self.existing_paths = Some(paths.into_iter().map(Into::into).collect());
        self
    }

    fn has_tool(&self, tool: &str) -> bool {
        self.available_tools.contains(tool)
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.existing_paths
            .as_ref()
            .map_or_else(|| path.exists(), |paths| paths.contains(path))
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
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
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
    doctor_with_environment(context, &DoctorEnvironment::from_path())
}

pub fn doctor_with_environment<R: Registry>(
    context: &CommandContext<R>,
    environment: &DoctorEnvironment,
) -> DoctorResponse {
    let mut checks = vec![
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
    ];

    checks.extend(REQUIRED_TOOLS.iter().map(|tool| {
        let ok = environment.has_tool(tool);
        DoctorCheck {
            name: format!("tool:{tool}"),
            ok,
            message: if ok {
                "available".to_string()
            } else {
                "not found on PATH".to_string()
            },
        }
    }));
    checks.push(repo_name_check(context));
    for repo in &context.config.repos {
        let repo_path_exists = environment.path_exists(&repo.path);
        checks.push(DoctorCheck {
            name: format!("repo:{}:path", repo.name),
            ok: repo_path_exists,
            message: if repo_path_exists {
                format!("path exists: {}", repo.path.display())
            } else {
                format!("path missing: {}", repo.path.display())
            },
        });

        let has_test_command = context
            .config
            .test_commands
            .iter()
            .any(|test_command| test_command.repo == repo.name);
        checks.push(DoctorCheck {
            name: format!("repo:{}:test-command", repo.name),
            ok: has_test_command,
            message: if has_test_command {
                "test command configured".to_string()
            } else {
                "no test command configured".to_string()
            },
        });
    }

    DoctorResponse { checks }
}

fn repo_name_check<R: Registry>(context: &CommandContext<R>) -> DoctorCheck {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for repo in &context.config.repos {
        if !seen.insert(repo.name.clone()) {
            duplicates.insert(repo.name.clone());
        }
    }

    if let Some(duplicate) = duplicates.into_iter().next() {
        DoctorCheck {
            name: "config:repo-names".to_string(),
            ok: false,
            message: format!("duplicate repo name: {duplicate}"),
        }
    } else {
        DoctorCheck {
            name: "config:repo-names".to_string(),
            ok: true,
            message: "repo names unique".to_string(),
        }
    }
}

pub fn status<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    list_tasks(context, None)
}

pub fn cockpit<R: Registry>(context: &CommandContext<R>) -> CockpitResponse {
    let repos = list_repos(context);
    let tasks = list_tasks(context, None);
    let review = review_queue(context);
    let inbox = inbox(context);
    let summary = cockpit_summary(&repos, &tasks, &review, &inbox);
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

fn cockpit_summary(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
    inbox: &InboxResponse,
) -> CockpitSummary {
    CockpitSummary {
        repos: repos.repos.len() as u32,
        tasks: tasks.tasks.len() as u32,
        active_tasks: repos.repos.iter().map(|repo| repo.active_tasks).sum(),
        attention_items: inbox.items.len() as u32,
        reviewable_tasks: review.tasks.len() as u32,
        cleanable_tasks: repos.repos.iter().map(|repo| repo.cleanable_tasks).sum(),
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
    task.lifecycle_status = LifecycleStatus::Provisioning;

    Ok(task)
}

fn ajax_worktree_path(repo_path: &std::path::Path, branch: &str) -> std::path::PathBuf {
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

pub fn open_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let needs_trunk_repair = task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::WorktrunkMissing)
        || task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists)
        || task
            .worktrunk_status
            .as_ref()
            .is_some_and(|status| !status.exists || !status.points_at_expected_path);
    let has_non_tmux_missing_substrate = task.has_side_flag(SideFlag::WorktreeMissing)
        || task.has_side_flag(SideFlag::BranchMissing)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists || !status.branch_exists);
    if needs_trunk_repair && !has_non_tmux_missing_substrate {
        return trunk_task_plan(context, qualified_handle);
    }

    let mut plan = CommandPlan::new(format!("open task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Open)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let tmux = TmuxAdapter::new("tmux");
    plan.commands
        .push(tmux.select_window(&task.tmux_session, &task.worktrunk_window));
    match mode {
        OpenMode::Attach => plan
            .commands
            .push(tmux.attach_window(&task.tmux_session, &task.worktrunk_window)),
        OpenMode::SwitchClient => plan
            .commands
            .push(tmux.switch_client_to_window(&task.tmux_session, &task.worktrunk_window)),
    };

    Ok(plan)
}

pub fn mark_task_opened<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let _ = find_task(context, qualified_handle)?;
    Ok(())
}

pub fn mark_task_trunk_repaired<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.tmux_status = Some(TmuxStatus::present(task.tmux_session.clone()));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            task.worktrunk_window.clone(),
            task.worktree_path.clone(),
        ));
        task.remove_side_flag(SideFlag::TmuxMissing);
        task.remove_side_flag(SideFlag::WorktrunkMissing);
        if task.live_status.as_ref().is_some_and(|status| {
            matches!(
                status.kind,
                LiveStatusKind::TmuxMissing | LiveStatusKind::WorktrunkMissing
            )
        }) {
            task.live_status = None;
        }
    }
    Ok(())
}

pub fn mark_task_check_started<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TestsRunning,
            "check running",
        ));
        task.remove_side_flag(SideFlag::TestsFailed);
    }
    Ok(())
}

pub fn mark_task_check_succeeded<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Active | LifecycleStatus::Waiting
    ) {
        context
            .registry
            .update_lifecycle(&task.id, LifecycleStatus::Reviewable)
            .map_err(CommandError::Registry)?;
    }
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.remove_side_flag(SideFlag::TestsFailed);
        if task
            .live_status
            .as_ref()
            .is_some_and(|status| status.kind == LiveStatusKind::TestsRunning)
        {
            task.live_status = None;
        }
    }
    Ok(())
}

pub fn mark_task_check_failed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.add_side_flag(SideFlag::TestsFailed);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "check failed",
        ));
    }
    Ok(())
}

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
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Merge)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let repo_path = task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    plan.requires_confirmation = task.side_flags().next().is_some();
    plan.commands
        .push(git.switch_branch(&repo_path, &task.base_branch));
    plan.commands
        .push(git.merge_branch(&repo_path, &task.branch));

    Ok(plan)
}

fn merge_preflight_blocked_reasons(task: &Task) -> Vec<String> {
    let mut reasons = Vec::new();
    if task.has_side_flag(SideFlag::Dirty)
        || task.has_side_flag(SideFlag::Conflicted)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| status.dirty || status.untracked_files > 0 || status.conflicted)
    {
        reasons.push("merge requires clean worktree evidence".to_string());
    }
    if task.has_side_flag(SideFlag::BranchMissing)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.branch_exists)
    {
        reasons.push("task branch is missing".to_string());
    }
    reasons
}

pub fn mark_task_merged<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Merged)
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
        }
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "merge failed",
        ));
    }
    Ok(())
}

pub fn mark_task_cleanup_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    command: &CommandSpec,
) -> Result<bool, CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let Some(task) = context.registry.get_task_mut(&task.id) else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    if command.program == "tmux"
        && command
            .args
            .first()
            .is_some_and(|arg| arg == "kill-session")
    {
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: task.tmux_session.clone(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: false,
            window_name: task.worktrunk_window.clone(),
            current_path: task.worktree_path.clone(),
            points_at_expected_path: false,
        });
        task.remove_side_flag(SideFlag::TmuxMissing);
        task.remove_side_flag(SideFlag::WorktrunkMissing);
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "worktree")
        && command.args.iter().any(|arg| arg == "remove")
    {
        if let Some(git_status) = task.git_status.as_mut() {
            git_status.worktree_exists = false;
            git_status.dirty = false;
            git_status.untracked_files = 0;
            git_status.conflicted = false;
        }
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.remove_side_flag(SideFlag::Dirty);
        task.remove_side_flag(SideFlag::Conflicted);
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "branch")
        && (command.args.iter().any(|arg| arg == "-d")
            || command.args.iter().any(|arg| arg == "-D"))
    {
        if let Some(git_status) = task.git_status.as_mut() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
        }
        task.add_side_flag(SideFlag::BranchMissing);
        task.remove_side_flag(SideFlag::Unpushed);
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
    refresh_git_evidence(context, qualified_handle, runner, merged)
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

    let has_branch_evidence = output.stdout.lines().any(|line| line.starts_with("## "));
    let mut git_status = GitAdapter::parse_status(&output.stdout, merged);
    if !has_branch_evidence && output.stdout.trim().is_empty() {
        if let Some(previous) = task.git_status.as_ref() {
            git_status = previous.clone();
        } else {
            return Ok(());
        }
    } else if !has_branch_evidence {
        if let Some(previous) = task.git_status.as_ref() {
            git_status.branch_exists = previous.branch_exists;
            git_status
                .current_branch
                .clone_from(&previous.current_branch);
        }
    }
    let task = context
        .registry
        .get_task_mut(&task.id)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    apply_git_evidence(task, git_status, has_branch_evidence);

    Ok(())
}

fn apply_git_evidence(
    task: &mut Task,
    git_status: crate::models::GitStatus,
    has_branch_evidence: bool,
) {
    if git_status.worktree_exists {
        task.remove_side_flag(SideFlag::WorktreeMissing);
    } else {
        task.add_side_flag(SideFlag::WorktreeMissing);
    }

    if has_branch_evidence {
        if git_status.branch_exists {
            task.remove_side_flag(SideFlag::BranchMissing);
        } else {
            task.add_side_flag(SideFlag::BranchMissing);
        }
    }

    if git_status.dirty || git_status.untracked_files > 0 {
        task.add_side_flag(SideFlag::Dirty);
    } else {
        task.remove_side_flag(SideFlag::Dirty);
    }

    if git_status.conflicted {
        task.add_side_flag(SideFlag::Conflicted);
    } else {
        task.remove_side_flag(SideFlag::Conflicted);
    }

    if git_status.has_unpushed_work() {
        task.add_side_flag(SideFlag::Unpushed);
    } else {
        task.remove_side_flag(SideFlag::Unpushed);
    }

    task.git_status = Some(git_status);
}

pub fn check_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("check task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Check)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

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
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Diff)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

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

pub fn mark_task_force_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    let Some(task) = context.registry.get_task_mut(&task_id) else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    task.lifecycle_status = LifecycleStatus::Removed;
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
        .filter(|task| is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .filter_map(|task| native_cleanup_commands(context, task).ok())
        .flatten()
        .collect();

    plan
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

pub fn sweep_cleanup_candidates<R: Registry>(context: &CommandContext<R>) -> Vec<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .map(Task::qualified_handle)
        .collect()
}

pub fn trunk_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let tmux = TmuxAdapter::new("tmux");
    let mut plan = CommandPlan::new(format!("open worktrunk: {qualified_handle}"));
    if task.lifecycle_status == LifecycleStatus::Removed {
        plan.blocked_reasons.push("task is removed".to_string());
        return Ok(plan);
    }
    if task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists)
    {
        plan.blocked_reasons.push(format!(
            "task worktree is missing: {}",
            task.worktree_path.display()
        ));
        return Ok(plan);
    }

    let tmux_session_exists = task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists);
    if !tmux_session_exists {
        plan.commands.push(tmux.new_detached_worktrunk_session(
            &task.tmux_session,
            &task.worktrunk_window,
            &task.worktree_path.display().to_string(),
        ));
    } else if task
        .worktrunk_status
        .as_ref()
        .is_some_and(|status| status.exists && !status.points_at_expected_path)
    {
        plan.commands
            .push(tmux.kill_window(&task.tmux_session, &task.worktrunk_window));
        plan.commands.push(tmux.ensure_worktrunk(
            &task.tmux_session,
            &task.worktrunk_window,
            &task.worktree_path.display().to_string(),
        ));
    } else if task
        .worktrunk_status
        .as_ref()
        .is_none_or(|status| !status.exists)
    {
        plan.commands.push(tmux.ensure_worktrunk(
            &task.tmux_session,
            &task.worktrunk_window,
            &task.worktree_path.display().to_string(),
        ));
    }
    plan.commands
        .push(tmux.select_window(&task.tmux_session, &task.worktrunk_window));
    plan.commands
        .push(tmux.attach_window(&task.tmux_session, &task.worktrunk_window));

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

fn count_active_tasks(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .filter(|task| {
            task.lifecycle_status == LifecycleStatus::Active && !task.has_missing_substrate()
        })
        .count() as u32
}

fn count_attention_items(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .map(|task| derive_attention_items(std::slice::from_ref(*task)).len() as u32)
        .sum()
}

fn is_visible_task(task: &Task) -> bool {
    task.lifecycle_status != LifecycleStatus::Removed
}

fn task_summary(task: &Task) -> TaskSummary {
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        needs_attention: !derive_attention_items(std::slice::from_ref(task)).is_empty(),
        live_status: task.live_status.clone(),
        actions: task_actions(task),
    }
}

fn task_actions(task: &Task) -> Vec<String> {
    if task.has_side_flag(SideFlag::TmuxMissing) || task.has_side_flag(SideFlag::WorktrunkMissing) {
        return vec![RecommendedAction::OpenTrunk.as_str().to_string()];
    }

    [
        (TaskOperation::Open, RecommendedAction::OpenTask),
        (TaskOperation::Merge, RecommendedAction::MergeTask),
        (TaskOperation::Clean, RecommendedAction::CleanTask),
        (TaskOperation::Remove, RecommendedAction::RemoveTask),
    ]
    .into_iter()
    .filter(|(operation, _)| task_operation_eligibility(task, *operation).is_allowed())
    .map(|(_, action)| action.as_str().to_string())
    .collect()
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
        check_task_plan, clean_task_plan, cockpit, diff_task_plan, doctor_with_environment, inbox,
        inspect_task, list_repos, list_tasks, mark_stale_tasks, merge_task_plan, new_task_plan,
        next, open_task_plan, remove_task_plan, review_queue, status, sweep_cleanup_plan,
        task_from_new_request, trunk_task_plan, CommandContext, CommandError, DoctorEnvironment,
        NewTaskRequest, OpenMode,
    };
    use crate::{
        adapters::{
            CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
            RecordingCommandRunner,
        },
        config::{Config, ManagedRepo, TestCommand},
        live::LiveStatusKind,
        models::{
            AgentClient, GitStatus, LifecycleStatus, RecommendedAction, SideFlag, Task, TaskId,
            TmuxStatus, WorktrunkStatus,
        },
        output::CockpitSummary,
        registry::{InMemoryRegistry, Registry},
    };
    use proptest::prelude::*;
    use rstest::rstest;

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
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        cleanable.tmux_status = Some(crate::models::TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
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

    fn shell_words(command: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let mut chars = command.chars().peekable();
        let mut in_single_quotes = false;
        let mut word_started = false;

        while let Some(character) = chars.next() {
            match character {
                '\'' => {
                    word_started = true;
                    in_single_quotes = !in_single_quotes;
                }
                '\\' if !in_single_quotes => {
                    word_started = true;
                    if let Some(escaped) = chars.next() {
                        current.push(escaped);
                    } else {
                        current.push(character);
                    }
                }
                ' ' if !in_single_quotes => {
                    if word_started {
                        words.push(std::mem::take(&mut current));
                        word_started = false;
                    }
                }
                _ => {
                    word_started = true;
                    current.push(character);
                }
            }
        }

        if word_started {
            words.push(current);
        }

        words
    }

    proptest! {
        #[test]
        fn native_new_task_agent_command_preserves_generated_title(
            title in "[^\\x00]{0,80}"
        ) {
            let context = CommandContext::new(
                Config {
                    repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                    ..Config::default()
                },
                InMemoryRegistry::default(),
            );
            let plan = new_task_plan(
                &context,
                NewTaskRequest {
                    repo: "web".to_string(),
                    title: title.clone(),
                    agent: "codex".to_string(),
                },
            )
            .unwrap();

            let send_keys = &plan.commands[2];
            let worktree_path = plan.commands[0].args[6].clone();

            prop_assert_eq!(send_keys.program.as_str(), "tmux");
            prop_assert_eq!(send_keys.args[0].as_str(), "send-keys");
            prop_assert_eq!(
                shell_words(&send_keys.args[3]),
                vec![
                    "codex".to_string(),
                    "--cd".to_string(),
                    worktree_path,
                    title,
                ]
            );
        }

        #[test]
        fn native_cleanup_commands_reflect_generated_resource_and_risk_status(
            tmux_exists in any::<bool>(),
            dirty in any::<bool>(),
            conflicted in any::<bool>(),
            side_dirty in any::<bool>(),
            side_conflicted in any::<bool>(),
            untracked_files in 0u32..4,
            merged in any::<bool>()
        ) {
            let mut context = context_with_cleanable_task();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            let git_status = task.git_status.as_mut().unwrap();
            git_status.dirty = dirty;
            git_status.conflicted = conflicted;
            git_status.untracked_files = untracked_files;
            git_status.merged = merged;
            task.tmux_status = Some(TmuxStatus {
                exists: tmux_exists,
                session_name: task.tmux_session.clone(),
            });
            if side_dirty {
                task.add_side_flag(SideFlag::Dirty);
            }
            if side_conflicted {
                task.add_side_flag(SideFlag::Conflicted);
            }

            let plan = clean_task_plan(&context, "web/fix-login").unwrap();
            let expected_force_worktree =
                dirty || conflicted || side_dirty || side_conflicted || untracked_files > 0;
            let expected_worktree_args: Vec<String> = if expected_force_worktree {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "--force",
                    "/tmp/worktrees/web-fix-login",
                ]
            } else {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login",
                ]
            }
            .into_iter()
            .map(str::to_string)
            .collect();
            let expected_branch_args: Vec<String> = if merged {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login",
                ]
            } else {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-D",
                    "ajax/fix-login",
                ]
            }
            .into_iter()
            .map(str::to_string)
            .collect();
            let has_expected_worktree_command = plan
                .commands
                .iter()
                .any(|command| command.program == "git" && command.args == expected_worktree_args);
            let has_expected_branch_command = plan
                .commands
                .iter()
                .any(|command| command.program == "git" && command.args == expected_branch_args);

            prop_assert!(plan.blocked_reasons.is_empty());
            prop_assert_eq!(
                plan.commands
                    .iter()
                    .any(|command| command.args == vec!["kill-session", "-t", "ajax-web-fix-login"]),
                tmux_exists
            );
            prop_assert!(has_expected_worktree_command);
            prop_assert!(has_expected_branch_command);
        }

        #[test]
        fn trunk_plan_repairs_generated_tmux_and_worktrunk_states(
            worktree_exists in any::<bool>(),
            tmux_exists in any::<bool>(),
            worktrunk_exists in any::<bool>(),
            points_at_expected_path in any::<bool>()
        ) {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.git_status = Some(GitStatus {
                worktree_exists,
                branch_exists: true,
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
            task.tmux_status = Some(TmuxStatus {
                exists: tmux_exists,
                session_name: task.tmux_session.clone(),
            });
            task.worktrunk_status = Some(WorktrunkStatus {
                exists: worktrunk_exists,
                window_name: task.worktrunk_window.clone(),
                current_path: if points_at_expected_path {
                    task.worktree_path.clone()
                } else {
                    "/tmp/other-worktree".into()
                },
                points_at_expected_path,
            });

            let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

            if !worktree_exists {
                prop_assert!(plan.commands.is_empty());
                prop_assert_eq!(
                    plan.blocked_reasons,
                    vec!["task worktree is missing: /tmp/worktrees/web-fix-login"]
                );
                return Ok(());
            }

            prop_assert!(plan.blocked_reasons.is_empty());
            prop_assert_eq!(
                &plan.commands[plan.commands.len() - 2..],
                &[
                    CommandSpec::new(
                        "tmux",
                        ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                    ),
                    CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                        .with_mode(CommandMode::InheritStdio),
                ]
            );

            let repair_commands = &plan.commands[..plan.commands.len() - 2];
            if !tmux_exists {
                prop_assert_eq!(
                    repair_commands,
                    &[CommandSpec::new(
                        "tmux",
                        [
                            "new-session",
                            "-d",
                            "-s",
                            "ajax-web-fix-login",
                            "-n",
                            "worktrunk",
                            "-c",
                            "/tmp/worktrees/web-fix-login",
                        ],
                    )]
                );
            } else if worktrunk_exists && !points_at_expected_path {
                prop_assert_eq!(
                    repair_commands,
                    &[
                        CommandSpec::new(
                            "tmux",
                            ["kill-window", "-t", "ajax-web-fix-login:worktrunk"]
                        ),
                        CommandSpec::new(
                            "tmux",
                            [
                                "new-window",
                                "-t",
                                "ajax-web-fix-login",
                                "-n",
                                "worktrunk",
                                "-c",
                                "/tmp/worktrees/web-fix-login",
                            ],
                        ),
                    ]
                );
            } else if !worktrunk_exists {
                prop_assert_eq!(
                    repair_commands,
                    &[CommandSpec::new(
                        "tmux",
                        [
                            "new-window",
                            "-t",
                            "ajax-web-fix-login",
                            "-n",
                            "worktrunk",
                            "-c",
                            "/tmp/worktrees/web-fix-login",
                        ],
                    )]
                );
            } else {
                prop_assert!(repair_commands.is_empty());
            }
        }

        #[test]
        fn stale_task_marking_uses_seven_day_boundary(
            seconds_before_boundary in 0u64..(7 * 24 * 60 * 60)
        ) {
            let last_activity = std::time::SystemTime::UNIX_EPOCH;
            let stale_after = std::time::Duration::from_secs(7 * 24 * 60 * 60);
            let mut before_context = context_with_tasks();
            before_context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .last_activity_at = last_activity;
            let before_changed = mark_stale_tasks(
                &mut before_context,
                last_activity + std::time::Duration::from_secs(seconds_before_boundary),
            );

            prop_assert_eq!(before_changed, 0);
            prop_assert!(!before_context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .has_side_flag(SideFlag::Stale));

            let mut boundary_context = context_with_tasks();
            boundary_context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .last_activity_at = last_activity;
            let boundary_changed =
                mark_stale_tasks(&mut boundary_context, last_activity + stale_after);

            prop_assert_eq!(boundary_changed, 1);
            prop_assert!(boundary_context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .has_side_flag(SideFlag::Stale));
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
    fn missing_substrate_tasks_are_not_counted_as_active() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::WorktreeMissing);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].active_tasks, 0);
    }

    #[test]
    fn repo_attention_count_includes_broken_visible_tasks() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TmuxMissing);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].attention_items, 1);
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
    fn missing_substrate_tasks_remain_visible_in_task_lists() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let response = list_tasks(&context, None);

        assert_eq!(response.tasks.len(), 1);
        assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
    }

    #[test]
    fn task_summary_marks_live_attention_without_side_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(crate::models::LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));

        let response = list_tasks(&context, None);

        assert!(response.tasks[0].needs_attention);
    }

    #[test]
    fn task_summaries_expose_lifecycle_aware_actions() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;

        let active = list_tasks(&context, None);
        assert_eq!(
            active.tasks[0].actions,
            vec![
                RecommendedAction::OpenTask.as_str().to_string(),
                RecommendedAction::RemoveTask.as_str().to_string(),
            ]
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Reviewable;
        let reviewable = list_tasks(&context, None);
        assert_eq!(
            reviewable.tasks[0].actions,
            vec![
                RecommendedAction::OpenTask.as_str().to_string(),
                RecommendedAction::MergeTask.as_str().to_string(),
                RecommendedAction::RemoveTask.as_str().to_string(),
            ]
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Cleanable;
        let cleanable = list_tasks(&context, None);
        assert_eq!(
            cleanable.tasks[0].actions,
            vec![
                RecommendedAction::OpenTask.as_str().to_string(),
                RecommendedAction::CleanTask.as_str().to_string(),
                RecommendedAction::RemoveTask.as_str().to_string(),
            ]
        );
    }

    #[test]
    fn task_summaries_expose_trunk_repair_for_missing_tmux_evidence() {
        for flag in [SideFlag::TmuxMissing, SideFlag::WorktrunkMissing] {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.remove_side_flag(SideFlag::NeedsInput);
            task.add_side_flag(flag);

            let response = list_tasks(&context, None);

            assert_eq!(
                response.tasks[0].actions,
                vec![RecommendedAction::OpenTrunk.as_str().to_string()],
                "{flag:?}"
            );
            assert_eq!(
                inbox(&context).items[0].recommended_action,
                RecommendedAction::OpenTrunk.as_str(),
                "{flag:?}"
            );
        }
    }

    #[test]
    fn removed_tasks_are_hidden_from_operational_summaries() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Removed;
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        task.live_status = Some(crate::models::LiveObservation::new(
            LiveStatusKind::WorktreeMissing,
            "worktree missing",
        ));

        assert!(list_tasks(&context, None).tasks.is_empty());
        assert!(inbox(&context).items.is_empty());
    }

    #[test]
    fn missing_substrate_tasks_are_visible_but_not_actionable() {
        for flag in [
            SideFlag::WorktreeMissing,
            SideFlag::BranchMissing,
            SideFlag::TmuxMissing,
            SideFlag::WorktrunkMissing,
        ] {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.remove_side_flag(SideFlag::NeedsInput);
            task.add_side_flag(flag);

            assert_eq!(list_tasks(&context, None).tasks.len(), 1, "{flag:?}");
            assert_eq!(review_queue(&context).tasks.len(), 1, "{flag:?}");
            assert_eq!(inbox(&context).items.len(), 1, "{flag:?}");
            assert_eq!(cockpit(&context).tasks.tasks.len(), 1, "{flag:?}");
            assert_eq!(list_repos(&context).repos[0].active_tasks, 0, "{flag:?}");
            assert_eq!(
                list_repos(&context).repos[0].reviewable_tasks,
                1,
                "{flag:?}"
            );
        }
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
    fn cockpit_includes_review_queue() {
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

        let response = cockpit(&context);

        assert_eq!(response.review.tasks.len(), 2);
        assert_eq!(response.review.tasks[0].qualified_handle, "web/fix-login");
        assert_eq!(response.review.tasks[1].qualified_handle, "api/add-cache");
    }

    #[test]
    fn cockpit_summary_counts_operator_work() {
        let mut context = context_with_tasks();
        let mut cleanable = Task::new(
            TaskId::new("task-2"),
            "api",
            "remove-cache",
            "Remove cache",
            "ajax/remove-cache",
            "main",
            "/tmp/worktrees/api-remove-cache",
            "ajax-api-remove-cache",
            "worktrunk",
            AgentClient::Claude,
        );
        cleanable.lifecycle_status = LifecycleStatus::Cleanable;
        context.registry.create_task(cleanable).unwrap();

        let response = cockpit(&context);

        assert_eq!(
            response.summary,
            CockpitSummary {
                repos: 2,
                tasks: 2,
                active_tasks: 0,
                attention_items: 2,
                reviewable_tasks: 1,
                cleanable_tasks: 1,
            }
        );
    }

    #[test]
    fn cockpit_next_matches_next_command() {
        let context = context_with_tasks();

        let response = cockpit(&context);

        assert_eq!(response.next, next(&context));
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
        let mut context = context_with_tasks();
        context.config.test_commands = vec![
            TestCommand::new("web", "cargo test"),
            TestCommand::new("api", "cargo test"),
        ];
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
            .with_existing_paths(["/Users/matt/projects/web", "/Users/matt/projects/api"]);

        let doctor = doctor_with_environment(&context, &environment);
        let status = status(&context);

        assert!(doctor.checks.iter().all(|check| check.ok));
        assert_eq!(status.tasks.len(), 1);
    }

    #[test]
    fn doctor_reports_required_tool_availability() {
        let context = context_with_tasks();
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux"]);

        let doctor = doctor_with_environment(&context, &environment);

        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "tool:git")
                .map(|check| (check.ok, check.message.as_str())),
            Some((true, "available"))
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "tool:codex")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "not found on PATH"))
        );
    }

    #[test]
    fn doctor_reports_repo_config_problems() {
        let config = Config {
            repos: vec![
                ManagedRepo::new("web", "/repos/web", "main"),
                ManagedRepo::new("web", "/missing/web-copy", "main"),
                ManagedRepo::new("api", "/missing/api", "main"),
            ],
            test_commands: vec![TestCommand::new("web", "cargo test")],
            ..Config::default()
        };
        let context = CommandContext::new(config, InMemoryRegistry::default());
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
            .with_existing_paths(["/repos/web"]);

        let doctor = doctor_with_environment(&context, &environment);

        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "config:repo-names")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "duplicate repo name: web"))
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "repo:api:path")
                .map(|check| check.ok),
            Some(false)
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "repo:api:test-command")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "no test command configured"))
        );
    }

    #[test]
    fn stale_task_marking_marks_inactive_old_tasks() {
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
    fn new_task_plan_validates_repo_and_builds_native_lifecycle() {
        let context = context_with_tasks();

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "fix logout".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert!(!plan.requires_confirmation);
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "add",
                        "-b",
                        "ajax/fix-logout",
                        "/Users/matt/projects/web__worktrees/ajax-fix-logout",
                        "main"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-logout",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/Users/matt/projects/web__worktrees/ajax-fix-logout"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "send-keys",
                        "-t",
                        "ajax-web-fix-logout:worktrunk",
                        "codex --cd /Users/matt/projects/web__worktrees/ajax-fix-logout 'fix logout'",
                        "Enter"
                    ]
                )
            ]
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
            "/Users/matt/projects/web__worktrees/ajax-fix-login"
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
    fn record_new_task_reuses_removed_task_handle() {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut removed = task_from_new_request(
            &context,
            &NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();
        removed.lifecycle_status = LifecycleStatus::Removed;
        context.registry.create_task(removed).unwrap();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
        };

        let task = super::record_new_task(&mut context, &request).unwrap();

        assert_eq!(task.qualified_handle(), "web/fix-login");
        assert_eq!(context.registry.list_tasks().len(), 1);
        assert_eq!(
            context.registry.list_tasks()[0].lifecycle_status,
            LifecycleStatus::Provisioning
        );
    }

    #[test]
    fn open_task_plan_targets_worktrunk_directly() {
        let context = context_with_tasks();

        let outside_tmux = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
        let inside_tmux =
            open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

        assert_eq!(
            outside_tmux.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
        assert_eq!(
            inside_tmux.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn open_task_plan_blocks_removed_tasks() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Removed;

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task is removed"]);
    }

    #[test]
    fn direct_task_plans_block_removed_tasks() {
        let mut context = context_with_test_command();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Removed;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });

        let plans = [
            open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap(),
            merge_task_plan(&context, "web/fix-login").unwrap(),
            clean_task_plan(&context, "web/fix-login").unwrap(),
            check_task_plan(&context, "web/fix-login").unwrap(),
            diff_task_plan(&context, "web/fix-login").unwrap(),
        ];

        for plan in plans {
            assert!(plan.commands.is_empty(), "{}", plan.title);
            assert!(
                plan.blocked_reasons
                    .iter()
                    .any(|reason| reason == "task is removed"),
                "{}: {:?}",
                plan.title,
                plan.blocked_reasons
            );
        }
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
    fn check_task_plan_blocks_missing_worktree() {
        let mut context = context_with_test_command();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let plan = check_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
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
    fn diff_task_plan_blocks_missing_worktree() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let plan = diff_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
    }

    #[test]
    fn trunk_task_plan_still_repairs_missing_tmux_flag() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::TmuxMissing);

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn open_task_plan_routes_missing_tmux_to_trunk_repair() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::TmuxMissing);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = None;

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert_eq!(plan.title, "open worktrunk: web/fix-login");
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-login",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn lifecycle_transitions_update_registry_status() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Created;

        super::mark_task_opened(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Created
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Mergeable;
        super::mark_task_merged(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Merged
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Cleanable;
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
    fn mark_task_opened_preserves_existing_lifecycle() {
        for status in [
            LifecycleStatus::Reviewable,
            LifecycleStatus::Merged,
            LifecycleStatus::Cleanable,
        ] {
            let mut context = context_with_tasks();
            context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status = status;

            super::mark_task_opened(&mut context, "web/fix-login").unwrap();

            assert_eq!(
                context
                    .registry
                    .get_task(&TaskId::new("task-1"))
                    .unwrap()
                    .lifecycle_status,
                status
            );
        }
    }

    #[test]
    fn merge_plan_requires_confirmation_when_task_needs_attention() {
        let context = context_with_tasks();

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "merge",
                        "--ff-only",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[test]
    fn merge_task_plan_blocks_non_review_states() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.blocked_reasons,
            vec!["merge requires reviewable or mergeable lifecycle"]
        );
    }

    #[test]
    fn merge_task_plan_allows_mergeable_tasks() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Mergeable;

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[rstest]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        }),
        None,
        "merge requires clean worktree evidence"
    )]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: true,
            last_commit: None,
        }),
        None,
        "merge requires clean worktree evidence"
    )]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: false,
            current_branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        }),
        None,
        "task branch is missing"
    )]
    #[case(None, Some(SideFlag::Dirty), "merge requires clean worktree evidence")]
    #[case(
        None,
        Some(SideFlag::Conflicted),
        "merge requires clean worktree evidence"
    )]
    #[case(None, Some(SideFlag::BranchMissing), "task branch is missing")]
    fn merge_task_plan_blocks_risky_or_missing_branch_evidence(
        #[case] git_status: Option<GitStatus>,
        #[case] side_flag: Option<SideFlag>,
        #[case] expected_reason: &str,
    ) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.git_status = git_status;
        if let Some(side_flag) = side_flag {
            task.add_side_flag(side_flag);
        }

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec![expected_reason]);
    }

    #[test]
    fn clean_plan_uses_policy_and_native_cleanup() {
        let context = context_with_cleanable_task();

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-d",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[rstest]
    #[case(SideFlag::Dirty)]
    #[case(SideFlag::Conflicted)]
    fn clean_plan_requires_confirmation_for_risky_cleanup(#[case] side_flag: SideFlag) {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(side_flag);
        if let Some(git_status) = task.git_status.as_mut() {
            match side_flag {
                SideFlag::Dirty => {
                    git_status.dirty = true;
                }
                SideFlag::Conflicted => {
                    git_status.conflicted = true;
                }
                _ => {}
            }
        }

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn clean_task_plan_blocks_non_cleanup_lifecycle() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.blocked_reasons,
            vec!["clean requires merged or cleanable lifecycle"]
        );
    }

    #[test]
    fn remove_task_plan_force_removes_active_task_resources() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });

        let plan = remove_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "--force",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-D",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[test]
    fn cleanup_and_remove_plans_are_distinct() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });

        let cleanup = clean_task_plan(&context, "web/fix-login").unwrap();
        let remove = remove_task_plan(&context, "web/fix-login").unwrap();

        assert!(!cleanup.requires_confirmation);
        assert!(remove.requires_confirmation);
        assert_ne!(cleanup.commands, remove.commands);
        assert!(remove.commands.iter().any(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "--force")
                && command.args.iter().any(|arg| arg == "worktree")
        }));
        assert!(remove.commands.iter().any(|command| {
            command.program == "git" && command.args.iter().any(|arg| arg == "-D")
        }));
    }

    #[test]
    fn cleanup_git_status_bookkeeping_updates_only_cleanup_evidence() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Merged;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.git_status = None;
        task.tmux_status = None;
        task.worktrunk_status = None;
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktrees/web-fix-login",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )]
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Merged);
        assert!(task.git_status.as_ref().is_some_and(|status| {
            status.worktree_exists
                && status.branch_exists
                && status.merged
                && !status.dirty
                && status.untracked_files == 0
        }));
        assert!(task.tmux_status.is_none());
        assert!(task.worktrunk_status.is_none());
        assert!(task.live_status.is_none());
    }

    #[test]
    fn cleanup_git_status_refreshes_even_when_cached_status_exists() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status.as_mut().unwrap().dirty = true;
        task.add_side_flag(SideFlag::Dirty);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktrees/web-fix-login",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )]
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.git_status.as_ref().unwrap().dirty);
        assert!(!task.has_side_flag(SideFlag::Dirty));
    }

    #[test]
    fn git_evidence_refresh_parses_status_and_side_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = None;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login [ahead 2]\nUU src/lib.rs\n?? notes.md\n",
        )]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        let git_status = task.git_status.as_ref().unwrap();
        assert!(git_status.worktree_exists);
        assert!(git_status.branch_exists);
        assert_eq!(git_status.current_branch.as_deref(), Some("ajax/fix-login"));
        assert!(git_status.dirty);
        assert!(git_status.conflicted);
        assert_eq!(git_status.untracked_files, 1);
        assert_eq!(git_status.unpushed_commits, 2);
        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::Unpushed));
    }

    #[test]
    fn git_evidence_refresh_clears_recovered_missing_worktree_and_branch_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn git_evidence_refresh_preserves_unresolved_missing_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::BranchMissing);
        let mut runner = QueuedRunner::new(vec![output(0, "## HEAD (no branch)\n")]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn failed_git_evidence_refresh_preserves_existing_missing_flags() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);
        let mut runner = QueuedRunner::new(vec![CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "not a git repository".to_string(),
        }]);

        let result = super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false);

        assert!(result.is_err());
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn confirmed_cleanup_deletes_existing_unmerged_branch() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status.as_mut().unwrap().merged = false;
        task.add_side_flag(SideFlag::NeedsInput);

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-D",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[test]
    fn sweep_cleanup_plans_only_safe_candidates() {
        let context = context_with_cleanable_task();

        let plan = sweep_cleanup_plan(&context);

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-d",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[test]
    fn sweep_cleanup_ignores_removed_tasks() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Removed)
            .unwrap();

        let plan = sweep_cleanup_plan(&context);
        let candidates = super::sweep_cleanup_candidates(&context);

        assert!(plan.commands.is_empty());
        assert!(candidates.is_empty());
    }

    #[test]
    fn trunk_task_plan_recreates_missing_tmux_session_with_worktrunk() {
        let context = context_with_tasks();

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-login",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn trunk_task_plan_repairs_worktrunk_when_tmux_session_exists() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: true,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/other".into(),
            points_at_expected_path: false,
        });

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["kill-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new(
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
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn trunk_task_plan_creates_missing_worktrunk_when_tmux_session_exists() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: false,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/worktrees/web-fix-login".into(),
            points_at_expected_path: false,
        });

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
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
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
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
        plan.commands.push(CommandSpec::new("git", ["status"]));

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
            .push(CommandSpec::new("git", ["merge", "ajax/fix-login"]));

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "git".to_string(),
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
            CommandSpec::new("git", ["status"]).with_cwd("/Users/matt/Desktop/Projects/autodoctor"),
        );

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 1,
                stderr: String::new(),
                cwd: Some("/Users/matt/Desktop/Projects/autodoctor".to_string()),
            })
        );
    }
}
