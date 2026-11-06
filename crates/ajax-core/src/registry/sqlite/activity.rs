use rusqlite::{params, TransactionBehavior};

use super::enums::{agent_client_name, agent_runtime_status_name, live_status_kind_name};
use super::load::load_task;
use super::migrations;
use super::row_codec::{database_error, system_time_to_unix_parts};
use super::save::{save_agent_attempts, save_live_status, save_side_flags};
use super::{revision, SqliteRegistryStore};
use crate::models::{SideFlag, Task};
use crate::registry::RegistrySnapshotError;

type StoredParts = (i64, u32);

#[derive(Debug, PartialEq, Eq)]
struct StoredAttempt {
    agent: &'static str,
    launch_target: String,
    started_at: StoredParts,
    finished_at: Option<StoredParts>,
    status: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
struct StoredActivity {
    agent_status: &'static str,
    last_activity_at: StoredParts,
    live_status: Option<(&'static str, String, StoredParts)>,
    side_flags: Vec<SideFlag>,
    agent_attempts: Vec<StoredAttempt>,
}

impl StoredActivity {
    fn of(task: &Task) -> Result<Self, RegistrySnapshotError> {
        let live_status = match (task.live_status.as_ref(), task.live_status_observed_at) {
            (Some(live), Some(observed)) => Some((
                live_status_kind_name(live.kind),
                live.summary.clone(),
                system_time_to_unix_parts(observed)?,
            )),
            _ => None,
        };
        let agent_attempts = task
            .agent_attempts
            .iter()
            .map(|attempt| {
                Ok(StoredAttempt {
                    agent: agent_client_name(attempt.agent),
                    launch_target: attempt.launch_target.clone(),
                    started_at: system_time_to_unix_parts(attempt.started_at)?,
                    finished_at: attempt
                        .finished_at
                        .map(system_time_to_unix_parts)
                        .transpose()?,
                    status: agent_runtime_status_name(attempt.status),
                })
            })
            .collect::<Result<Vec<_>, RegistrySnapshotError>>()?;
        Ok(Self {
            agent_status: agent_runtime_status_name(task.agent_status),
            last_activity_at: system_time_to_unix_parts(task.last_activity_at)?,
            live_status,
            side_flags: task.side_flags().collect(),
            agent_attempts,
        })
    }
}

impl SqliteRegistryStore {
    pub fn record_activity(
        &self,
        task: &Task,
        expected: &Task,
    ) -> Result<u64, RegistrySnapshotError> {
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

        let expected_activity = StoredActivity::of(expected)?;
        let on_disk = load_task(&transaction, task.id.as_str())?
            .map(|current| StoredActivity::of(&current))
            .transpose()?;
        if on_disk.as_ref() != Some(&expected_activity) {
            return Err(RegistrySnapshotError::Decode(format!(
                "registry task activity changed on disk: {}",
                task.id.as_str()
            )));
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
