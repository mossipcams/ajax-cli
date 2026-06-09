use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{AgentAdapter, AgentLaunch, CommandSpec, GitAdapter, TmuxAdapter},
    config::WorktreePlacement,
    lifecycle::mark_provisioning,
    models::{
        AgentAttempt, AgentClient, GitStatus, LifecycleStatus, RuntimeObservationSource, SideFlag,
        Task, TaskId, TaskOperationKind, TmuxStatus, WorktrunkStatus,
    },
    registry::{Registry, RegistryError},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

const INSTALL_HUSKY_HOOKS: &str =
    "cd \"$1\" 2>/dev/null || exit 0; if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
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
    let tmux_session = format!("ajax-{}-{handle}", request.repo);
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let selected_agent = agent_from_name(&request.agent);
    let agent = AgentAdapter::new(&request.agent);
    let agent_launch = agent.launch(
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
    plan.commands
        .push(git.fetch_origin_branch(&repo_path, &repo.default_branch));
    if let Some(graphify_update) = &repo.graphify_update {
        plan.commands
            .push(CommandSpec::new("sh", ["-lc", graphify_update.as_str()]).with_cwd(&repo.path));
    }
    plan.commands.push(git.add_worktree(
        &repo_path,
        &worktree_path_string,
        &branch,
        &format!("origin/{}", repo.default_branch),
    ));
    plan.commands
        .push(install_husky_hooks_command(&worktree_path));
    if let Some(bootstrap) = &repo.bootstrap {
        plan.commands
            .push(CommandSpec::new("sh", ["-lc", bootstrap.as_str()]).with_cwd(&worktree_path));
    }
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
                .update_worktrunk_status(
                    task_id,
                    Some(WorktrunkStatus::present(
                        task.worktrunk_window,
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

pub fn is_new_task_husky_hook_command(command: &CommandSpec) -> bool {
    command.program == "/bin/sh"
        && command.args.len() == 4
        && command.args[0] == "-lc"
        && command.args[1] == INSTALL_HUSKY_HOOKS
        && command.args[2] == "sh"
}

pub fn is_git_worktree_add_command(command: &CommandSpec) -> bool {
    command.program == "git"
        && command
            .args
            .windows(2)
            .any(|window| window == ["worktree", "add"])
}

pub fn is_worktrunk_new_session_command(command: &CommandSpec) -> bool {
    command.program == "tmux" && command.args.first().is_some_and(|arg| arg == "new-session")
}

pub fn is_agent_send_keys_command(command: &CommandSpec) -> bool {
    command.program == "tmux" && command.args.first().is_some_and(|arg| arg == "send-keys")
}

pub fn start_provisioning_step_for_command(command: &CommandSpec) -> Option<StartProvisioningStep> {
    if is_git_worktree_add_command(command) {
        Some(StartProvisioningStep::WorktreeCreated)
    } else if is_worktrunk_new_session_command(command) {
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
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    (hasher.finish() & 0xffff_ffff) as u32
}

fn install_husky_hooks_command(worktree_path: &Path) -> CommandSpec {
    CommandSpec {
        program: "/bin/sh".to_string(),
        args: vec![
            "-lc".to_string(),
            INSTALL_HUSKY_HOOKS.to_string(),
            "sh".to_string(),
            worktree_path.display().to_string(),
        ],
        cwd: None,
        mode: crate::adapters::CommandMode::Capture,
        timeout: None,
    }
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

#[cfg(test)]
mod tests {
    use super::{
        is_git_worktree_add_command, mark_new_task_provisioning_step_completed, new_task_plan,
        record_new_task, task_from_new_request, NewTaskRequest, StartProvisioningStep,
    };
    use crate::{
        adapters::{CommandSpec, GitAdapter},
        commands::CommandContext,
        config::{Config, ManagedRepo, RuntimePathRequest, WorktreePlacement},
        models::{AgentRuntimeStatus, LifecycleStatus, SideFlag},
        registry::{InMemoryRegistry, Registry},
    };
    use std::path::Path;

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
    fn new_task_plan_claude_agent_command_omits_cd_flag() {
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
        assert!(launch.ends_with("-- claude"));
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
    fn new_task_plan_fetches_origin_and_branches_from_remote_tracking_ref() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let plan = new_task_plan(&context, request).unwrap();
        let git = GitAdapter::new("git");

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
    fn new_task_plan_runs_graphify_update_in_repo_root_when_configured() {
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

        assert_eq!(
            plan.commands[1],
            CommandSpec::new("sh", ["-lc", "graphify extract --update"]).with_cwd("/repo/web")
        );
        assert!(is_git_worktree_add_command(&plan.commands[2]));
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
            .worktrunk_status
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
}
