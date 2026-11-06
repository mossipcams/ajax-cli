use std::collections::BTreeSet;

use rusqlite::{params, ToSql, Transaction};

use super::super::{InMemoryRegistry, RegistrySnapshotError};
use crate::ghost_task::is_registry_ghost_task;
use crate::models::{
    GitStatus, LiveObservation, RuntimeProjection, StepReceipt, Task, TaskId, TaskWindowStatus,
    TmuxStatus,
};

use super::enums::*;
use super::execution_recovery::delete_runs_for_task_in;
use super::row_codec::*;

fn persisted_task_ids(transaction: &Transaction<'_>) -> Result<Vec<String>, RegistrySnapshotError> {
    let mut statement = transaction
        .prepare("SELECT task_id FROM registry_tasks")
        .map_err(database_error)?;
    let task_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(task_ids)
}

const TASK_TABLES: &[&str] = &[
    "registry_tasks",
    "registry_task_workflow",
    "registry_task_live_status",
    "registry_task_runtime_projection",
    "registry_task_git_evidence",
    "registry_task_tmux_evidence",
    "registry_task_window_evidence",
    "registry_task_side_flags",
    "registry_task_metadata",
    "registry_agent_attempts",
];

pub(crate) fn save_registry(
    transaction: &Transaction<'_>,
    registry: &InMemoryRegistry,
) -> Result<(), RegistrySnapshotError> {
    let persisted_task_ids = persisted_task_ids(transaction)?;

    let live_task_ids = registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();

    for task_id in persisted_task_ids
        .iter()
        .filter(|task_id| !live_task_ids.contains(&TaskId::new(task_id.as_str())))
    {
        delete_runs_for_task_in(transaction, task_id)?;
    }

    let live_keys = live_task_ids
        .iter()
        .map(|task_id| vec![task_id.as_str().to_string()])
        .collect::<BTreeSet<_>>();
    for table in TASK_TABLES {
        delete_rows_not_in(transaction, table, &["task_id"], None, &live_keys)?;
    }

    for task in registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
    {
        save_task(transaction, task)?;
    }

    let mut event_count = 0_i64;
    for (sequence, event) in registry
        .events
        .iter()
        .filter(|event| live_task_ids.contains(&event.task_id))
        .enumerate()
    {
        let (occurred_at_seconds, occurred_at_nanos) =
            system_time_to_unix_parts(event.occurred_at)?;
        upsert_if_changed(
            transaction,
            "registry_events",
            &["sequence"],
            &[
                "task_id",
                "kind",
                "message",
                "occurred_at_unix_seconds",
                "occurred_at_subsec_nanos",
            ],
            params![
                sequence as i64,
                event.task_id.as_str(),
                registry_event_kind_name(event.kind),
                event.message,
                occurred_at_seconds,
                occurred_at_nanos,
            ],
        )?;
        event_count = sequence as i64 + 1;
    }
    transaction
        .execute(
            "DELETE FROM registry_events WHERE sequence >= ?1",
            [event_count],
        )
        .map_err(database_error)?;

    let mut receipt_keys = BTreeSet::new();
    for receipt in registry
        .step_receipts
        .values()
        .filter(|receipt| live_task_ids.contains(&receipt.task_id))
    {
        save_step_receipt(transaction, receipt)?;
        receipt_keys.insert(vec![
            receipt.task_id.as_str().to_string(),
            receipt.operation.as_str().to_string(),
            receipt.step_key.clone(),
            receipt.target.clone(),
        ]);
    }
    delete_rows_not_in(
        transaction,
        "step_receipts",
        &["task_id", "operation", "step_key", "target"],
        None,
        &receipt_keys,
    )?;

    Ok(())
}

fn upsert_if_changed(
    transaction: &Transaction<'_>,
    table: &str,
    keys: &[&str],
    values: &[&str],
    params: &[&dyn ToSql],
) -> Result<(), RegistrySnapshotError> {
    let columns = keys.iter().chain(values).copied().collect::<Vec<_>>();
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>();
    let on_conflict = if values.is_empty() {
        "DO NOTHING".to_string()
    } else {
        let excluded = values
            .iter()
            .map(|column| format!("excluded.{column}"))
            .collect::<Vec<_>>();
        format!(
            "DO UPDATE SET ({values}) = ({excluded}) WHERE ({values}) IS NOT ({excluded})",
            values = values.join(", "),
            excluded = excluded.join(", "),
        )
    };
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({}) ON CONFLICT({}) {on_conflict}",
        columns.join(", "),
        placeholders.join(", "),
        keys.join(", "),
    );
    transaction
        .prepare_cached(&sql)
        .and_then(|mut statement| statement.execute(params))
        .map_err(database_error)?;
    Ok(())
}

fn delete_rows_not_in(
    transaction: &Transaction<'_>,
    table: &str,
    keys: &[&str],
    task_id: Option<&str>,
    keep: &BTreeSet<Vec<String>>,
) -> Result<(), RegistrySnapshotError> {
    let scope = if task_id.is_some() {
        " WHERE task_id = ?1"
    } else {
        ""
    };
    let mut statement = transaction
        .prepare_cached(&format!(
            "SELECT rowid, {} FROM {table}{scope}",
            keys.join(", ")
        ))
        .map_err(database_error)?;
    let read_row = |row: &rusqlite::Row<'_>| {
        let rowid = row.get::<_, i64>(0)?;
        let key = (1..=keys.len())
            .map(|index| row.get::<_, String>(index))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rowid, key))
    };
    let rows = match task_id {
        Some(task_id) => statement.query_map([task_id], read_row),
        None => statement.query_map([], read_row),
    }
    .map_err(database_error)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(database_error)?;
    for (rowid, _) in rows.iter().filter(|(_, key)| !keep.contains(key)) {
        transaction
            .execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [rowid])
            .map_err(database_error)?;
    }
    Ok(())
}

pub(crate) fn save_task(
    transaction: &Transaction<'_>,
    task: &Task,
) -> Result<(), RegistrySnapshotError> {
    let task_id = task.id.as_str();
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

    upsert_if_changed(
        transaction,
        "registry_tasks",
        &["task_id"],
        &[
            "repo",
            "handle",
            "title",
            "branch",
            "base_branch",
            "worktree_path",
            "tmux_session",
            "task_window",
            "selected_agent",
        ],
        params![
            task_id,
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
    )?;

    upsert_if_changed(
        transaction,
        "registry_task_workflow",
        &["task_id"],
        &[
            "lifecycle_status",
            "agent_status",
            "created_at_unix_seconds",
            "created_at_subsec_nanos",
            "last_activity_at_unix_seconds",
            "last_activity_at_subsec_nanos",
            "attention_acknowledged_at_unix_seconds",
            "attention_acknowledged_at_subsec_nanos",
        ],
        params![
            task_id,
            lifecycle_status_name(task.lifecycle_status),
            agent_runtime_status_name(task.agent_status),
            created_at_seconds,
            created_at_nanos,
            last_activity_seconds,
            last_activity_nanos,
            attention_acknowledged_parts.map(|(seconds, _)| seconds),
            attention_acknowledged_parts.map(|(_, nanos)| nanos),
        ],
    )?;

    save_live_status(
        transaction,
        task_id,
        task.live_status.as_ref(),
        live_status_observed_parts,
    )?;
    save_runtime_projection(
        transaction,
        task_id,
        &task.runtime_projection,
        runtime_observed_seconds,
        runtime_observed_nanos,
    )?;
    save_git_status(transaction, task_id, task.git_status.as_ref())?;
    save_tmux_status(transaction, task_id, task.tmux_status.as_ref())?;
    save_task_window_status(transaction, task_id, task.task_window_status.as_ref())?;

    save_side_flags(transaction, task)?;

    let mut metadata_keys = BTreeSet::new();
    for (key, value) in &task.metadata {
        upsert_if_changed(
            transaction,
            "registry_task_metadata",
            &["task_id", "key"],
            &["value"],
            params![task_id, key, value],
        )?;
        metadata_keys.insert(vec![key.clone()]);
    }
    delete_rows_not_in(
        transaction,
        "registry_task_metadata",
        &["key"],
        Some(task_id),
        &metadata_keys,
    )?;

    save_agent_attempts(transaction, task)?;

    Ok(())
}

pub(crate) fn save_side_flags(
    transaction: &Transaction<'_>,
    task: &Task,
) -> Result<(), RegistrySnapshotError> {
    let task_id = task.id.as_str();
    let mut flag_keys = BTreeSet::new();
    for flag in task.side_flags() {
        let flag = side_flag_name(flag);
        upsert_if_changed(
            transaction,
            "registry_task_side_flags",
            &["task_id", "flag"],
            &[],
            params![task_id, flag],
        )?;
        flag_keys.insert(vec![flag.to_string()]);
    }
    delete_rows_not_in(
        transaction,
        "registry_task_side_flags",
        &["flag"],
        Some(task_id),
        &flag_keys,
    )
}

pub(crate) fn save_agent_attempts(
    transaction: &Transaction<'_>,
    task: &Task,
) -> Result<(), RegistrySnapshotError> {
    let task_id = task.id.as_str();
    for (sequence, attempt) in task.agent_attempts.iter().enumerate() {
        let (started_at_seconds, started_at_nanos) = system_time_to_unix_parts(attempt.started_at)?;
        let finished_at_parts = attempt
            .finished_at
            .map(system_time_to_unix_parts)
            .transpose()?;
        upsert_if_changed(
            transaction,
            "registry_agent_attempts",
            &["task_id", "sequence"],
            &[
                "agent",
                "launch_target",
                "started_at_unix_seconds",
                "started_at_subsec_nanos",
                "finished_at_unix_seconds",
                "finished_at_subsec_nanos",
                "status",
            ],
            params![
                task_id,
                sequence as i64,
                agent_client_name(attempt.agent),
                attempt.launch_target,
                started_at_seconds,
                started_at_nanos,
                finished_at_parts.map(|(seconds, _)| seconds),
                finished_at_parts.map(|(_, nanos)| nanos),
                agent_runtime_status_name(attempt.status),
            ],
        )?;
    }
    transaction
        .execute(
            "DELETE FROM registry_agent_attempts WHERE task_id = ?1 AND sequence >= ?2",
            params![task_id, task.agent_attempts.len() as i64],
        )
        .map_err(database_error)?;
    Ok(())
}

fn delete_task_row(
    transaction: &Transaction<'_>,
    table: &str,
    task_id: &str,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .prepare_cached(&format!("DELETE FROM {table} WHERE task_id = ?1"))
        .and_then(|mut statement| statement.execute([task_id]))
        .map_err(database_error)?;
    Ok(())
}

pub(crate) fn save_live_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    live: Option<&LiveObservation>,
    observed: Option<(i64, u32)>,
) -> Result<(), RegistrySnapshotError> {
    let (Some(live), Some((observed_seconds, observed_nanos))) = (live, observed) else {
        return delete_task_row(transaction, "registry_task_live_status", task_id);
    };
    upsert_if_changed(
        transaction,
        "registry_task_live_status",
        &["task_id"],
        &[
            "live_status_kind",
            "live_status_summary",
            "live_status_observed_at_unix_seconds",
            "live_status_observed_at_subsec_nanos",
        ],
        params![
            task_id,
            live_status_kind_name(live.kind),
            live.summary.as_str(),
            observed_seconds,
            observed_nanos,
        ],
    )
}

pub(crate) fn save_runtime_projection(
    transaction: &Transaction<'_>,
    task_id: &str,
    projection: &RuntimeProjection,
    observed_seconds: i64,
    observed_nanos: u32,
) -> Result<(), RegistrySnapshotError> {
    upsert_if_changed(
        transaction,
        "registry_task_runtime_projection",
        &["task_id"],
        &[
            "runtime_health",
            "runtime_observed_at_unix_seconds",
            "runtime_observed_at_subsec_nanos",
            "runtime_observation_source",
            "runtime_observation_error",
        ],
        params![
            task_id,
            projection.health.as_str(),
            observed_seconds,
            observed_nanos,
            projection.source.as_str(),
            projection.observation_error.as_deref(),
        ],
    )
}

pub(crate) fn save_git_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    git: Option<&GitStatus>,
) -> Result<(), RegistrySnapshotError> {
    let Some(git) = git else {
        return delete_task_row(transaction, "registry_task_git_evidence", task_id);
    };
    upsert_if_changed(
        transaction,
        "registry_task_git_evidence",
        &["task_id"],
        &[
            "git_worktree_exists",
            "git_branch_exists",
            "git_current_branch",
            "git_dirty",
            "git_ahead",
            "git_behind",
            "git_merged",
            "git_untracked_files",
            "git_unpushed_commits",
            "git_conflicted",
            "git_last_commit",
        ],
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
}

pub(crate) fn save_tmux_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    tmux: Option<&TmuxStatus>,
) -> Result<(), RegistrySnapshotError> {
    let Some(tmux) = tmux else {
        return delete_task_row(transaction, "registry_task_tmux_evidence", task_id);
    };
    upsert_if_changed(
        transaction,
        "registry_task_tmux_evidence",
        &["task_id"],
        &["tmux_exists", "tmux_session_name"],
        params![task_id, tmux.exists, tmux.session_name.as_str()],
    )
}

pub(crate) fn save_task_window_status(
    transaction: &Transaction<'_>,
    task_id: &str,
    task: Option<&TaskWindowStatus>,
) -> Result<(), RegistrySnapshotError> {
    let Some(task) = task else {
        return delete_task_row(transaction, "registry_task_window_evidence", task_id);
    };
    upsert_if_changed(
        transaction,
        "registry_task_window_evidence",
        &["task_id"],
        &[
            "task_window_exists",
            "task_window_name",
            "task_window_current_path",
            "task_window_points_at_expected_path",
        ],
        params![
            task_id,
            task.exists,
            task.window_name.as_str(),
            task.current_path.to_string_lossy().as_ref(),
            task.points_at_expected_path,
        ],
    )
}

pub(crate) fn save_step_receipt(
    transaction: &Transaction<'_>,
    receipt: &StepReceipt,
) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(receipt.created_at)?;
    upsert_if_changed(
        transaction,
        "step_receipts",
        &["task_id", "operation", "step_key", "target"],
        &[
            "status",
            "receipt_json",
            "created_at_unix_seconds",
            "created_at_subsec_nanos",
        ],
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
}

#[cfg(test)]
mod tests {
    use super::super::migrations::migrate;
    use super::*;
    use crate::models::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, SideFlag, TaskOperationKind,
    };
    use crate::registry::{Registry, RegistryEventKind};
    use rusqlite::Connection;
    use std::time::{Duration, SystemTime};

    fn task(id: &str, metadata_keys: &[&str], attempts: usize) -> Task {
        let mut task = Task::new(
            TaskId::new(id),
            "web",
            id,
            "Fix login",
            format!("ajax/{id}"),
            "main",
            format!("/tmp/worktrees/web-{id}"),
            format!("ajax-web-{id}"),
            "task",
            AgentClient::Codex,
        );
        for key in metadata_keys {
            task.metadata
                .insert(key.to_string(), format!("{key}-value"));
        }
        for index in 0..attempts {
            task.agent_attempts.push(AgentAttempt {
                agent: AgentClient::Codex,
                launch_target: format!("tmux:%{index}"),
                started_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
                finished_at: None,
                status: AgentRuntimeStatus::Running,
            });
        }
        task
    }

    fn registry(tasks: Vec<Task>, flagged: bool, events: usize) -> InMemoryRegistry {
        let mut registry = InMemoryRegistry::default();
        for mut task in tasks {
            if flagged {
                task.add_side_flag(SideFlag::Dirty);
            }
            let id = task.id.clone();
            registry.create_task(task).unwrap();
            registry
                .record_step_receipt(StepReceipt::succeeded(
                    id.clone(),
                    TaskOperationKind::Start,
                    "worktree",
                    id.as_str(),
                    "{}",
                ))
                .unwrap();
            for index in 0..events {
                registry
                    .record_event(
                        id.clone(),
                        RegistryEventKind::UserNote,
                        format!("event {index}"),
                    )
                    .unwrap();
            }
        }
        registry
    }

    fn open() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        migrate(&connection).unwrap();
        connection
    }

    fn save(connection: &mut Connection, registry: &InMemoryRegistry) -> u64 {
        let transaction = connection.transaction().unwrap();
        let before = transaction.total_changes();
        save_registry(&transaction, registry).unwrap();
        let changed = transaction.total_changes() - before;
        transaction.commit().unwrap();
        changed
    }

    fn dump(connection: &Connection) -> Vec<String> {
        let mut rows = Vec::new();
        for table in TASK_TABLES
            .iter()
            .chain(&["registry_events", "step_receipts"])
        {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table}"))
                .unwrap();
            let columns = statement.column_count();
            let mut query = statement.query([]).unwrap();
            while let Some(row) = query.next().unwrap() {
                let values = (0..columns)
                    .map(|index| format!("{:?}", row.get_ref(index).unwrap()))
                    .collect::<Vec<_>>();
                rows.push(format!("{table}: {}", values.join(", ")));
            }
        }
        rows.sort();
        rows
    }

    #[test]
    fn unchanged_registry_resave_writes_no_rows() {
        let mut connection = open();
        let registry = registry(
            vec![task("task-1", &["a", "b"], 2), task("task-2", &["c"], 1)],
            true,
            3,
        );

        assert!(save(&mut connection, &registry) > 0);
        assert_eq!(save(&mut connection, &registry), 0);
    }

    #[test]
    fn diff_save_matches_fresh_full_save() {
        let before = registry(
            vec![task("task-1", &["a", "b"], 2), task("task-2", &["c"], 1)],
            true,
            3,
        );
        let mut changed = task("task-1", &["a"], 1);
        changed.title = "Renamed".to_string();
        let after = registry(vec![changed, task("task-3", &[], 0)], false, 1);

        let mut diffed = open();
        save(&mut diffed, &before);
        save(&mut diffed, &after);
        let mut fresh = open();
        save(&mut fresh, &after);

        assert_eq!(dump(&diffed), dump(&fresh));
    }
}
