//! Strict state-machine / property oracles for lifecycle + substrate truth.
//!
//! Durable facts (tmux/git/window status + side flags) must win over stale
//! `LiveStatusKind::*Missing` observations for operator actions and blockers.

use crate::lifecycle::{
    hydrate_lifecycle_status, mark_active, transition_lifecycle, validate_lifecycle_transition,
    LifecycleTransitionReason,
};
use crate::models::{
    AgentClient, GitStatus, LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction,
    SideFlag, Task, TaskId, TaskWindowStatus, TmuxStatus,
};
use crate::recommended::{available_operator_actions, operator_action, primary_blocker_reason};
use proptest::prelude::*;

fn sample_task() -> Task {
    Task::new(
        TaskId::new("fsm-task"),
        "repo",
        "handle",
        "title",
        "ajax/handle",
        "main",
        "/tmp/worktrees/repo-handle",
        "ajax-repo-handle",
        "task",
        AgentClient::Codex,
    )
}

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

fn healthy_active_task() -> Task {
    let mut task = sample_task();
    mark_active(&mut task).expect("Created -> Active");
    task.apply_git_status(present_git());
    task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
    task.apply_task_window_status(Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/repo-handle",
    )));
    task
}

fn lifecycle_status_strategy() -> impl Strategy<Value = LifecycleStatus> {
    prop_oneof![
        Just(LifecycleStatus::Created),
        Just(LifecycleStatus::Provisioning),
        Just(LifecycleStatus::Active),
        Just(LifecycleStatus::Waiting),
        Just(LifecycleStatus::Reviewable),
        Just(LifecycleStatus::Mergeable),
        Just(LifecycleStatus::Merged),
        Just(LifecycleStatus::Cleanable),
        Just(LifecycleStatus::Removing),
        Just(LifecycleStatus::TeardownIncomplete),
        Just(LifecycleStatus::Removed),
        Just(LifecycleStatus::Orphaned),
        Just(LifecycleStatus::Error),
    ]
}

fn transition_reason_strategy() -> impl Strategy<Value = LifecycleTransitionReason> {
    prop_oneof![
        Just(LifecycleTransitionReason::Generic),
        Just(LifecycleTransitionReason::Recovery),
        Just(LifecycleTransitionReason::OperationResult),
        Just(LifecycleTransitionReason::ForceRemove),
        Just(LifecycleTransitionReason::Restore),
    ]
}

#[derive(Clone, Copy, Debug)]
enum SubstrateOp {
    TmuxPresent,
    TmuxMissing,
    WindowPresent,
    WindowMissing,
    GitPresent,
    GitWorktreeMissing,
    LiveTmuxMissing,
    LiveWindowMissing,
    LiveWorktreeMissing,
    LiveShellIdle,
    LiveAgentRunning,
    ClearLive,
}

fn substrate_op_strategy() -> impl Strategy<Value = SubstrateOp> {
    prop_oneof![
        Just(SubstrateOp::TmuxPresent),
        Just(SubstrateOp::TmuxMissing),
        Just(SubstrateOp::WindowPresent),
        Just(SubstrateOp::WindowMissing),
        Just(SubstrateOp::GitPresent),
        Just(SubstrateOp::GitWorktreeMissing),
        Just(SubstrateOp::LiveTmuxMissing),
        Just(SubstrateOp::LiveWindowMissing),
        Just(SubstrateOp::LiveWorktreeMissing),
        Just(SubstrateOp::LiveShellIdle),
        Just(SubstrateOp::LiveAgentRunning),
        Just(SubstrateOp::ClearLive),
    ]
}

fn apply_substrate_op(task: &mut Task, op: SubstrateOp) {
    match op {
        SubstrateOp::TmuxPresent => {
            task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));
        }
        SubstrateOp::TmuxMissing => {
            task.apply_tmux_status(Some(TmuxStatus {
                exists: false,
                session_name: "ajax-repo-handle".to_string(),
            }));
        }
        SubstrateOp::WindowPresent => {
            task.apply_task_window_status(Some(TaskWindowStatus::present(
                "task",
                "/tmp/worktrees/repo-handle",
            )));
        }
        SubstrateOp::WindowMissing => {
            task.apply_task_window_status(Some(TaskWindowStatus::missing(
                "task",
                "/tmp/worktrees/repo-handle",
            )));
        }
        SubstrateOp::GitPresent => {
            task.apply_git_status(present_git());
        }
        SubstrateOp::GitWorktreeMissing => {
            let mut git = present_git();
            git.worktree_exists = false;
            task.apply_git_status(git);
        }
        SubstrateOp::LiveTmuxMissing => {
            task.live_status = Some(LiveObservation::new(
                LiveStatusKind::TmuxMissing,
                "tmux session missing",
            ));
        }
        SubstrateOp::LiveWindowMissing => {
            task.live_status = Some(LiveObservation::new(
                LiveStatusKind::TaskWindowMissing,
                "task window missing",
            ));
        }
        SubstrateOp::LiveWorktreeMissing => {
            task.live_status = Some(LiveObservation::new(
                LiveStatusKind::WorktreeMissing,
                "worktree missing",
            ));
        }
        SubstrateOp::LiveShellIdle => {
            task.live_status = Some(LiveObservation::new(LiveStatusKind::ShellIdle, "idle"));
        }
        SubstrateOp::LiveAgentRunning => {
            task.live_status = Some(LiveObservation::new(
                LiveStatusKind::AgentRunning,
                "agent running",
            ));
        }
        SubstrateOp::ClearLive => {
            task.live_status = None;
        }
    }
}

fn durable_all_present(task: &Task) -> bool {
    durable_tmux_present(task) && durable_window_present(task) && durable_git_present(task)
}

fn durable_tmux_present(task: &Task) -> bool {
    task.tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
        && !task.has_side_flag(SideFlag::TmuxMissing)
}

fn durable_window_present(task: &Task) -> bool {
    task.task_window_status
        .as_ref()
        .is_some_and(|status| status.exists && status.points_at_expected_path)
        && !task.has_side_flag(SideFlag::TaskWindowMissing)
}

fn durable_git_present(task: &Task) -> bool {
    task.git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists && status.branch_exists)
        && !task.has_side_flag(SideFlag::WorktreeMissing)
        && !task.has_side_flag(SideFlag::BranchMissing)
}

fn assert_not_drop_only_when_durable_present(task: &Task) -> Result<(), TestCaseError> {
    let actions = available_operator_actions(task);
    let plan = operator_action(task);
    let live = task.live_status.as_ref().map(|live| live.kind);
    let blocker = primary_blocker_reason(task);

    prop_assert_ne!(
        &actions,
        &vec![OperatorAction::Drop],
        "durable substrate present but actions={:?}; live={:?}; blocker={:?}; plan={:?}",
        actions,
        live,
        blocker,
        plan,
    );
    prop_assert!(
        actions.contains(&OperatorAction::Resume),
        "expected Resume when durable substrate is present; actions={:?}; live={:?}",
        actions,
        live,
    );
    prop_assert_ne!(
        plan.action,
        OperatorAction::Drop,
        "primary action must not be Drop when durable substrate is present; plan={:?}; live={:?}",
        plan,
        live,
    );
    prop_assert_ne!(
        plan.reason.as_str(),
        "invalid_task",
        "must not classify as invalid_task when durable substrate is present; plan={:?}; live={:?}",
        plan,
        live,
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn lifecycle_transition_sequences_match_validator(
        start in lifecycle_status_strategy(),
        ops in prop::collection::vec(
            (lifecycle_status_strategy(), transition_reason_strategy()),
            0..64,
        ),
    ) {
        let mut task = sample_task();
        hydrate_lifecycle_status(&mut task, start);

        for (to, reason) in ops {
            let from = task.lifecycle_status;
            let expected = validate_lifecycle_transition(from, to, reason);
            let applied = transition_lifecycle(&mut task, to, reason);

            match (expected, applied) {
                (Ok(()), Ok(())) => {
                    prop_assert_eq!(task.lifecycle_status, to);
                }
                (Err(expected_err), Err(applied_err)) => {
                    prop_assert_eq!(expected_err, applied_err);
                    prop_assert_eq!(task.lifecycle_status, from);
                }
                (Ok(()), Err(err)) => {
                    return Err(TestCaseError::fail(format!(
                        "validator allowed {from:?} -> {to:?} ({reason:?}) but transition failed: {err:?}"
                    )));
                }
                (Err(err), Ok(())) => {
                    return Err(TestCaseError::fail(format!(
                        "validator rejected {from:?} -> {to:?} ({reason:?}) as {err:?} but transition mutated"
                    )));
                }
            }
        }
    }

    #[test]
    fn durable_substrate_facts_win_over_stale_live_missing(
        ops in prop::collection::vec(substrate_op_strategy(), 1..48),
    ) {
        let mut task = healthy_active_task();

        for op in ops {
            apply_substrate_op(&mut task, op);
            if !durable_all_present(&task) {
                continue;
            }

            // Stale live *Missing must not invent missing substrate against durable facts.
            prop_assert!(
                !task.has_missing_substrate(),
                "has_missing_substrate true despite durable present; live={:?}",
                task.live_status.as_ref().map(|live| live.kind),
            );
            prop_assert!(
                !task.has_missing_worktree(),
                "has_missing_worktree true despite durable git present; live={:?}",
                task.live_status.as_ref().map(|live| live.kind),
            );

            if let Some(blocker) = primary_blocker_reason(&task) {
                prop_assert!(
                    !matches!(
                        blocker,
                        "tmux session missing" | "task window missing" | "worktree missing"
                    ),
                    "blocker claims missing substrate while durable facts are present: {blocker}; live={:?}",
                    task.live_status.as_ref().map(|live| live.kind),
                );
            }

            assert_not_drop_only_when_durable_present(&task)?;
        }
    }
}

#[test]
fn stale_live_tmux_missing_with_present_session_is_not_drop_only() {
    let mut task = healthy_active_task();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::TmuxMissing,
        "stale tmux missing",
    ));

    assert!(durable_all_present(&task));
    assert!(
        !task.has_missing_substrate(),
        "durable session present must not report missing substrate"
    );
    assert_ne!(
        primary_blocker_reason(&task),
        Some("tmux session missing"),
        "blocker must not claim tmux missing when session exists"
    );

    let actions = available_operator_actions(&task);
    let plan = operator_action(&task);
    assert_ne!(
        actions,
        vec![OperatorAction::Drop],
        "issue #788: present session + stale live TmuxMissing must not be Drop-only; got {actions:?}"
    );
    assert!(actions.contains(&OperatorAction::Resume), "{actions:?}");
    assert_ne!(plan.action, OperatorAction::Drop, "{plan:?}");
    assert_ne!(plan.reason, "invalid_task", "{plan:?}");
}

#[test]
fn stale_live_task_window_missing_with_present_window_is_not_drop_only() {
    let mut task = healthy_active_task();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::TaskWindowMissing,
        "stale task window missing",
    ));

    assert!(durable_all_present(&task));
    assert!(!task.has_missing_substrate());
    assert_ne!(primary_blocker_reason(&task), Some("task window missing"));

    let actions = available_operator_actions(&task);
    let plan = operator_action(&task);
    assert_ne!(
        actions,
        vec![OperatorAction::Drop],
        "present window + stale live TaskWindowMissing must not be Drop-only; got {actions:?}"
    );
    assert!(actions.contains(&OperatorAction::Resume), "{actions:?}");
    assert_ne!(plan.action, OperatorAction::Drop, "{plan:?}");
    assert_ne!(plan.reason, "invalid_task", "{plan:?}");
}

#[test]
fn stale_live_worktree_missing_with_present_worktree_is_not_treated_missing() {
    let mut task = healthy_active_task();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WorktreeMissing,
        "stale worktree missing",
    ));

    assert!(durable_all_present(&task));
    assert!(
        !task.has_missing_worktree(),
        "durable worktree present must not report missing worktree"
    );
    assert!(!task.has_missing_substrate());
    assert_ne!(primary_blocker_reason(&task), Some("worktree missing"));

    let actions = available_operator_actions(&task);
    let plan = operator_action(&task);
    assert_ne!(
        actions,
        vec![OperatorAction::Drop],
        "present worktree + stale live WorktreeMissing must not be Drop-only; got {actions:?}"
    );
    assert!(
        !actions.contains(&OperatorAction::Repair) || actions.contains(&OperatorAction::Resume),
        "must not collapse to repair-only false missing; actions={actions:?}"
    );
    assert!(actions.contains(&OperatorAction::Resume), "{actions:?}");
    assert_ne!(plan.reason, "invalid_task", "{plan:?}");
}

#[test]
fn apply_tmux_present_clears_stale_live_tmux_missing() {
    let mut task = healthy_active_task();
    task.live_status = Some(LiveObservation::new(LiveStatusKind::TmuxMissing, "stale"));
    task.apply_tmux_status(Some(TmuxStatus::present("ajax-repo-handle")));

    assert!(
        !matches!(
            task.live_status.as_ref().map(|live| live.kind),
            Some(LiveStatusKind::TmuxMissing)
        ),
        "apply_tmux_status(present) must clear stale live TmuxMissing; live={:?}",
        task.live_status
    );
}
