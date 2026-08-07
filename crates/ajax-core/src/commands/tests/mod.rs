#![allow(unused_imports)]
pub(super) use super::{
    check_task_plan, clean_task_plan, cockpit, cockpit_inbox, diff_task_plan,
    doctor_with_environment, inbox, inspect_task, list_repos, list_tasks, mark_stale_tasks,
    merge_task_plan, new_task_plan, next, observe_drop_resources, open_task_plan,
    plan_drop_from_observation, plan_drop_from_observation_for_task,
    refresh_git_substrate_evidence, remove_task_plan, review_queue, status, sweep_cleanup_plan,
    task_from_new_request, task_window_repair_plan, CommandContext, CommandError,
    DoctorEnvironment, DropObservation, DropOp, NewTaskRequest, OpenMode, ResourceState,
    StartProvisioningStep,
};
pub(super) use crate::{
    adapters::{
        CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter,
        RecordingCommandRunner,
    },
    config::{Config, ManagedRepo, TestCommand},
    live::LiveStatusKind,
    models::{
        AgentClient, AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, GitStatus,
        LifecycleStatus, LiveObservation, OperatorAction, RuntimeHealth, RuntimeObservationSource,
        RuntimeProjection, SideFlag, StepReceipt, Task, TaskId, TaskWindowStatus, TmuxStatus,
    },
    output::CockpitSummary,
    registry::{InMemoryRegistry, Registry, RegistryError, RegistryEvent, RegistryEventKind},
};
pub(super) use proptest::prelude::*;
pub(super) use rstest::rstest;
pub(super) use std::cell::Cell;

pub(super) fn context_with_tasks() -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![
            ManagedRepo::new("web", "/Users/matt/projects/web", "main"),
            ManagedRepo::new("api", "/Users/matt/projects/api", "main"),
        ],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = Task::new(
        TaskId::new("task-1"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/tmp/worktrees/web-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    );
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.add_side_flag(SideFlag::NeedsInput);
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

#[derive(Default)]
pub(super) struct CountingRegistry {
    inner: InMemoryRegistry,
    list_tasks_calls: Cell<u32>,
}

impl CountingRegistry {
    fn from_registry(inner: InMemoryRegistry) -> Self {
        Self {
            inner,
            list_tasks_calls: Cell::new(0),
        }
    }

    fn list_tasks_calls(&self) -> u32 {
        self.list_tasks_calls.get()
    }
}

impl Registry for CountingRegistry {
    fn create_task(&mut self, task: Task) -> Result<(), RegistryError> {
        self.inner.create_task(task)
    }

    fn delete_task(&mut self, task_id: &TaskId) -> Result<(), RegistryError> {
        self.inner.delete_task(task_id)
    }

    fn get_task(&self, task_id: &TaskId) -> Option<&Task> {
        self.inner.get_task(task_id)
    }

    fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task> {
        self.inner.get_task_mut(task_id)
    }

    fn list_tasks(&self) -> Vec<&Task> {
        self.list_tasks_calls.set(self.list_tasks_calls.get() + 1);
        self.inner.list_tasks()
    }

    fn update_lifecycle(
        &mut self,
        task_id: &TaskId,
        status: LifecycleStatus,
    ) -> Result<(), RegistryError> {
        self.inner.update_lifecycle(task_id, status)
    }

    fn record_event(
        &mut self,
        task_id: TaskId,
        kind: RegistryEventKind,
        message: impl Into<String>,
    ) -> Result<(), RegistryError> {
        self.inner.record_event(task_id, kind, message)
    }

    fn update_git_status(
        &mut self,
        task_id: &TaskId,
        status: GitStatus,
    ) -> Result<(), RegistryError> {
        self.inner.update_git_status(task_id, status)
    }

    fn update_tmux_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TmuxStatus>,
    ) -> Result<(), RegistryError> {
        self.inner.update_tmux_status(task_id, status)
    }

    fn update_task_window_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TaskWindowStatus>,
    ) -> Result<(), RegistryError> {
        self.inner.update_task_window_status(task_id, status)
    }

    fn apply_live_observation(
        &mut self,
        task_id: &TaskId,
        observation: LiveObservation,
    ) -> Result<(), RegistryError> {
        self.inner.apply_live_observation(task_id, observation)
    }

    fn list_events(&self) -> Vec<&RegistryEvent> {
        self.inner.list_events()
    }

    fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent> {
        self.inner.events_for_task(task_id)
    }

    fn record_step_receipt(&mut self, receipt: StepReceipt) -> Result<(), RegistryError> {
        self.inner.record_step_receipt(receipt)
    }

    fn step_receipts_for_task(&self, task_id: &TaskId) -> Vec<&StepReceipt> {
        self.inner.step_receipts_for_task(task_id)
    }
}

pub(super) fn counting_context_with_tasks() -> CommandContext<CountingRegistry> {
    let context = context_with_tasks();
    CommandContext::new(
        context.config,
        CountingRegistry::from_registry(context.registry),
    )
}

pub(super) fn context_with_cleanable_task() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_tasks();
    let task_id = TaskId::new("task-1");
    let task = context.registry.get_task(&task_id).cloned().unwrap();
    let mut cleanable = task;
    cleanable.lifecycle_status = LifecycleStatus::Cleanable;
    cleanable.git_status = Some(GitStatus {
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
    cleanable.tmux_status = Some(crate::models::TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    context.registry = InMemoryRegistry::default();
    context.registry.create_task(cleanable).unwrap();
    context
}

pub(super) fn context_with_test_command() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_tasks();
    context.config.test_commands = vec![TestCommand::new("web", "cargo test")];
    context
}

#[derive(Default)]
pub(super) struct QueuedRunner {
    outputs: std::collections::VecDeque<CommandOutput>,
    commands: Vec<CommandSpec>,
}

impl QueuedRunner {
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: outputs.into(),
            commands: Vec::new(),
        }
    }
}

impl CommandRunner for QueuedRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        self.outputs
            .pop_front()
            .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
    }
}

pub(super) fn output(status_code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        status_code,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

pub(super) fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single_quotes = false;
    let mut word_started = false;

    while let Some(character) = chars.next() {
        match character {
            '\'' => {
                word_started = true;
                in_single_quotes = !in_single_quotes;
            }
            '\\' if !in_single_quotes => {
                word_started = true;
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                } else {
                    current.push(character);
                }
            }
            ' ' if !in_single_quotes => {
                if word_started {
                    words.push(std::mem::take(&mut current));
                    word_started = false;
                }
            }
            _ => {
                word_started = true;
                current.push(character);
            }
        }
    }

    if word_started {
        words.push(current);
    }

    words
}

proptest! {
    #[test]
    fn native_new_task_agent_command_does_not_send_generated_title(
        title in "[^\\x00]{0,80}"
    ) {
        let context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: title.clone(),
                agent: "codex".to_string(),
        orchestration_chat: false,
            },
        )
        .unwrap();

        let worktree_command = plan
            .commands
            .iter()
            .find(|command| super::is_git_worktree_add_command(command))
            .expect("worktree add command");
        let send_keys = plan
            .commands
            .iter()
            .find(|command| super::is_agent_send_keys_command(command))
            .expect("agent send-keys command");
        let worktree_path = worktree_command.args[6].clone();
        let handle = worktree_command.args[5]
            .strip_prefix("ajax/")
            .expect("generated task branch");
        let _task_id = format!("web/{handle}");

        prop_assert_eq!(send_keys.program.as_str(), "tmux");
        prop_assert_eq!(send_keys.args[0].as_str(), "send-keys");
        let launch_words = shell_words(&send_keys.args[3]);
        prop_assert_eq!(
            &launch_words[launch_words.len() - 3..],
            &[
                "codex".to_string(),
                "--cd".to_string(),
                worktree_path,
            ]
        );
    }

    #[test]
    fn native_cleanup_commands_reflect_generated_resource_and_risk_status(
        tmux_exists in any::<bool>(),
        dirty in any::<bool>(),
        conflicted in any::<bool>(),
        side_dirty in any::<bool>(),
        side_conflicted in any::<bool>(),
        untracked_files in 0u32..4,
        merged in any::<bool>()
    ) {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        let git_status = task.git_status.as_mut().unwrap();
        git_status.dirty = dirty;
        git_status.conflicted = conflicted;
        git_status.untracked_files = untracked_files;
        git_status.merged = merged;
        task.tmux_status = Some(TmuxStatus {
            exists: tmux_exists,
            session_name: task.tmux_session.clone(),
        });
        if side_dirty {
            task.add_side_flag(SideFlag::Dirty);
        }
        if side_conflicted {
            task.add_side_flag(SideFlag::Conflicted);
        }

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();
        let expected_force_worktree =
            dirty || conflicted || side_dirty || side_conflicted || untracked_files > 0;
        let expected_worktree_args: Vec<String> = if expected_force_worktree {
            vec![
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "remove",
                "--force",
                "/tmp/worktrees/web-fix-login",
            ]
        } else {
            vec![
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "remove",
                "/tmp/worktrees/web-fix-login",
            ]
        }
        .into_iter()
        .map(str::to_string)
        .collect();
        let expected_branch_args: Vec<String> = if merged {
            vec![
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-d",
                "ajax/fix-login",
            ]
        } else {
            vec![
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-D",
                "ajax/fix-login",
            ]
        }
        .into_iter()
        .map(str::to_string)
        .collect();
        let has_expected_worktree_command = plan
            .commands
            .iter()
            .any(|command| command.program == "git" && command.args == expected_worktree_args);
        let has_expected_branch_command = plan
            .commands
            .iter()
            .any(|command| command.program == "git" && command.args == expected_branch_args);

        prop_assert!(plan.blocked_reasons.is_empty());
        prop_assert_eq!(
            plan.commands
                .iter()
                .any(|command| command.args == vec!["kill-session", "-t", "ajax-web-fix-login"]),
            tmux_exists
        );
        prop_assert!(has_expected_worktree_command);
        prop_assert!(has_expected_branch_command);
    }

    #[test]
    fn task_window_plan_repairs_generated_tmux_and_task_states(
        worktree_exists in any::<bool>(),
        tmux_exists in any::<bool>(),
        task_window_exists in any::<bool>(),
        points_at_expected_path in any::<bool>()
    ) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = Some(GitStatus {
            worktree_exists,
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
        task.tmux_status = Some(TmuxStatus {
            exists: tmux_exists,
            session_name: task.tmux_session.clone(),
        });
        task.task_window_status = Some(TaskWindowStatus {
            exists: task_window_exists,
            window_name: task.task_window.clone(),
            current_path: if points_at_expected_path {
                task.worktree_path.clone()
            } else {
                "/tmp/other-worktree".into()
            },
            points_at_expected_path,
        });

        let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

        prop_assert!(plan.blocked_reasons.is_empty());
        prop_assert_eq!(
            &plan.commands[plan.commands.len() - 2..],
            &[
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:task"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio),
            ]
        );

        let repair_commands = &plan.commands[..plan.commands.len() - 2];
        let worktree_prefix = if !worktree_exists {
            prop_assert!(super::is_git_worktree_add_command(&repair_commands[0]));
            prop_assert_eq!(
                &repair_commands[0].args,
                &[
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "/tmp/worktrees/web-fix-login",
                    "ajax/fix-login",
                ]
            );
            prop_assert!(!repair_commands[0].args.iter().any(|arg| arg == "-b"));
            1
        } else {
            0
        };
        let tmux_repair = &repair_commands[worktree_prefix..];
        if !tmux_exists {
            prop_assert_eq!(
                tmux_repair,
                &[CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-login",
                        "-n",
                        "task",
                        "-c",
                        "/tmp/worktrees/web-fix-login",
                    ],
                )]
            );
        } else if task_window_exists && !points_at_expected_path {
            prop_assert_eq!(
                tmux_repair,
                &[
                    CommandSpec::new(
                        "tmux",
                        ["kill-window", "-t", "ajax-web-fix-login:task"]
                    ),
                    CommandSpec::new(
                        "tmux",
                        [
                            "new-window",
                            "-t",
                            "ajax-web-fix-login",
                            "-n",
                            "task",
                            "-c",
                            "/tmp/worktrees/web-fix-login",
                        ],
                    ),
                ]
            );
        } else if !task_window_exists {
            prop_assert_eq!(
                tmux_repair,
                &[CommandSpec::new(
                    "tmux",
                    [
                        "new-window",
                        "-t",
                        "ajax-web-fix-login",
                        "-n",
                        "task",
                        "-c",
                        "/tmp/worktrees/web-fix-login",
                    ],
                )]
            );
        } else {
            prop_assert!(tmux_repair.is_empty());
        }
    }

    #[test]
    fn stale_task_marking_uses_seven_day_boundary(
        seconds_before_boundary in 0u64..(7 * 24 * 60 * 60)
    ) {
        let last_activity = std::time::SystemTime::UNIX_EPOCH;
        let stale_after = std::time::Duration::from_secs(7 * 24 * 60 * 60);
        let mut before_context = context_with_tasks();
        before_context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .last_activity_at = last_activity;
        let before_changed = mark_stale_tasks(
            &mut before_context,
            last_activity + std::time::Duration::from_secs(seconds_before_boundary),
        );

        prop_assert_eq!(before_changed, 0);
        prop_assert!(!before_context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::Stale));

        let mut boundary_context = context_with_tasks();
        boundary_context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .last_activity_at = last_activity;
        let boundary_changed =
            mark_stale_tasks(&mut boundary_context, last_activity + stale_after);

        prop_assert_eq!(boundary_changed, 1);
        prop_assert!(boundary_context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::Stale));
    }
}

mod suite_1;
mod suite_2;
mod suite_3;
mod suite_4;
mod suite_5;
mod suite_6;
