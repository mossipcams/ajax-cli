use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    adapters::{CommandRunner, GitAdapter, TmuxAdapter},
    agent_status::{ProcessLiveness, StatusObservation},
    commands::{self, CommandContext, CommandError},
    config::WorktreePlacement,
    live::{self, LiveObservation, LiveStatusKind},
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveStatusClass,
        RuntimeHealth, RuntimeObservationSource, Task, TaskId, TaskWindowStatus,
    },
    registry::{Registry, RegistryError},
    runtime::RUNTIME_PROJECTION_FRESH_FOR,
};

pub(crate) mod ci_monitor;
mod github_checks;

#[cfg(test)]
use github_checks::{
    apply_github_checks_observation, clear_github_ci_evidence, github_probe_is_retired,
    CI_PROBE_ERROR_KEY,
};

/// Run identity of the primary (session-level) agent run.
pub const PRIMARY_RUN_ID: &str = "primary";

/// Source of native hook-derived agent-status evidence for a task.
///
/// Implementors (in `ajax-cli`) own filesystem I/O: they fold the canonical
/// JSONL event log per run into reducer observations and translate the launch
/// wrapper's confirmed exit / liveness into a terminal observation. Core never
/// reads files and never parses status strings; it reduces the observations
/// this trait yields.
pub trait AgentStatusSource {
    /// Reducer-ready observations for the task, one or more per active run.
    fn observations_for_task(&self, task_id: &TaskId) -> Vec<StatusObservation>;

    /// Confirmed launch-wrapper process liveness, if observed. Never alone
    /// implies the agent is running.
    fn process_liveness_for_task(&self, _task_id: &TaskId) -> Option<ProcessLiveness> {
        None
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
pub struct NoAgentStatusSource;

impl AgentStatusSource for NoAgentStatusSource {
    fn observations_for_task(&self, _task_id: &TaskId) -> Vec<StatusObservation> {
        Vec::new()
    }
}

pub fn refresh_runtime_context<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CommandError> {
    refresh_runtime_context_with_tier(context, runner, &NoAgentStatusSource, RefreshTier::Full)
}

pub fn refresh_runtime_context_with_tier<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    agent_status_source: &impl AgentStatusSource,
    tier: RefreshTier,
) -> Result<bool, CommandError> {
    let mut tasks: Vec<Task> = context.registry.list_tasks().into_iter().cloned().collect();
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
    let mut task_snapshots: Vec<Task> = probe_task_ids
        .iter()
        .filter_map(|task_id| task_lookup.get(task_id).cloned())
        .collect();
    let should_discover_orphans = task_snapshots.iter().any(should_probe_live_substrate);
    let has_unregistered_ajax_sessions =
        unregistered_ajax_sessions_in_tmux(&sessions_output, &registered_sessions);
    let should_scan_orphans =
        should_scan_for_orphan_worktrees(&task_snapshots) || has_unregistered_ajax_sessions;
    let should_run_orphan_discovery =
        should_discover_orphans && (tier == RefreshTier::Full || should_scan_orphans);
    let matching_panes_output = if has_unregistered_ajax_sessions {
        runner
            .run(&tmux.list_all_panes())
            .ok()
            .filter(|output| output.status_code == 0)
            .map(|output| output.stdout)
    } else {
        None
    };
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
    if let Some(output) = matching_panes_output.as_ref() {
        for task in &mut tasks {
            if TmuxAdapter::parse_session_status(&task.tmux_session, &sessions_output).exists {
                continue;
            }
            let expected_path = task.worktree_path.to_string_lossy();
            let Some(session) = output.lines().find_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let session = fields.next()?;
                let _window = fields.next()?;
                let path = fields.next()?;
                (path == expected_path).then(|| session.to_string())
            }) else {
                continue;
            };
            task.tmux_session = session.clone();
            if let Some(snapshot) = task_snapshots
                .iter_mut()
                .find(|snapshot| snapshot.id == task.id)
            {
                snapshot.tmux_session = session.clone();
            }
            if let Some(stored) = context.registry.get_task_mut(&task.id) {
                stored.tmux_session = session;
            }
            changed = true;
        }
    }

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
                    .task_window_status
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
            let missing_task = TaskWindowStatus::missing(
                task_snapshot.task_window.clone(),
                task_snapshot.worktree_path.clone(),
            );
            changed |= task_snapshot.task_window_status.as_ref() != Some(&missing_task);
            context
                .registry
                .update_task_window_status(&task_id, Some(missing_task))
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
        let mut task_window_status = TmuxAdapter::parse_task_window_status_for_session(
            &task_snapshot.tmux_session,
            &task_snapshot.task_window,
            &task_snapshot.worktree_path.display().to_string(),
            windows_output,
        );
        if !task_window_status.exists && all_windows_output_empty {
            let windows_command = tmux.list_windows(&task_snapshot.tmux_session);
            if let Ok(output) = runner.run(&windows_command) {
                if output.status_code == 0 {
                    task_window_status = TmuxAdapter::parse_task_window_status(
                        &task_snapshot.task_window,
                        &task_snapshot.worktree_path.display().to_string(),
                        &output.stdout,
                    );
                }
            }
        }
        changed |= task_snapshot.task_window_status.as_ref() != Some(&task_window_status);

        let task_window_status_changed =
            task_snapshot.task_window_status.as_ref() != Some(&task_window_status);
        let had_stale_task_window_missing =
            task_snapshot.has_side_flag(crate::models::SideFlag::TaskWindowMissing);
        changed |= task_window_status_changed || had_stale_task_window_missing;

        if task_window_status_changed || had_stale_task_window_missing {
            context
                .registry
                .update_task_window_status(&task_id, Some(task_window_status.clone()))
                .map_err(CommandError::Registry)?;
        }
        refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);

        if !task_window_status.exists {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::TaskWindowMissing, "task window missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        }

        // Native hook-derived agent status: fold canonical events per run into
        // reducer observations (plus confirmed wrapper exit / liveness), then
        // reduce to one live observation. There is no string round-trip and no
        // pane-text inference.
        let now = SystemTime::now();
        let observations = agent_status_source.observations_for_task(&task_snapshot.id);
        let process_liveness = agent_status_source.process_liveness_for_task(&task_snapshot.id);
        if observations.is_empty()
            && process_liveness.is_none()
            && !crate::ui_state::agent_process_is_alive(&task_snapshot)
        {
            // Zero agent evidence from every source. A leftover running claim
            // can only be stale here: interactive provisioning sets
            // `AgentRunning` on send-keys, but the sources that retract it are
            // all silent, so skipping would leave the operator surfaces
            // reporting "Agent working" forever.
            clear_stale_agent_running(context, &task_id, &task_snapshot, &mut changed);
            continue;
        }
        let projection =
            crate::agent_status::reduce_agent_status(crate::agent_status::ReduceInput {
                now,
                primary_run_id: PRIMARY_RUN_ID.to_string(),
                process_liveness,
                observations: &observations,
            });

        // Precedence tier 3: a fresh wrapper heartbeat proves the process
        // exists without asserting activity. Stamping it lets the projector
        // report a live-but-quiet task as Idle rather than Unknown; it never
        // becomes `AgentRunning`. Recorded before the Unknown bail below,
        // because liveness is exactly the evidence that survives when no
        // native event has arrived yet.
        if let Some(task) = context.registry.get_task_mut(&task_id) {
            let previous = task.clone();
            if projection.process_alive {
                if !task
                    .metadata
                    .contains_key(crate::ui_state::AGENT_PROCESS_ALIVE_KEY)
                {
                    task.metadata.insert(
                        crate::ui_state::AGENT_PROCESS_ALIVE_KEY.to_string(),
                        "1".to_string(),
                    );
                }
            } else {
                task.metadata
                    .remove(crate::ui_state::AGENT_PROCESS_ALIVE_KEY);
            }
            changed |= *task != previous;
        }

        let agent = task_snapshot.selected_agent;
        let reconcile_running = projection.phase
            == crate::agent_status::ParentPhase::ActivelyWorking
            && matches!(
                agent,
                AgentClient::Claude | AgentClient::Codex | AgentClient::Cursor | AgentClient::Pi
            );
        // Actionable waits only — Done/"Response ready" is Waiting-class but must
        // not open the idle reconcile capture gate (Bugbot).
        let prior_actionable_or_running =
            task_snapshot.live_status.as_ref().is_some_and(|status| {
                matches!(
                    status.kind,
                    LiveStatusKind::AgentRunning
                        | LiveStatusKind::WaitingForApproval
                        | LiveStatusKind::WaitingForInput
                )
            });
        let reconcile_idle = projection.phase == crate::agent_status::ParentPhase::FullyCompleted
            && matches!(
                agent,
                AgentClient::Claude | AgentClient::Codex | AgentClient::Cursor
            )
            && prior_actionable_or_running;
        let unknown_fallback = projection.phase == crate::agent_status::ParentPhase::Unknown
            && crate::pane_fallback::profile_allows_any_pane_wait_fallback(agent);

        if reconcile_running || reconcile_idle || unknown_fallback {
            let capture_command =
                tmux.capture_pane(&task_snapshot.tmux_session, &task_snapshot.task_window);
            if let Ok(output) = runner.run(&capture_command) {
                if output.status_code == 0 {
                    let pane_observation = if unknown_fallback {
                        crate::pane_fallback::maybe_pane_wait(agent, &output.stdout)
                    } else {
                        crate::pane_fallback::reconcile_wait_from_pane(agent, &output.stdout)
                    };
                    if let Some(observation) = pane_observation {
                        let blocked_by_ack = observation.kind.class() == LiveStatusClass::Waiting
                            && task_snapshot
                                .attention_acknowledged_at
                                .is_some_and(|ack| now <= ack);
                        if !blocked_by_ack {
                            if let Some(task) = context.registry.get_task_mut(&task_id) {
                                let previous = task.clone();
                                live::apply_observation_at(task, observation, now);
                                refresh_cached_annotations(task);
                                changed |= *task != previous;
                            }
                        }
                        // Wait chrome is visible: never fall through to apply
                        // Working/Done from lifecycle in the same tick (Bugbot).
                        continue;
                    }
                }
            }
        }
        // Preserve prior live evidence when the reducer has nothing trustworthy.
        // Claude (native waits) never takes unknown_fallback, so without this
        // continue we would apply LiveStatusKind::Unknown and clear waiting
        // while leaving NeedsInput — re-arming attention after ack (Bugbot).
        if projection.phase == crate::agent_status::ParentPhase::Unknown {
            continue;
        }
        let observation = projection.live.clone();
        let observed_at = projection.selected_observed_at.unwrap_or(now);
        // Waiting/completion evidence at or before an acknowledgment is held:
        // opening a task suppresses it until newer evidence arrives.
        if observation.kind.class() == crate::models::LiveStatusClass::Waiting
            && task_snapshot
                .attention_acknowledged_at
                .is_some_and(|ack| observed_at <= ack)
        {
            continue;
        }
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
            task.remove_side_flag(crate::models::SideFlag::TaskWindowMissing);
            match projection.selected_source {
                // Confirmed wrapper exit is trusted process evidence and may
                // advance lifecycle on terminal completion.
                Some(crate::agent_status::ObservationSource::ProcessExit) => {
                    live::apply_trusted_observation_at(task, observation, observed_at);
                }
                // Folded native lifecycle events are authoritative for activity.
                Some(crate::agent_status::ObservationSource::ProviderLifecycle) => {
                    live::apply_authoritative_observation_at(task, observation, observed_at);
                }
                _ => {
                    live::apply_observation_at(task, observation, observed_at);
                }
            }
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
            windows_output
                .as_ref()
                .and_then(|output| output.as_ref().ok())
                .map(String::as_str)
                .or(matching_panes_output.as_deref()),
            &mut registered_task_handles,
            &mut registered_runtime_tasks,
        )?;
    }

    if tier == RefreshTier::Full {
        ci_monitor::refresh_ci_monitor(
            context,
            runner,
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            &tasks,
            &mut changed,
        );
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
        || task.has_side_flag(crate::models::SideFlag::TaskWindowMissing)
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
    windows_output: Option<&str>,
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
            let existing_session_for_worktree = registered_runtime_tasks
                .iter()
                .find(
                    |(_, existing_repo, existing_branch, existing_worktree_path)| {
                        existing_repo == &repo.name
                            && existing_worktree_path.to_string_lossy() == worktree.path
                            && existing_branch != branch
                    },
                )
                .and_then(|(existing_task_id, _, _, _)| context.registry.get_task(existing_task_id))
                .map(|task| task.tmux_session.clone())
                .filter(|session| {
                    TmuxAdapter::parse_session_status(session, sessions_output).exists
                });
            let session_for_worktree_window = windows_output.and_then(|output| {
                output.lines().find_map(|line| {
                    let mut fields = line.splitn(3, '\t');
                    let session = fields.next()?;
                    let _window = fields.next()?;
                    let path = fields.next()?;
                    (path == worktree.path).then(|| session.to_string())
                })
            });
            let tmux_session = existing_session_for_worktree
                .or(session_for_worktree_window)
                .unwrap_or_else(|| format!("ajax-{}-{handle}", repo.name));
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
                "task",
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
                task.task_window_status = Some(TaskWindowStatus {
                    exists: false,
                    window_name: task.task_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                });
                task.add_side_flag(crate::models::SideFlag::TmuxMissing);
                task.add_side_flag(crate::models::SideFlag::TaskWindowMissing);
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

/// Retract a running claim that no evidence source backs any more.
///
/// Only the caller's zero-evidence branch may use this: it asserts absence, not
/// death. The task is left with no running claim rather than marked dead, so a
/// later observation is free to describe what actually happened.
///
/// A task that still carries a live status is left alone even here. That is a
/// newer observation than provisioning, and the live-status machinery owns
/// retracting it; clearing on top would flap a steady-state running task to
/// idle on every refresh where the hook source happens to be silent.
fn clear_stale_agent_running<R: Registry>(
    context: &mut CommandContext<R>,
    task_id: &TaskId,
    task_snapshot: &Task,
    changed: &mut bool,
) {
    if task_snapshot.live_status.is_some() {
        return;
    }
    if !task_snapshot.has_side_flag(crate::models::SideFlag::AgentRunning)
        && task_snapshot.agent_status != AgentRuntimeStatus::Running
    {
        return;
    }
    let Some(task) = context.registry.get_task_mut(task_id) else {
        return;
    };
    let previous = task.clone();
    task.remove_side_flag(crate::models::SideFlag::AgentRunning);
    if task.agent_status == AgentRuntimeStatus::Running {
        task.agent_status = AgentRuntimeStatus::Unknown;
    }
    refresh_cached_annotations(task);
    *changed |= *task != previous;
}
#[cfg(test)]
mod tests;
