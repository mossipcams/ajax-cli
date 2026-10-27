use super::SqliteRegistryStore;
use crate::execution::{
    resolve_reconciled, resolve_recovery, ExecutionState as State, InterruptReason, Operation,
    ReconcileOutcome, RecoveryAction, ReplayPolicy,
};
use crate::registry::sqlite::{
    CheckpointIntent, CheckpointResult, ExecutionCommit, ExecutionCommitError, NewExecutionTask,
    NewPrompt, NewRun, TaskTransition,
};

const PARENT_TASK: &str = "run-1:prompt:p1";

fn db_path(scenario: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ajax-core-execution-crash-{}-{}.db",
        std::process::id(),
        scenario
    ));
    path
}

fn run_child(scenario: &str, db: &std::path::Path) -> std::process::ExitStatus {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "registry::sqlite::tests::execution_crash::crash_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AJAX_CRASH_SCENARIO", scenario)
        .env("AJAX_CRASH_DB", db)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.status().expect("spawn child test process")
}

fn new_run() -> NewRun {
    NewRun {
        run_id: "run-1".into(),
        task_id: "web/t".into(),
        harness: "mock".into(),
        model: "auto".into(),
        created_unix_ms: 1,
    }
}

fn admission() -> ExecutionCommit {
    ExecutionCommit {
        run_id: "run-1".into(),
        new_tasks: vec![NewExecutionTask {
            task_exec_id: PARENT_TASK.into(),
            parent_id: None,
            kind: "prompt".into(),
        }],
        prompts: vec![NewPrompt {
            idempotency_key: "p1".into(),
            task_exec_id: PARENT_TASK.into(),
            content_json: "{\"prompt_text\":\"hi\"}".into(),
        }],
        events: vec!["\"user:hi\"".into()],
        ..Default::default()
    }
}

fn dispatch(store: &SqliteRegistryStore, policy: ReplayPolicy, op_id: Option<String>) {
    let intent = CheckpointIntent {
        task_exec_id: PARENT_TASK.into(),
        operation: Operation::PromptDispatch,
        replay_policy: policy,
        external_operation_id: op_id,
        intent_json: "{}".into(),
    };
    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            transitions: vec![TaskTransition {
                task_exec_id: PARENT_TASK.into(),
                to: State::Running,
                outcome_json: None,
            }],
            checkpoint_intents: vec![intent.clone()],
            ..Default::default()
        })
        .expect("dispatch commit");
}

fn transition_to(store: &SqliteRegistryStore, id: &str, to: State) {
    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            transitions: vec![TaskTransition {
                task_exec_id: id.into(),
                to,
                outcome_json: None,
            }],
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("transition {id} -> {to:?}: {error}"));
}

#[test]
fn crash_child() {
    let Some(scenario) = std::env::var("AJAX_CRASH_SCENARIO").ok() else {
        return;
    };
    let db = std::env::temp_dir().join(
        std::env::var("AJAX_CRASH_DB")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| panic!("missing AJAX_CRASH_DB for scenario {scenario}")),
    );
    let store = SqliteRegistryStore::new(&db);

    match scenario.as_str() {
        "before_admission" => {
            store.create_run(&new_run()).expect("create run");
        }
        "admitted_not_dispatched" => {
            store.create_run(&new_run()).expect("create run");
            let commit = admission();
            store.commit_execution(&commit).expect("admission commit");
        }
        "dispatched_uncertain" | "cancel_intent_survives" => {
            store.create_run(&new_run()).expect("create run");
            let commit = admission();
            store.commit_execution(&commit).expect("admission commit");
            dispatch(&store, ReplayPolicy::Unsafe, None);
        }
        "dispatched_reconcilable" => {
            store.create_run(&new_run()).expect("create run");
            let commit = admission();
            store.commit_execution(&commit).expect("admission commit");
            dispatch(&store, ReplayPolicy::Reconcile, Some("op-9".into()));
        }
        "result_committed_before_crash" => {
            store.create_run(&new_run()).expect("create run");
            let commit = admission();
            store.commit_execution(&commit).expect("admission commit");
            dispatch(&store, ReplayPolicy::Unsafe, None);
            store
                .commit_execution(&ExecutionCommit {
                    run_id: "run-1".into(),
                    checkpoint_results: vec![CheckpointResult {
                        task_exec_id: PARENT_TASK.into(),
                        result_json: "{\"ok\":true}".into(),
                    }],
                    transitions: vec![TaskTransition {
                        task_exec_id: PARENT_TASK.into(),
                        to: State::Completing,
                        outcome_json: None,
                    }],
                    ..Default::default()
                })
                .expect("result commit");
        }
        "child_running_blocks_parent" => {
            store.create_run(&new_run()).expect("create run");
            store
                .commit_execution(&ExecutionCommit {
                    run_id: "run-1".into(),
                    new_tasks: vec![
                        NewExecutionTask {
                            task_exec_id: "run-1:turn".into(),
                            parent_id: None,
                            kind: "turn".into(),
                        },
                        NewExecutionTask {
                            task_exec_id: "run-1:sub".into(),
                            parent_id: Some("run-1:turn".into()),
                            kind: "prompt".into(),
                        },
                    ],
                    ..Default::default()
                })
                .expect("task commit");
            transition_to(&store, "run-1:turn", State::Running);
            transition_to(&store, "run-1:sub", State::Running);
        }
        "multiple_runs" => {
            for (run_id, task_id) in [("run-1", "web/a"), ("run-2", "web/b"), ("run-3", "web/c")] {
                store
                    .create_run(&NewRun {
                        run_id: run_id.into(),
                        task_id: task_id.into(),
                        harness: "mock".into(),
                        model: "auto".into(),
                        created_unix_ms: 1,
                    })
                    .expect("create run");
                store
                    .commit_execution(&ExecutionCommit {
                        run_id: run_id.into(),
                        new_tasks: vec![NewExecutionTask {
                            task_exec_id: format!("{run_id}:prompt:p1"),
                            parent_id: None,
                            kind: "prompt".into(),
                        }],
                        prompts: vec![NewPrompt {
                            idempotency_key: "p1".into(),
                            task_exec_id: format!("{run_id}:prompt:p1"),
                            content_json: "{\"prompt_text\":\"hi\"}".into(),
                        }],
                        ..Default::default()
                    })
                    .expect("admission commit");
            }
            store
                .commit_execution(&ExecutionCommit {
                    run_id: "run-2".into(),
                    transitions: vec![TaskTransition {
                        task_exec_id: "run-2:prompt:p1".into(),
                        to: State::Running,
                        outcome_json: None,
                    }],
                    checkpoint_intents: vec![CheckpointIntent {
                        task_exec_id: "run-2:prompt:p1".into(),
                        operation: Operation::PromptDispatch,
                        replay_policy: ReplayPolicy::Unsafe,
                        external_operation_id: None,
                        intent_json: "{}".into(),
                    }],
                    ..Default::default()
                })
                .expect("dispatch commit");
        }
        other => panic!("unknown crash scenario {other:?}"),
    }

    std::process::abort();
}

#[test]
fn before_admission_crash_leaves_nothing() {
    let db = db_path("before_admission");
    let _ = std::fs::remove_file(&db);
    let status = run_child("before_admission", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    assert!(store.prompts_for_run("run-1").unwrap().is_empty());
    assert!(store.unfinished_executions().unwrap().is_empty());
    assert!(store.execution_events_since("run-1", 0).unwrap().is_empty());

    let _ = std::fs::remove_file(&db);
}

#[test]
fn admitted_not_dispatched_prompt_survives_once() {
    let db = db_path("admitted_not_dispatched");
    let _ = std::fs::remove_file(&db);
    let status = run_child("admitted_not_dispatched", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].idempotency_key, "p1");
    assert_eq!(prompts[0].content_json, "{\"prompt_text\":\"hi\"}");
    assert_eq!(prompts[0].state, State::Pending);

    let events = store.execution_events_since("run-1", 0).unwrap();
    assert_eq!(events, vec![(1, "\"user:hi\"".into())]);

    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].task_exec_id, PARENT_TASK);
    assert_eq!(unfinished[0].state, State::Pending);
    assert!(unfinished[0].checkpoint.is_none());
    let (key, content) = unfinished[0].prompt.clone().expect("prompt survives");
    assert_eq!(
        (key, content),
        ("p1".into(), "{\"prompt_text\":\"hi\"}".into())
    );

    let receipt = store
        .commit_execution(&admission())
        .expect("re-admission commit");
    assert_eq!(receipt.duplicate_prompts, vec!["p1"]);
    assert!(receipt.event_seqs.is_empty());

    let prompts = store.prompts_for_run("run-1").unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].state, State::Pending);

    let _ = std::fs::remove_file(&db);
}

#[test]
fn dispatched_uncertain_never_replays() {
    let db = db_path("dispatched_uncertain");
    let _ = std::fs::remove_file(&db);
    let status = run_child("dispatched_uncertain", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].task_exec_id, PARENT_TASK);
    assert_eq!(unfinished[0].state, State::Running);

    let checkpoint = unfinished[0].checkpoint.expect("checkpoint survives");
    assert!(!checkpoint.has_result);

    assert_eq!(
        resolve_recovery(&checkpoint, false),
        RecoveryAction::Interrupt(InterruptReason::UnsafeAmbiguous)
    );

    transition_to(&store, PARENT_TASK, State::Interrupted);
    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].state, State::Interrupted);
    assert_ne!(unfinished[0].state, State::Completed);

    let _ = std::fs::remove_file(&db);
}

#[test]
fn dispatched_reconcilable_adopts_or_interrupts() {
    let db = db_path("dispatched_reconcilable");
    let _ = std::fs::remove_file(&db);
    let status = run_child("dispatched_reconcilable", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    let entry = &unfinished[0];
    assert_eq!(entry.state, State::Running);
    assert_eq!(entry.external_operation_id.as_deref(), Some("op-9"));

    let checkpoint = entry.checkpoint.expect("checkpoint survives");
    assert!(!checkpoint.has_result);
    assert_eq!(
        resolve_recovery(&checkpoint, false),
        RecoveryAction::Reconcile
    );

    assert_eq!(
        resolve_reconciled(ReconcileOutcome::Unknown),
        RecoveryAction::Interrupt(InterruptReason::ReconcileInconclusive)
    );
    assert_eq!(
        resolve_reconciled(ReconcileOutcome::Executed),
        RecoveryAction::AdoptExternal
    );

    let _ = std::fs::remove_file(&db);
}

#[test]
fn result_committed_before_crash_resumes() {
    let db = db_path("result_committed_before_crash");
    let _ = std::fs::remove_file(&db);
    let status = run_child("result_committed_before_crash", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].task_exec_id, PARENT_TASK);
    assert_eq!(unfinished[0].state, State::Completing);

    let checkpoint = unfinished[0].checkpoint.expect("checkpoint survives");
    assert!(checkpoint.has_result);
    assert_eq!(
        resolve_recovery(&checkpoint, false),
        RecoveryAction::ResumeFromResult
    );

    transition_to(&store, PARENT_TASK, State::Completed);
    let unfinished = store.unfinished_executions().unwrap();
    assert!(unfinished.is_empty(), "completed task leaves the list");

    let _ = std::fs::remove_file(&db);
}

#[test]
fn child_running_blocks_parent_completion() {
    let db = db_path("child_running_blocks_parent");
    let _ = std::fs::remove_file(&db);
    let status = run_child("child_running_blocks_parent", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    assert!(matches!(
        store.commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            transitions: vec![TaskTransition {
                task_exec_id: "run-1:turn".into(),
                to: State::Completing,
                outcome_json: None,
            }],
            ..Default::default()
        }),
        Err(ExecutionCommitError::ChildrenNotTerminal { task_exec_id }) if task_exec_id == "run-1:turn"
    ));

    transition_to(&store, "run-1:sub", State::Failed);
    transition_to(&store, "run-1:turn", State::Completing);
    transition_to(&store, "run-1:turn", State::Completed);

    let unfinished = store.unfinished_executions().unwrap();
    assert!(unfinished
        .iter()
        .all(|entry| entry.task_exec_id != "run-1:turn"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn cancel_intent_survives_crash() {
    let db = db_path("cancel_intent_survives");
    let _ = std::fs::remove_file(&db);
    let status = run_child("cancel_intent_survives", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);

    store
        .commit_execution(&ExecutionCommit {
            run_id: "run-1".into(),
            cancel_requests: vec![PARENT_TASK.into()],
            ..Default::default()
        })
        .expect("cancel commit");

    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 1);
    assert!(unfinished[0].cancel_requested);

    let checkpoint = unfinished[0].checkpoint.expect("checkpoint survives");
    assert_eq!(resolve_recovery(&checkpoint, true), RecoveryAction::Cancel);

    let _ = std::fs::remove_file(&db);
}

#[test]
fn multiple_runs_all_survive_with_their_states() {
    let db = db_path("multiple_runs");
    let _ = std::fs::remove_file(&db);
    let status = run_child("multiple_runs", &db);
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let store = SqliteRegistryStore::new(&db);
    let unfinished = store.unfinished_executions().unwrap();
    assert_eq!(unfinished.len(), 3);

    let by_run: std::collections::BTreeMap<_, _> = unfinished
        .iter()
        .map(|entry| (entry.run_id.clone(), entry.state))
        .collect();
    assert_eq!(by_run.get("run-1"), Some(&State::Pending));
    assert_eq!(by_run.get("run-2"), Some(&State::Running));
    assert_eq!(by_run.get("run-3"), Some(&State::Pending));

    assert_eq!(
        store.list_run_task_ids().unwrap(),
        vec!["web/a", "web/b", "web/c"]
    );

    let _ = std::fs::remove_file(&db);
}
