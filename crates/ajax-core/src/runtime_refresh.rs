use std::{
    collections::{BTreeMap, BTreeSet},
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentStatusCacheEntry {
    pub value: String,
    pub observed_at: SystemTime,
    pub fresh: bool,
    pub source: AgentStatusCacheSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentStatusCacheSource {
    Hook,
    RuntimeWrapper,
}

pub trait AgentStatusCache {
    fn status_entries_for_session(&self, session: &str) -> Vec<AgentStatusCacheEntry>;

    fn status_entries_for_task(
        &self,
        _task_id: &TaskId,
        session: &str,
    ) -> Vec<AgentStatusCacheEntry> {
        self.status_entries_for_session(session)
    }
}

/// Controls how much substrate work a refresh pass performs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RefreshTier {
    /// Tmux/live updates only; orphan git discovery runs when gates fire.
    Live,
    /// Always eligible for orphan git discovery when tasks are probed.
    #[default]
    Full,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoAgentStatusCache;

impl AgentStatusCache for NoAgentStatusCache {
    fn status_entries_for_session(&self, _session: &str) -> Vec<AgentStatusCacheEntry> {
        Vec::new()
    }
}

pub fn refresh_runtime_context<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CommandError> {
    refresh_runtime_context_with_tier(context, runner, &NoAgentStatusCache, RefreshTier::Full)
}

pub fn refresh_runtime_context_with_agent_status_cache<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    agent_status_cache: &impl AgentStatusCache,
) -> Result<bool, CommandError> {
    refresh_runtime_context_with_agent_status_cache_and_tier(
        context,
        runner,
        agent_status_cache,
        RefreshTier::Full,
    )
}

pub fn refresh_runtime_context_with_agent_status_cache_and_tier<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    agent_status_cache: &impl AgentStatusCache,
    tier: RefreshTier,
) -> Result<bool, CommandError> {
    refresh_runtime_context_with_tier(context, runner, agent_status_cache, tier)
}

pub fn refresh_runtime_context_with_tier<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    agent_status_cache: &impl AgentStatusCache,
    tier: RefreshTier,
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
    let registered_sessions = tasks
        .iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .map(|task| task.tmux_session.clone())
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
        Ok(output) => {
            let reason = format!(
                "tmux list-sessions probe failed: exited with status {}",
                output.status_code
            );
            for task_id in &probe_task_ids {
                record_runtime_probe_failure(context, task_id, reason.clone(), &mut changed);
            }
            return Ok(changed);
        }
        Err(error) => {
            let reason = format!("tmux list-sessions probe failed: {error}");
            for task_id in &probe_task_ids {
                record_runtime_probe_failure(context, task_id, reason.clone(), &mut changed);
            }
            return Ok(changed);
        }
    };

    let task_lookup: BTreeMap<TaskId, Task> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect();
    let task_snapshots: Vec<Task> = probe_task_ids
        .iter()
        .filter_map(|task_id| task_lookup.get(task_id).cloned())
        .collect();
    let should_discover_orphans = task_snapshots.iter().any(should_probe_live_substrate);
    let should_scan_orphans = should_scan_for_orphan_worktrees(&task_snapshots)
        || unregistered_ajax_sessions_in_tmux(&sessions_output, &registered_sessions);
    let should_run_orphan_discovery =
        should_discover_orphans && (tier == RefreshTier::Full || should_scan_orphans);
    let windows_output = if task_snapshots
        .iter()
        .any(|task| TmuxAdapter::parse_session_status(&task.tmux_session, &sessions_output).exists)
    {
        let windows_command = tmux.list_all_windows();
        match runner.run(&windows_command) {
            Ok(output) if output.status_code == 0 => Some(Ok(output.stdout)),
            Ok(output) => Some(Err(format!(
                "tmux list-windows probe failed: exited with status {}",
                output.status_code
            ))),
            Err(error) => Some(Err(format!("tmux list-windows probe failed: {error}"))),
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
                if !matches!(
                    task.lifecycle_status,
                    LifecycleStatus::Removing | LifecycleStatus::TeardownIncomplete
                ) {
                    live::apply_observation(
                        task,
                        LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux session missing"),
                    );
                }
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
            let reason = windows_output
                .as_ref()
                .and_then(|output| output.as_ref().err())
                .cloned()
                .unwrap_or_else(|| "tmux list-windows probe failed: not observed".to_string());
            record_runtime_probe_failure(context, &task_id, reason, &mut changed);
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

        let now = SystemTime::now();
        let agent_status_entries = agent_status_cache
            .status_entries_for_task(&task_snapshot.id, &task_snapshot.tmux_session);
        let candidates: Vec<live::StatusCandidate> = agent_status_entries
            .iter()
            .map(|entry| {
                live::StatusCandidate::new(
                    match entry.source {
                        AgentStatusCacheSource::Hook => live::AgentEvidenceSource::Hook,
                        AgentStatusCacheSource::RuntimeWrapper => {
                            live::AgentEvidenceSource::RuntimeWrapper
                        }
                    },
                    entry.value.clone(),
                    entry.observed_at,
                )
            })
            .collect();
        let decision = live::select_status_observation(live::StatusDecisionInput {
            selected_agent: task_snapshot.selected_agent,
            prior: task_snapshot.live_status.as_ref(),
            acknowledged_at: task_snapshot.attention_acknowledged_at,
            now,
            candidates: &candidates,
        });
        if let (true, Some(observation), Some(source), Some(observed_at)) = (
            decision.applied,
            decision.observation,
            decision.source,
            decision.observed_at,
        ) {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                let live_status_unchanged = task
                    .live_status
                    .as_ref()
                    .is_some_and(|status| status.kind == observation.kind)
                    && task
                        .live_status_observed_at
                        .is_some_and(|current| current >= observed_at);
                let needs_agent_running_flag = observation.kind == LiveStatusKind::AgentRunning
                    && !task.has_side_flag(crate::models::SideFlag::AgentRunning);
                if live_status_unchanged && !needs_agent_running_flag {
                    continue;
                }
                let previous = task.clone();
                task.remove_side_flag(crate::models::SideFlag::TmuxMissing);
                task.remove_side_flag(crate::models::SideFlag::WorktrunkMissing);
                match source {
                    live::AgentEvidenceSource::Hook => {
                        live::apply_authoritative_observation_at(task, observation, observed_at);
                    }
                    live::AgentEvidenceSource::RuntimeWrapper => {
                        live::apply_trusted_observation_at(task, observation, observed_at);
                    }
                }
                refresh_cached_annotations(task);
                changed |= *task != previous;
            }
            continue;
        }

        if decision.acknowledged_hold {
            // A fresh Claude waiting hook is acknowledged: hold the current
            // non-actionable state without probing the pane.
            continue;
        }

        let pane_command =
            tmux.capture_pane(&task_snapshot.tmux_session, &task_snapshot.worktrunk_window);
        let pane_output = match runner.run(&pane_command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(output) => {
                record_runtime_probe_failure(
                    context,
                    &task_id,
                    format!(
                        "tmux capture-pane probe failed: exited with status {}",
                        output.status_code
                    ),
                    &mut changed,
                );
                continue;
            }
            Err(error) => {
                record_runtime_probe_failure(
                    context,
                    &task_id,
                    format!("tmux capture-pane probe failed: {error}"),
                    &mut changed,
                );
                continue;
            }
        };
        let observation = live::classify_agent_pane(task_snapshot.selected_agent, &pane_output);
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

    if should_run_orphan_discovery
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

fn unregistered_ajax_sessions_in_tmux(
    sessions_output: &str,
    registered_sessions: &BTreeSet<String>,
) -> bool {
    sessions_output.lines().any(|line| {
        let session = line.trim();
        session.starts_with("ajax-") && !registered_sessions.contains(session)
    })
}

fn needs_git_substrate_refresh(tasks: &[Task]) -> bool {
    let now = SystemTime::now();
    tasks.iter().any(|task| {
        let has_missing_git_substrate = task
            .has_side_flag(crate::models::SideFlag::WorktreeMissing)
            || task.has_side_flag(crate::models::SideFlag::BranchMissing);
        let has_stale_cached_git_status = task.git_status.is_some()
            && (task.runtime_projection.source == RuntimeObservationSource::Unknown
                || task.runtime_projection.health == RuntimeHealth::Unobservable
                || task
                    .runtime_projection
                    .requires_refresh(now, RUNTIME_PROJECTION_FRESH_FOR));

        task.lifecycle_status != LifecycleStatus::Removed
            && (has_missing_git_substrate || has_stale_cached_git_status)
    })
}

fn should_probe_live_substrate(task: &Task) -> bool {
    matches!(
        task.lifecycle_status,
        LifecycleStatus::Provisioning
            | LifecycleStatus::Active
            | LifecycleStatus::Waiting
            | LifecycleStatus::Reviewable
            | LifecycleStatus::Removing
            | LifecycleStatus::TeardownIncomplete
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

fn record_runtime_probe_failure<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    reason: String,
    changed: &mut bool,
) {
    if let Some(task) = context.registry.get_task_mut(task_id) {
        let previous = task.runtime_projection.clone();
        task.record_runtime_probe_failure(RuntimeObservationSource::TmuxProbe, reason);
        refresh_cached_annotations(task);
        *changed |= task.runtime_projection != previous;
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
        refresh_runtime_context, refresh_runtime_context_with_agent_status_cache,
        refresh_runtime_context_with_agent_status_cache_and_tier,
        refresh_runtime_context_with_tier, AgentStatusCache, AgentStatusCacheEntry,
        AgentStatusCacheSource, NoAgentStatusCache, RefreshTier,
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
        fn status_entries_for_session(&self, _session: &str) -> Vec<AgentStatusCacheEntry> {
            self.values
                .iter()
                .cloned()
                .map(|value| AgentStatusCacheEntry {
                    value,
                    observed_at: SystemTime::now(),
                    fresh: true,
                    source: AgentStatusCacheSource::Hook,
                })
                .collect()
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
        get_task_calls: Cell<u32>,
        worktrunk_status_updates: Cell<u32>,
    }

    impl CountingRegistry {
        fn from_registry(inner: InMemoryRegistry) -> Self {
            Self {
                inner,
                list_tasks_calls: Cell::new(0),
                get_task_calls: Cell::new(0),
                worktrunk_status_updates: Cell::new(0),
            }
        }

        fn list_tasks_calls(&self) -> u32 {
            self.list_tasks_calls.get()
        }

        fn get_task_calls(&self) -> u32 {
            self.get_task_calls.get()
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
            self.get_task_calls.set(self.get_task_calls.get() + 1);
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

    fn context_with_teardown_incomplete_task() -> CommandContext<CountingRegistry> {
        let mut context = context_with_task_for_missing_session();
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::TeardownIncomplete;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "drop incomplete at delete branch",
        ));
        task.metadata
            .insert("drop_failed_step".to_string(), "delete branch".to_string());
        task.metadata.insert(
            "drop_failed_detail".to_string(),
            "branch still present".to_string(),
        );
        context
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
        sessions_output: Option<String>,
    }

    impl CommandRunner for OrphanRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => self
                    .sessions_output
                    .as_deref()
                    .unwrap_or("ajax-web-fix-login\n"),
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
    fn wrapper_completion_beats_stale_hook_working_status() {
        struct MixedAgentStatusCache;

        impl AgentStatusCache for MixedAgentStatusCache {
            fn status_entries_for_session(&self, _session: &str) -> Vec<AgentStatusCacheEntry> {
                Vec::new()
            }

            fn status_entries_for_task(
                &self,
                _task_id: &TaskId,
                _session: &str,
            ) -> Vec<AgentStatusCacheEntry> {
                vec![
                    AgentStatusCacheEntry {
                        value: "working".to_string(),
                        observed_at: SystemTime::UNIX_EPOCH,
                        fresh: false,
                        source: AgentStatusCacheSource::Hook,
                    },
                    AgentStatusCacheEntry {
                        value: "done".to_string(),
                        observed_at: SystemTime::now(),
                        fresh: true,
                        source: AgentStatusCacheSource::RuntimeWrapper,
                    },
                ]
            }
        }

        let mut context = context_with_task_for_missing_session();
        let mut runner = HealthyRefreshRunner::default();

        let changed = refresh_runtime_context_with_agent_status_cache(
            &mut context,
            &mut runner,
            &MixedAgentStatusCache,
        )
        .unwrap();

        assert!(changed);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    }

    struct ScriptedAgentStatusCache {
        entries: Vec<AgentStatusCacheEntry>,
    }

    impl AgentStatusCache for ScriptedAgentStatusCache {
        fn status_entries_for_session(&self, _session: &str) -> Vec<AgentStatusCacheEntry> {
            Vec::new()
        }

        fn status_entries_for_task(
            &self,
            _task_id: &TaskId,
            _session: &str,
        ) -> Vec<AgentStatusCacheEntry> {
            self.entries.clone()
        }
    }

    fn scripted_entry(
        source: AgentStatusCacheSource,
        value: &str,
        ago: Duration,
    ) -> AgentStatusCacheEntry {
        AgentStatusCacheEntry {
            value: value.to_string(),
            observed_at: SystemTime::now() - ago,
            fresh: true,
            source,
        }
    }

    fn captured_pane(runner: &HealthyRefreshRunner) -> usize {
        runner
            .commands
            .iter()
            .filter(|command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"))
            .count()
    }

    #[test]
    fn missing_session_skips_wrapper_hook_and_pane_status_application() {
        let mut context = context_with_task_for_missing_session();
        let mut runner = MissingSessionRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![
                scripted_entry(
                    AgentStatusCacheSource::RuntimeWrapper,
                    "done",
                    Duration::from_secs(1),
                ),
                scripted_entry(
                    AgentStatusCacheSource::Hook,
                    "working",
                    Duration::from_secs(1),
                ),
            ],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
        assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }

    #[test]
    fn old_wrapper_completion_beats_new_hook_during_refresh() {
        let mut context = context_with_active_task();
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![
                scripted_entry(
                    AgentStatusCacheSource::RuntimeWrapper,
                    "done",
                    Duration::from_secs(3_600),
                ),
                scripted_entry(
                    AgentStatusCacheSource::Hook,
                    "working",
                    Duration::from_secs(1),
                ),
            ],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    }

    #[test]
    fn stale_codex_working_falls_through_to_agent_aware_pane_capture() {
        let mut context = context_with_active_task();
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "working",
                Duration::from_secs(21),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 1);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn fresh_codex_wait_skips_pane_capture() {
        let mut context = context_with_active_task();
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "wait",
                Duration::from_secs(119),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 0);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn stale_codex_wait_uses_pane_fallback() {
        let mut context = context_with_active_task();
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "wait",
                Duration::from_secs(121),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 1);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn unsupported_agent_hook_is_ignored_and_pane_is_captured() {
        let mut context = context_with_active_task();
        context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap()
            .selected_agent = AgentClient::Other;
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "working",
                Duration::from_secs(1),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 1);
    }

    #[test]
    fn malformed_newest_hook_does_not_hide_older_valid_wait() {
        let mut context = context_with_active_task();
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![
                scripted_entry(
                    AgentStatusCacheSource::Hook,
                    "garbage",
                    Duration::from_secs(1),
                ),
                scripted_entry(
                    AgentStatusCacheSource::Hook,
                    "wait",
                    Duration::from_secs(10),
                ),
            ],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 0);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn pane_capture_failure_preserves_prior_credible_live_status() {
        struct FailingPaneRunner;
        impl CommandRunner for FailingPaneRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                if command.args.first().map(String::as_str) == Some("capture-pane") {
                    return Err(CommandRunError::SpawnFailed("pane unavailable".to_string()));
                }
                Ok(CommandOutput {
                    status_code: 0,
                    stdout: runtime_stdout(&command.args).to_string(),
                    stderr: String::new(),
                })
            }
        }

        for (prior_kind, agent_status) in [
            (LiveStatusKind::Done, AgentRuntimeStatus::Done),
            (LiveStatusKind::WaitingForInput, AgentRuntimeStatus::Waiting),
        ] {
            let mut context = context_with_task_for_missing_session();
            {
                let task = context
                    .registry
                    .get_task_mut(&TaskId::new(TASK_ID))
                    .unwrap();
                task.live_status = Some(LiveObservation::new(prior_kind, "prior"));
                task.agent_status = agent_status;
            }
            let mut runner = FailingPaneRunner;

            refresh_runtime_context(&mut context, &mut runner).unwrap();

            let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
            assert_eq!(
                task.live_status.as_ref().map(|status| status.kind),
                Some(prior_kind),
                "{prior_kind:?}"
            );
            assert_eq!(
                task.runtime_projection.observation_error.as_deref(),
                Some("tmux capture-pane probe failed: failed to start command: pane unavailable"),
                "{prior_kind:?}"
            );
        }
    }

    #[test]
    fn acknowledged_old_claude_wait_stays_idle_without_pane_capture() {
        let mut context = context_with_active_task();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new(TASK_ID))
                .unwrap();
            task.selected_agent = AgentClient::Claude;
            task.agent_status = AgentRuntimeStatus::NotStarted;
            task.live_status = None;
            task.attention_acknowledged_at = Some(SystemTime::now() - Duration::from_secs(5));
        }
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "wait",
                Duration::from_secs(10),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 0);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.live_status, None);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn new_claude_wait_after_acknowledgment_restores_attention() {
        let acknowledged_at = SystemTime::now() - Duration::from_secs(10);
        let mut context = context_with_active_task();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new(TASK_ID))
                .unwrap();
            task.selected_agent = AgentClient::Claude;
            task.agent_status = AgentRuntimeStatus::NotStarted;
            task.live_status = None;
            task.attention_acknowledged_at = Some(acknowledged_at);
        }
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![AgentStatusCacheEntry {
                value: "wait".to_string(),
                observed_at: acknowledged_at + Duration::from_nanos(1),
                fresh: true,
                source: AgentStatusCacheSource::Hook,
            }],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(task
            .annotations
            .iter()
            .any(|annotation| annotation.kind == crate::models::AnnotationKind::NeedsMe));
    }

    #[test]
    fn acknowledged_old_codex_wait_stays_idle_without_pane_capture() {
        let mut context = context_with_active_task();
        context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap()
            .attention_acknowledged_at = Some(SystemTime::now() - Duration::from_secs(1));
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![scripted_entry(
                AgentStatusCacheSource::Hook,
                "wait",
                Duration::from_secs(10),
            )],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        assert_eq!(captured_pane(&runner), 0);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.live_status, None);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn newer_same_kind_waiting_evidence_is_applied_after_acknowledgment() {
        let acknowledged_at = SystemTime::now() - Duration::from_secs(10);
        let previous_observed_at = acknowledged_at - Duration::from_secs(1);
        let next_observed_at = acknowledged_at + Duration::from_nanos(1);
        let mut context = context_with_active_task();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new(TASK_ID))
                .unwrap();
            task.live_status = Some(LiveObservation::new(
                LiveStatusKind::WaitingForInput,
                "old wait",
            ));
            task.live_status_observed_at = Some(previous_observed_at);
            task.attention_acknowledged_at = Some(acknowledged_at);
        }
        let mut runner = HealthyRefreshRunner::default();
        let cache = ScriptedAgentStatusCache {
            entries: vec![AgentStatusCacheEntry {
                value: "wait".to_string(),
                observed_at: next_observed_at,
                fresh: true,
                source: AgentStatusCacheSource::Hook,
            }],
        };

        refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.live_status_observed_at, Some(next_observed_at));
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn status_decision_preserves_steady_state_command_budget() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = GitSkippingRunner::default();
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };

        refresh_runtime_context_with_agent_status_cache_and_tier(
            &mut context,
            &mut runner,
            &cache,
            RefreshTier::Live,
        )
        .unwrap();

        let git_worktree_lists = runner
            .commands
            .iter()
            .filter(|command| git_worktree_list(&command.args))
            .count();
        let capture_panes = runner
            .commands
            .iter()
            .filter(|command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"))
            .count();
        let tmux_commands = runner
            .commands
            .iter()
            .filter(|command| {
                matches!(
                    command.args.first().map(String::as_str),
                    Some("list-sessions" | "list-windows")
                )
            })
            .count();

        assert_eq!(git_worktree_lists, 0);
        assert_eq!(capture_panes, 0);
        assert!(tmux_commands <= 2, "got {tmux_commands}");
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
    fn missing_session_refresh_preserves_teardown_incomplete_failure_status() {
        let mut context = context_with_teardown_incomplete_task();
        let mut runner = MissingSessionRunner::default();

        refresh_runtime_context(&mut context, &mut runner).unwrap();

        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| status.summary.as_str()),
            Some("drop incomplete at delete branch")
        );
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

    #[test]
    fn steady_state_refresh_recovers_orphan_worktrees() {
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

        let changed = super::refresh_runtime_context_with_tier(
            &mut context,
            &mut runner,
            &NoAgentStatusCache,
            super::RefreshTier::Full,
        )
        .unwrap();

        assert!(changed);
        assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
        assert!(context.registry.get_task(&TaskId::new("web/b")).is_some());
        assert!(context.registry.get_task(&TaskId::new("web/c")).is_some());
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
    fn steady_state_fresh_projections_skip_orphan_git_scan_on_live_tier() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = OrphanRecoveryRunner::default();

        refresh_runtime_context_with_tier(
            &mut context,
            &mut runner,
            &NoAgentStatusCache,
            RefreshTier::Live,
        )
        .unwrap();

        assert!(
            !runner.commands.iter().any(|command| {
                command.args.len() >= 5
                    && command.args[2] == "worktree"
                    && command.args[3] == "list"
            }),
            "live tier with fresh projections should not list worktrees: {:?}",
            runner.commands
        );
    }

    #[test]
    fn steady_state_recovers_orphan_when_tmux_lists_unregistered_ajax_session() {
        let base = context_with_unchanged_running_task();
        let mut context = CommandContext::new(base.config, base.registry);
        let mut runner = OrphanRecoveryRunner {
            sessions_output: Some("ajax-web-fix-login\najax-web-a\n".to_string()),
            ..Default::default()
        };

        let changed = refresh_runtime_context_with_tier(
            &mut context,
            &mut runner,
            &NoAgentStatusCache,
            RefreshTier::Live,
        )
        .unwrap();

        assert!(changed);
        assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
    }

    #[test]
    fn steady_state_refresh_reuses_initial_task_snapshot() {
        let base = context_with_unchanged_running_task();
        let mut context =
            CommandContext::new(base.config, CountingRegistry::from_registry(base.registry));
        let mut runner = GitSkippingRunner::default();

        let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert_eq!(
            context.registry.get_task_calls(),
            0,
            "refresh should reuse the initial list_tasks snapshot, got {} get_task calls",
            context.registry.get_task_calls()
        );
    }

    #[test]
    fn steady_state_refresh_skips_capture_pane_when_agent_cache_is_stable() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = GitSkippingRunner::default();
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };

        let _changed =
            refresh_runtime_context_with_agent_status_cache(&mut context, &mut runner, &cache)
                .unwrap();

        assert!(
            !runner.commands.iter().any(|command| {
                matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
            }),
            "stable agent cache with unchanged live status should skip capture-pane"
        );
    }

    #[test]
    fn steady_state_refresh_skips_global_list_windows_when_no_probed_session_exists() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.tmux_session = "ajax-web-missing-session".to_string();
        let mut runner = MissingSessionRunner::default();

        let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(
            !runner.commands.iter().any(|command| {
                matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
            }),
            "missing probed session should not list all windows: {:?}",
            runner.commands
        );
    }

    #[test]
    fn steady_state_refresh_operation_budget() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = GitSkippingRunner::default();
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string()],
        };

        let _changed = refresh_runtime_context_with_agent_status_cache_and_tier(
            &mut context,
            &mut runner,
            &cache,
            RefreshTier::Live,
        )
        .unwrap();

        let git_worktree_lists = runner
            .commands
            .iter()
            .filter(|command| git_worktree_list(&command.args))
            .count();
        let capture_panes = runner
            .commands
            .iter()
            .filter(|command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane"))
            .count();
        let tmux_commands = runner
            .commands
            .iter()
            .filter(|command| {
                matches!(
                    command.args.first().map(String::as_str),
                    Some("list-sessions" | "list-windows")
                )
            })
            .count();

        assert_eq!(git_worktree_lists, 0);
        assert_eq!(capture_panes, 0);
        assert!(
            tmux_commands <= 2,
            "expected at most list-sessions + list-windows, got {tmux_commands}"
        );
    }

    #[test]
    fn steady_state_refresh_skips_branch_refresh_when_git_state_is_fresh() {
        let mut context = context_with_unchanged_running_task();
        let mut runner = GitSkippingRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert!(
            !runner
                .commands
                .iter()
                .any(|command| git_branch_list(&command.args)),
            "fresh runtime refresh should not list repo branches: {:?}",
            runner.commands
        );
    }

    #[test]
    fn missing_git_status_with_missing_flags_still_refreshes_git_substrate() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.git_status = None;
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        let mut runner = HealthyRefreshRunner::default();

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_eq!(task.git_status.as_ref(), Some(&clean_git_status()));
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));
        assert!(runner
            .commands
            .iter()
            .any(|command| git_worktree_list(&command.args)));
        assert!(runner
            .commands
            .iter()
            .any(|command| git_branch_list(&command.args)));
    }

    #[test]
    fn tmux_probe_failure_preserves_session_and_records_probe_error() {
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
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
        assert!(!task.has_side_flag(SideFlag::TmuxMissing));
        assert_eq!(
            task.runtime_projection.observation_error.as_deref(),
            Some("tmux list-sessions probe failed: failed to start command: tmux unavailable")
        );
    }

    #[test]
    fn window_probe_failure_preserves_task_window_evidence() {
        struct FailingWindowRunner;

        impl CommandRunner for FailingWindowRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                match command.args.first().map(String::as_str) {
                    Some("list-sessions") => Ok(CommandOutput {
                        status_code: 0,
                        stdout: format!("{TASK_SESSION}\n"),
                        stderr: String::new(),
                    }),
                    Some("list-windows") => Err(CommandRunError::SpawnFailed(
                        "tmux windows unavailable".to_string(),
                    )),
                    _ => Ok(CommandOutput {
                        status_code: 0,
                        stdout: runtime_stdout(&command.args).to_string(),
                        stderr: String::new(),
                    }),
                }
            }
        }

        let mut context = context_with_task_for_missing_session();
        let mut runner = FailingWindowRunner;

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert!(task
            .worktrunk_status
            .as_ref()
            .is_some_and(|status| status.exists && status.points_at_expected_path));
        assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));
        assert_eq!(
            task.runtime_projection.observation_error.as_deref(),
            Some(
                "tmux list-windows probe failed: failed to start command: tmux windows unavailable"
            )
        );
    }

    #[test]
    fn pane_probe_failure_does_not_report_agent_command_failure() {
        struct FailingPaneRunner;

        impl CommandRunner for FailingPaneRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                if command.args.first().map(String::as_str) == Some("capture-pane") {
                    return Err(CommandRunError::SpawnFailed("pane unavailable".to_string()));
                }

                Ok(CommandOutput {
                    status_code: 0,
                    stdout: runtime_stdout(&command.args).to_string(),
                    stderr: String::new(),
                })
            }
        }

        let mut context = context_with_task_for_missing_session();
        let mut runner = FailingPaneRunner;

        let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
        assert_ne!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
        assert_eq!(
            task.runtime_projection.observation_error.as_deref(),
            Some("tmux capture-pane probe failed: failed to start command: pane unavailable")
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

    fn context_with_many_active_tasks(count: usize) -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![
                ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH),
                ManagedRepo::new("api", "/Users/matt/projects/api", BASE_BRANCH),
            ],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        for index in 0..count {
            let repo = if index % 2 == 0 { REPO_NAME } else { "api" };
            let handle = format!("task-{index}");
            let branch = format!("ajax/{handle}");
            let session = format!("ajax-{repo}-{handle}");
            let worktree = format!("/tmp/worktrees/{repo}-{handle}");
            let mut task = Task::new(
                TaskId::new(format!("{repo}/{handle}")),
                repo,
                &handle,
                format!("Task {index}"),
                &branch,
                BASE_BRANCH,
                &worktree,
                &session,
                TASK_WINDOW,
                AgentClient::Codex,
            );
            task.lifecycle_status = LifecycleStatus::Active;
            task.git_status = Some(clean_git_status());
            task.tmux_status = Some(TmuxStatus::present(&session));
            task.worktrunk_status = Some(WorktrunkStatus::present(TASK_WINDOW, &worktree));
            registry.create_task(task).unwrap();
        }
        CommandContext::new(config, registry)
    }

    #[test]
    fn live_refresh_many_active_tasks_use_bounded_tmux_commands() {
        let mut context = context_with_many_active_tasks(24);
        let mut runner = GitSkippingRunner::default();
        let cache = StaticAgentStatusCache {
            values: vec!["working".to_string(); 24],
        };

        refresh_runtime_context_with_agent_status_cache_and_tier(
            &mut context,
            &mut runner,
            &cache,
            RefreshTier::Live,
        )
        .unwrap();

        let list_sessions = runner
            .commands
            .iter()
            .filter(|command| command.args.first().map(String::as_str) == Some("list-sessions"))
            .count();
        let list_all_windows = runner
            .commands
            .iter()
            .filter(|command| {
                command.args.first().map(String::as_str) == Some("list-windows")
                    && command.args.contains(&"-a".to_string())
            })
            .count();

        assert_eq!(list_sessions, 1);
        assert!(list_all_windows <= 1);
    }

    #[test]
    fn hyphenated_repo_registered_session_does_not_trigger_orphan_recovery() {
        let config = Config {
            repos: vec![ManagedRepo::new("api-v2", "/repo/api-v2", BASE_BRANCH)],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new("api-v2/fix-login"),
            "api-v2",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            BASE_BRANCH,
            "/repo/api-v2__worktrees/ajax-fix-login",
            "ajax-api-v2-fix-login",
            TASK_WINDOW,
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(clean_git_status());
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::now(),
            RuntimeObservationSource::TmuxProbe,
        );
        registry.create_task(task).unwrap();
        let mut context = CommandContext::new(config, registry);
        let mut runner = OrphanRecoveryRunner {
            sessions_output: Some("ajax-api-v2-fix-login\n".to_string()),
            ..Default::default()
        };

        refresh_runtime_context_with_tier(
            &mut context,
            &mut runner,
            &NoAgentStatusCache,
            RefreshTier::Live,
        )
        .unwrap();

        assert_eq!(context.registry.list_tasks().len(), 1);
        assert!(
            !runner
                .commands
                .iter()
                .any(|command| git_worktree_list(&command.args)),
            "registered hyphenated sessions must not trigger orphan git discovery"
        );
    }

    #[test]
    fn exact_registered_session_names_gate_orphan_recovery() {
        let base = context_with_unchanged_running_task();
        let mut context = CommandContext::new(base.config, base.registry);
        let mut runner = OrphanRecoveryRunner {
            sessions_output: Some("ajax-web-fix-login\najax-web-a\n".to_string()),
            ..Default::default()
        };

        let changed = refresh_runtime_context_with_tier(
            &mut context,
            &mut runner,
            &NoAgentStatusCache,
            RefreshTier::Live,
        )
        .unwrap();

        assert!(changed);
        assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new(TASK_ID))
                .expect("registered session should remain")
                .tmux_session,
            TASK_SESSION
        );
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
