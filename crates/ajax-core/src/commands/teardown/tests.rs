use std::collections::BTreeSet;

use super::mark_task_cleanup_step_completed;
use super::*;
use crate::{
    adapters::{CommandMode, CommandSpec},
    commands::CommandContext,
    config::{Config, ManagedRepo},
    models::{AgentClient, GitStatus, LifecycleStatus, Task, TaskId},
    registry::{InMemoryRegistry, Registry},
};

fn context_with_task() -> CommandContext<InMemoryRegistry> {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut task = Task::new(
        TaskId::new("web/fix-login"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/repo/web__worktrees/ajax-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Cleanable;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    context.registry.create_task(task).unwrap();
    context
}

fn fast_worktree_remove_command() -> CommandSpec {
    CommandSpec {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "mv \"$2\" \"$3\" && git -C \"$1\" worktree prune && { rm -rf \"$3\" >/dev/null 2>&1 & }"
                    .to_string(),
                "ajax-fast-worktree-remove".to_string(),
                "/repo/web".to_string(),
                "/repo/web__worktrees/ajax-fix-login".to_string(),
                "/repo/web__worktrees/.ajax-trash/fix-login-123".to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
            timeout: None,
        }
}

#[test]
fn fast_worktree_remove_command_marks_worktree_cleanup_completed() {
    let mut context = context_with_task();
    let command = fast_worktree_remove_command();

    let updated =
        mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

    assert!(updated);
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists));
}

#[test]
fn drop_resource_catalog_preserves_order_states_and_step_keys() {
    let observation = DropObservation {
        agent: ResourceState::Present,
        tmux_session: ResourceState::Absent,
        worktree: ResourceState::Unknown,
        branch: ResourceState::Present,
    };

    let ordered_ops = DROP_TEARDOWN_ORDER.to_vec();

    assert_eq!(
        ordered_ops,
        vec![
            DropOp::EnsureAgentStopped,
            DropOp::EnsureWorktreeAbsent,
            DropOp::EnsureBranchAbsent,
            DropOp::EnsureTmuxSessionAbsent,
        ]
    );
    assert_eq!(
        ordered_ops
            .iter()
            .map(|op| op.observed_state(&observation))
            .collect::<Vec<_>>(),
        vec![
            ResourceState::Present,
            ResourceState::Unknown,
            ResourceState::Present,
            ResourceState::Absent,
        ]
    );

    let step_keys = ordered_ops
        .iter()
        .map(|op| op.step_key())
        .collect::<Vec<_>>();
    assert_eq!(
        step_keys,
        vec![
            "agent_stopped",
            "worktree_absent",
            "branch_absent",
            "tmux_session_absent",
        ]
    );
    assert_eq!(step_keys.len(), BTreeSet::from_iter(step_keys).len());
}
