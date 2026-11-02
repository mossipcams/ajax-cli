#![allow(unused_imports)]
pub(super) use super::{
    parse_agent_client, parse_agent_runtime_status, parse_lifecycle_status, parse_live_status_kind,
    parse_registry_event_kind, parse_side_flag, SqliteRegistryStore,
};
pub(super) use crate::models::{
    AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
    LiveStatusKind, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, SideFlag,
    StepReceipt, Task, TaskId, TaskOperationKind, TaskWindowStatus, TmuxStatus,
};
pub(super) use crate::registry::{
    InMemoryRegistry, Registry, RegistryEvent, RegistryEventKind, RegistrySnapshotError,
};
pub(super) use rstest::rstest;
pub(super) use std::time::{Duration, SystemTime};

pub(super) fn task(id: &str, repo: &str, handle: &str) -> Task {
    Task::new(
        TaskId::new(id),
        repo,
        handle,
        "Fix login",
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/{repo}-{handle}"),
        format!("ajax-{repo}-{handle}"),
        "task",
        AgentClient::Codex,
    )
}

mod execution_child;
mod execution_crash;
mod execution_drop;
mod execution_history;
mod execution_prompts;
mod execution_recovery;
mod execution_store;
mod suite_1;
mod suite_2;
mod suite_3;
mod suite_4;

pub(super) fn table_columns(connection: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
