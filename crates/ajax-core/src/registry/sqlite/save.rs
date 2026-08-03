use std::collections::BTreeSet;

use rusqlite::{params, Transaction};

use super::super::{InMemoryRegistry, RegistrySnapshotError};
use crate::ghost_task::is_registry_ghost_task;
use crate::models::{
    GitStatus, LiveObservation, RuntimeProjection, StepReceipt, Task, TaskWindowStatus, TmuxStatus,
};

use super::enums::*;
use super::row_codec::*;

pub(crate) fn save_registry(
    transaction: &Transaction<'_>,
    registry: &InMemoryRegistry,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute("DELETE FROM registry_events", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_agent_attempts", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_metadata", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_side_flags", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_workflow", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_live_status", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_runtime_projection", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_git_evidence", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_tmux_evidence", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_window_evidence", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_tasks", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM step_receipts", [])
        .map_err(database_error)?;

    let live_task_ids = registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();

    for task in registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
    {
        save_task(transaction, task)?;
    }

    for (sequence, event) in registry
        .events
        .iter()
        .filter(|event| live_task_ids.contains(&event.task_id))
        .enumerate()
    {
        let (occurred_at_seconds, occurred_at_nanos) =
            system_time_to_unix_parts(event.occurred_at)?;
        transaction
            .execute(
                "INSERT INTO registry_events \
                     (sequence, task_id, kind, message, occurred_at_unix_seconds, \
                      occurred_at_subsec_nanos) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    sequence as i64,
                    event.task_id.as_str(),
                    registry_event_kind_name(event.kind),
                    event.message,
                    occurred_at_seconds,
                    occurred_at_nanos,
                ],
            )
            .map_err(database_error)?;
    }

    for receipt in registry
        .step_receipts
        .values()
        .filter(|receipt| live_task_ids.contains(&receipt.task_id))
    {
        save_step_receipt(transaction, receipt)?;
    }

    Ok(())
}

pub(crate) fn save_task(
    transaction: &Transaction<'_>,
    task: &Task,
) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(task.created_at)?;
    let (last_activity_seconds, last_activity_nanos) =
        system_time_to_unix_parts(task.last_activity_at)?;
    let (runtime_observed_seconds, runtime_observed_nanos) =
        system_time_to_unix_parts(task.runtime_projection.observed_at)?;
    let attention_acknowledged_parts = task
        .attention_acknowledged_at
        .map(system_time_to_unix_parts)
        .transpose()?;
    let live_status_observed_parts = task
        .live_status_observed_at
        .map(system_time_to_unix_parts)
        .transpose()?;
    let git = task.git_status.as_ref();
    let tmux = task.tmux_status.as_ref();
    let task_window = task.task_window_status.as_ref();
    let live = task.live_status.as_ref();
    transaction
        .execute(
            "INSERT INTO registry_tasks \
             (task_id, repo, handle, title, branch, base_branch, worktree_path, tmux_session, \
              task_window, selected_agent) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(task_id) DO UPDATE SET \
                repo = excluded.repo, \
                handle = excluded.handle, \
                title = excluded.title, \
                branch = excluded.branch, \
                base_branch = excluded.base_branch, \
                worktree_path = excluded.worktree_path, \
                tmux_session = excluded.tmux_session, \
                task_window = excluded.task_window, \
                selected_agent = excluded.selected_agent",
            params![
                task.id.as_str(),
                task.repo,
                task.handle,
                task.title,
                task.branch,
                task.base_branch,
                task.worktree_path.to_string_lossy().as_ref(),
                task.tmux_session,
                task.task_window,
                agent_client_name(task.selected_agent),
            ],
        )
        .map_err(database_error)?;

    transaction
        .execute(
            "INSERT INTO registry_task_workflow \
             (task_id, lifecycle_status, agent_status, created_at_unix_seconds, \
              created_at_subsec_nanos, last_activity_at_unix_seconds, \
              last_activity_at_subsec_nanos, attention_acknowledged_at_unix_seconds, \
              attention_acknowledged_at_subsec_nanos) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(task_id) DO UPDATE SET \
                lifecycle_status = excluded.lifecycle_status, \
                agent_status = excluded.agent_status, \
                created_at_unix_seconds = excluded.created_at_unix_seconds, \
                created_at_subsec_nanos = excluded.created_at_subsec_nanos, \
                last_activity_at_unix_seconds = excluded.last_activity_at_unix_seconds, \
                last_activity_at_subsec_nanos = excluded.last_activity_at_subsec_nanos, \
                attention_acknowledged_at_unix_seconds = excluded.attention_acknowledged_at_unix_seconds, \
                attention_acknowledged_at_subsec_nanos = excluded.attention_acknowledged_at_subsec_nanos",
            params![
                task.id.as_str(),
                lifecycle_status_name(task.lifecycle_status),
                agent_runtime_status_name(task.agent_status),
                created_at_seconds,
                created_at_nanos,
                last_activity_seconds,
                last_activity_nanos,
                attention_acknowledged_parts.map(|(seconds, _)| seconds),
                attention_acknowledged_parts.map(|(_, nanos)| nanos),
            ],
        )
        .map_err(database_error)?;

    save_live_status(
        transaction,
        task.id.as_str(),
        live,
        live_status_observed_parts,
    )?;
    save_runtime_projection(
        transaction,
        task.id.as_str(),
        &task.runtime_projection,
        runtime_observed_seconds,
        runtime_observed_nanos,
    )?;
    save_git_status(transaction, task.id.as_str(), git)?;
    save_tmux_status(transaction, task.id.as_str(), tmux)?;
    save_task_window_status(transaction, task.id.as_str(), task_window)?;

    for flag in task.side_flags() {
        transaction
            .execute(
                "INSERT INTO registry_task_side_flags (task_id, flag) VALUES (?1, ?2)",
                params![task.id.as_str(), side_flag_name(flag)],
            )
            .map_err(database_error)?;
    }

    for (key, value) in &task.metadata {
        transaction
            .execute(
                "INSERT INTO registry_task_metadata (task_id, key, value) VALUES (?1, ?2, ?3)",
                params![task.id.as_str(), key, value],
            )
            .map_err(database_error)?;
    }

    for (sequence, attempt) in task.agent_attempts.iter().enumerate() {
        let (started_at_seconds, started_at_nanos) = system_time_to_unix_parts(attempt.started_at)?;
        let finished_at_parts = attempt
            .finished_at
            .map(system_time_to_unix_parts)
            .transpose()?;
        transaction
            .execute(
                "INSERT INTO registry_agent_attempts \
                 (task_id, sequence, agent, launch_target, started_at_unix_seconds, \
                  started_at_subsec_nanos, finished_at_unix_seconds, \
                  finished_at_subsec_nanos, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    task.id.as_str(),
                    sequence as i64,
                    agent_client_name(attempt.agent),
                    attempt.launch_target,
                    started_at_seconds,
                    started_at_nanos,
                    finished_at_parts.map(|(seconds, _)| seconds),
                    finished_at_parts.map(|(_, nanos)| nanos),
                    agent_runtime_status_name(attempt.status),
                ],
            )
            .map_err(database_error)?;
    }

    Ok(())
}

pub(crate) fn save_live_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    live: Option<&LiveObservation>,
    observed: Option<(i64, u32)>,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute(
            "DELETE FROM registry_task_live_status WHERE task_id = ?1",
            [task_id],
        )
        .map_err(database_error)?;
    let Some(live) = live else {
        return Ok(());
    };
    let Some((observed_seconds, observed_nanos)) = observed else {
        return Ok(());
    };

    transaction
        .execute(
            "INSERT INTO registry_task_live_status \
             (task_id, live_status_kind, live_status_summary, \
              live_status_observed_at_unix_seconds, live_status_observed_at_subsec_nanos) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                task_id,
                live_status_kind_name(live.kind),
                live.summary.as_str(),
                observed_seconds,
                observed_nanos,
            ],
        )
        .map_err(database_error)?;

    Ok(())
}

pub(crate) fn save_runtime_projection(
    transaction: &Transaction<'_>,
    task_id: &str,
    projection: &RuntimeProjection,
    observed_seconds: i64,
    observed_nanos: u32,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute(
            "DELETE FROM registry_task_runtime_projection WHERE task_id = ?1",
            [task_id],
        )
        .map_err(database_error)?;

    transaction
        .execute(
            "INSERT INTO registry_task_runtime_projection \
             (task_id, runtime_health, runtime_observed_at_unix_seconds, \
              runtime_observed_at_subsec_nanos, runtime_observation_source, \
              runtime_observation_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                task_id,
                projection.health.as_str(),
                observed_seconds,
                observed_nanos,
                projection.source.as_str(),
                projection.observation_error.as_deref(),
            ],
        )
        .map_err(database_error)?;

    Ok(())
}

pub(crate) fn save_git_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    git: Option<&GitStatus>,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute(
            "DELETE FROM registry_task_git_evidence WHERE task_id = ?1",
            [task_id],
        )
        .map_err(database_error)?;
    let Some(git) = git else {
        return Ok(());
    };
    transaction
        .execute(
            "INSERT INTO registry_task_git_evidence \
             (task_id, git_worktree_exists, git_branch_exists, git_current_branch, git_dirty, \
              git_ahead, git_behind, git_merged, git_untracked_files, git_unpushed_commits, \
              git_conflicted, git_last_commit) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                task_id,
                git.worktree_exists,
                git.branch_exists,
                git.current_branch.as_deref(),
                git.dirty,
                git.ahead,
                git.behind,
                git.merged,
                git.untracked_files,
                git.unpushed_commits,
                git.conflicted,
                git.last_commit.as_deref(),
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

pub(crate) fn save_tmux_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    tmux: Option<&TmuxStatus>,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute(
            "DELETE FROM registry_task_tmux_evidence WHERE task_id = ?1",
            [task_id],
        )
        .map_err(database_error)?;
    let Some(tmux) = tmux else {
        return Ok(());
    };
    transaction
        .execute(
            "INSERT INTO registry_task_tmux_evidence \
             (task_id, tmux_exists, tmux_session_name) VALUES (?1, ?2, ?3)",
            params![task_id, tmux.exists, tmux.session_name.as_str()],
        )
        .map_err(database_error)?;
    Ok(())
}

pub(crate) fn save_task_window_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    task: Option<&TaskWindowStatus>,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute(
            "DELETE FROM registry_task_window_evidence WHERE task_id = ?1",
            [task_id],
        )
        .map_err(database_error)?;
    let Some(task) = task else {
        return Ok(());
    };
    transaction
        .execute(
            "INSERT INTO registry_task_window_evidence \
             (task_id, task_window_exists, task_window_name, task_window_current_path, \
              task_window_points_at_expected_path) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                task_id,
                task.exists,
                task.window_name.as_str(),
                task.current_path.to_string_lossy().as_ref(),
                task.points_at_expected_path,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

pub(crate) fn save_step_receipt(
    transaction: &Transaction<'_>,
    receipt: &StepReceipt,
) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(receipt.created_at)?;
    transaction
        .execute(
            "INSERT INTO step_receipts \
             (task_id, operation, step_key, target, status, receipt_json, \
              created_at_unix_seconds, created_at_subsec_nanos) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(task_id, operation, step_key, target) DO UPDATE SET \
                status = excluded.status, \
                receipt_json = excluded.receipt_json, \
                created_at_unix_seconds = excluded.created_at_unix_seconds, \
                created_at_subsec_nanos = excluded.created_at_subsec_nanos",
            params![
                receipt.task_id.as_str(),
                receipt.operation.as_str(),
                receipt.step_key,
                receipt.target,
                receipt.status.as_str(),
                receipt.receipt_json,
                created_at_seconds,
                created_at_nanos,
            ],
        )
        .map_err(database_error)?;

    Ok(())
}
