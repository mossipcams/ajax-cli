use super::execution_recovery::rows;
use super::{task, InMemoryRegistry};
use crate::registry::{ExecutionCommit, NewExecutionTask, NewPrompt, NewRun, SqliteRegistryStore};

fn run(id: &str, task_id: &str) -> NewRun {
    NewRun {
        run_id: id.into(),
        task_id: task_id.into(),
        harness: "codex".into(),
        model: "model".into(),
        created_unix_ms: 1234,
    }
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "ajax-execution-drop-{}-{name}-{nonce}.sqlite",
        std::process::id()
    ))
}

fn registry(tasks: &[&str]) -> InMemoryRegistry {
    let tasks = tasks
        .iter()
        .map(|id| {
            let handle = id.rsplit('/').next().unwrap();
            task(id, "repo", handle)
        })
        .collect::<Vec<_>>();
    InMemoryRegistry {
        tasks: tasks.into_iter().map(|t| (t.id.clone(), t)).collect(),
        events: Vec::new(),
        step_receipts: Default::default(),
    }
}

fn seed_chat_run(store: &SqliteRegistryStore, run_id: &str, task_id: &str, native: &str) {
    store
        .create_run_with_history(
            &run(run_id, task_id),
            Some(native),
            1,
            &["transcript event".into()],
            0,
        )
        .unwrap();
    store
        .commit_execution(&ExecutionCommit {
            run_id: run_id.into(),
            new_tasks: vec![NewExecutionTask {
                task_exec_id: format!("{run_id}-exec"),
                parent_id: None,
                kind: "prompt".into(),
            }],
            prompts: vec![NewPrompt {
                idempotency_key: format!("{run_id}-key"),
                task_exec_id: format!("{run_id}-exec"),
                content_json: "{}".into(),
            }],
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn recreated_task_with_same_handle_starts_with_no_chat_run_after_cli_drop() {
    let store = SqliteRegistryStore::new(temp_path("recreate"));
    store
        .save(&registry(&["repo/handle", "repo/other"]))
        .unwrap();
    seed_chat_run(&store, "run-a", "repo/handle", "acp-a");
    seed_chat_run(&store, "run-b", "repo/other", "acp-b");
    let other_run = store.latest_run_for_task("repo/other").unwrap();
    let other_events = rows(
        &store,
        "SELECT * FROM execution_events WHERE run_id = 'run-b' ORDER BY rowid",
    );
    let other_prompts = store.prompts_for_run("run-b").unwrap();
    assert!(store.latest_run_for_task("repo/handle").unwrap().is_some());
    assert!(!other_events.is_empty());
    assert_eq!(other_prompts.len(), 1);

    // CLI drop: the task leaves the registry and the whole registry is saved.
    store.save(&registry(&["repo/other"])).unwrap();
    // Recreate a task with the same handle.
    store
        .save(&registry(&["repo/handle", "repo/other"]))
        .unwrap();

    assert_eq!(store.latest_run_for_task("repo/handle").unwrap(), None);
    for table in [
        "agent_runs",
        "execution_events",
        "prompt_submissions",
        "execution_tasks",
    ] {
        assert_eq!(
            rows(
                &store,
                &format!("SELECT * FROM {table} WHERE run_id = 'run-a'")
            ),
            Vec::<Vec<_>>::new(),
            "{table} still holds the dropped task's run"
        );
    }
    assert_eq!(store.latest_run_for_task("repo/other").unwrap(), other_run);
    assert_eq!(
        rows(
            &store,
            "SELECT * FROM execution_events WHERE run_id = 'run-b' ORDER BY rowid",
        ),
        other_events
    );
    assert_eq!(store.prompts_for_run("run-b").unwrap(), other_prompts);
}
