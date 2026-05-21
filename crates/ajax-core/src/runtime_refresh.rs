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

pub fn refresh_runtime_context<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
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
