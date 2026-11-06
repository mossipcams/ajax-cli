use rusqlite::{params, TransactionBehavior};

use super::enums::agent_runtime_status_name;
use super::migrations;
use super::row_codec::{database_error, system_time_to_unix_parts};
use super::save::{save_agent_attempts, save_live_status, save_side_flags};
use super::{revision, SqliteRegistryStore};
use crate::models::Task;
use crate::registry::RegistrySnapshotError;

impl SqliteRegistryStore {
    pub fn record_activity(&self, task: &Task) -> Result<u64, RegistrySnapshotError> {
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;

        match transaction.query_row(
            "SELECT 1 FROM registry_tasks WHERE task_id = ?1",
            [task.id.as_str()],
            |_| Ok(()),
        ) {
            Ok(()) => {}
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(RegistrySnapshotError::Decode(format!(
                    "registry task is missing from registry_tasks: {}",
                    task.id.as_str()
                )));
            }
            Err(error) => return Err(database_error(error)),
        }

        let (last_activity_seconds, last_activity_nanos) =
            system_time_to_unix_parts(task.last_activity_at)?;
        transaction
            .execute(
                "UPDATE registry_task_workflow \
                 SET agent_status = ?1, \
                     last_activity_at_unix_seconds = ?2, \
                     last_activity_at_subsec_nanos = ?3 \
                 WHERE task_id = ?4",
                params![
                    agent_runtime_status_name(task.agent_status),
                    last_activity_seconds,
                    last_activity_nanos,
                    task.id.as_str(),
                ],
            )
            .map_err(database_error)?;

        let live_status_observed_parts = task
            .live_status_observed_at
            .map(system_time_to_unix_parts)
            .transpose()?;
        save_live_status(
            &transaction,
            task.id.as_str(),
            task.live_status.as_ref(),
            live_status_observed_parts,
        )?;
        save_side_flags(&transaction, task)?;
        save_agent_attempts(&transaction, task)?;

        let next = revision(&transaction)?.saturating_add(1);
        transaction
            .execute(
                "UPDATE registry_meta SET value = ?1 WHERE key = 'revision'",
                [next as i64],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(next)
    }
}
