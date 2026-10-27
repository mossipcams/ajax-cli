use super::{task, InMemoryRegistry};
use crate::execution::{
    resolve_recovery, ExecutionState as State, InterruptReason, Operation, RecoveryAction,
    ReplayPolicy, CHECKPOINT_SCHEMA_VERSION,
};
use crate::registry::{
    CheckpointIntent, CheckpointResult, ExecutionCommit, ExecutionCommitError, NewExecutionTask,
    NewPrompt, NewRun, OwnerClaim, RunRecord, RunUpdate, SqliteRegistryStore, TaskTransition,
};
use rusqlite::types::Value;

fn new_run(id: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        task_id: "registry-task".into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn store() -> SqliteRegistryStore {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let store = SqliteRegistryStore::new(std::env::temp_dir().join(format!(
        "ajax-execution-recovery-{}-{nonce}.sqlite",
        std::process::id()
    )));
    store.create_run(&new_run("run")).unwrap();
    store
}

fn commit() -> ExecutionCommit {
    ExecutionCommit {
        run_id: "run".into(),
        ..Default::default()
    }
}

fn new_task(id: &str) -> NewExecutionTask {
    NewExecutionTask {
        task_exec_id: id.into(),
        parent_id: None,
        kind: "prompt".into(),
    }
}

fn prompt(id: &str) -> NewPrompt {
    NewPrompt {
        idempotency_key: id.into(),
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
        replay_policy: ReplayPolicy::Unsafe,
        external_operation_id: Some("external".into()),
        intent_json: "{\"prompt\":1}".into(),
    }
}

pub(super) fn rows(store: &SqliteRegistryStore, sql: &str) -> Vec<Vec<Value>> {
    let connection = store.open().unwrap();
    let mut statement = connection.prepare(sql).unwrap();
    let columns = statement.column_count();
    statement
        .query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn snapshot(store: &SqliteRegistryStore) -> Vec<Vec<Vec<Value>>> {
    [
        "agent_runs",
        "execution_tasks",
        "prompt_submissions",
        "execution_checkpoints",
        "execution_events",
    ]
    .iter()
    .map(|table| rows(store, &format!("SELECT * FROM {table} ORDER BY rowid")))
    .collect()
}

#[test]
fn duplicate_admission_skips_every_section_of_a_mixed_commit() {
    let store = store();
    let apply = |change| store.commit_execution(&change).unwrap();
    apply(ExecutionCommit {
        new_tasks: vec![new_task("existing")],
        prompts: vec![prompt("existing")],
        checkpoint_intents: vec![intent("existing")],
        ..commit()
    });
    let before = snapshot(&store);
    let receipt = apply(ExecutionCommit {
        new_tasks: vec![new_task("new")],
        prompts: vec![prompt("new"), prompt("existing")],
        transitions: vec![to("existing", State::Running)],
        cancel_requests: vec!["existing".into()],
        checkpoint_results: vec![CheckpointResult {
            task_exec_id: "existing".into(),
            result_json: "{}".into(),
        }],
        checkpoint_intents: vec![intent("existing")],
        run_update: Some(RunUpdate {
            model: Some("changed".into()),
            ..Default::default()
        }),
        events: vec!["{}".into()],
        ..commit()
    });
    assert!(receipt.event_seqs.is_empty());
    assert_eq!(receipt.duplicate_prompts, vec!["existing"]);
    assert_eq!(snapshot(&store), before);
}

#[test]
fn latest_run_uses_insertion_order_and_reads_all_fields() {
    let store = store();
    assert!(store.latest_run_for_task("missing").unwrap().is_none());
    let mut run = new_run("newest");
    run.created_unix_ms = 1;
    store.create_run(&run).unwrap();
    store
        .create_run(&NewRun {
            task_id: "other-task".into(),
            ..new_run("other")
        })
        .unwrap();
    store
        .commit_execution(&ExecutionCommit {
            run_id: "newest".into(),
            run_update: Some(RunUpdate {
                harness: None,
                native_session_id: Some(Some("session".into())),
                capabilities_json: Some("[]".into()),
                model: Some("updated".into()),
                child: Some(Some((42, 43))),
                owner_boot_id: Some(Some("boot".into())),
                child_started: None,
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        store.latest_run_for_task("registry-task").unwrap(),
        Some(RunRecord {
            run_id: "newest".into(),
            task_id: "registry-task".into(),
            harness: "codex".into(),
            model: "updated".into(),
            native_session_id: Some("session".into()),
            capabilities_json: "[]".into(),
            child: Some((42, 43)),
            owner_boot_id: Some("boot".into()),
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            next_event_seq: 1,
            unreadable_history_rows: 0,
            child_started: None,
        })
    );
}

#[test]
fn imported_history_continues_sequence_and_duplicate_run_writes_nothing() {
    let store = store();
    let run = new_run("imported");
    store
        .create_run_with_history(&run, Some("native"), 12, &["a".into(), "b".into()], 0)
        .unwrap();
    let record = store.latest_run_for_task("registry-task").unwrap().unwrap();
    assert_eq!(record.native_session_id.as_deref(), Some("native"));
    assert_eq!(record.next_event_seq, 14);
    assert_eq!(record.capabilities_json, "{}");
    assert_eq!(record.child, None);
    assert_eq!(record.owner_boot_id, None);
    assert_eq!(
        store
            .commit_execution(&ExecutionCommit {
                run_id: "imported".into(),
                events: vec!["c".into()],
                ..Default::default()
            })
            .unwrap()
            .event_seqs,
        vec![14]
    );
    assert_eq!(
        store.execution_events_since("imported", 0).unwrap(),
        vec![(12, "a".into()), (13, "b".into()), (14, "c".into())]
    );
    let before = snapshot(&store);
    assert!(store
        .create_run_with_history(&run, None, 100, &["retry".into()], 0)
        .is_err());
    assert_eq!(snapshot(&store), before);
    store
        .create_run_with_history(&new_run("empty"), None, 5, &[], 0)
        .unwrap();
    assert_eq!(
        store
            .latest_run_for_task("registry-task")
            .unwrap()
            .unwrap()
            .next_event_seq,
        5
    );
}

#[test]
fn invalid_history_sequences_and_failed_event_insert_write_nothing() {
    let store = store();
    let before = snapshot(&store);
    for first_seq in [0, u64::MAX, i64::MAX as u64] {
        assert!(store
            .create_run_with_history(&new_run("bad"), None, first_seq, &["{}".into()], 0)
            .is_err());
        assert_eq!(snapshot(&store), before);
    }
    store
        .open()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_history BEFORE INSERT ON execution_events WHEN NEW.seq = 2
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
        )
        .unwrap();
    assert!(store
        .create_run_with_history(&new_run("bad"), None, 1, &["a".into(), "b".into()], 0)
        .is_err());
    assert_eq!(snapshot(&store), before);
}

#[test]
fn unfinished_tasks_include_latest_checkpoint_prompt_and_recovery_decision() {
    let store = store();
    store
        .commit_execution(&ExecutionCommit {
            new_tasks: vec![
                new_task("queued"),
                new_task("dispatched"),
                new_task("unknown"),
                new_task("closed"),
                new_task("interrupted"),
                new_task("cancelled"),
                new_task("completed"),
                new_task("failed"),
            ],
            prompts: vec![prompt("dispatched"), prompt("queued"), prompt("closed")],
            transitions: vec![
                to("dispatched", State::Running),
                to("closed", State::Running),
                to("interrupted", State::Running),
                to("interrupted", State::Interrupted),
                to("cancelled", State::Cancelled),
                to("completed", State::Running),
                to("completed", State::Completing),
                to("completed", State::Completed),
                to("failed", State::Running),
                to("failed", State::Failed),
            ],
            checkpoint_intents: vec![intent("dispatched"), intent("unknown"), intent("closed")],
            cancel_requests: vec!["closed".into()],
            ..commit()
        })
        .unwrap();
    store
        .commit_execution(&ExecutionCommit {
            checkpoint_results: vec![CheckpointResult {
                task_exec_id: "dispatched".into(),
                result_json: "{}".into(),
            }],
            checkpoint_intents: vec![intent("dispatched"), intent("closed")],
            ..commit()
        })
        .unwrap();
    store
        .commit_execution(&ExecutionCommit {
            checkpoint_results: vec![CheckpointResult {
                task_exec_id: "closed".into(),
                result_json: "null".into(),
            }],
            ..commit()
        })
        .unwrap();
    store.open().unwrap().execute_batch(
        "UPDATE execution_checkpoints SET operation = 'future_operation', replay_policy = 'future_policy',
         schema_version = 7 WHERE task_exec_id = 'unknown';"
    ).unwrap();
    store.create_run(&new_run("a-run")).unwrap();
    store
        .commit_execution(&ExecutionCommit {
            run_id: "a-run".into(),
            new_tasks: vec![
                new_task("parent"),
                NewExecutionTask {
                    parent_id: Some("parent".into()),
                    kind: "child".into(),
                    ..new_task("child")
                },
            ],
            ..Default::default()
        })
        .unwrap();
    let tasks = store.unfinished_executions().unwrap();
    assert_eq!(
        tasks
            .iter()
            .map(|t| t.task_exec_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "parent",
            "child",
            "dispatched",
            "queued",
            "closed",
            "unknown",
            "interrupted"
        ]
    );
    assert_eq!(tasks[1].parent_id.as_deref(), Some("parent"));
    assert_eq!(tasks[1].kind, "child");
    assert_eq!(tasks[1].run_id, "a-run");
    assert_eq!(tasks[0].prompt, None);
    let dispatched = &tasks[2];
    assert_eq!(dispatched.state, State::Running);
    assert!(!dispatched.cancel_requested);
    assert_eq!(dispatched.prompt, Some(("dispatched".into(), "{}".into())));
    let checkpoint = dispatched.checkpoint.unwrap();
    assert_eq!(checkpoint.operation, Some(Operation::PromptDispatch));
    assert_eq!(checkpoint.policy, ReplayPolicy::Unsafe);
    assert_eq!(checkpoint.schema_version, CHECKPOINT_SCHEMA_VERSION);
    assert!(!checkpoint.has_result);
    assert_eq!(
        dispatched.checkpoint_intent_json.as_deref(),
        Some("{\"prompt\":1}")
    );
    assert_eq!(
        dispatched.external_operation_id.as_deref(),
        Some("external")
    );
    assert_eq!(
        resolve_recovery(&checkpoint, dispatched.cancel_requested),
        RecoveryAction::Interrupt(InterruptReason::UnsafeAmbiguous)
    );
    assert_eq!(tasks[3].state, State::Pending);
    assert_eq!(tasks[3].checkpoint, None);
    assert_eq!(tasks[3].checkpoint_intent_json, None);
    assert_eq!(tasks[3].external_operation_id, None);
    assert!(tasks[4].checkpoint.unwrap().has_result);
    assert!(tasks[4].cancel_requested);
    let unknown = tasks[5].checkpoint.unwrap();
    assert_eq!(unknown.operation, None);
    assert_eq!(unknown.policy, ReplayPolicy::Unsafe);
    assert_eq!(unknown.schema_version, 7);
    assert!(!unknown.has_result);
    assert_eq!(tasks[6].state, State::Interrupted);
}

#[test]
fn cancellation_is_atomic_and_cannot_target_another_run() {
    let store = store();
    store
        .commit_execution(&ExecutionCommit {
            new_tasks: vec![new_task("task")],
            transitions: vec![to("task", State::Running)],
            cancel_requests: vec!["task".into()],
            ..commit()
        })
        .unwrap();
    assert!(store.unfinished_executions().unwrap()[0].cancel_requested);
    store.create_run(&new_run("other")).unwrap();
    let before = snapshot(&store);
    for (run_id, target) in [("run", "missing"), ("other", "task")] {
        let error = store
            .commit_execution(&ExecutionCommit {
                run_id: run_id.into(),
                new_tasks: vec![new_task("new")],
                transitions: vec![to("new", State::Running)],
                cancel_requests: vec!["new".into(), target.into()],
                events: vec!["{}".into()],
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(error, ExecutionCommitError::UnknownTask(id) if id == target));
        assert_eq!(snapshot(&store), before);
    }
    store
        .commit_execution(&ExecutionCommit {
            new_tasks: vec![new_task("pending")],
            ..commit()
        })
        .unwrap();
    let before = snapshot(&store);
    assert!(store
        .commit_execution(&ExecutionCommit {
            cancel_requests: vec!["pending".into()],
            checkpoint_results: vec![CheckpointResult {
                task_exec_id: "missing".into(),
                result_json: "{}".into()
            }],
            ..commit()
        })
        .is_err());
    assert_eq!(snapshot(&store), before);
    assert!(store
        .commit_execution(&ExecutionCommit {
            cancel_requests: vec!["pending".into()],
            checkpoint_intents: vec![intent("missing")],
            ..commit()
        })
        .is_err());
    assert_eq!(snapshot(&store), before);
}

#[test]
fn pruning_preserves_other_runs_and_monotonic_sequences() {
    let store = store();
    store
        .create_run_with_history(&new_run("other"), None, 1, &["other".into()], 0)
        .unwrap();
    store
        .commit_execution(&ExecutionCommit {
            events: vec!["a".into(), "b".into(), "c".into()],
            ..commit()
        })
        .unwrap();
    assert_eq!(store.prune_execution_events("run", 2).unwrap(), 1);
    assert_eq!(
        store.execution_events_since("run", 0).unwrap(),
        vec![(2, "b".into()), (3, "c".into())]
    );
    assert_eq!(store.prune_execution_events("run", u64::MAX).unwrap(), 0);
    assert_eq!(store.prune_execution_events("missing", 0).unwrap(), 0);
    assert_eq!(
        store
            .commit_execution(&ExecutionCommit {
                events: vec!["d".into()],
                ..commit()
            })
            .unwrap()
            .event_seqs,
        vec![4]
    );
    assert_eq!(store.prune_execution_events("run", 0).unwrap(), 3);
    assert!(store.execution_events_since("run", 0).unwrap().is_empty());
    assert_eq!(
        store
            .commit_execution(&ExecutionCommit {
                events: vec!["e".into()],
                ..commit()
            })
            .unwrap()
            .event_seqs,
        vec![5]
    );
    assert_eq!(
        store.execution_events_since("other", 0).unwrap(),
        vec![(1, "other".into())]
    );
}

#[test]
fn deletion_removes_all_execution_rows_for_only_the_selected_task_atomically() {
    let store = store();
    let task = task("registry-task", "repo", "handle");
    store
        .save(&InMemoryRegistry {
            tasks: [(task.id.clone(), task)].into_iter().collect(),
            events: Vec::new(),
            step_receipts: Default::default(),
        })
        .unwrap();
    let registry_rows = rows(&store, "SELECT * FROM registry_tasks");
    store.create_run(&new_run("second")).unwrap();
    store
        .create_run(&NewRun {
            task_id: "other-task".into(),
            ..new_run("other")
        })
        .unwrap();
    for run_id in ["run", "second", "other"] {
        store
            .commit_execution(&ExecutionCommit {
                run_id: run_id.into(),
                new_tasks: vec![new_task(run_id)],
                prompts: vec![prompt(run_id)],
                checkpoint_intents: vec![intent(run_id)],
                events: vec!["{}".into()],
                ..Default::default()
            })
            .unwrap();
    }
    let before = snapshot(&store);
    store
        .open()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_delete BEFORE DELETE ON agent_runs
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
        )
        .unwrap();
    assert!(store.delete_runs_for_task("registry-task").is_err());
    assert_eq!(snapshot(&store), before);
    store
        .open()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_delete")
        .unwrap();
    let expected: Vec<_> = before
        .into_iter()
        .map(|rows| {
            rows.into_iter()
                .filter(|row| row.contains(&Value::Text("other".into())))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(store.delete_runs_for_task("registry-task").unwrap(), 2);
    assert_eq!(snapshot(&store), expected);
    assert_eq!(rows(&store, "SELECT * FROM registry_tasks"), registry_rows);
    assert_eq!(store.delete_runs_for_task("registry-task").unwrap(), 0);
    assert_eq!(store.delete_runs_for_task("other-task").unwrap(), 1);
    assert!(snapshot(&store).iter().all(Vec::is_empty));
    assert_eq!(rows(&store, "SELECT * FROM registry_tasks"), registry_rows);
}

#[test]
fn owner_fence_takeover_rejects_old_owner_without_writes() {
    let store_a = store().with_owner_fence("A");
    let store_b = SqliteRegistryStore::new(&store_a.path).with_owner_fence("B");
    assert_eq!(
        store_a.claim_execution_owner("A", 10, 100, 100).unwrap(),
        OwnerClaim::Acquired { previous: None }
    );
    store_a
        .commit_execution(&ExecutionCommit {
            events: vec!["before takeover".into()],
            ..commit()
        })
        .unwrap();
    assert_eq!(
        store_b.claim_execution_owner("B", 20, 201, 100).unwrap(),
        OwnerClaim::Acquired {
            previous: Some(("A".into(), 10))
        }
    );
    let change = ExecutionCommit {
        new_tasks: vec![new_task("new")],
        prompts: vec![prompt("new")],
        transitions: vec![to("new", State::Running)],
        checkpoint_intents: vec![intent("new")],
        run_update: Some(RunUpdate {
            model: Some("changed".into()),
            ..Default::default()
        }),
        events: vec!["after takeover".into()],
        ..commit()
    };
    let before = snapshot(&store_a);
    let error = store_a.commit_execution(&change).unwrap_err();
    assert!(matches!(error, ExecutionCommitError::Fenced));
    assert_eq!(
        error.to_string(),
        "execution fenced: this host no longer owns the execution lease"
    );
    assert_eq!(snapshot(&store_a), before);
    assert_eq!(
        store_a.execution_events_since("run", 0).unwrap(),
        vec![(1, "before takeover".into())]
    );
    assert_eq!(
        store_a
            .latest_run_for_task("registry-task")
            .unwrap()
            .unwrap()
            .next_event_seq,
        2
    );
    assert_eq!(
        store_b.commit_execution(&change).unwrap().event_seqs,
        vec![2]
    );
    assert_eq!(
        store_b.execution_events_since("run", 0).unwrap(),
        vec![(1, "before takeover".into()), (2, "after takeover".into())]
    );
}

#[test]
fn owner_fence_absent_preserves_unfenced_commits() {
    let store = store();
    for (index, owner) in [None, Some("A"), Some("B")].into_iter().enumerate() {
        if let Some(owner) = owner {
            assert!(matches!(
                store
                    .claim_execution_owner(owner, 10, index as i64 * 101, 100)
                    .unwrap(),
                OwnerClaim::Acquired { .. }
            ));
        }
        assert_eq!(
            store
                .commit_execution(&ExecutionCommit {
                    events: vec!["unfenced".into()],
                    ..commit()
                })
                .unwrap()
                .event_seqs,
            vec![index as u64 + 1]
        );
    }
}

#[test]
fn owner_fence_release_rejects_old_owner_without_writes() {
    let store = store();
    assert_eq!(
        store.claim_execution_owner("A", 10, 100, 100).unwrap(),
        OwnerClaim::Acquired { previous: None }
    );
    let change = ExecutionCommit {
        owner_fence: Some("A".into()),
        events: vec!["owned".into()],
        ..commit()
    };
    store.commit_execution(&change).unwrap();
    store.release_execution_owner("A").unwrap();
    let before = snapshot(&store);
    assert!(matches!(
        store.commit_execution(&change),
        Err(ExecutionCommitError::Fenced)
    ));
    assert_eq!(snapshot(&store), before);
}

#[test]
fn owner_fence_commit_overrides_store_default() {
    let store = store().with_owner_fence("A");
    assert_eq!(
        store.claim_execution_owner("B", 20, 100, 100).unwrap(),
        OwnerClaim::Acquired { previous: None }
    );
    let change = ExecutionCommit {
        owner_fence: Some("B".into()),
        events: vec!["override".into()],
        ..commit()
    };
    assert_eq!(store.commit_execution(&change).unwrap().event_seqs, vec![1]);
    let store = store.with_owner_fence("B");
    let before = snapshot(&store);
    assert!(matches!(
        store.commit_execution(&ExecutionCommit {
            owner_fence: Some("A".into()),
            ..change
        }),
        Err(ExecutionCommitError::Fenced)
    ));
    assert_eq!(snapshot(&store), before);
}

#[test]
fn owner_lease_refresh_refusal_takeover_and_release() {
    let store = store();
    let claim = |boot, pid, now| store.claim_execution_owner(boot, pid, now, 100).unwrap();
    assert!(!store.heartbeat_execution_owner("a", 0).unwrap());
    assert_eq!(claim("a", 10, 100), OwnerClaim::Acquired { previous: None });
    assert_eq!(claim("a", 10, 200), OwnerClaim::Acquired { previous: None });
    let before = rows(&store, "SELECT * FROM execution_owner");
    assert_eq!(
        claim("b", 20, 299),
        OwnerClaim::HeldBy {
            boot_id: "a".into(),
            pid: 10
        }
    );
    assert_eq!(
        claim("b", 20, 300),
        OwnerClaim::HeldBy {
            boot_id: "a".into(),
            pid: 10
        }
    );
    assert_eq!(rows(&store, "SELECT * FROM execution_owner"), before);
    assert!(store.heartbeat_execution_owner("a", 250).unwrap());
    assert_eq!(
        claim("b", 20, 301),
        OwnerClaim::HeldBy {
            boot_id: "a".into(),
            pid: 10
        }
    );
    assert_eq!(
        claim("b", 20, 351),
        OwnerClaim::Acquired {
            previous: Some(("a".into(), 10))
        }
    );
    let before = rows(&store, "SELECT * FROM execution_owner");
    assert!(!store.heartbeat_execution_owner("a", 500).unwrap());
    store.release_execution_owner("a").unwrap();
    assert_eq!(rows(&store, "SELECT * FROM execution_owner"), before);
    assert_eq!(
        claim("c", 30, 400),
        OwnerClaim::HeldBy {
            boot_id: "b".into(),
            pid: 20
        }
    );
    store.release_execution_owner("b").unwrap();
    assert!(rows(&store, "SELECT * FROM execution_owner").is_empty());
    assert!(!store.heartbeat_execution_owner("b", 500).unwrap());
    assert_eq!(claim("c", 30, 500), OwnerClaim::Acquired { previous: None });
}
