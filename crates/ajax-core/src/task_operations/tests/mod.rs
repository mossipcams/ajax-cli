#![allow(unused_imports)]
pub(super) use std::collections::VecDeque;

pub(super) use super::drop_task::{
    complete_drop_task_operation, drop_op_execution_decision, execute_drop_task_operation,
    plan_drop_task_operation, DropExecutionDecision, DropTaskCompletion,
};
pub(super) use super::kernel::execute_external_plan;
pub(super) use super::operator_dispatch::{
    execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
};
pub(super) use super::start::{execute_start_task_operation, plan_start_task_operation};
pub(super) use super::sweep_cleanup::execute_sweep_cleanup_operation;
pub(super) use crate::commands::DropOp;
pub(super) use crate::models::StepReceipt;
pub(super) use crate::{
    adapters::{CommandOutput, CommandRunner, CommandSpec},
    commands::{
        CommandContext, CommandError, CommandPlan, NewTaskRequest, OpenMode, ResourceState,
    },
    config::{Config, ManagedRepo, TestCommand},
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, SideFlag, Task, TaskId, TaskOperationKind, TaskWindowStatus, TmuxStatus,
    },
    registry::{InMemoryRegistry, Registry},
};

#[derive(Default)]
pub(super) struct FirstCommandFailsRunner {
    commands: Vec<CommandSpec>,
}

#[derive(Default)]
pub(super) struct RecordingQueuedRunner {
    outputs: VecDeque<CommandOutput>,
    commands: Vec<CommandSpec>,
}

impl RecordingQueuedRunner {
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: outputs.into(),
            commands: Vec::new(),
        }
    }
}

impl CommandRunner for RecordingQueuedRunner {
    fn run(
        &mut self,
        command: &CommandSpec,
    ) -> Result<CommandOutput, crate::adapters::CommandRunError> {
        self.commands.push(command.clone());
        Ok(self.outputs.pop_front().unwrap_or(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }))
    }
}

impl CommandRunner for FirstCommandFailsRunner {
    fn run(
        &mut self,
        command: &CommandSpec,
    ) -> Result<CommandOutput, crate::adapters::CommandRunError> {
        self.commands.push(command.clone());
        Ok(CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: "boom".to_string(),
        })
    }
}

pub(super) struct QueuedRunner {
    outputs: VecDeque<CommandOutput>,
}

impl QueuedRunner {
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: outputs.into(),
        }
    }
}

impl CommandRunner for QueuedRunner {
    fn run(
        &mut self,
        _command: &CommandSpec,
    ) -> Result<CommandOutput, crate::adapters::CommandRunError> {
        Ok(self.outputs.pop_front().unwrap_or(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        }))
    }
}

pub(super) fn output(
    status_code: i32,
    stdout: impl Into<String>,
    stderr: impl Into<String>,
) -> CommandOutput {
    CommandOutput {
        status_code,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

pub(super) fn present_drop_observation_outputs() -> Vec<CommandOutput> {
    vec![
        output(0, "ajax-web-fix-login\n", ""),
        output(
            0,
            "worktree /repo/web__worktrees/ajax-fix-login\nbranch refs/heads/ajax/fix-login\n\n",
            "",
        ),
        output(0, "ajax/fix-login\n", ""),
    ]
}

pub(super) fn absent_drop_observation_outputs() -> Vec<CommandOutput> {
    vec![output(0, "", ""), output(0, "", ""), output(0, "", "")]
}

pub(super) fn context() -> CommandContext<InMemoryRegistry> {
    CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    )
}

pub(super) fn context_with_cleanable_task() -> CommandContext<InMemoryRegistry> {
    let mut context = context();
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
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/repo/web__worktrees/ajax-fix-login",
    ));
    context.registry.create_task(task).unwrap();
    context
}

pub(super) fn context_with_reviewable_task() -> CommandContext<InMemoryRegistry> {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            test_commands: vec![TestCommand::new("web", "cargo nextest run")],
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
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/repo/web__worktrees/ajax-fix-login",
    ));
    context.registry.create_task(task).unwrap();
    context
}

pub(super) fn context_with_two_cleanable_tasks() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_cleanable_task();
    if let Some(task) = context.registry.get_task_mut(&TaskId::new("web/fix-login")) {
        task.tmux_status = None;
        task.task_window_status = None;
    }
    let mut task = Task::new(
        TaskId::new("web/fix-sidebar"),
        "web",
        "fix-sidebar",
        "Fix sidebar",
        "ajax/fix-sidebar",
        "main",
        "/repo/web__worktrees/ajax-fix-sidebar",
        "ajax-web-fix-sidebar",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Cleanable;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-sidebar".to_string()),
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

mod suite_1;
mod suite_2;
mod suite_3;
mod suite_4;

pub(super) fn context_with_named_checkout_mismatch() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.add_side_flag(SideFlag::BranchMissing);
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: false,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.refresh_runtime_projection();
    context
}

pub(super) fn sweep_success_runner_outputs(
    context: &CommandContext<InMemoryRegistry>,
) -> Vec<CommandOutput> {
    let candidates = crate::commands::sweep_cleanup_candidates(context);
    let trash_sweeps = crate::commands::sweep_trash_commands(context);
    let total_plan_commands = candidates
        .iter()
        .map(|candidate| {
            crate::commands::clean_task_plan(context, candidate)
                .unwrap()
                .commands
                .len()
        })
        .sum();
    let mut runner_outputs: Vec<CommandOutput> =
        trash_sweeps.iter().map(|_| output(0, "", "")).collect();
    runner_outputs.push(output(0, "", ""));
    runner_outputs.extend((0..total_plan_commands).map(|_| output(0, "", "")));
    runner_outputs.extend(absent_drop_observation_outputs().into_iter().skip(1));
    runner_outputs
}
