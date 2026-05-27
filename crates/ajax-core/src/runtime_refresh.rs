use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    adapters::{CommandRunner, GitAdapter, TmuxAdapter},
    commands::{self, CommandContext, CommandError},
    config::WorktreePlacement,
    live::{self, LiveObservation, LiveStatusKind},
    models::{
        AgentClient, GitStatus, LifecycleStatus, RuntimeHealth, RuntimeObservationSource, Task,
        TaskId, WorktrunkStatus,
    },
    registry::{Registry, RegistryError},
    runtime::RUNTIME_PROJECTION_FRESH_FOR,
};

pub trait AgentStatusCache {
    fn status_values_for_session(&self, session: &str) -> Vec<String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoAgentStatusCache;

impl AgentStatusCache for NoAgentStatusCache {
    fn status_values_for_session(&self, _session: &str) -> Vec<String> {
        Vec::new()
    }
}

pub fn refresh_runtime_context<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CommandError> {
    refresh_runtime_context_with_agent_status_cache(context, runner, &NoAgentStatusCache)
}

pub fn refresh_runtime_context_with_agent_status_cache<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    agent_status_cache: &impl AgentStatusCache,
) -> Result<bool, CommandError> {
    let tasks: Vec<Task> = context.registry.list_tasks().into_iter().cloned().collect();
    let should_probe_tasks = tasks.iter().any(should_probe_live_substrate);
    if !should_probe_tasks {
        return Ok(false);
    }
    let mut registered_task_handles = tasks
        .iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .map(|task| (task.repo.clone(), task.handle.clone()))
        .collect::<BTreeSet<_>>();
    let probe_task_ids: Vec<TaskId> = tasks
        .iter()
        .filter(|task| should_probe_live_substrate(task))
        .map(|task| task.id.clone())
        .collect();
    let mut registered_runtime_tasks = tasks
        .iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .map(|task| {
            (
                task.id.clone(),
                task.repo.clone(),
                task.branch.clone(),
                task.worktree_path.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut changed = if needs_git_substrate_refresh(&tasks) {
        commands::refresh_git_substrate_evidence(context, runner)?
    } else {
        false
    };

    let tmux = TmuxAdapter::new("tmux");
    let sessions_command = tmux.list_sessions();
    let sessions_output = match runner.run(&sessions_command) {
        Ok(output) if output.status_code == 0 => output.stdout,
        Ok(_output) => return Ok(false),
        Err(_error) => return Ok(false),
    };

    let task_snapshots: Vec<Task> = probe_task_ids
        .iter()
        .filter_map(|task_id| context.registry.get_task(task_id).cloned())
        .collect();
    let should_discover_orphans = task_snapshots.iter().any(should_probe_live_substrate);
    let windows_output = if task_snapshots
        .iter()
        .any(|task| TmuxAdapter::parse_session_status(&task.tmux_session, &sessions_output).exists)
    {
        let windows_command = tmux.list_all_windows();
        match runner.run(&windows_command) {
            Ok(output) if output.status_code == 0 => Some(Ok(output.stdout)),
            Ok(_) | Err(_) => Some(Err(())),
        }
    } else {
        None
    };

    let should_scan_orphans = should_scan_for_orphan_worktrees(&task_snapshots);

    for task_snapshot in task_snapshots {
        let task_id = task_snapshot.id.clone();
        let session_status =
            TmuxAdapter::parse_session_status(&task_snapshot.tmux_session, &sessions_output);

        if !session_status.exists {
            let has_fresh_complete_command_result_runtime = task_snapshot.runtime_projection.source
                == RuntimeObservationSource::CommandResult
                && !task_snapshot
                    .runtime_projection
                    .requires_refresh(SystemTime::now(), RUNTIME_PROJECTION_FRESH_FOR)
                && task_snapshot
                    .worktrunk_status
                    .as_ref()
                    .is_some_and(|status| status.exists && status.points_at_expected_path);
            if has_fresh_complete_command_result_runtime
                && task_snapshot.tmux_status.is_some()
                && task_snapshot.live_status.is_none()
                && !task_snapshot.has_side_flag(crate::models::SideFlag::TmuxMissing)
            {
                continue;
            }
            changed |= task_snapshot.tmux_status.as_ref() != Some(&session_status);
            context
                .registry
                .update_tmux_status(&task_id, Some(session_status.clone()))
                .map_err(CommandError::Registry)?;
            let missing_worktrunk = WorktrunkStatus {
                exists: false,
                window_name: task_snapshot.worktrunk_window.clone(),
                current_path: task_snapshot.worktree_path.clone(),
                points_at_expected_path: false,
            };
            changed |= task_snapshot.worktrunk_status.as_ref() != Some(&missing_worktrunk);
            context
                .registry
                .update_worktrunk_status(&task_id, Some(missing_worktrunk))
                .map_err(CommandError::Registry)?;
            refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                task.remove_side_flag(crate::models::SideFlag::AgentRunning);
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux session missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        }
        changed |= task_snapshot.tmux_status.as_ref() != Some(&session_status);

        let tmux_status_changed = task_snapshot.tmux_status.as_ref() != Some(&session_status);
        let had_stale_tmux_missing =
            task_snapshot.has_side_flag(crate::models::SideFlag::TmuxMissing);
        changed |= tmux_status_changed || had_stale_tmux_missing;

        if tmux_status_changed || had_stale_tmux_missing {
            context
                .registry
                .update_tmux_status(&task_id, Some(session_status.clone()))
                .map_err(CommandError::Registry)?;
        }

        let Some(Ok(windows_output)) = windows_output.as_ref() else {
            context
                .registry
                .update_worktrunk_status(
                    &task_id,
                    Some(WorktrunkStatus {
                        exists: false,
                        window_name: task_snapshot.worktrunk_window.clone(),
                        current_path: task_snapshot.worktree_path.clone(),
                        points_at_expected_path: false,
                    }),
                )
                .map_err(CommandError::Registry)?;
            refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        };
        let all_windows_output_empty = windows_output.trim().is_empty();
        let mut worktrunk_status = TmuxAdapter::parse_worktrunk_status_for_session(
            &task_snapshot.tmux_session,
            &task_snapshot.worktrunk_window,
            &task_snapshot.worktree_path.display().to_string(),
            windows_output,
        );
        if !worktrunk_status.exists && all_windows_output_empty {
            let windows_command = tmux.list_windows(&task_snapshot.tmux_session);
            if let Ok(output) = runner.run(&windows_command) {
                if output.status_code == 0 {
                    worktrunk_status = TmuxAdapter::parse_worktrunk_status(
                        &task_snapshot.worktrunk_window,
                        &task_snapshot.worktree_path.display().to_string(),
                        &output.stdout,
                    );
                }
            }
        }
        changed |= task_snapshot.worktrunk_status.as_ref() != Some(&worktrunk_status);

        let worktrunk_status_changed =
            task_snapshot.worktrunk_status.as_ref() != Some(&worktrunk_status);
        let had_stale_worktrunk_missing =
            task_snapshot.has_side_flag(crate::models::SideFlag::WorktrunkMissing);
        changed |= worktrunk_status_changed || had_stale_worktrunk_missing;

        if worktrunk_status_changed || had_stale_worktrunk_missing {
            context
                .registry
                .update_worktrunk_status(&task_id, Some(worktrunk_status.clone()))
                .map_err(CommandError::Registry)?;
        }
        refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);

        if !worktrunk_status.exists {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        }

        let agent_status_values =
            agent_status_cache.status_values_for_session(&task_snapshot.tmux_session);
        if let Some(observation) =
            live::reduce_agent_status_values(agent_status_values.iter().map(String::as_str))
        {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                let previous = task.clone();
                task.remove_side_flag(crate::models::SideFlag::TmuxMissing);
                task.remove_side_flag(crate::models::SideFlag::WorktrunkMissing);
                live::apply_authoritative_observation(task, observation);
                refresh_cached_annotations(task);
                changed |= *task != previous;
            }
            continue;
        }

        let pane_command =
            tmux.capture_pane(&task_snapshot.tmux_session, &task_snapshot.worktrunk_window);
        let pane_output = match runner.run(&pane_command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(_) | Err(_) => {
                if let Some(task) = context.registry.get_task_mut(&task_id) {
                    live::apply_observation(
                        task,
                        LiveObservation::new(LiveStatusKind::CommandFailed, "live refresh failed"),
                    );
                    refresh_cached_annotations(task);
                    changed = true;
                }
                continue;
            }
        };
        let observation = live::classify_pane(&pane_output);
        if let Some(task) = context.registry.get_task_mut(&task_id) {
            let live_status_unchanged = task.live_status.as_ref() == Some(&observation);
            let had_recoverable_missing_flag = task
                .has_side_flag(crate::models::SideFlag::TmuxMissing)
                || task.has_side_flag(crate::models::SideFlag::WorktrunkMissing);
            let needs_agent_running_flag = observation.kind == LiveStatusKind::AgentRunning
                && !task.has_side_flag(crate::models::SideFlag::AgentRunning);
            if live_status_unchanged && !had_recoverable_missing_flag && !needs_agent_running_flag {
                continue;
            }
            let previous = task.clone();
            task.remove_side_flag(crate::models::SideFlag::TmuxMissing);
            task.remove_side_flag(crate::models::SideFlag::WorktrunkMissing);
            live::apply_observation(task, observation);
            refresh_cached_annotations(task);
            changed |= *task != previous;
        }
    }

    if should_discover_orphans
        && should_scan_orphans
        && !sessions_output.trim().is_empty()
        && windows_output.as_ref().is_none_or(|output| output.is_ok())
    {
        changed |= recover_missing_tasks_from_substrate(
            context,
            runner,
            &sessions_output,
            &mut registered_task_handles,
            &mut registered_runtime_tasks,
        )?;
    }

    Ok(changed)
}
fn should_scan_for_orphan_worktrees(task_snapshots: &[Task]) -> bool {
    let now = SystemTime::now();
    if task_snapshots
        .iter()
        .any(|task| task.lifecycle_status == LifecycleStatus::Provisioning)
    {
        return true;
    }

    task_snapshots.iter().any(|task| {
        if !should_probe_live_substrate(task) {
            return false;
        }

        task.runtime_projection.source == RuntimeObservationSource::Unknown
            || task.runtime_projection.health == RuntimeHealth::Unobservable
            || task
                .runtime_projection
                .requires_refresh(now, RUNTIME_PROJECTION_FRESH_FOR)
    })
}

fn needs_git_substrate_refresh(tasks: &[Task]) -> bool {
    let now = SystemTime::now();
    tasks.iter().any(|task| {
        task.lifecycle_status != LifecycleStatus::Removed
            && task.git_status.is_some()
            && (task.has_side_flag(crate::models::SideFlag::WorktreeMissing)
                || task.has_side_flag(crate::models::SideFlag::BranchMissing)
                || task.runtime_projection.source == RuntimeObservationSource::Unknown
                || task.runtime_projection.health == RuntimeHealth::Unobservable
                || task
                    .runtime_projection
                    .requires_refresh(now, RUNTIME_PROJECTION_FRESH_FOR))
    })
}

fn should_probe_live_substrate(task: &Task) -> bool {
    matches!(
        task.lifecycle_status,
        LifecycleStatus::Provisioning
            | LifecycleStatus::Active
            | LifecycleStatus::Waiting
            | LifecycleStatus::Reviewable
    ) || has_recoverable_error_live_status(task)
        || task.has_side_flag(crate::models::SideFlag::AgentRunning)
        || task.has_side_flag(crate::models::SideFlag::TmuxMissing)
        || task.has_side_flag(crate::models::SideFlag::WorktrunkMissing)
}

fn has_recoverable_error_live_status(task: &Task) -> bool {
    task.lifecycle_status == LifecycleStatus::Error
        && task.live_status.as_ref().is_some_and(|status| {
            matches!(
                status.kind,
                LiveStatusKind::WaitingForApproval
                    | LiveStatusKind::WaitingForInput
                    | LiveStatusKind::Blocked
                    | LiveStatusKind::RateLimited
                    | LiveStatusKind::AuthRequired
                    | LiveStatusKind::MergeConflict
                    | LiveStatusKind::CiFailed
                    | LiveStatusKind::ContextLimit
                    | LiveStatusKind::CommandFailed
            )
        })
}

fn refresh_runtime_projection_from_tmux_probe<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    changed: &mut bool,
) {
    if let Some(task) = context.registry.get_task_mut(task_id) {
        let previous_health = task.runtime_projection.health;
        task.refresh_runtime_projection_from_source(RuntimeObservationSource::TmuxProbe);
        *changed |= task.runtime_projection.health != previous_health;
    }
}

fn recover_missing_tasks_from_substrate<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    sessions_output: &str,
    registered_tasks: &mut BTreeSet<(String, String)>,
    registered_runtime_tasks: &mut Vec<(TaskId, String, String, PathBuf)>,
) -> Result<bool, CommandError> {
    if context.config.repos.is_empty() {
        return Ok(false);
    }

    let git = GitAdapter::new("git");
    let mut changed = false;

    for repo in context.config.repos.clone() {
        let command = git.list_worktrees(&repo.path.display().to_string());
        let output = match runner.run(&command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(_) | Err(_) => continue,
        };

        for worktree in GitAdapter::parse_worktrees(&output) {
            if !worktree_allowed_for_runtime(
                &context.runtime_paths.worktree_placement,
                &worktree.path,
            ) {
                continue;
            }
            let Some(branch) = worktree.branch.as_deref() else {
                continue;
            };
            let Some(handle) = branch.strip_prefix("ajax/") else {
                continue;
            };
            if handle.is_empty() {
                continue;
            }

            let task_key = (repo.name.clone(), handle.to_string());
            if registered_tasks.contains(&task_key) {
                continue;
            }

            let task_id = TaskId::new(format!("{}/{}", repo.name, handle));
            let tmux_session = format!("ajax-{}-{handle}", repo.name);
            let tmux_status = TmuxAdapter::parse_session_status(&tmux_session, sessions_output);

            let mut task = Task::new(
                task_id.clone(),
                repo.name.clone(),
                handle.to_string(),
                handle.replace('-', " "),
                branch.to_string(),
                repo.default_branch.clone(),
                worktree.path,
                tmux_session,
                "worktrunk",
                AgentClient::Codex,
            );
            crate::lifecycle::mark_active(&mut task).map_err(|error| {
                CommandError::Registry(RegistryError::InvalidLifecycleTransition(error))
            })?;
            task.git_status = Some(GitStatus {
                worktree_exists: true,
                branch_exists: true,
                current_branch: Some(branch.to_string()),
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: None,
            });
            task.tmux_status = Some(tmux_status.clone());
            if !tmux_status.exists {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                });
                task.add_side_flag(crate::models::SideFlag::TmuxMissing);
                task.add_side_flag(crate::models::SideFlag::WorktrunkMissing);
                live::apply_observation(
                    &mut task,
                    LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux session missing"),
                );
                refresh_cached_annotations(&mut task);
            }
            let stale_task_ids = registered_runtime_tasks
                .iter()
                .filter(
                    |(_, existing_repo, existing_branch, existing_worktree_path)| {
                        existing_repo == &repo.name
                            && existing_worktree_path == &task.worktree_path
                            && existing_branch != &task.branch
                    },
                )
                .map(|(task_id, _, _, _)| task_id.clone())
                .collect::<Vec<_>>();
            for stale_task_id in stale_task_ids {
                context
                    .registry
                    .delete_task(&stale_task_id)
                    .map_err(CommandError::Registry)?;
                registered_runtime_tasks
                    .retain(|(existing_task_id, _, _, _)| existing_task_id != &stale_task_id);
            }
            registered_runtime_tasks.push((
                task.id.clone(),
                task.repo.clone(),
                task.branch.clone(),
                task.worktree_path.clone(),
            ));
            context
                .registry
                .create_task(task)
                .map_err(CommandError::Registry)?;
            registered_tasks.insert(task_key);
            changed = true;
        }
    }

    Ok(changed)
}

fn worktree_allowed_for_runtime(placement: &WorktreePlacement, worktree_path: &str) -> bool {
    match placement {
        WorktreePlacement::LegacySibling => true,
        WorktreePlacement::Root(root) => Path::new(worktree_path).starts_with(root),
    }
}

fn refresh_cached_annotations(task: &mut Task) {
    task.annotations = crate::attention::annotate(task);
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        time::{Duration, SystemTime},
    };

    use super::{
        refresh_runtime_context, refresh_runtime_context_with_agent_status_cache, AgentStatusCache,
    };
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo, RuntimePathRequest},
        live::{LiveObservation, LiveStatusKind},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, RuntimeHealth,
            RuntimeObservationSource, RuntimeProjection, SideFlag, StepReceipt, Task, TaskId,
            TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry, RegistryError, RegistryEvent, RegistryEventKind},
    };

    struct StaticAgentStatusCache {
        values: Vec<String>,
    }

    const BASE_BRANCH: &str = "main";
    const REPO_NAME: &str = "web";
    const REPO_PATH: &str = "/Users/matt/projects/web";
    const TASK_BRANCH: &str = "ajax/fix-login";
    const TASK_ID: &str = "task-1";
    const TASK_SESSION: &str = "ajax-web-fix-login";
    const TASK_WORKTREE: &str = "/tmp/worktrees/web-fix-login";
    const TASK_WINDOW: &str = "worktrunk";

    impl AgentStatusCache for StaticAgentStatusCache {
        fn status_values_for_session(&self, _session: &str) -> Vec<String> {
            self.values.clone()
        }
    }

    #[derive(Default)]
    struct RuntimeRefreshRunner;

    impl CommandRunner for RuntimeRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = runtime_stdout(&command.args);

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn runtime_stdout(args: &[String]) -> &'static str {
        match arg(args, 0) {
            "list-sessions" => "ajax-web-fix-login\n",
            "-C" if git_worktree_list(args) => {
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
            }
            "-C" if git_branch_list(args) => "main\najax/fix-login\n",
            "list-windows" => "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n",
            "capture-pane" => "› Improve documentation\n\n  gpt-5.5 high · ~/repo\n",
            _ => "",
        }
    }

    fn arg(args: &[String], index: usize) -> &str {
        args.get(index).map(String::as_str).unwrap_or_default()
    }

    fn git_worktree_list(args: &[String]) -> bool {
        arg(args, 1) == REPO_PATH
            && arg(args, 2) == "worktree"
            && arg(args, 3) == "list"
            && arg(args, 4) == "--porcelain"
    }

    fn git_branch_list(args: &[String]) -> bool {
        arg(args, 1) == REPO_PATH
            && arg(args, 2) == "branch"
            && arg(args, 3) == "--format=%(refname:short)"
    }

    fn context_with_active_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = task_fixture();
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(clean_git_status());
        registry.create_task(task).unwrap();

        CommandContext::new(config, registry)
    }

    fn task_fixture() -> Task {
        Task::new(
            TaskId::new(TASK_ID),
            REPO_NAME,
            "fix-login",
            "Fix login",
            TASK_BRANCH,
            BASE_BRANCH,
            TASK_WORKTREE,
            TASK_SESSION,
            TASK_WINDOW,
            AgentClient::Codex,
        )
    }

    fn clean_git_status() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some(TASK_BRANCH.to_string()),
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

    #[derive(Default)]
    struct HealthyRefreshRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for HealthyRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => runtime_stdout(&command.args),
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[derive(Default)]
    struct CountingRegistry {
        inner: InMemoryRegistry,
        list_tasks_calls: Cell<u32>,
        worktrunk_status_updates: Cell<u32>,
    }

    impl CountingRegistry {
        fn from_registry(inner: InMemoryRegistry) -> Self {
            Self {
                inner,
                list_tasks_calls: Cell::new(0),
                worktrunk_status_updates: Cell::new(0),
            }
        }

        fn list_tasks_calls(&self) -> u32 {
            self.list_tasks_calls.get()
        }

        fn worktrunk_status_updates(&self) -> u32 {
            self.worktrunk_status_updates.get()
        }
    }

    impl Registry for CountingRegistry {
        fn create_task(&mut self, task: Task) -> Result<(), RegistryError> {
            self.inner.create_task(task)
        }

        fn delete_task(&mut self, task_id: &TaskId) -> Result<(), RegistryError> {
            self.inner.delete_task(task_id)
        }

        fn get_task(&self, task_id: &TaskId) -> Option<&Task> {
            self.inner.get_task(task_id)
        }

        fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task> {
            self.inner.get_task_mut(task_id)
        }

        fn list_tasks(&self) -> Vec<&Task> {
            self.list_tasks_calls.set(self.list_tasks_calls.get() + 1);
            self.inner.list_tasks()
        }

        fn update_lifecycle(
            &mut self,
            task_id: &TaskId,
            status: LifecycleStatus,
        ) -> Result<(), RegistryError> {
            self.inner.update_lifecycle(task_id, status)
        }

        fn record_event(
            &mut self,
            task_id: TaskId,
            kind: RegistryEventKind,
            message: impl Into<String>,
        ) -> Result<(), RegistryError> {
            self.inner.record_event(task_id, kind, message)
        }

        fn update_git_status(
            &mut self,
            task_id: &TaskId,
            status: GitStatus,
        ) -> Result<(), RegistryError> {
            self.inner.update_git_status(task_id, status)
        }

        fn update_tmux_status(
            &mut self,
            task_id: &TaskId,
            status: Option<TmuxStatus>,
        ) -> Result<(), RegistryError> {
            self.inner.update_tmux_status(task_id, status)
        }

        fn update_worktrunk_status(
            &mut self,
            task_id: &TaskId,
            status: Option<WorktrunkStatus>,
        ) -> Result<(), RegistryError> {
            self.worktrunk_status_updates
                .set(self.worktrunk_status_updates.get() + 1);
            self.inner.update_worktrunk_status(task_id, status)
        }

        fn apply_live_observation(
            &mut self,
            task_id: &TaskId,
            observation: LiveObservation,
        ) -> Result<(), RegistryError> {
            self.inner.apply_live_observation(task_id, observation)
        }

        fn list_events(&self) -> Vec<&RegistryEvent> {
            self.inner.list_events()
        }

        fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent> {
            self.inner.events_for_task(task_id)
        }

        fn record_step_receipt(&mut self, receipt: StepReceipt) -> Result<(), RegistryError> {
            self.inner.record_step_receipt(receipt)
        }

        fn step_receipts_for_task(&self, task_id: &TaskId) -> Vec<&StepReceipt> {
            self.inner.step_receipts_for_task(task_id)
        }
    }

    fn context_with_unchanged_running_task() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        ));
        task.add_side_flag(SideFlag::AgentRunning);
        task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
        task.worktrunk_status = Some(WorktrunkStatus::present(TASK_WINDOW, TASK_WORKTREE));
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::now(),
            RuntimeObservationSource::TmuxProbe,
        );
        task.last_activity_at = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
        context
    }

    fn context_with_task_for_missing_session() -> CommandContext<CountingRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = task_fixture();
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(clean_git_status());
        task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
        task.worktrunk_status = Some(WorktrunkStatus::present(TASK_WINDOW, TASK_WORKTREE));
        registry.create_task(task).unwrap();

        CommandContext::new(config, CountingRegistry::from_registry(registry))
    }

    #[derive(Default)]
    struct MissingSessionRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for MissingSessionRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-other-task\n",
                [command, ..] if command == "list-windows" => {
                    "ajax-other-task\tworktrunk\t/tmp/worktrees/web-other-task\n"
                }
                [command, ..] if command == "capture-pane" => "",
                _ => runtime_stdout(&command.args),
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[derive(Default)]
    struct OrphanRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for OrphanRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == REPO_PATH
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\nworktree /tmp/worktrees/web-a\nHEAD 3333333\nbranch refs/heads/ajax/a\n\nworktree /tmp/worktrees/web-b\nHEAD 4444444\nbranch refs/heads/ajax/b\n\nworktree /tmp/worktrees/web-c\nHEAD 5555555\nbranch refs/heads/ajax/c\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == REPO_PATH
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\najax/a\najax/b\najax/c\n"
                }
                _ => runtime_stdout(&command.args),
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn hook_status_cache_can_drive_live_refresh_before_pane_classification() {
        let mut context = context_with_active_task();
        let mut runner = RuntimeRefreshRunner;
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn hook_working_status_reactivates_previously_done_task() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.agent_status = AgentRuntimeStatus::Done;
        task.live_status = Some(LiveObservation::new(LiveStatusKind::Done, "done"));
        let mut runner = RuntimeRefreshRunner;
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn unchanged_live_refresh_does_not_report_durable_state_change() {
        let mut context = context_with_unchanged_running_task();
        let previous = context
            .registry
            .get_task(&TaskId::new(TASK_ID))
            .unwrap()
            .clone();
        let mut runner = HealthyRefreshRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        let refreshed = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert!(!changed);
        assert_eq!(refreshed.last_activity_at, previous.last_activity_at);
        assert!(runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }

    #[test]
    fn missing_session_refresh_updates_worktrunk_evidence_once() {
        let mut context = context_with_task_for_missing_session();
        let mut runner = MissingSessionRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        assert_eq!(context.registry.worktrunk_status_updates(), 1);
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }

    #[test]
    fn orphan_recovery_uses_one_registry_snapshot_for_discovered_worktrees() {
        let base = context_with_unchanged_running_task();
        let mut context =
            CommandContext::new(base.config, CountingRegistry::from_registry(base.registry));
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::UNIX_EPOCH,
            RuntimeObservationSource::TmuxProbe,
        );
        let mut runner = OrphanRecoveryRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
        assert!(context.registry.get_task(&TaskId::new("web/b")).is_some());
        assert!(context.registry.get_task(&TaskId::new("web/c")).is_some());
        assert_eq!(
            context.registry.list_tasks_calls(),
            2,
            "expected refresh to reuse the initial task snapshot plus one git refresh scan, got {} list_tasks calls",
            context.registry.list_tasks_calls()
        );
    }

    #[derive(Default)]
    struct GitSkippingRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for GitSkippingRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => runtime_stdout(&command.args),
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn steady_state_refresh_skips_git_substrate_commands() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = GitSkippingRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert!(
            !runner
                .commands
                .iter()
                .any(|command| git_worktree_list(&command.args) || git_branch_list(&command.args)),
            "fresh runtime refresh should not probe git substrate: {:?}",
            runner.commands
        );
    }

    #[test]
    fn tmux_probe_failure_marks_missing_session() {
        struct FailingTmuxRunner {
            inner: MissingSessionRunner,
        }

        impl CommandRunner for FailingTmuxRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                if command
                    .args
                    .first()
                    .is_some_and(|arg| arg == "list-sessions")
                {
                    return Err(CommandRunError::SpawnFailed("tmux unavailable".to_string()));
                }
                self.inner.run(command)
            }
        }

        let mut context = context_with_task_for_missing_session();
        let mut runner = FailingTmuxRunner {
            inner: MissingSessionRunner::default(),
        };

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
    }


    #[test]
    fn orphan_recovery_deletes_stale_same_worktree_task_before_insert() {
        let config = Config {
            repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut stale = Task::new(
            TaskId::new("web/stale-task"),
            REPO_NAME,
            "stale-task",
            "Stale task",
            "ajax/stale-task",
            BASE_BRANCH,
            TASK_WORKTREE,
            "ajax-web-stale-task",
            TASK_WINDOW,
            AgentClient::Codex,
        );
        stale.lifecycle_status = LifecycleStatus::Active;
        stale.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/stale-task".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        registry.create_task(stale).unwrap();
        let mut context = CommandContext::new(config, registry);
        let mut runner = OrphanRecoveryRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        assert!(context
            .registry
            .get_task(&TaskId::new("web/stale-task"))
            .is_none());
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_some());
    }

    #[test]
    fn rooted_runtime_recovery_ignores_legacy_sibling_worktrees() {
        let base = context_with_unchanged_running_task();
        let runtime_paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();
        let mut context = CommandContext::with_runtime_paths(
            base.config,
            CountingRegistry::from_registry(base.registry),
            runtime_paths,
        );
        let mut runner = OrphanRecoveryRunner::default();

        let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(context.registry.get_task(&TaskId::new("web/a")).is_none());
        assert!(context.registry.get_task(&TaskId::new("web/b")).is_none());
        assert!(context.registry.get_task(&TaskId::new("web/c")).is_none());
    }
}
