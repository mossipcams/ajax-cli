use crate::execution::ExecutionState;
use crate::registry::{
    ExecutionCommit, ExecutionCommitError, NewExecutionTask, NewPrompt, NewRun,
    PromptContentUpdate, RunPrompt, SqliteRegistryStore, TaskTransition,
};

fn run(id: &str, task: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        task_id: task.into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ajax-execution-prompts-{}-{name}-{}.sqlite",
        std::process::id(),
        nonce()
    ))
}

fn task_exec(id: &str) -> NewExecutionTask {
    NewExecutionTask {
        kind: "prompt".into(),
        parent_id: None,
        task_exec_id: id.into(),
    }
}

fn prompt(key: &str, task_exec_id: &str, content: &str) -> NewPrompt {
    NewPrompt {
        idempotency_key: key.into(),
        content_json: content.into(),
        task_exec_id: task_exec_id.into(),
    }
}

fn admission_commit(store: &SqliteRegistryStore, run_id: &str) {
    store
        .commit_execution(&ExecutionCommit {
            run_id: run_id.into(),
            new_tasks: vec![task_exec("t1"), task_exec("t2")],
            prompts: vec![prompt("k1", "t1", "one"), prompt("k2", "t2", "two")],
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn prompts_for_run_returns_every_prompt_in_admission_order_with_states() {
    let store = SqliteRegistryStore::new(temp_path("prompt-states"));
    store.create_run(&run("run-1", "registry-task")).unwrap();
    admission_commit(&store, "run-1");

    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(
        prompts,
        vec![
            RunPrompt {
                idempotency_key: "k1".into(),
                task_exec_id: "t1".into(),
                content_json: "one".into(),
                state: ExecutionState::Pending,
            },
            RunPrompt {
                idempotency_key: "k2".into(),
                task_exec_id: "t2".into(),
                content_json: "two".into(),
                state: ExecutionState::Pending,
            }
        ]
    );

    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            transitions: vec![
                TaskTransition {
                    task_exec_id: "t1".into(),
                    to: ExecutionState::Running,
                    outcome_json: None,
                },
                TaskTransition {
                    task_exec_id: "t1".into(),
                    to: ExecutionState::Completing,
                    outcome_json: None,
                },
                TaskTransition {
                    task_exec_id: "t1".into(),
                    to: ExecutionState::Completed,
                    outcome_json: None,
                },
            ],
            ..Default::default()
        })
        .unwrap();

    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(prompts[0].state, ExecutionState::Completed);
    assert_eq!(prompts[1].state, ExecutionState::Pending);
}

#[test]
fn prompt_content_update_replaces_content_of_pending_prompt() {
    let store = SqliteRegistryStore::new(temp_path("prompt-update"));
    store.create_run(&run("run-1", "registry-task")).unwrap();
    admission_commit(&store, "run-1");

    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            prompt_content_updates: vec![PromptContentUpdate {
                idempotency_key: "k1".into(),
                content_json: "one-revised".into(),
            }],
            ..Default::default()
        })
        .unwrap();

    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(prompts[0].content_json, "one-revised");
    assert_eq!(prompts[1].content_json, "two");
}

#[test]
fn prompt_content_update_rejects_running_task_and_rolls_back_commit() {
    let store = SqliteRegistryStore::new(temp_path("prompt-not-queued"));
    store.create_run(&run("run-1", "registry-task")).unwrap();
    admission_commit(&store, "run-1");

    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            transitions: vec![TaskTransition {
                task_exec_id: "t1".into(),
                to: ExecutionState::Running,
                outcome_json: None,
            }],
            ..Default::default()
        })
        .unwrap();

    let result = store.commit_execution(&ExecutionCommit {
        run_id: "run-1".into(),
        prompt_content_updates: vec![PromptContentUpdate {
            idempotency_key: "k1".into(),
            content_json: "should-not-land".into(),
        }],
        events: vec!["event-in-failed-commit".into()],
        ..Default::default()
    });

    assert!(matches!(
        result,
        Err(ExecutionCommitError::PromptNotQueued { .. })
    ));

    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(prompts[0].content_json, "one");
    assert!(store.execution_events_since("run-1", 0).unwrap().is_empty());
}

#[test]
fn prompt_content_update_for_unknown_key_is_unknown_task() {
    let store = SqliteRegistryStore::new(temp_path("prompt-unknown"));
    store.create_run(&run("run-1", "registry-task")).unwrap();
    admission_commit(&store, "run-1");

    let result = store.commit_execution(&ExecutionCommit {
        run_id: "run-1".into(),
        prompt_content_updates: vec![PromptContentUpdate {
            idempotency_key: "missing".into(),
            content_json: "x".into(),
        }],
        ..Default::default()
    });

    assert!(matches!(
        result,
        Err(ExecutionCommitError::UnknownTask(key)) if key == "missing"
    ));
}
