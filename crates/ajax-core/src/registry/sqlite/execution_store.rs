//! Durable execution persistence on top of the registry SQLite store.
//!
//! The pure domain model lives in [`crate::execution`]; this module only maps
//! it onto the `agent_runs`, `execution_tasks`, `prompt_submissions`,
//! `execution_checkpoints`, and `execution_events` tables.

use std::{collections::HashSet, error::Error, fmt};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::row_codec::database_error;
use super::{migrations, SqliteRegistryStore};
use crate::execution::{
    children_allow_completion, transition, ExecutionState, Operation, ReplayPolicy,
    CHECKPOINT_SCHEMA_VERSION,
};
use crate::registry::RegistrySnapshotError;

/// A new agent run to record in `agent_runs`.
#[derive(Clone, Debug)]
pub struct NewRun {
    pub run_id: String,
    pub task_id: String,
    pub harness: String,
    pub model: String,
    pub created_unix_ms: i64,
}

/// A new execution task inserted in state `Pending`.
#[derive(Clone, Debug)]
pub struct NewExecutionTask {
    pub task_exec_id: String,
    pub parent_id: Option<String>,
    pub kind: String,
}

/// A prompt submission; a repeat `(run_id, idempotency_key)` is admitted once.
#[derive(Clone, Debug)]
pub struct NewPrompt {
    pub idempotency_key: String,
    pub task_exec_id: String,
    pub content_json: String,
}
/// An in-place content update for an already-admitted queued prompt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptContentUpdate {
    pub idempotency_key: String,
    pub content_json: String,
}

/// A state transition applied to an existing execution task.
#[derive(Clone, Debug)]
pub struct TaskTransition {
    pub task_exec_id: String,
    pub to: ExecutionState,
    pub outcome_json: Option<String>,
}

/// A new checkpoint intent for the next version of a task.
#[derive(Clone, Debug)]
pub struct CheckpointIntent {
    pub task_exec_id: String,
    pub operation: Operation,
    pub replay_policy: ReplayPolicy,
    pub external_operation_id: Option<String>,
    pub intent_json: String,
}

/// A checkpoint result recorded on the task's highest-version open checkpoint.
#[derive(Clone, Debug)]
pub struct CheckpointResult {
    pub task_exec_id: String,
    pub result_json: String,
}

/// Optional in-place updates to the run row. Each field is tri-state: absent
/// means "leave unchanged", `Some(outer)` means "apply the inner value" (which
/// may be `None` to clear a nullable column).
#[derive(Clone, Debug, Default)]
pub struct RunUpdate {
    pub native_session_id: Option<Option<String>>,
    pub capabilities_json: Option<String>,
    pub model: Option<String>,
    /// `(child_pid, child_pgid)`; the inner `None` clears both.
    pub child: Option<Option<(i64, i64)>>,
    pub owner_boot_id: Option<Option<String>>,
}

/// Everything a single durable execution commit may contain. Fill only what is
/// needed; all sections are applied in one transaction.
#[derive(Clone, Debug, Default)]
pub struct ExecutionCommit {
    pub run_id: String,
    pub new_tasks: Vec<NewExecutionTask>,
    pub prompts: Vec<NewPrompt>,
    pub transitions: Vec<TaskTransition>,
    pub cancel_requests: Vec<String>,
    pub prompt_content_updates: Vec<PromptContentUpdate>,
    /// Idempotency keys whose stored `content_json` is wiped to `'{}'`; allowed in any task state.
    pub prompt_content_clears: Vec<String>,
    pub checkpoint_intents: Vec<CheckpointIntent>,
    pub checkpoint_results: Vec<CheckpointResult>,
    /// Event payloads (JSON) appended with consecutive per-run sequence numbers.
    pub events: Vec<String>,
    pub run_update: Option<RunUpdate>,
}

/// Receipt for a committed execution commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Sequence numbers assigned to the commit's events, in order.
    pub event_seqs: Vec<u64>,
    /// Duplicate idempotency keys that caused the entire commit to be skipped.
    pub duplicate_prompts: Vec<String>,
}

/// Failure of [`SqliteRegistryStore::commit_execution`]. Any variant rolls the
/// whole commit back.
#[derive(Debug)]
pub enum ExecutionCommitError {
    UnknownRun(String),
    UnknownTask(String),
    InvalidTransition {
        task_exec_id: String,
        from: ExecutionState,
        to: ExecutionState,
    },
    ChildrenNotTerminal {
        task_exec_id: String,
    },
    NoOpenCheckpoint {
        task_exec_id: String,
    },
    PromptNotQueued {
        idempotency_key: String,
    },
    Store(RegistrySnapshotError),
}

impl fmt::Display for ExecutionCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRun(run_id) => write!(formatter, "unknown agent run: {run_id}"),
            Self::UnknownTask(task_exec_id) => {
                write!(formatter, "unknown execution task: {task_exec_id}")
            }
            Self::InvalidTransition {
                task_exec_id,
                from,
                to,
            } => write!(
                formatter,
                "invalid transition for task {task_exec_id}: {from:?} -> {to:?}"
            ),
            Self::ChildrenNotTerminal { task_exec_id } => write!(
                formatter,
                "task {task_exec_id} has children that are not terminal"
            ),
            Self::NoOpenCheckpoint { task_exec_id } => write!(
                formatter,
                "task {task_exec_id} has no open checkpoint for the change"
            ),
            Self::PromptNotQueued { idempotency_key } => {
                write!(formatter, "prompt {idempotency_key} is no longer queued")
            }
            Self::Store(error) => write!(formatter, "registry store error: {error}"),
        }
    }
}

impl Error for ExecutionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RegistrySnapshotError> for ExecutionCommitError {
    fn from(error: RegistrySnapshotError) -> Self {
        Self::Store(error)
    }
}

impl From<rusqlite::Error> for ExecutionCommitError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Store(database_error(error))
    }
}

pub(super) fn parse_state(text: &str) -> Result<ExecutionState, RegistrySnapshotError> {
    ExecutionState::parse(text)
        .ok_or_else(|| RegistrySnapshotError::Decode(format!("execution_tasks.state: {text}")))
}

fn task_state(
    connection: &Connection,
    run_id: &str,
    task_exec_id: &str,
) -> Result<ExecutionState, ExecutionCommitError> {
    let text: Option<String> = connection
        .query_row(
            "SELECT state FROM execution_tasks WHERE task_exec_id = ?1 AND run_id = ?2",
            params![task_exec_id, run_id],
            |row| row.get(0),
        )
        .optional()?;
    let text = text.ok_or_else(|| ExecutionCommitError::UnknownTask(task_exec_id.to_owned()))?;
    Ok(parse_state(&text)?)
}

fn children_allow_completion_for(
    connection: &Connection,
    task_exec_id: &str,
) -> Result<bool, ExecutionCommitError> {
    let mut statement =
        connection.prepare("SELECT state FROM execution_tasks WHERE parent_id = ?1")?;
    let rows = statement.query_map(params![task_exec_id], |row| row.get::<_, String>(0))?;
    let mut states = Vec::new();
    for text in rows {
        states.push(parse_state(&text?)?);
    }
    Ok(children_allow_completion(&states))
}

impl SqliteRegistryStore {
    /// Records a new agent run. Event sequencing starts at 1 for the run.
    pub fn create_run(&self, run: &NewRun) -> Result<(), RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        connection.execute(
            "INSERT INTO agent_runs (run_id, task_id, harness, model, capabilities_json, schema_version, created_unix_ms) \
             VALUES (?1, ?2, ?3, ?4, '{}', ?5, ?6)",
            params![
                run.run_id,
                run.task_id,
                run.harness,
                run.model,
                i64::from(CHECKPOINT_SCHEMA_VERSION),
                run.created_unix_ms
            ],
        ).map_err(database_error)?;
        Ok(())
    }

    /// Applies an entire [`ExecutionCommit`] in a single transaction. Any error
    /// rolls everything back: no task, prompt, checkpoint, or event row and no
    /// `next_event_seq` advance survive. Any duplicate prompt skips the whole commit.
    pub fn commit_execution(
        &self,
        commit: &ExecutionCommit,
    ) -> Result<CommitReceipt, ExecutionCommitError> {
        let mut connection = self.open()?;
        migrations::migrate(&connection)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let start_seq: i64 = transaction
            .query_row(
                "SELECT next_event_seq FROM agent_runs WHERE run_id = ?1",
                params![commit.run_id],
                |row| row.get(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    ExecutionCommitError::UnknownRun(commit.run_id.clone())
                }
                other => ExecutionCommitError::Store(database_error(other)),
            })?;

        // A duplicate admission makes the entire commit a no-op, including events.
        let mut duplicate_prompts = Vec::new();
        let mut keys_in_commit = HashSet::new();
        for prompt in &commit.prompts {
            let already_present =
                self.prompt_key_present(&transaction, &commit.run_id, &prompt.idempotency_key)?;
            if already_present || !keys_in_commit.insert(&prompt.idempotency_key) {
                duplicate_prompts.push(prompt.idempotency_key.clone());
            }
        }
        if !duplicate_prompts.is_empty() {
            return Ok(CommitReceipt {
                event_seqs: Vec::new(),
                duplicate_prompts,
            });
        }

        // 1. New execution tasks, inserted in state Pending.
        for task in &commit.new_tasks {
            transaction.execute(
                "INSERT INTO execution_tasks (task_exec_id, run_id, parent_id, kind, state) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    task.task_exec_id,
                    commit.run_id,
                    task.parent_id,
                    task.kind,
                    ExecutionState::Pending.as_str()
                ],
            )?;
        }

        // 2. Prompt submissions.
        for prompt in &commit.prompts {
            task_state(&transaction, &commit.run_id, &prompt.task_exec_id)?;
            transaction.execute(
                "INSERT INTO prompt_submissions (run_id, idempotency_key, task_exec_id, content_json) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![commit.run_id, prompt.idempotency_key, prompt.task_exec_id, prompt.content_json],
            )?;
        }

        // 3. Checkpoint results on each task's highest-version open checkpoint.
        for result in &commit.checkpoint_results {
            task_state(&transaction, &commit.run_id, &result.task_exec_id)?;
            let version: Option<i64> = transaction.query_row(
                "SELECT version FROM execution_checkpoints WHERE task_exec_id = ?1 \
                 AND version = (SELECT MAX(version) FROM execution_checkpoints WHERE task_exec_id = ?1) \
                 AND result_json IS NULL",
                params![result.task_exec_id],
                |row| row.get(0),
            ).optional()?;
            let version = version.ok_or_else(|| ExecutionCommitError::NoOpenCheckpoint {
                task_exec_id: result.task_exec_id.clone(),
            })?;
            transaction.execute(
                "UPDATE execution_checkpoints SET result_json = ?2 WHERE task_exec_id = ?1 AND version = ?3",
                params![result.task_exec_id, result.result_json, version],
            )?;
        }

        // 4. State transitions validated against the pure domain model.
        for task_transition in &commit.transitions {
            let from = task_state(&transaction, &commit.run_id, &task_transition.task_exec_id)?;
            transition(from, task_transition.to).map_err(|_| {
                ExecutionCommitError::InvalidTransition {
                    task_exec_id: task_transition.task_exec_id.clone(),
                    from,
                    to: task_transition.to,
                }
            })?;
            if matches!(
                task_transition.to,
                ExecutionState::Completing | ExecutionState::Completed
            ) {
                let allowed =
                    children_allow_completion_for(&transaction, &task_transition.task_exec_id)?;
                if !allowed {
                    return Err(ExecutionCommitError::ChildrenNotTerminal {
                        task_exec_id: task_transition.task_exec_id.clone(),
                    });
                }
            }
            transaction.execute(
                "UPDATE execution_tasks SET state = ?2, outcome_json = ?3 WHERE task_exec_id = ?1",
                params![
                    task_transition.task_exec_id,
                    task_transition.to.as_str(),
                    task_transition.outcome_json
                ],
            )?;
        }

        for task_exec_id in &commit.cancel_requests {
            task_state(&transaction, &commit.run_id, task_exec_id)?;
            transaction.execute(
                "UPDATE execution_tasks SET cancel_requested = 1 WHERE task_exec_id = ?1",
                params![task_exec_id],
            )?;
        }
        // In-place content updates on prompts whose tasks are still queued.
        for update in &commit.prompt_content_updates {
            let task_exec_id: Option<String> = transaction
                .query_row(
                    "SELECT task_exec_id FROM prompt_submissions \
                     WHERE run_id = ?1 AND idempotency_key = ?2",
                    params![commit.run_id, update.idempotency_key],
                    |row| row.get(0),
                )
                .optional()?;
            let task_exec_id = task_exec_id
                .ok_or_else(|| ExecutionCommitError::UnknownTask(update.idempotency_key.clone()))?;
            if task_state(&transaction, &commit.run_id, &task_exec_id)? != ExecutionState::Pending {
                return Err(ExecutionCommitError::PromptNotQueued {
                    idempotency_key: update.idempotency_key.clone(),
                });
            }
            transaction.execute(
                "UPDATE prompt_submissions SET content_json = ?2 \
                 WHERE run_id = ?1 AND idempotency_key = ?3",
                params![commit.run_id, update.content_json, update.idempotency_key],
            )?;
        }

        // Clear stored prompt content for the given keys, in any task state.
        for key in &commit.prompt_content_clears {
            let updated = transaction.execute(
                "UPDATE prompt_submissions SET content_json = '{}' \
                 WHERE run_id = ?1 AND idempotency_key = ?2",
                params![commit.run_id, key],
            )?;
            if updated == 0 {
                return Err(ExecutionCommitError::UnknownTask(key.clone()));
            }
        }

        // 5. New checkpoint intents on the next version of each task.
        for intent in &commit.checkpoint_intents {
            task_state(&transaction, &commit.run_id, &intent.task_exec_id)?;
            let existing_version: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM execution_checkpoints WHERE task_exec_id = ?1",
                params![intent.task_exec_id],
                |row| row.get(0),
            )?;
            let version = existing_version.checked_add(1).ok_or_else(|| {
                RegistrySnapshotError::Encode("checkpoint version overflow".into())
            })?;
            transaction.execute(
                "INSERT INTO execution_checkpoints (task_exec_id, version, operation, replay_policy, external_operation_id, schema_version, intent_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    intent.task_exec_id,
                    version,
                    intent.operation.as_str(),
                    intent.replay_policy.as_str(),
                    intent.external_operation_id,
                    i64::from(CHECKPOINT_SCHEMA_VERSION),
                    intent.intent_json
                ],
            )?;
        }

        // 6. In-place run updates.
        if let Some(run_update) = &commit.run_update {
            if let Some(native_session_id) = &run_update.native_session_id {
                transaction.execute(
                    "UPDATE agent_runs SET native_session_id = ?2 WHERE run_id = ?1",
                    params![commit.run_id, native_session_id],
                )?;
            }
            if let Some(capabilities_json) = &run_update.capabilities_json {
                transaction.execute(
                    "UPDATE agent_runs SET capabilities_json = ?2 WHERE run_id = ?1",
                    params![commit.run_id, capabilities_json],
                )?;
            }
            if let Some(model) = &run_update.model {
                transaction.execute(
                    "UPDATE agent_runs SET model = ?2 WHERE run_id = ?1",
                    params![commit.run_id, model],
                )?;
            }
            if let Some(child) = &run_update.child {
                transaction.execute(
                    "UPDATE agent_runs SET child_pid = ?2, child_pgid = ?3 WHERE run_id = ?1",
                    params![
                        commit.run_id,
                        child.map(|(pid, _)| pid),
                        child.map(|(_, pgid)| pgid)
                    ],
                )?;
            }
            if let Some(owner_boot_id) = &run_update.owner_boot_id {
                transaction.execute(
                    "UPDATE agent_runs SET owner_boot_id = ?2 WHERE run_id = ?1",
                    params![commit.run_id, owner_boot_id],
                )?;
            }
        }

        // 7. Events with consecutive per-run sequence numbers.
        let mut event_seqs = Vec::with_capacity(commit.events.len());
        let mut next_seq = start_seq;
        for payload in &commit.events {
            let seq = next_seq;
            next_seq = next_seq
                .checked_add(1)
                .ok_or_else(|| RegistrySnapshotError::Encode("event sequence overflow".into()))?;
            transaction.execute(
                "INSERT INTO execution_events (run_id, seq, payload_json) VALUES (?1, ?2, ?3)",
                params![commit.run_id, seq, payload],
            )?;
            event_seqs.push(u64::try_from(seq).map_err(|_| {
                ExecutionCommitError::Store(RegistrySnapshotError::Decode(format!(
                    "event seq {seq} does not fit in u64"
                )))
            })?);
        }
        transaction.execute(
            "UPDATE agent_runs SET next_event_seq = ?2 WHERE run_id = ?1",
            params![commit.run_id, next_seq],
        )?;

        transaction
            .commit()
            .map_err(|error| ExecutionCommitError::Store(database_error(error)))?;

        Ok(CommitReceipt {
            event_seqs,
            duplicate_prompts,
        })
    }

    /// Returns the run's events with `seq > after_seq`, ordered by `seq`.
    pub fn execution_events_since(
        &self,
        run_id: &str,
        after_seq: u64,
    ) -> Result<Vec<(u64, String)>, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        let Ok(after_seq) = i64::try_from(after_seq) else {
            return Ok(Vec::new());
        };
        let mut statement = connection.prepare(
            "SELECT seq, payload_json FROM execution_events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq",
        ).map_err(database_error)?;
        let rows = statement
            .query_map(params![run_id, after_seq], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(database_error)?;
        rows.map(|row| {
            let (seq, payload) = row.map_err(database_error)?;
            let seq = u64::try_from(seq)
                .map_err(|error| RegistrySnapshotError::Decode(error.to_string()))?;
            Ok((seq, payload))
        })
        .collect()
    }

    /// Returns the current state of an execution task, or `None` if unknown.
    pub fn execution_task_state(
        &self,
        task_exec_id: &str,
    ) -> Result<Option<ExecutionState>, RegistrySnapshotError> {
        let connection = self.open()?;
        migrations::migrate(&connection)?;
        let text: Option<String> = connection
            .query_row(
                "SELECT state FROM execution_tasks WHERE task_exec_id = ?1",
                params![task_exec_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)?;
        text.as_deref().map(parse_state).transpose()
    }

    fn prompt_key_present(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        run_id: &str,
        idempotency_key: &str,
    ) -> Result<bool, ExecutionCommitError> {
        Ok(transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM prompt_submissions WHERE run_id = ?1 AND idempotency_key = ?2)",
            params![run_id, idempotency_key],
            |row| row.get(0),
        )?)
    }
}
