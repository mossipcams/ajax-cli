use std::collections::{BTreeSet, VecDeque};

use super::mark_task_cleanup_step_completed;
use super::*;
use crate::{
    adapters::{CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec},
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

struct ScriptedObservationRunner {
    outputs: VecDeque<CommandOutput>,
}

impl CommandRunner for ScriptedObservationRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.outputs
            .pop_front()
            .ok_or_else(|| CommandRunError::SpawnFailed("missing scripted output".to_string()))
    }
}

fn observe_branch_state(
    branch: &str,
    local_branches: &str,
    remote_branches: &str,
) -> ResourceState {
    let mut context = context_with_task();
    let mut task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();
    task.branch = branch.to_string();
    // Order matches observation: tmux sessions, worktrees, local branches, remote branches.
    let mut runner = ScriptedObservationRunner {
        outputs: VecDeque::from([
            CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: local_branches.to_string(),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: remote_branches.to_string(),
                stderr: String::new(),
            },
        ]),
    };

    observe_drop_resources(&mut context, &task, &mut runner)
        .unwrap()
        .branch
}

#[test]
fn drop_observation_counts_remote_refs_only_for_ajax_branches() {
    // A stale origin tracking ref for a non-ajax branch is not removable by
    // delete_branch_substrate, so it must not keep the branch observed as present.
    assert_eq!(
        observe_branch_state(
            "fix/sandbox/denylist-profile",
            "main\n",
            "origin/main\norigin/fix/sandbox/denylist-profile\n",
        ),
        ResourceState::Absent
    );
    // An ajax/ branch that exists only on the remote is still removable, so it stays present.
    assert_eq!(
        observe_branch_state(
            "ajax/fix-login",
            "main\n",
            "origin/main\norigin/ajax/fix-login\n"
        ),
        ResourceState::Present
    );
}
