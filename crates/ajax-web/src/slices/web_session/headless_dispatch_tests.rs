//! End-to-end headless dispatch tests with the Claude fake: a foreign-owned
//! chat's queued prompts are dispatched without a browser attached. Each test
//! seeds its own scratch dir, runs on one shared multi-threaded tokio runtime
//! (the session actor needs worker threads), and bounds every wait by a
//! deadline so a bug fails the test instead of hanging it.

use std::path::Path;
use std::time::{Duration, Instant};

use ajax_core::models::AgentClient;
use ajax_core::registry::SqliteRegistryStore;

use crate::adapters::web_session_acp::with_test_claude_sdk_module;
use crate::adapters::web_session_store::{
    prompt_ledger, record_child_identity, try_load as load_transcript,
};

use super::headless_dispatch::{run_all, HeadlessTarget};
use super::test_support;
use super::{SessionStorage, TaskSessionDirectory};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Seed a foreign-owned run (`foreign-boot` owner) whose ledger holds one
/// `Queued` prompt, exactly as the startup scan expects to find.
fn seed_queued(dir: &Path, handle: &str, prompt_id: &str, prompt_text: &str) {
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    record_child_identity(&store, dir, handle, "claude", None, "foreign-boot")
        .expect("record child identity");
    let mut ledger = prompt_ledger::PromptLedger::default();
    ledger.upsert_queued(
        prompt_id.to_owned(),
        format!("transcript {prompt_id}"),
        prompt_text.to_owned(),
        Vec::new(),
    );
    prompt_ledger::try_persist(&store, dir, handle, &ledger).expect("persist ledger");
}

fn target(dir: &Path, handle: &str, agent: AgentClient) -> HeadlessTarget {
    HeadlessTarget {
        handle: handle.to_owned(),
        worktree_path: dir.join("wt"),
        model: "auto".to_owned(),
        agent,
    }
}

#[test]
fn startup_scan_lists_and_runner_dispatches_a_queued_prompt() {
    let dir = test_support::scratch_dir("headless-dispatch-startup");
    seed_queued(&dir, "web/queued", "p1", "hello");
    std::fs::create_dir_all(dir.join("wt")).expect("create worktree dir");

    let directory = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));
    assert_eq!(
        directory.startup_dispatch_handles(),
        Ok(vec!["web/queued".to_owned()]),
        "startup scan must list the foreign-owned handle with a queued prompt"
    );
    assert_eq!(
        directory.startup_dispatch_handles(),
        Ok(Vec::new()),
        "the startup scan runs once per process"
    );

    with_test_claude_sdk_module(
        &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
        || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build runtime");
            let results = rt.block_on(async {
                run_all(
                    &directory,
                    vec![target(&dir, "web/queued", AgentClient::Claude)],
                    Duration::from_secs(20),
                )
                .await
            });

            assert_eq!(results.len(), 1);
            let (handle, result) = &results[0];
            assert_eq!(handle, "web/queued");
            result.as_ref().expect("headless dispatch must succeed");
        },
    );

    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let ledger = prompt_ledger::try_load(&store, &dir, "web/queued").expect("load ledger");
    assert_eq!(
        ledger.entry("p1").map(|entry| entry.phase),
        Some(prompt_ledger::PromptPhase::Completed),
        "the dispatched prompt must be Completed"
    );

    let session = load_transcript::<serde_json::Value>(&store, &dir, "web/queued", "claude")
        .expect("load transcript");
    assert!(
        session.events.iter().any(|event| {
            event.get("type").and_then(serde_json::Value::as_str) == Some("message")
                && event.get("role").and_then(serde_json::Value::as_str) == Some("agent")
        }),
        "the transcript must contain an agent message"
    );
}

#[test]
fn a_failing_chat_does_not_block_the_next_one() {
    let dir = test_support::scratch_dir("headless-dispatch-failure-isolation");
    seed_queued(&dir, "web/a-fails", "p1", "first fails");
    seed_queued(&dir, "web/b-runs", "p1", "second runs");
    std::fs::create_dir_all(dir.join("wt")).expect("create worktree dir");

    let directory = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));

    with_test_claude_sdk_module(
        &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
        || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build runtime");
            // The first target's worktree directory does not exist, so its
            // Claude sidecar spawn fails fast; the second Claude target must
            // still run to completion, in input order. No real harness is ever
            // started.
            // target must still run to completion, in input order.
            let results = rt.block_on(async {
                run_all(
                    &directory,
                    vec![
                        HeadlessTarget {
                            handle: "web/a-fails".to_owned(),
                            worktree_path: dir.join("does-not-exist"),
                            model: "auto".to_owned(),
                            agent: AgentClient::Claude,
                        },
                        target(&dir, "web/b-runs", AgentClient::Claude),
                    ],
                    Duration::from_secs(20),
                )
                .await
            });

            assert_eq!(results.len(), 2);
            assert_eq!(results[0].0, "web/a-fails");
            assert!(
                results[0].1.is_err(),
                "the missing-worktree Claude target must fail: {:?}",
                results[0].1
            );
            assert_eq!(results[1].0, "web/b-runs");
            results[1]
                .1
                .as_ref()
                .expect("the Claude target must succeed after a failure");
        },
    );

    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let failed = prompt_ledger::try_load(&store, &dir, "web/a-fails").expect("load ledger");
    assert_eq!(
        failed.entry("p1").map(|entry| entry.phase),
        Some(prompt_ledger::PromptPhase::Queued),
        "a failed acquire must leave the prompt queued"
    );
    let ran = prompt_ledger::try_load(&store, &dir, "web/b-runs").expect("load ledger");
    assert_eq!(
        ran.entry("p1").map(|entry| entry.phase),
        Some(prompt_ledger::PromptPhase::Completed),
        "a later failure must not block the next chat"
    );
}

#[test]
fn a_turn_that_never_finishes_times_out_and_releases() {
    let dir = test_support::scratch_dir("headless-dispatch-timeout");
    seed_queued(&dir, "web/slow", "p1", "SLOW first");
    std::fs::create_dir_all(dir.join("wt")).expect("create worktree dir");

    let directory = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));

    with_test_claude_sdk_module(
        &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
        || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build runtime");
            // The fake streams until interrupted, so the turn never finishes:
            // the runner must hit its 1 s deadline and give up.
            let (results, elapsed) = rt.block_on(async {
                let started = Instant::now();
                let results = run_all(
                    &directory,
                    vec![target(&dir, "web/slow", AgentClient::Claude)],
                    Duration::from_secs(1),
                )
                .await;
                (results, started.elapsed())
            });

            assert_eq!(results.len(), 1);
            let Err(error) = results[0].1.as_ref() else {
                panic!("a never-finishing turn must time out");
            };
            assert!(
                error.contains("timed out"),
                "the timeout reason must say it timed out: {error}"
            );
            assert!(
                elapsed < Duration::from_secs(10),
                "run_all must return well before the 10 s budget, took {elapsed:?}"
            );
        },
    );

    // The prompt was never dispatched to completion; it stays queued so a
    // later browser attach can pick it up.
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let ledger = prompt_ledger::try_load(&store, &dir, "web/slow").expect("load ledger");
    assert_ne!(
        ledger.entry("p1").map(|entry| entry.phase),
        Some(prompt_ledger::PromptPhase::Completed),
        "a timed-out turn must not complete its prompt"
    );
}
