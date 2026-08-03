use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::Row;

use super::super::RegistrySnapshotError;
use crate::lifecycle::hydrate_lifecycle_status;
use crate::models::{
    AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation, LiveStatusKind,
    RuntimeProjection, Task, TaskId, TaskWindowStatus, TmuxStatus,
};

use super::enums::*;

pub(crate) fn task_from_row(row: &Row<'_>) -> Result<Task, RegistrySnapshotError> {
    let task_id = TaskId::new(col::<String>(row, "task_id")?);
    let repo = col::<String>(row, "repo")?;
    let handle = col::<String>(row, "handle")?;
    let title = col::<String>(row, "title")?;
    let branch = col::<String>(row, "branch")?;
    let base_branch = col::<String>(row, "base_branch")?;
    let worktree_path = col::<String>(row, "worktree_path")?;
    let tmux_session = col::<String>(row, "tmux_session")?;
    let task_window = col::<String>(row, "task_window")?;
    let selected_agent = parse_agent_client(&col::<String>(row, "selected_agent")?)?;
    let persisted_lifecycle_status =
        parse_lifecycle_status(&col::<String>(row, "lifecycle_status")?)?;
    let mut agent_status = parse_agent_runtime_status(&col::<String>(row, "agent_status")?)?;
    let created_at = timestamp_from_row(row, "created_at")?;
    let last_activity_at = timestamp_from_row(row, "last_activity_at")?;
    let mut live_status = live_status_from_row(row)?;
    let mut live_status_observed_at =
        optional_timestamp_from_row(row, "live_status_observed_at", "live status observation")?;
    let git_status = git_status_from_row(row)?;
    let tmux_status = tmux_status_from_row(row)?;
    let task_window_status = task_window_status_from_row(row)?;
    let mut runtime_projection = runtime_projection_from_row(row)?;

    let lifecycle_status = if persisted_lifecycle_status == LifecycleStatus::Waiting {
        if !matches!(
            live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput)
        ) {
            live_status = Some(LiveObservation::new(
                LiveStatusKind::WaitingForInput,
                "waiting for input",
            ));
        }
        LifecycleStatus::Active
    } else {
        persisted_lifecycle_status
    };

    let legacy_agent_unknown = agent_status == AgentRuntimeStatus::Unknown;
    let legacy_live_unknown = matches!(
        live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::Unknown)
    );
    if legacy_agent_unknown {
        agent_status = AgentRuntimeStatus::NotStarted;
    }
    if legacy_live_unknown {
        live_status = None;
        live_status_observed_at = None;
    }
    if (legacy_agent_unknown || legacy_live_unknown)
        && runtime_projection.observation_error.is_none()
    {
        runtime_projection.observation_error = Some("agent status not observed".to_string());
    }

    let mut task = Task::new(
        task_id,
        repo,
        handle,
        title,
        branch,
        base_branch,
        worktree_path,
        tmux_session,
        task_window,
        selected_agent,
    );
    hydrate_lifecycle_status(&mut task, lifecycle_status);
    task.agent_status = agent_status;
    task.created_at = created_at;
    task.last_activity_at = last_activity_at;
    task.live_status = live_status;
    task.live_status_observed_at = live_status_observed_at;
    task.git_status = git_status;
    task.tmux_status = tmux_status;
    task.task_window_status = task_window_status;
    task.runtime_projection = runtime_projection;
    task.attention_acknowledged_at =
        optional_timestamp_from_row(row, "attention_acknowledged_at", "attention acknowledgment")?;

    Ok(task)
}

pub(crate) fn runtime_projection_from_row(
    row: &Row<'_>,
) -> Result<RuntimeProjection, RegistrySnapshotError> {
    let health = parse_runtime_health(&col::<String>(row, "runtime_health")?)?;
    let observed_at = timestamp_from_row(row, "runtime_observed_at")?;
    let source =
        parse_runtime_observation_source(&col::<String>(row, "runtime_observation_source")?)?;

    let observation_error = col::<Option<String>>(row, "runtime_observation_error")?;

    Ok(match observation_error {
        Some(error) => {
            RuntimeProjection::with_observation_error(health, observed_at, source, error)
        }
        None => RuntimeProjection::new(health, observed_at, source),
    })
}

pub(crate) fn live_status_from_row(
    row: &Row<'_>,
) -> Result<Option<LiveObservation>, RegistrySnapshotError> {
    let Some(kind) = col::<Option<String>>(row, "live_status_kind")? else {
        return Ok(None);
    };
    let summary = col::<Option<String>>(row, "live_status_summary")?
        .ok_or_else(|| RegistrySnapshotError::Decode("live status summary missing".to_string()))?;

    Ok(Some(LiveObservation::new(
        parse_live_status_kind(&kind)?,
        summary,
    )))
}

pub(crate) fn git_status_from_row(
    row: &Row<'_>,
) -> Result<Option<GitStatus>, RegistrySnapshotError> {
    let Some(worktree_exists) = col::<Option<bool>>(row, "git_worktree_exists")? else {
        return Ok(None);
    };

    Ok(Some(GitStatus {
        worktree_exists,
        branch_exists: req(row, "git_branch_exists", "git branch")?,
        current_branch: col(row, "git_current_branch")?,
        dirty: req(row, "git_dirty", "git dirty")?,
        ahead: req(row, "git_ahead", "git ahead")?,
        behind: req(row, "git_behind", "git behind")?,
        merged: req(row, "git_merged", "git merged")?,
        untracked_files: req(row, "git_untracked_files", "git untracked files")?,
        unpushed_commits: req(row, "git_unpushed_commits", "git unpushed commits")?,
        conflicted: req(row, "git_conflicted", "git conflicted")?,
        last_commit: col(row, "git_last_commit")?,
    }))
}

pub(crate) fn tmux_status_from_row(
    row: &Row<'_>,
) -> Result<Option<TmuxStatus>, RegistrySnapshotError> {
    let Some(exists) = col::<Option<bool>>(row, "tmux_exists")? else {
        return Ok(None);
    };
    Ok(Some(TmuxStatus {
        exists,
        session_name: req(row, "tmux_session_name", "tmux session")?,
    }))
}

pub(crate) fn task_window_status_from_row(
    row: &Row<'_>,
) -> Result<Option<TaskWindowStatus>, RegistrySnapshotError> {
    let Some(exists) = col::<Option<bool>>(row, "task_window_exists")? else {
        return Ok(None);
    };
    Ok(Some(TaskWindowStatus {
        exists,
        window_name: req(row, "task_window_name", "task window")?,
        current_path: PathBuf::from(req::<String>(row, "task_window_current_path", "task path")?),
        points_at_expected_path: req(row, "task_window_points_at_expected_path", "task path flag")?,
    }))
}

/// Reads a nullable column that must be present, mapping `NULL` to a decode error.
pub(crate) fn req<T: rusqlite::types::FromSql>(
    row: &Row<'_>,
    name: &str,
    label: &str,
) -> Result<T, RegistrySnapshotError> {
    col::<Option<T>>(row, name)?
        .ok_or_else(|| RegistrySnapshotError::Decode(format!("{label} missing")))
}

pub(crate) fn database_error(error: rusqlite::Error) -> RegistrySnapshotError {
    RegistrySnapshotError::Database(error.to_string())
}

pub(crate) fn col<T: rusqlite::types::FromSql>(
    row: &Row<'_>,
    name: &str,
) -> Result<T, RegistrySnapshotError> {
    row.get(name).map_err(database_error)
}

pub(crate) fn timestamp_from_row(
    row: &Row<'_>,
    prefix: &str,
) -> Result<SystemTime, RegistrySnapshotError> {
    unix_parts_to_system_time(
        col(row, &format!("{prefix}_unix_seconds"))?,
        col(row, &format!("{prefix}_subsec_nanos"))?,
    )
}

pub(crate) fn optional_timestamp_from_row(
    row: &Row<'_>,
    prefix: &str,
    label: &str,
) -> Result<Option<SystemTime>, RegistrySnapshotError> {
    let seconds: Option<i64> = col(row, &format!("{prefix}_unix_seconds"))?;
    let nanos: Option<u32> = col(row, &format!("{prefix}_subsec_nanos"))?;
    match (seconds, nanos) {
        (Some(seconds), Some(nanos)) => Ok(Some(unix_parts_to_system_time(seconds, nanos)?)),
        (None, None) => Ok(None),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "{label} timestamp is incomplete"
        ))),
    }
}

pub(crate) fn system_time_to_unix_parts(
    time: SystemTime,
) -> Result<(i64, u32), RegistrySnapshotError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))?;
    let seconds = duration.as_secs();
    i64::try_from(seconds)
        .map_err(|error| RegistrySnapshotError::Encode(format!("timestamp out of range: {error}")))
        .map(|seconds| (seconds, duration.subsec_nanos()))
}

pub(crate) fn unix_parts_to_system_time(
    seconds: i64,
    nanos: u32,
) -> Result<SystemTime, RegistrySnapshotError> {
    if nanos >= 1_000_000_000 {
        return Err(RegistrySnapshotError::Decode(format!(
            "timestamp nanoseconds out of range: {nanos}"
        )));
    }
    let seconds = u64::try_from(seconds).map_err(|error| {
        RegistrySnapshotError::Decode(format!("negative timestamp is unsupported: {error}"))
    })?;

    Ok(UNIX_EPOCH + Duration::new(seconds, nanos))
}
