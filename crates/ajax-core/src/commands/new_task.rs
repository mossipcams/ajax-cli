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
    if let Some(graphify_update) = &repo.graphify_update {
        let graphify_command = format!("({graphify_update}) >/dev/null 2>&1 &");
        plan.commands.push(
            CommandSpec::new("sh", ["-lc", graphify_command.as_str()]).with_cwd(&worktree_path),
        );
    }
    plan.commands.push(tmux.new_detached_task_session(
        &tmux_session,
        DEFAULT_TASK_WINDOW_NAME,
        &worktree_path_string,
    ));
    plan.commands.push(setup_task_environment_command(
        &repo_path,
        &worktree_path_string,
        repo.bootstrap.as_deref(),
    ));
    plan.commands.push(tmux.send_agent_command(
        &tmux_session,
        DEFAULT_TASK_WINDOW_NAME,
        &command_line(&launch),
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

fn setup_task_environment_command(
    repo_path: &str,
    worktree_path: &str,
    bootstrap: Option<&str>,
) -> CommandSpec {
    let mut command = String::from("if [ -d \"$1\" ]; then cd \"$1\" && ");
    command.push_str(HUSKY_GUARD);
    if let Some(bootstrap) = bootstrap {
        command.push_str("; ");
        command.push_str(bootstrap);
    }
    command.push_str("; fi");
    CommandSpec::new(
        "sh",
        ["-lc", command.as_str(), "ajax-setup-task", worktree_path],
    )
    .with_cwd(repo_path)
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

fn validate_managed_repo_name(repo: &str) -> Result<(), CommandError> {
    if repo.is_empty() || repo.contains('/') || repo.contains('\\') || repo.contains("..") {
        return Err(CommandError::PlanBlocked(vec![format!(
            "invalid repo name: {repo}"
        )]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_git_worktree_add_command, is_task_window_new_session_command,
        mark_new_task_provisioning_step_completed, new_task_plan, new_task_plan_with_observation,
        record_new_task, task_from_new_request, NewTaskRequest, StartPlanObservation,
        StartProvisioningStep, DEFAULT_TASK_WINDOW_NAME,
    };
    use crate::{
        adapters::{CommandSpec, GitAdapter},
        commands::CommandContext,
        config::{Config, ManagedRepo, RuntimePathRequest, WorktreePlacement},
        models::{AgentRuntimeStatus, LifecycleStatus, SideFlag},
        registry::{InMemoryRegistry, Registry},
    };
    use std::{path::Path, time::Duration};

    fn context() -> CommandContext<InMemoryRegistry> {
        CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        )
    }

    fn agent_send_keys_line(plan: &crate::commands::CommandPlan) -> &str {
        plan.commands
            .iter()
            .find(|command| {
                command.program == "tmux" && command.args.first() == Some(&"send-keys".to_string())
            })
            .map(|command| command.args[3].as_str())
            .expect("expected tmux send-keys command")
    }

    #[test]
    fn rooted_repo_dir_hash_is_stable_for_known_path() {
        let path = Path::new("/Users/matt/projects/web");
        let first = super::rooted_repo_dir("web", path);
        let second = super::rooted_repo_dir("web", path);

        assert_eq!(first, "web-8ac1d219");
        assert_eq!(second, first);
    }

    #[test]
    fn start_task_identity_uses_core_slug_rules() {
        let first = super::start_task_identity("web", "Fix login");
        let second = super::start_task_identity("web", "Fix login!");

        assert_eq!(first, crate::models::TaskId::new("web/fix-login"));
        assert_eq!(second, first);
    }

    #[test]
    fn unknown_agent_is_preserved_for_execution_but_classified_other() {
        let context = context();
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "custom-agent-cli".to_string(),
            },
        )
        .unwrap();

        let launch = agent_send_keys_line(&plan);
        assert!(launch.ends_with("-- custom-agent-cli"));
        assert_eq!(
            task_from_new_request(
                &context,
                &NewTaskRequest {
                    repo: "web".to_string(),
                    title: "Fix login".to_string(),
                    agent: "custom-agent-cli".to_string(),
                }
            )
            .unwrap()
            .selected_agent,
            crate::models::AgentClient::Other
        );
    }

    #[test]
    fn punctuation_only_title_uses_deterministic_fallback_id() {
        let first = super::start_task_identity("web", "!!!");
        let second = super::start_task_identity("web", "!!!");

        assert_eq!(first, crate::models::TaskId::new("web/task"));
        assert_eq!(second, first);
    }

    #[test]
    fn repo_name_cannot_escape_managed_namespace() {
        let context = context();
        for repo in ["../escape", "web/evil", r"web\evil", ".."] {
            let error = new_task_plan(
                &context,
                NewTaskRequest {
                    repo: repo.to_string(),
                    title: "Fix login".to_string(),
                    agent: "codex".to_string(),
                },
            )
            .unwrap_err();
            assert_eq!(
                error,
                crate::commands::CommandError::PlanBlocked(vec![format!(
                    "invalid repo name: {repo}"
                )])
            );
        }
    }

    #[test]
    fn new_task_plan_claude_agent_command_omits_cd_flag_and_skips_permissions() {
        let context = context();
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "claude".to_string(),
            },
        )
        .unwrap();

        let launch = agent_send_keys_line(&plan);
        assert!(launch.starts_with("ajax-cli __agent-runtime --task-id web/fix-login"));
        assert!(launch.ends_with("-- claude --dangerously-skip-permissions"));
        assert!(!launch.contains("--cd"));
    }

    #[test]
    fn new_task_plan_cursor_agent_command_uses_agent_subcommand() {
        let context = context();
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "cursor".to_string(),
            },
        )
        .unwrap();

        let launch = agent_send_keys_line(&plan);
        assert!(launch.starts_with("ajax-cli __agent-runtime --task-id web/fix-login"));
        assert!(launch.ends_with("-- cursor agent"));
    }

    #[test]
    fn new_task_plan_launches_agent_through_runtime_wrapper() {
        let context = CommandContext::with_runtime_paths(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
            RuntimePathRequest::new("/home/test").resolve(),
        );

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            agent_send_keys_line(&plan),
            "ajax-cli __agent-runtime --task-id web/fix-login --state-root /home/test/.cache/ajax/agent-runtime -- codex --cd /repo/web__worktrees/ajax-fix-login"
        );
    }

    #[test]
    fn new_task_plan_has_no_standalone_husky_command() {
        let context = context();
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert!(
            plan.commands.iter().any(|command| command.program == "sh"),
            "expected standalone setup command: {:?}",
            plan.commands
        );
    }

    #[test]
    fn new_task_plan_chains_setup_before_agent_in_task_session() {
        let context = context();
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        let launch = agent_send_keys_line(&plan);
        assert!(launch.starts_with("ajax-cli __agent-runtime --task-id web/fix-login"));
        assert!(launch.ends_with(
            "ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- codex --cd /repo/web__worktrees/ajax-fix-login"
        ));
    }

    #[test]
    fn new_task_plan_chains_bootstrap_between_husky_and_agent() {
        let mut repo = ManagedRepo::new("web", "/repo/web", "main");
        repo.bootstrap = Some("npm install".to_string());
        let context = CommandContext::new(
            Config {
                repos: vec![repo],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        let launch = agent_send_keys_line(&plan);
        assert!(launch.starts_with("ajax-cli __agent-runtime --task-id web/fix-login"));
        assert!(
            plan.commands.iter().any(|command| {
                command.program == "sh"
                    && command
                        .args
                        .get(1)
                        .is_some_and(|arg| arg.contains("npm install"))
            }),
            "expected standalone bootstrap command: {:?}",
            plan.commands
        );
    }

    #[test]
    fn new_task_plan_fetches_origin_and_branches_from_remote_tracking_ref() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request).unwrap();
        let git = GitAdapter::new("git");

        assert_eq!(plan.commands.len(), 5);
        assert_eq!(
            plan.commands[0],
            git.fetch_origin_branch("/repo/web", "main")
        );
        assert_eq!(
            plan.commands[1],
            git.add_worktree(
                "/repo/web",
                "/repo/web__worktrees/ajax-fix-login",
                "ajax/fix-login",
                "origin/main"
            )
        );
    }

    #[test]
    fn new_task_plan_skips_fetch_when_origin_fetch_is_fresh() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let observation = StartPlanObservation {
            origin_fetch_age: Some(Duration::from_secs(30)),
            target_branch_exists: false,
        };

        let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
        let git = GitAdapter::new("git");

        assert_eq!(plan.commands.len(), 4);
        assert_eq!(
            plan.commands[0],
            git.add_worktree(
                "/repo/web",
                "/repo/web__worktrees/ajax-fix-login",
                "ajax/fix-login",
                "origin/main"
            )
        );
        assert!(plan
            .commands
            .iter()
            .all(|command| !command.args.iter().any(|arg| arg == "fetch")));
    }

    #[test]
    fn new_task_plan_fetches_when_origin_fetch_is_stale() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let observation = StartPlanObservation {
            origin_fetch_age: Some(Duration::from_secs(120)),
            target_branch_exists: false,
        };

        let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
        let git = GitAdapter::new("git");

        assert_eq!(plan.commands.len(), 5);
        assert_eq!(
            plan.commands[0],
            git.fetch_origin_branch("/repo/web", "main")
        );
    }

    #[test]
    fn new_task_plan_fetches_when_origin_fetch_age_is_unknown() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let observation = StartPlanObservation {
            origin_fetch_age: None,
            target_branch_exists: false,
        };

        let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
        let git = GitAdapter::new("git");

        assert_eq!(plan.commands.len(), 5);
        assert_eq!(
            plan.commands[0],
            git.fetch_origin_branch("/repo/web", "main")
        );
    }

    #[test]
    fn new_task_plan_runs_graphify_update_detached() {
        let mut repo = ManagedRepo::new("web", "/repo/web", "main");
        repo.graphify_update = Some("graphify extract --update".to_string());
        let context = CommandContext::new(
            Config {
                repos: vec![repo],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request).unwrap();

        assert_eq!(plan.commands.len(), 6);
        assert_eq!(
            plan.commands[2],
            CommandSpec::new(
                "sh",
                ["-lc", "(graphify extract --update) >/dev/null 2>&1 &"]
            )
            .with_cwd("/repo/web__worktrees/ajax-fix-login")
        );
    }

    #[test]
    fn new_task_plan_runs_graphify_update_in_new_worktree_when_configured() {
        let mut repo = ManagedRepo::new("web", "/repo/web", "main");
        repo.graphify_update = Some("graphify extract --update".to_string());
        let context = CommandContext::new(
            Config {
                repos: vec![repo],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request).unwrap();

        assert_eq!(plan.commands.len(), 6);
        assert_eq!(
            plan.commands[2],
            CommandSpec::new(
                "sh",
                ["-lc", "(graphify extract --update) >/dev/null 2>&1 &"]
            )
            .with_cwd("/repo/web__worktrees/ajax-fix-login")
        );
        assert!(is_git_worktree_add_command(&plan.commands[1]));
        assert!(is_task_window_new_session_command(&plan.commands[3]));
        assert_eq!(plan.commands[3].args[5], "task");
    }

    #[test]
    fn default_new_task_plan_preserves_legacy_sibling_worktree_path() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request).unwrap();

        let worktree_command = plan
            .commands
            .iter()
            .find(|command| is_git_worktree_add_command(command))
            .expect("worktree add command");
        assert!(worktree_command
            .args
            .contains(&"/repo/web__worktrees/ajax-fix-login".to_string()));
    }

    #[test]
    fn rooted_new_task_plan_and_recorded_task_use_runtime_worktree_root() {
        let runtime_paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();
        let worktree_root = match &runtime_paths.worktree_placement {
            WorktreePlacement::Root(root) => root.clone(),
            WorktreePlacement::LegacySibling => panic!("expected rooted placement"),
        };
        let context = CommandContext::with_runtime_paths(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
            runtime_paths,
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request.clone()).unwrap();
        let task = task_from_new_request(&context, &request).unwrap();
        let worktree_command = plan
            .commands
            .iter()
            .find(|command| is_git_worktree_add_command(command))
            .expect("worktree add command");
        let planned_worktree = worktree_command
            .args
            .iter()
            .find(|arg| arg.starts_with(worktree_root.to_str().unwrap()))
            .unwrap();

        assert!(task.worktree_path.starts_with(&worktree_root));
        assert_eq!(Path::new(planned_worktree), task.worktree_path);
        assert!(plan.commands.iter().any(|command| command
            .args
            .iter()
            .any(|arg| arg == task.worktree_path.to_str().unwrap())));
    }

    #[test]
    fn start_provisioning_named_steps_update_state_without_numeric_command_indexes() {
        let mut context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let task = record_new_task(&mut context, &request).unwrap();
        let task_id = task.id.clone();

        mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::WorktreeCreated,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        let git = task.git_status.as_ref().unwrap();
        assert!(git.worktree_exists);
        assert!(git.branch_exists);
        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);

        mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::TaskSessionCreated,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
        assert!(task
            .task_window_status
            .as_ref()
            .is_some_and(|status| status.exists && status.points_at_expected_path));
        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);

        mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::AgentCommandSent,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert!(task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(task.agent_attempts.len(), 1);
        assert_eq!(task.agent_attempts[0].status, AgentRuntimeStatus::Running);
    }

    fn start_collision_task(
        repo: &str,
        handle: &str,
        branch: &str,
        worktree_path: std::path::PathBuf,
    ) -> crate::models::Task {
        use crate::models::{AgentClient, Task, TaskId};
        let tmux_session = format!("ajax-{repo}-{handle}");
        Task::new(
            TaskId::new(format!("{repo}/{handle}")),
            repo.to_string(),
            handle.to_string(),
            handle.to_string(),
            branch.to_string(),
            "main".to_string(),
            worktree_path,
            tmux_session,
            DEFAULT_TASK_WINDOW_NAME.to_string(),
            AgentClient::Codex,
        )
    }

    #[test]
    fn new_task_plan_blocks_when_worktree_path_already_exists() {
        let root = std::env::temp_dir().join(format!(
            "ajax-start-blocked-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_path = root.join("web");
        let worktree_path = root.join("web__worktrees").join("ajax-fix-login");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new(
                    "web",
                    repo_path.display().to_string(),
                    "main",
                )],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let error = new_task_plan(&context, request).unwrap_err();
        let crate::commands::CommandError::PlanBlocked(messages) = &error else {
            panic!("expected PlanBlocked, got {error:?}");
        };
        let message = messages.join("\n");
        assert!(
            message.contains(&worktree_path.display().to_string()),
            "expected message to mention worktree path: {message}"
        );
        assert!(message.contains("already exists"), "message: {message}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn new_task_plan_blocks_when_target_branch_already_exists() {
        let context = context();
        let observation = StartPlanObservation {
            origin_fetch_age: None,
            target_branch_exists: true,
        };
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let error = new_task_plan_with_observation(&context, request, &observation).unwrap_err();
        let crate::commands::CommandError::PlanBlocked(messages) = &error else {
            panic!("expected PlanBlocked, got {error:?}");
        };
        let message = messages.join("\n");
        assert!(message.contains("ajax/fix-login"), "message: {message}");
        assert!(message.contains("branch"), "message: {message}");
    }

    #[test]
    fn new_task_plan_blocks_when_registry_claims_worktree_path_or_branch() {
        use std::path::PathBuf;

        // worktree-path claim
        {
            let mut context = context();
            context
                .registry
                .create_task(start_collision_task(
                    "web",
                    "owasp",
                    "ajax/owasp",
                    PathBuf::from("/repo/web__worktrees/ajax-fix-login"),
                ))
                .unwrap();
            let request = NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            };

            let error = new_task_plan(&context, request).unwrap_err();
            let crate::commands::CommandError::PlanBlocked(messages) = &error else {
                panic!("expected PlanBlocked, got {error:?}");
            };
            let message = messages.join("\n");
            assert!(
                message.contains("web/owasp"),
                "expected claiming handle: {message}"
            );
            assert!(
                message.contains("/repo/web__worktrees/ajax-fix-login"),
                "message: {message}"
            );
        }

        // branch claim
        {
            let mut context = context();
            context
                .registry
                .create_task(start_collision_task(
                    "web",
                    "owasp",
                    "ajax/fix-login",
                    PathBuf::from("/repo/web__worktrees/ajax-owasp"),
                ))
                .unwrap();
            let request = NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
            };

            let error = new_task_plan(&context, request).unwrap_err();
            let crate::commands::CommandError::PlanBlocked(messages) = &error else {
                panic!("expected PlanBlocked, got {error:?}");
            };
            let message = messages.join("\n");
            assert!(
                message.contains("web/owasp"),
                "expected claiming handle: {message}"
            );
            assert!(message.contains("ajax/fix-login"), "message: {message}");
        }
    }
}
