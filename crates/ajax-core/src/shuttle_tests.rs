//! Strict Shuttle harness: concurrent durable substrate updates vs stale live.
//!
//! Explores interleavings across tmux/window/git apply paths and stale
//! `LiveStatusKind::*Missing` writes. When durable facts end present, operator
//! actions and blockers must not claim missing substrate.

use crate::lifecycle::mark_active;
use crate::models::{
    AgentClient, GitStatus, LiveObservation, LiveStatusKind, OperatorAction, SideFlag, Task,
    TaskId, TaskWindowStatus, TmuxStatus,
};
use crate::recommended::{available_operator_actions, operator_action, primary_blocker_reason};
use crate::registry::{InMemoryRegistry, Registry};
use shuttle::sync::{Arc, Mutex};
use shuttle::thread;

fn present_git() -> GitStatus {
    GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/handle".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: Some("abc123".to_string()),
    }
}

fn seeded_task() -> Task {
    let mut task = Task::new(
        TaskId::new("shuttle-task"),
        "repo",
        "handle",
        "title",
        "ajax/handle",
        "main",
        "/tmp/worktrees/repo-handle",
        "ajax-repo-handle",
        "task",
        AgentClient::Codex,
    );
    mark_active(&mut task).expect("Created -> Active");
    task.apply_git_status(present_git());
    task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
    task.apply_task_window_status(Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/repo-handle",
    )));
    task
}

fn durable_all_present(task: &Task) -> bool {
    task.tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
        && !task.has_side_flag(SideFlag::TmuxMissing)
        && task
            .task_window_status
            .as_ref()
            .is_some_and(|status| status.exists && status.points_at_expected_path)
        && !task.has_side_flag(SideFlag::TaskWindowMissing)
        && task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists && status.branch_exists)
        && !task.has_side_flag(SideFlag::WorktreeMissing)
        && !task.has_side_flag(SideFlag::BranchMissing)
}

fn assert_durable_present_is_operable(task: &Task) {
    assert!(
        durable_all_present(task),
        "postcondition requires durable substrate present"
    );
    assert!(
        !task.has_missing_substrate(),
        "durable present must not report missing substrate; live={:?}",
        task.live_status.as_ref().map(|live| live.kind)
    );
    if let Some(blocker) = primary_blocker_reason(task) {
        assert!(
            !matches!(
                blocker,
                "tmux session missing" | "task window missing" | "worktree missing"
            ),
            "blocker={blocker}; live={:?}",
            task.live_status.as_ref().map(|live| live.kind)
        );
    }
    let actions = available_operator_actions(task);
    let plan = operator_action(task);
    assert_ne!(
        actions,
        vec![OperatorAction::Drop],
        "issue #788-class: durable present must not collapse to Drop-only; live={:?} actions={actions:?}",
        task.live_status.as_ref().map(|live| live.kind),
    );
    assert!(
        actions.contains(&OperatorAction::Resume),
        "expected Resume; actions={actions:?}"
    );
    assert_ne!(plan.action, OperatorAction::Drop, "{plan:?}");
    assert_ne!(plan.reason, "invalid_task", "{plan:?}");
}

#[test]
fn concurrent_stale_live_missing_vs_durable_applies_stay_operable() {
    shuttle::check_random(
        || {
            let mut registry = InMemoryRegistry::default();
            registry
                .create_task(seeded_task())
                .expect("create sample task");
            let registry = Arc::new(Mutex::new(registry));
            let task_id = TaskId::new("shuttle-task");

            let mut handles = Vec::new();

            // Durable writers.
            for op in 0u8..3 {
                let registry = Arc::clone(&registry);
                let task_id = task_id.clone();
                handles.push(thread::spawn(move || {
                    let mut registry = registry.lock().unwrap();
                    let task = registry.get_task_mut(&task_id).expect("task");
                    match op {
                        0 => task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle"))),
                        1 => task.apply_task_window_status(Some(TaskWindowStatus::present(
                            "task",
                            "/tmp/worktrees/repo-handle",
                        ))),
                        _ => task.apply_git_status(present_git()),
                    }
                }));
            }

            // Stale live writers.
            for kind in [
                LiveStatusKind::TmuxMissing,
                LiveStatusKind::TaskWindowMissing,
                LiveStatusKind::WorktreeMissing,
            ] {
                let registry = Arc::clone(&registry);
                let task_id = task_id.clone();
                handles.push(thread::spawn(move || {
                    let mut registry = registry.lock().unwrap();
                    let task = registry.get_task_mut(&task_id).expect("task");
                    task.live_status = Some(LiveObservation::new(kind, "stale missing"));
                }));
            }

            for handle in handles {
                handle.join().unwrap();
            }

            // Force durable present so the oracle targets stale-live false positives.
            {
                let mut registry = registry.lock().unwrap();
                let task = registry.get_task_mut(&task_id).expect("task");
                task.apply_git_status(present_git());
                task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
                task.apply_task_window_status(Some(TaskWindowStatus::present(
                    "task",
                    "/tmp/worktrees/repo-handle",
                )));
            }

            let registry = registry.lock().unwrap();
            let task = registry.get_task(&task_id).expect("task remains");
            assert_durable_present_is_operable(task);
        },
        2000,
    );
}

#[test]
fn concurrent_apply_tmux_present_clears_stale_live_tmux_missing() {
    shuttle::check_random(
        || {
            let task = Arc::new(Mutex::new({
                let mut task = seeded_task();
                task.live_status = Some(LiveObservation::new(LiveStatusKind::TmuxMissing, "stale"));
                task
            }));

            let clearer = {
                let task = Arc::clone(&task);
                thread::spawn(move || {
                    let mut task = task.lock().unwrap();
                    task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
                })
            };
            let staler = {
                let task = Arc::clone(&task);
                thread::spawn(move || {
                    let mut task = task.lock().unwrap();
                    task.live_status = Some(LiveObservation::new(
                        LiveStatusKind::TmuxMissing,
                        "stale again",
                    ));
                })
            };
            clearer.join().unwrap();
            staler.join().unwrap();

            let mut task = task.lock().unwrap();
            task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
            assert!(
                !matches!(
                    task.live_status.as_ref().map(|live| live.kind),
                    Some(LiveStatusKind::TmuxMissing)
                ),
                "apply_tmux_status(present) must clear stale live TmuxMissing; live={:?}",
                task.live_status
            );
            assert_durable_present_is_operable(&task);
        },
        1000,
    );
}
