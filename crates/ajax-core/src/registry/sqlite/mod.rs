use std::{collections::BTreeMap, path::PathBuf};

use rusqlite::{Connection, Transaction};

use super::{refresh_task_annotations, InMemoryRegistry, RegistrySnapshotError};
use crate::ghost_task::is_registry_ghost_task;

mod enums;
mod load;
mod migrations;
mod row_codec;
mod save;

use load::{load_events, load_step_receipts, load_tasks};
use row_codec::database_error;
use save::save_registry;

#[cfg(test)]
pub(crate) use enums::{
    parse_agent_client, parse_agent_runtime_status, parse_lifecycle_status, parse_live_status_kind,
    parse_registry_event_kind, parse_side_flag,
};

pub struct SqliteRegistryStore {
    path: PathBuf,
}

impl SqliteRegistryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load_tasks_only(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;

        let mut tasks = load_tasks(&connection)?;
        for task in &mut tasks {
            refresh_task_annotations(task);
        }

        Ok(InMemoryRegistry {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events: Vec::new(),
            step_receipts: BTreeMap::new(),
        })
    }

    fn open(&self) -> Result<Connection, RegistrySnapshotError> {
        Connection::open(&self.path).map_err(database_error)
    }

    pub fn current_revision(&self) -> Result<u64, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        revision(&connection)
    }

    pub fn save_if_revision(
        &self,
        registry: &InMemoryRegistry,
        expected_revision: u64,
    ) -> Result<u64, RegistrySnapshotError> {
        self.save_if_revision_with_empty_policy(registry, expected_revision, false)
    }

    pub fn save_if_revision_allowing_empty_rewrite(
        &self,
        registry: &InMemoryRegistry,
        expected_revision: u64,
    ) -> Result<u64, RegistrySnapshotError> {
        self.save_if_revision_with_empty_policy(registry, expected_revision, true)
    }

    fn save_if_revision_with_empty_policy(
        &self,
        registry: &InMemoryRegistry,
        expected_revision: u64,
        allow_empty_rewrite: bool,
    ) -> Result<u64, RegistrySnapshotError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RegistrySnapshotError::Io(error.to_string()))?;
        }
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection.transaction().map_err(database_error)?;
        let actual = revision(&transaction)?;
        if actual != expected_revision {
            return Err(RegistrySnapshotError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        prevent_accidental_empty_rewrite(&transaction, registry, allow_empty_rewrite)?;
        save_registry(&transaction, registry)?;
        let next = actual.saturating_add(1);
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

impl SqliteRegistryStore {
    pub fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;

        let mut tasks = load_tasks(&connection)?;
        for task in &mut tasks {
            refresh_task_annotations(task);
        }
        let events = load_events(&connection)?;
        let step_receipts = load_step_receipts(&connection)?;

        Ok(InMemoryRegistry {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events,
            step_receipts: step_receipts
                .into_iter()
                .map(|receipt| (receipt.identity(), receipt))
                .collect(),
        })
    }

    pub fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RegistrySnapshotError::Io(error.to_string()))?;
        }

        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection.transaction().map_err(database_error)?;
        prevent_accidental_empty_rewrite(&transaction, registry, false)?;
        save_registry(&transaction, registry)?;
        transaction
            .execute(
                "UPDATE registry_meta SET value = value + 1 WHERE key = 'revision'",
                [],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }
}

fn prevent_accidental_empty_rewrite(
    transaction: &Transaction<'_>,
    registry: &InMemoryRegistry,
    allow_empty_rewrite: bool,
) -> Result<(), RegistrySnapshotError> {
    if allow_empty_rewrite || registry_has_persistable_tasks(registry) {
        return Ok(());
    }
    let existing_task_count: i64 = transaction
        .query_row("SELECT count(*) FROM registry_tasks", [], |row| row.get(0))
        .map_err(database_error)?;
    if existing_task_count > 0 {
        return Err(RegistrySnapshotError::EmptyRegistryOverwrite);
    }
    Ok(())
}

fn registry_has_persistable_tasks(registry: &InMemoryRegistry) -> bool {
    registry
        .tasks
        .values()
        .any(|task| !is_registry_ghost_task(task))
}

fn revision(connection: &Connection) -> Result<u64, RegistrySnapshotError> {
    connection
        .query_row(
            "SELECT value FROM registry_meta WHERE key = 'revision'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(database_error)
        .and_then(|value| {
            u64::try_from(value).map_err(|error| RegistrySnapshotError::Decode(error.to_string()))
        })
}

#[cfg(test)]
mod tests;
