use std::time::SystemTime;

use crate::{
    adapters::{CommandRunner, GitAdapter, TmuxAdapter},
    commands::{self, CommandContext, CommandError},
    live::{self, LiveObservation, LiveStatusKind},
    models::{
        AgentClient, GitStatus, LifecycleStatus, RuntimeObservationSource, Task, TaskId,
        WorktrunkStatus,
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
    let tasks = context.registry.list_tasks();
    let should_probe_tasks = tasks.iter().any(|task| should_probe_live_substrate(task));
    let should_refresh_sessions =
        should_probe_tasks || tasks.iter().any(|task| task.tmux_status.is_some());
    if !should_refresh_sessions {
        return Ok(false);
    }

    let mut changed = if should_probe_tasks {
        commands::refresh_git_substrate_evidence(context, runner).unwrap_or_default()
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

    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| should_probe_live_substrate(task))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let task_snapshots = task_ids
        .iter()
        .filter_map(|task_id| context.registry.get_task(task_id).cloned())
        .collect::<Vec<_>>();
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
            let previous = task.clone();
            task.remove_side_flag(crate::models::SideFlag::TmuxMissing);
            task.remove_side_flag(crate::models::SideFlag::WorktrunkMissing);
            live::apply_observation(task, observation);
            refresh_cached_annotations(task);
            changed |= *task != previous;
        }
    }

    if should_discover_orphans
        && !sessions_output.trim().is_empty()
        && windows_output.as_ref().is_none_or(|output| output.is_ok())
    {
        changed |= recover_missing_tasks_from_substrate(context, runner, &sessions_output)?;
    }

    Ok(changed)
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
        let previous = task.runtime_projection.clone();
        task.refresh_runtime_projection_from_source(RuntimeObservationSource::TmuxProbe);
        *changed |= task.runtime_projection != previous;
    }
}

fn registered_task_exists<R: Registry>(
    context: &CommandContext<R>,
    repo_name: &str,
    handle: &str,
) -> bool {
    context.registry.list_tasks().into_iter().any(|task| {
        task.repo == repo_name
            && task.handle == handle
            && task.lifecycle_status != LifecycleStatus::Removed
    })
}

fn recover_missing_tasks_from_substrate<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    sessions_output: &str,
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
            let Some(branch) = worktree.branch.as_deref() else {
                continue;
            };
            let Some(handle) = branch.strip_prefix("ajax/") else {
                continue;
            };
            if handle.is_empty() {
                continue;
            }

            if registered_task_exists(context, &repo.name, handle) {
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
            context
                .registry
                .create_task(task)
                .map_err(CommandError::Registry)?;
            changed = true;
        }
    }

    Ok(changed)
}

fn refresh_cached_annotations(task: &mut Task) {
    task.annotations = crate::attention::annotate(task);
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
            LiveStatusKind, Task, TaskId,
        },
        registry::{InMemoryRegistry, Registry},
    };

    use super::{refresh_runtime_context_with_agent_status_cache, AgentStatusCache};

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
}
