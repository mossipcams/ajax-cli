//! Durable execution reads, history retention, recovery, and owner leases.

use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::execution_store::parse_state;
use super::{database_error, migrations, NewRun, SqliteRegistryStore};
use crate::execution::{
    ExecutionState, Operation, ReplayPolicy, UnfinishedCheckpoint, CHECKPOINT_SCHEMA_VERSION,
};
use crate::registry::RegistrySnapshotError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecord {
    pub run_id: String,
    pub task_id: String,
    pub harness: String,
    pub model: String,
    pub native_session_id: Option<String>,
    pub capabilities_json: String,
    pub child: Option<(i64, i64)>,
    pub owner_boot_id: Option<String>,
    pub schema_version: u32,
    pub next_event_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnfinishedExecution {
    pub task_exec_id: String,
    pub run_id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub state: ExecutionState,
    pub cancel_requested: bool,
    /// `(idempotency_key, content_json)` of the task's first submission.
    pub prompt: Option<(String, String)>,
    pub checkpoint: Option<UnfinishedCheckpoint>,
    pub checkpoint_intent_json: Option<String>,
    pub external_operation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerClaim {
    Acquired { previous: Option<(String, i64)> },
    HeldBy { boot_id: String, pid: i64 },
}

impl SqliteRegistryStore {
    /// Returns the last inserted run for a registry task.
    pub fn latest_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<RunRecord>, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        connection
            .query_row(
                "SELECT run_id, task_id, harness, model, native_session_id, capabilities_json, \
             child_pid, child_pgid, owner_boot_id, schema_version, next_event_seq \
             FROM agent_runs WHERE task_id = ?1 ORDER BY rowid DESC LIMIT 1",
                [task_id],
                |row| {
                    Ok(RunRecord {
                        run_id: row.get(0)?,
                        task_id: row.get(1)?,
                        harness: row.get(2)?,
                        model: row.get(3)?,
                        native_session_id: row.get(4)?,
                        capabilities_json: row.get(5)?,
                        child: row.get::<_, Option<i64>>(6)?.zip(row.get(7)?),
                        owner_boot_id: row.get(8)?,
                        schema_version: row.get(9)?,
                        next_event_seq: {
                            let seq = row.get::<_, i64>(10)?;
                            u64::try_from(seq)
                                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(10, seq))?
                        },
                    })
                },
            )
            .optional()
            .map_err(database_error)
    }

    /// Imports a run and its history atomically, preserving the event sequence.
    pub fn create_run_with_history(
        &self,
        run: &NewRun,
        native_session_id: Option<&str>,
        first_seq: u64,
        events: &[String],
    ) -> Result<(), RegistrySnapshotError> {
        let first_seq = i64::try_from(first_seq)
            .ok()
            .filter(|seq| *seq >= 1)
            .ok_or_else(|| RegistrySnapshotError::Encode("invalid first event sequence".into()))?;
        let next_seq = i64::try_from(events.len())
            .ok()
            .and_then(|len| first_seq.checked_add(len))
            .ok_or_else(|| RegistrySnapshotError::Encode("event sequence overflow".into()))?;
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection.transaction().map_err(database_error)?;
        transaction.execute(
            "INSERT INTO agent_runs (run_id, task_id, harness, model, native_session_id, \
             schema_version, next_event_seq, created_unix_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![run.run_id, run.task_id, run.harness, run.model, native_session_id,
                CHECKPOINT_SCHEMA_VERSION, next_seq, run.created_unix_ms],
        ).map_err(database_error)?;
        for (seq, payload) in (first_seq..next_seq).zip(events) {
            transaction
                .execute(
                    "INSERT INTO execution_events (run_id, seq, payload_json) VALUES (?1, ?2, ?3)",
                    params![run.run_id, seq, payload],
                )
                .map_err(database_error)?;
        }
        transaction.commit().map_err(database_error)
    }

    /// Loads nonterminal tasks in run and prompt admission order for recovery.
    pub fn unfinished_executions(&self) -> Result<Vec<UnfinishedExecution>, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        let mut statement = connection.prepare(
            "SELECT t.task_exec_id, t.run_id, t.parent_id, t.kind, t.state, t.cancel_requested, \
             p.idempotency_key, p.content_json, c.operation, c.replay_policy, c.schema_version, \
             c.result_json IS NOT NULL, c.intent_json, c.external_operation_id \
             FROM execution_tasks t \
             LEFT JOIN prompt_submissions p ON p.submission_id = ( \
                 SELECT MIN(submission_id) FROM prompt_submissions \
                 WHERE task_exec_id = t.task_exec_id AND run_id = t.run_id) \
             LEFT JOIN execution_checkpoints c ON c.task_exec_id = t.task_exec_id \
                 AND c.version = (SELECT MAX(version) FROM execution_checkpoints \
                                  WHERE task_exec_id = t.task_exec_id) \
             WHERE t.state NOT IN ('completed', 'failed', 'cancelled') \
             ORDER BY t.run_id, p.submission_id IS NULL, p.submission_id, t.rowid",
        ).map_err(database_error)?;
        let mut rows = statement.query([]).map_err(database_error)?;
        let mut tasks = Vec::new();
        while let Some(row) = rows.next().map_err(database_error)? {
            let operation: Option<String> = row.get(8).map_err(database_error)?;
            let checkpoint = if let Some(operation) = operation {
                Some(UnfinishedCheckpoint {
                    operation: Operation::parse(&operation),
                    policy: ReplayPolicy::parse(&row.get::<_, String>(9).map_err(database_error)?),
                    schema_version: row.get(10).map_err(database_error)?,
                    has_result: row.get(11).map_err(database_error)?,
                })
            } else {
                None
            };
            tasks.push(UnfinishedExecution {
                task_exec_id: row.get(0).map_err(database_error)?,
                run_id: row.get(1).map_err(database_error)?,
                parent_id: row.get(2).map_err(database_error)?,
                kind: row.get(3).map_err(database_error)?,
                state: parse_state(&row.get::<_, String>(4).map_err(database_error)?)?,
                cancel_requested: row.get(5).map_err(database_error)?,
                prompt: row
                    .get::<_, Option<String>>(6)
                    .map_err(database_error)?
                    .zip(row.get(7).map_err(database_error)?),
                checkpoint,
                checkpoint_intent_json: row.get(12).map_err(database_error)?,
                external_operation_id: row.get(13).map_err(database_error)?,
            });
        }
        Ok(tasks)
    }

    /// Keeps the newest events without resetting the run's sequence counter.
    pub fn prune_execution_events(
        &self,
        run_id: &str,
        keep_last: u64,
    ) -> Result<u64, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        let keep_last = i64::try_from(keep_last).unwrap_or(i64::MAX);
        connection.execute(
            "DELETE FROM execution_events WHERE run_id = ?1 AND seq IN ( \
             SELECT seq FROM execution_events WHERE run_id = ?1 ORDER BY seq DESC LIMIT -1 OFFSET ?2)",
            params![run_id, keep_last],
        ).map(|count| count as u64).map_err(database_error)
    }

    /// Deletes only execution data belonging to this registry task, atomically.
    pub fn delete_runs_for_task(&self, task_id: &str) -> Result<u64, RegistrySnapshotError> {
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        transaction
            .execute(
                "DELETE FROM execution_checkpoints WHERE task_exec_id IN ( \
             SELECT task_exec_id FROM execution_tasks WHERE run_id IN ( \
             SELECT run_id FROM agent_runs WHERE task_id = ?1))",
                [task_id],
            )
            .map_err(database_error)?;
        for table in ["prompt_submissions", "execution_events", "execution_tasks"] {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE run_id IN (SELECT run_id FROM agent_runs WHERE task_id = ?1)"),
                [task_id],
            ).map_err(database_error)?;
        }
        let deleted = transaction
            .execute("DELETE FROM agent_runs WHERE task_id = ?1", [task_id])
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(deleted as u64)
    }

    /// Claims the single owner lease, refreshing this boot or replacing a stale owner.
    pub fn claim_execution_owner(
        &self,
        boot_id: &str,
        pid: i64,
        now_unix_ms: i64,
        stale_after_ms: i64,
    ) -> Result<OwnerClaim, RegistrySnapshotError> {
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let owner = transaction
            .query_row(
                "SELECT boot_id, pid, heartbeat_unix_ms FROM execution_owner WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(database_error)?;
        let previous = match owner {
            Some((old_boot, _, _)) if old_boot == boot_id => {
                transaction
                    .execute(
                        "UPDATE execution_owner SET heartbeat_unix_ms = ?1 WHERE id = 1",
                        [now_unix_ms],
                    )
                    .map_err(database_error)?;
                transaction.commit().map_err(database_error)?;
                return Ok(OwnerClaim::Acquired { previous: None });
            }
            Some((old_boot, old_pid, heartbeat)) => {
                if i128::from(heartbeat) >= i128::from(now_unix_ms) - i128::from(stale_after_ms) {
                    return Ok(OwnerClaim::HeldBy {
                        boot_id: old_boot,
                        pid: old_pid,
                    });
                }
                Some((old_boot, old_pid))
            }
            None => None,
        };
        transaction.execute(
            "INSERT INTO execution_owner (id, boot_id, pid, heartbeat_unix_ms) VALUES (1, ?1, ?2, ?3) \
             ON CONFLICT(id) DO UPDATE SET boot_id = excluded.boot_id, pid = excluded.pid, \
             heartbeat_unix_ms = excluded.heartbeat_unix_ms",
            params![boot_id, pid, now_unix_ms],
        ).map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(OwnerClaim::Acquired { previous })
    }

    /// Refreshes the lease only while this boot still owns it.
    pub fn heartbeat_execution_owner(
        &self,
        boot_id: &str,
        now_unix_ms: i64,
    ) -> Result<bool, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        connection
            .execute(
                "UPDATE execution_owner SET heartbeat_unix_ms = ?2 WHERE id = 1 AND boot_id = ?1",
                params![boot_id, now_unix_ms],
            )
            .map(|count| count != 0)
            .map_err(database_error)
    }

    /// Releases the lease only while this boot still owns it.
    pub fn release_execution_owner(&self, boot_id: &str) -> Result<(), RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        connection
            .execute(
                "DELETE FROM execution_owner WHERE id = 1 AND boot_id = ?1",
                [boot_id],
            )
            .map_err(database_error)?;
        Ok(())
    }
}
