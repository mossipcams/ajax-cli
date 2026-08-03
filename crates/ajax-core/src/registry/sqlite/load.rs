use std::collections::BTreeMap;

use rusqlite::Connection;

use super::super::{RegistryEvent, RegistrySnapshotError};
use crate::ghost_task::is_registry_ghost_task;
use crate::models::{AgentAttempt, StepReceipt, Task, TaskId};

use super::enums::*;
use super::row_codec::*;

pub(crate) fn load_tasks(connection: &Connection) -> Result<Vec<Task>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT t.task_id, t.repo, t.handle, t.title, t.branch, t.base_branch, \
             t.worktree_path, t.tmux_session, t.task_window, t.selected_agent, \
             w.lifecycle_status, w.agent_status, w.created_at_unix_seconds, \
             w.created_at_subsec_nanos, w.last_activity_at_unix_seconds, \
             w.last_activity_at_subsec_nanos, l.live_status_kind, l.live_status_summary, \
             l.live_status_observed_at_unix_seconds, l.live_status_observed_at_subsec_nanos, \
             g.git_worktree_exists, g.git_branch_exists, g.git_current_branch, g.git_dirty, \
             g.git_ahead, g.git_behind, g.git_merged, g.git_untracked_files, \
             g.git_unpushed_commits, g.git_conflicted, g.git_last_commit, tm.tmux_exists, \
             tm.tmux_session_name, wt.task_window_exists, wt.task_window_name, \
             wt.task_window_current_path, wt.task_window_points_at_expected_path, \
             r.runtime_health, r.runtime_observed_at_unix_seconds, \
             r.runtime_observed_at_subsec_nanos, r.runtime_observation_source, \
             r.runtime_observation_error, w.attention_acknowledged_at_unix_seconds, \
             w.attention_acknowledged_at_subsec_nanos \
             FROM registry_tasks t \
             LEFT JOIN registry_task_workflow w ON w.task_id = t.task_id \
             LEFT JOIN registry_task_live_status l ON l.task_id = t.task_id \
             LEFT JOIN registry_task_runtime_projection r ON r.task_id = t.task_id \
             LEFT JOIN registry_task_git_evidence g ON g.task_id = t.task_id \
             LEFT JOIN registry_task_tmux_evidence tm ON tm.task_id = t.task_id \
             LEFT JOIN registry_task_window_evidence wt ON wt.task_id = t.task_id \
             ORDER BY t.task_id",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut tasks = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        tasks.push(task_from_row(row)?);
    }

    load_task_side_flags_by_task(connection, &mut tasks)?;
    tasks.retain(|task| !is_registry_ghost_task(task));
    load_task_metadata_by_task(connection, &mut tasks)?;
    load_agent_attempts_by_task(connection, &mut tasks)?;

    Ok(tasks)
}

pub(crate) fn task_indexes_by_id(tasks: &[Task]) -> BTreeMap<TaskId, usize> {
    tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.id.clone(), index))
        .collect()
}

pub(crate) fn load_task_side_flags_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare("SELECT task_id, flag FROM registry_task_side_flags ORDER BY task_id, flag")
        .map_err(database_error)?;
    let flags = statement
        .query_map([], |row| {
            Ok((
                TaskId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    for (task_id, flag) in flags {
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        tasks[index].add_side_flag(parse_side_flag(&flag)?);
    }

    Ok(())
}

pub(crate) fn load_task_metadata_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare("SELECT task_id, key, value FROM registry_task_metadata ORDER BY task_id, key")
        .map_err(database_error)?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                TaskId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    for (task_id, key, value) in entries {
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        tasks[index].metadata.insert(key, value);
    }

    Ok(())
}

pub(crate) fn load_agent_attempts_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare(
            "SELECT task_id, agent, launch_target, started_at_unix_seconds, finished_at_unix_seconds, \
             started_at_subsec_nanos, finished_at_subsec_nanos, status \
             FROM registry_agent_attempts ORDER BY task_id, sequence",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;

    while let Some(row) = rows.next().map_err(database_error)? {
        let task_id = TaskId::new(row.get::<_, String>(0).map_err(database_error)?);
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        let agent = parse_agent_client(&row.get::<_, String>(1).map_err(database_error)?)?;
        let launch_target = row.get::<_, String>(2).map_err(database_error)?;
        let started_at = unix_parts_to_system_time(
            row.get::<_, i64>(3).map_err(database_error)?,
            row.get::<_, u32>(5).map_err(database_error)?,
        )?;
        let finished_seconds = row.get::<_, Option<i64>>(4).map_err(database_error)?;
        let finished_nanos = row.get::<_, Option<u32>>(6).map_err(database_error)?;
        let finished_at = match (finished_seconds, finished_nanos) {
            (Some(seconds), Some(nanos)) => Some(unix_parts_to_system_time(seconds, nanos)?),
            (None, None) => None,
            _ => {
                return Err(RegistrySnapshotError::Decode(
                    "agent attempt finished timestamp is incomplete".to_string(),
                ))
            }
        };
        let status = parse_agent_runtime_status(&row.get::<_, String>(7).map_err(database_error)?)?;
        tasks[index].agent_attempts.push(AgentAttempt {
            agent,
            launch_target,
            started_at,
            finished_at,
            status,
        });
    }

    Ok(())
}

pub(crate) fn load_events(
    connection: &Connection,
) -> Result<Vec<RegistryEvent>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, kind, message, occurred_at_unix_seconds, occurred_at_subsec_nanos \
             FROM registry_events ORDER BY sequence",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut events = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        events.push(RegistryEvent {
            task_id: TaskId::new(row.get::<_, String>(0).map_err(database_error)?),
            kind: parse_registry_event_kind(&row.get::<_, String>(1).map_err(database_error)?)?,
            message: row.get::<_, String>(2).map_err(database_error)?,
            occurred_at: unix_parts_to_system_time(
                row.get::<_, i64>(3).map_err(database_error)?,
                row.get::<_, u32>(4).map_err(database_error)?,
            )?,
        });
    }

    Ok(events)
}

pub(crate) fn load_step_receipts(
    connection: &Connection,
) -> Result<Vec<StepReceipt>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, operation, step_key, target, status, receipt_json, \
             created_at_unix_seconds, created_at_subsec_nanos \
             FROM step_receipts ORDER BY task_id, operation, step_key, target",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut receipts = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        let operation =
            parse_task_operation_kind(&row.get::<_, String>(1).map_err(database_error)?)?;
        let status = parse_step_receipt_status(&row.get::<_, String>(4).map_err(database_error)?)?;
        receipts.push(StepReceipt {
            task_id: TaskId::new(row.get::<_, String>(0).map_err(database_error)?),
            operation,
            step_key: row.get::<_, String>(2).map_err(database_error)?,
            target: row.get::<_, String>(3).map_err(database_error)?,
            status,
            receipt_json: row.get::<_, String>(5).map_err(database_error)?,
            created_at: unix_parts_to_system_time(
                row.get::<_, i64>(6).map_err(database_error)?,
                row.get::<_, u32>(7).map_err(database_error)?,
            )?,
        });
    }

    Ok(receipts)
}
