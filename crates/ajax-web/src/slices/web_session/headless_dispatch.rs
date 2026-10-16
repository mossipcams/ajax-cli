//! Headless dispatch of a chat's queued prompts with no browser attached: the
//! companion acquires each stored session (resuming its native session and
//! failing closed), waits until every prompt is done, then releases it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ajax_core::models::AgentClient;

use super::TaskSessionDirectory;

/// One chat whose queued prompts should run with no browser attached.
pub(crate) struct HeadlessTarget {
    pub handle: String,
    pub worktree_path: PathBuf,
    pub model: String,
    pub agent: AgentClient,
}

/// Dispatches one chat's queued prompts with no browser attached: acquires the
/// session (resuming the stored native session and failing closed), waits until
/// its prompts are done, then releases it. Returns `Err(reason)` when the
/// session cannot be acquired (the prompts stay queued) or the deadline passes.
pub(crate) async fn run_queued_prompts(
    directory: &Arc<TaskSessionDirectory>,
    target: &HeadlessTarget,
    max_wait: Duration,
) -> Result<(), String> {
    if let Err(error) = directory
        .acquire(
            &target.handle,
            &target.worktree_path,
            &target.model,
            target.agent,
        )
        .await
    {
        tracing::warn!(
            target: "ajax_web",
            handle = %target.handle,
            %error,
            "headless dispatch: could not acquire session"
        );
        return Err(error);
    }

    let deadline = Instant::now() + max_wait;
    let mut idle_polls = 0usize;
    let outcome = loop {
        let done = directory.unfinished_prompt_count(&target.handle) == 0
            && directory
                .attach_snapshot(&target.handle, target.model.clone(), None)
                .await
                .snapshot
                .turn_state
                != "busy";
        idle_polls = if done { idle_polls + 1 } else { 0 };
        if idle_polls >= 2 {
            break Ok(());
        }
        if Instant::now() >= deadline {
            break Err(format!("headless dispatch timed out for {}", target.handle));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    directory.release(&target.handle).await;
    outcome
}

/// Dispatches `targets` strictly one at a time in the given order (concurrency 1),
/// so one chat's failure never blocks or interleaves with another. Logs each
/// failure and returns the per-handle results in input order.
pub(crate) async fn run_all(
    directory: &Arc<TaskSessionDirectory>,
    targets: Vec<HeadlessTarget>,
    max_wait: Duration,
) -> Vec<(String, Result<(), String>)> {
    let mut results = Vec::with_capacity(targets.len());
    for target in &targets {
        let result = run_queued_prompts(directory, target, max_wait).await;
        if let Err(error) = &result {
            tracing::warn!(
                target: "ajax_web",
                handle = %target.handle,
                %error,
                "headless dispatch failed"
            );
        }
        results.push((target.handle.clone(), result));
    }
    results
}
