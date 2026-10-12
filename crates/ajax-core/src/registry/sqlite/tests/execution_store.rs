use super::{task, InMemoryRegistry};
use crate::execution::{
    ExecutionState as State, Operation, ReplayPolicy, CHECKPOINT_SCHEMA_VERSION,
};
use crate::registry::{
    CheckpointIntent, CheckpointResult, ExecutionCommit, ExecutionCommitError, NewExecutionTask,
    NewPrompt, NewRun, RunUpdate, SqliteRegistryStore, TaskTransition,
};
use rusqlite::{types::Value, Connection};

const EXECUTION_TABLES: [&str; 6] = [
    "agent_runs",
    "execution_tasks",
    "prompt_submissions",
    "execution_checkpoints",
    "execution_events",
    "execution_owner",
];

fn new_run(run_id: &str) -> NewRun {
    NewRun {
        run_id: run_id.into(),
        task_id: "registry-task".into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn temp_path() -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "ajax-execution-{}-{nonce}.sqlite",
        std::process::id()
    ))
}

fn store() -> SqliteRegistryStore {
    let store = SqliteRegistryStore::new(temp_path());
    store.create_run(&new_run("run")).unwrap();
    store
}

fn commit() -> ExecutionCommit {
    ExecutionCommit {
        run_id: "run".into(),
        ..Default::default()
    }
}

fn new_task(id: &str, parent: Option<&str>) -> NewExecutionTask {
    NewExecutionTask {
        task_exec_id: id.into(),
        parent_id: parent.map(str::to_owned),
        kind: "prompt".into(),
    }
}

fn prompt(id: &str) -> NewPrompt {
    NewPrompt {
        idempotency_key: "key".into(),
        task_exec_id: id.into(),
        content_json: "{}".into(),
    }
}

fn to(id: &str, state: State) -> TaskTransition {
    TaskTransition {
        task_exec_id: id.into(),
        to: state,
        outcome_json: None,
    }
}

fn intent(id: &str) -> CheckpointIntent {
    CheckpointIntent {
        task_exec_id: id.into(),
        operation: Operation::PromptDispatch,
        replay_policy: ReplayPolicy::Reconcile,
        external_operation_id: Some("external".into()),
        intent_json: "{\"prompt\":1}".into(),
    }
}

fn result(id: &str) -> CheckpointResult {
    CheckpointResult {
        task_exec_id: id.into(),
        result_json: "{\"ok\":true}".into(),
    }
}

fn state(store: &SqliteRegistryStore, id: &str) -> Option<State> {
    store.execution_task_state(id).unwrap()
}

fn number(store: &SqliteRegistryStore, sql: &str) -> i64 {
    store
        .open()
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn rows(connection: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut statement = connection.prepare(sql).unwrap();
    let columns = statement.column_count();
    statement
        .query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn prompt_admission_creates_task_prompt_and_event_atomically() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    let receipt = apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        prompts: vec![prompt("task")],
        events: vec!["{\"admitted\":true}".into()],
        ..commit()
    });
    assert_eq!(receipt.event_seqs, vec![1]);
    assert!(receipt.duplicate_prompts.is_empty());
    assert_eq!(state(&store, "task"), Some(State::Pending));
    assert_eq!(state(&store, "unknown"), None);
    assert_eq!(number(&store, "SELECT count(*) FROM prompt_submissions"), 1);
    assert_eq!(
        number(&store, "SELECT schema_version FROM agent_runs"),
        i64::from(CHECKPOINT_SCHEMA_VERSION)
    );
    assert_eq!(
        store.execution_events_since("run", 0).unwrap(),
        vec![(1, "{\"admitted\":true}".into())]
    );
}

#[test]
fn duplicate_idempotency_key_makes_the_entire_commit_a_no_op() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    let admission = ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        prompts: vec![prompt("task")],
        events: vec!["{}".into()],
        ..commit()
    };
    store.commit_execution(&admission).unwrap();
    let retry = store.commit_execution(&admission).unwrap();
    assert_eq!(retry.duplicate_prompts, vec!["key"]);
    assert!(retry.event_seqs.is_empty());
    let receipt = apply(ExecutionCommit {
        new_tasks: vec![new_task("retry", None), new_task("other", None)],
        prompts: vec![prompt("retry")],
        transitions: vec![to("task", State::Running)],
        events: vec!["{}".into()],
        ..commit()
    });
    assert_eq!(receipt.duplicate_prompts, vec!["key"]);
    assert!(receipt.event_seqs.is_empty());
    assert_eq!(state(&store, "retry"), None);
    assert_eq!(state(&store, "other"), None);
    assert_eq!(state(&store, "task"), Some(State::Pending));
    assert_eq!(number(&store, "SELECT count(*) FROM prompt_submissions"), 1);
    let events = store.execution_events_since("run", 0).unwrap();
    assert_eq!(events, vec![(1, "{}".into())]);
    assert_eq!(number(&store, "SELECT next_event_seq FROM agent_runs"), 2);
}

#[test]
fn duplicate_keys_within_one_commit_make_the_entire_commit_a_no_op() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    let receipt = apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None), new_task("retry", None)],
        prompts: vec![prompt("task"), prompt("task"), prompt("retry")],
        events: vec!["{}".into()],
        ..commit()
    });
    assert_eq!(receipt.duplicate_prompts, vec!["key", "key"]);
    assert!(receipt.event_seqs.is_empty());
    assert_eq!(state(&store, "task"), None);
    assert_eq!(state(&store, "retry"), None);
    assert_eq!(number(&store, "SELECT count(*) FROM prompt_submissions"), 0);
    assert!(store.execution_events_since("run", 0).unwrap().is_empty());
    assert_eq!(number(&store, "SELECT next_event_seq FROM agent_runs"), 1);
}

#[test]
fn event_sequences_are_consecutive_across_commits_and_independent_per_run() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    assert_eq!(
        apply(ExecutionCommit {
            events: vec!["1".into(), "2".into()],
            ..commit()
        })
        .event_seqs,
        vec![1, 2]
    );
    store.commit_execution(&commit()).unwrap();
    assert_eq!(
        apply(ExecutionCommit {
            events: vec!["3".into()],
            ..commit()
        })
        .event_seqs,
        vec![3]
    );
    assert_eq!(
        store.execution_events_since("run", 1).unwrap(),
        vec![(2, "2".into()), (3, "3".into())]
    );
    assert!(store
        .execution_events_since("run", u64::MAX)
        .unwrap()
        .is_empty());
    assert_eq!(number(&store, "SELECT next_event_seq FROM agent_runs"), 4);
    store.create_run(&new_run("other")).unwrap();
    assert_eq!(
        apply(ExecutionCommit {
            run_id: "other".into(),
            events: vec!["0".into()],
            ..Default::default()
        })
        .event_seqs,
        vec![1]
    );
}

#[test]
fn effect_sandwich_records_result_before_transition_and_next_intent() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        transitions: vec![to("task", State::Running)],
        checkpoint_intents: vec![intent("task")],
        ..commit()
    });
    let connection = store.open().unwrap();
    let first = rows(&connection, "SELECT version, operation, replay_policy, external_operation_id, schema_version, intent_json, result_json FROM execution_checkpoints");
    assert_eq!(
        first,
        vec![vec![
            Value::Integer(1),
            Value::Text("prompt_dispatch".into()),
            Value::Text("reconcile".into()),
            Value::Text("external".into()),
            Value::Integer(i64::from(CHECKPOINT_SCHEMA_VERSION)),
            Value::Text("{\"prompt\":1}".into()),
            Value::Null,
        ]]
    );
    apply(ExecutionCommit {
        checkpoint_results: vec![result("task")],
        transitions: vec![to("task", State::Waiting)],
        checkpoint_intents: vec![intent("task")],
        ..commit()
    });
    assert_eq!(
        rows(
            &connection,
            "SELECT version, result_json FROM execution_checkpoints ORDER BY version"
        ),
        vec![
            vec![Value::Integer(1), Value::Text("{\"ok\":true}".into())],
            vec![Value::Integer(2), Value::Null],
        ]
    );
    assert_eq!(state(&store, "task"), Some(State::Waiting));
}

#[test]
fn invalid_transition_rolls_back_new_rows_and_sequence() {
    let store = store();
    let error = store
        .commit_execution(&ExecutionCommit {
            new_tasks: vec![new_task("task", None)],
            prompts: vec![prompt("task")],
            transitions: vec![to("task", State::Completed)],
            checkpoint_intents: vec![intent("task")],
            events: vec!["{}".into()],
            ..commit()
        })
        .unwrap_err();
    assert!(matches!(error, ExecutionCommitError::InvalidTransition {
        task_exec_id, from: State::Pending, to: State::Completed
    } if task_exec_id == "task"));
    for table in &EXECUTION_TABLES[1..5] {
        assert_eq!(number(&store, &format!("SELECT count(*) FROM {table}")), 0);
    }
    assert_eq!(number(&store, "SELECT next_event_seq FROM agent_runs"), 1);
}

#[test]
fn parent_completion_observes_children_inside_the_transaction() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("parent", None), new_task("child", Some("parent"))],
        transitions: vec![to("parent", State::Running), to("child", State::Running)],
        ..commit()
    });
    assert!(matches!(store.commit_execution(&ExecutionCommit {
        transitions: vec![to("parent", State::Completing)], ..commit()
    }), Err(ExecutionCommitError::ChildrenNotTerminal { task_exec_id }) if task_exec_id == "parent"));
    apply(ExecutionCommit {
        transitions: vec![
            to("child", State::Completing),
            to("child", State::Completed),
            to("parent", State::Completing),
        ],
        ..commit()
    });
    assert_eq!(state(&store, "parent"), Some(State::Completing));
    assert!(matches!(
        store.commit_execution(&ExecutionCommit {
            new_tasks: vec![new_task("new-child", Some("parent"))],
            transitions: vec![to("parent", State::Completed)],
            ..commit()
        }),
        Err(ExecutionCommitError::ChildrenNotTerminal { .. })
    ));
    assert_eq!(state(&store, "new-child"), None);
    apply(ExecutionCommit {
        transitions: vec![TaskTransition {
            outcome_json: Some("{\"done\":true}".into()),
            ..to("parent", State::Completed)
        }],
        ..commit()
    });
    assert_eq!(state(&store, "parent"), Some(State::Completed));
    assert_eq!(
        rows(
            &store.open().unwrap(),
            "SELECT outcome_json FROM execution_tasks WHERE task_exec_id = 'parent'"
        ),
        vec![vec![Value::Text("{\"done\":true}".into())]]
    );
}

#[test]
fn unknown_run_and_tasks_are_errors() {
    let store = store();
    assert!(matches!(store.commit_execution(&ExecutionCommit {
        run_id: "missing".into(), new_tasks: vec![new_task("task", None)], events: vec!["{}".into()],
        ..Default::default()
    }), Err(ExecutionCommitError::UnknownRun(id)) if id == "missing"));
    for change in [
        ExecutionCommit {
            prompts: vec![prompt("missing")],
            ..commit()
        },
        ExecutionCommit {
            transitions: vec![to("missing", State::Running)],
            ..commit()
        },
        ExecutionCommit {
            checkpoint_intents: vec![intent("missing")],
            ..commit()
        },
        ExecutionCommit {
            checkpoint_results: vec![result("missing")],
            ..commit()
        },
    ] {
        assert!(
            matches!(store.commit_execution(&change), Err(ExecutionCommitError::UnknownTask(id)) if id == "missing")
        );
    }
    assert_eq!(number(&store, "SELECT count(*) FROM execution_tasks"), 0);
    assert_eq!(number(&store, "SELECT next_event_seq FROM agent_runs"), 1);
}

#[test]
fn checkpoint_result_requires_the_latest_checkpoint_to_be_open() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        ..commit()
    });
    let completion = ExecutionCommit {
        checkpoint_results: vec![result("task")],
        ..commit()
    };
    assert!(matches!(
        store.commit_execution(&completion),
        Err(ExecutionCommitError::NoOpenCheckpoint { .. })
    ));
    apply(ExecutionCommit {
        checkpoint_intents: vec![intent("task"), intent("task")],
        ..commit()
    });
    store.commit_execution(&completion).unwrap();
    assert!(matches!(
        store.commit_execution(&completion),
        Err(ExecutionCommitError::NoOpenCheckpoint { .. })
    ));
    assert_eq!(
        rows(
            &store.open().unwrap(),
            "SELECT version, result_json FROM execution_checkpoints ORDER BY version"
        ),
        vec![
            vec![Value::Integer(1), Value::Null],
            vec![Value::Integer(2), Value::Text("{\"ok\":true}".into())],
        ]
    );
}

#[test]
fn run_updates_preserve_unset_fields_and_clear_nullable_fields() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        run_update: Some(RunUpdate {
            native_session_id: Some(Some("session".into())),
            capabilities_json: Some("{\"resume\":true}".into()),
            model: Some("new-model".into()),
            child: Some(Some((42, 43))),
            owner_boot_id: Some(Some("boot".into())),
        }),
        ..commit()
    });
    let sql = "SELECT native_session_id, capabilities_json, model, child_pid, child_pgid, owner_boot_id FROM agent_runs";
    let expected = vec![vec![
        Value::Text("session".into()),
        Value::Text("{\"resume\":true}".into()),
        Value::Text("new-model".into()),
        Value::Integer(42),
        Value::Integer(43),
        Value::Text("boot".into()),
    ]];
    assert_eq!(rows(&store.open().unwrap(), sql), expected);
    apply(ExecutionCommit {
        run_update: Some(RunUpdate::default()),
        ..commit()
    });
    assert_eq!(rows(&store.open().unwrap(), sql), expected);
    apply(ExecutionCommit {
        run_update: Some(RunUpdate {
            native_session_id: Some(None),
            child: Some(None),
            owner_boot_id: Some(None),
            ..Default::default()
        }),
        ..commit()
    });
    let mut cleared = expected;
    for column in [0, 3, 4, 5] {
        cleared[0][column] = Value::Null;
    }
    assert_eq!(rows(&store.open().unwrap(), sql), cleared);
}

#[test]
fn late_store_error_rolls_back_every_section() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        checkpoint_intents: vec![intent("task")],
        ..commit()
    });
    let connection = store.open().unwrap();
    connection
        .execute_batch(
            r#"
        CREATE TRIGGER fail_sequence BEFORE UPDATE OF next_event_seq ON agent_runs
        BEGIN SELECT RAISE(ABORT, 'injected failure'); END;
    "#,
        )
        .unwrap();
    let tables = &EXECUTION_TABLES[..5];
    let before: Vec<_> = tables
        .iter()
        .map(|table| rows(&connection, &format!("SELECT * FROM {table}")))
        .collect();
    assert!(matches!(
        store.commit_execution(&ExecutionCommit {
            new_tasks: vec![new_task("new", None)],
            prompts: vec![prompt("new")],
            checkpoint_results: vec![result("task")],
            transitions: vec![to("task", State::Running)],
            checkpoint_intents: vec![intent("task")],
            events: vec!["{}".into()],
            run_update: Some(RunUpdate {
                model: Some("changed".into()),
                ..Default::default()
            }),
            ..commit()
        }),
        Err(ExecutionCommitError::Store(_))
    ));
    for (table, expected) in tables.iter().zip(before) {
        assert_eq!(
            rows(&connection, &format!("SELECT * FROM {table}")),
            expected,
            "{table}"
        );
    }
    connection
        .execute_batch("DROP TRIGGER fail_sequence")
        .unwrap();
    assert_eq!(
        store
            .commit_execution(&ExecutionCommit {
                events: vec!["{}".into()],
                ..commit()
            })
            .unwrap()
            .event_seqs,
        vec![1]
    );
}

#[test]
fn v9_migration_preserves_registry_row_and_adds_six_execution_tables() {
    let path = temp_path();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch(r#"
        CREATE TABLE registry_tasks (
            task_id TEXT PRIMARY KEY NOT NULL, repo TEXT NOT NULL, handle TEXT NOT NULL,
            title TEXT NOT NULL, branch TEXT NOT NULL, base_branch TEXT NOT NULL,
            worktree_path TEXT NOT NULL, tmux_session TEXT NOT NULL,
            task_window TEXT NOT NULL, selected_agent TEXT NOT NULL
        );
        INSERT INTO registry_tasks VALUES (
            'old', 'repo', 'handle', 'title', 'branch', 'main', '/tmp/worktree', 'session', 'task', 'codex'
        );
        CREATE TABLE registry_meta (key TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL);
        INSERT INTO registry_meta VALUES ('revision', 37);
        PRAGMA user_version = 9;
    "#).unwrap();
    let before = rows(&connection, "SELECT * FROM registry_tasks");
    let store = SqliteRegistryStore::new(path);
    assert_eq!(store.current_revision().unwrap(), 37);
    assert_eq!(number(&store, "PRAGMA user_version"), 10);
    assert_eq!(rows(&connection, "SELECT * FROM registry_tasks"), before);
    for table in EXECUTION_TABLES {
        assert_eq!(
            number(
                &store,
                &format!(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = '{table}'"
                )
            ),
            1
        );
    }
    store.create_run(&new_run("run")).unwrap();
    assert_eq!(rows(&connection, "SELECT * FROM registry_tasks"), before);
}

#[test]
fn registry_save_preserves_execution_rows() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("task", None)],
        prompts: vec![prompt("task")],
        checkpoint_intents: vec![intent("task")],
        events: vec!["{}".into()],
        ..commit()
    });
    let connection = store.open().unwrap();
    connection
        .execute(
            "INSERT INTO execution_owner VALUES (1, 'boot', 42, 1234)",
            [],
        )
        .unwrap();
    let tables = EXECUTION_TABLES;
    let before: Vec<_> = tables
        .iter()
        .map(|table| rows(&connection, &format!("SELECT * FROM {table}")))
        .collect();
    let task = task("registry-task", "repo", "handle");
    let registry = InMemoryRegistry {
        tasks: [(task.id.clone(), task)].into_iter().collect(),
        events: Vec::new(),
        step_receipts: Default::default(),
    };
    store.save(&registry).unwrap();
    store.save(&registry).unwrap();
    assert_eq!(number(&store, "SELECT count(*) FROM registry_tasks"), 1);
    for (table, expected) in tables.iter().zip(before) {
        assert_eq!(
            rows(&connection, &format!("SELECT * FROM {table}")),
            expected,
            "{table}"
        );
    }
}
