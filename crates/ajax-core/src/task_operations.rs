pub mod drop_task;
pub mod kernel;
pub mod start;
pub mod sweep_cleanup;
pub mod task_command;

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::drop_task::{
        complete_drop_task_operation, drop_op_execution_decision, execute_drop_task_operation,
        plan_drop_task_operation, DropExecutionDecision, DropTaskCompletion,
    };
    use super::kernel::execute_external_plan;
    use super::start::{execute_start_task_operation, plan_start_task_operation};
    use super::sweep_cleanup::execute_sweep_cleanup_operation;
    use super::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    };
    use crate::commands::DropOp;
    use crate::models::StepReceipt;
    use crate::{
        adapters::{CommandOutput, CommandRunner, CommandSpec},
        commands::{
            CommandContext, CommandError, CommandPlan, NewTaskRequest, OpenMode, ResourceState,
        },
        config::{Config, ManagedRepo, TestCommand},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
            LiveStatusKind, SideFlag, Task, TaskId, TaskOperationKind, TaskWindowStatus,
            TmuxStatus,
        },
        registry::{InMemoryRegistry, Registry},
    };

    #[derive(Default)]
    struct FirstCommandFailsRunner {
        commands: Vec<CommandSpec>,
    }

    #[derive(Default)]
    struct RecordingQueuedRunner {
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

    struct QueuedRunner {
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

    fn output(
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

    fn present_drop_observation_outputs() -> Vec<CommandOutput> {
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

    fn absent_drop_observation_outputs() -> Vec<CommandOutput> {
        vec![output(0, "", ""), output(0, "", ""), output(0, "", "")]
    }

    fn context() -> CommandContext<InMemoryRegistry> {
        CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        )
    }

    fn context_with_cleanable_task() -> CommandContext<InMemoryRegistry> {
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

    fn context_with_reviewable_task() -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                test_commands: vec![TestCommand::new("web", "cargo nextest run")],
                notify: None,
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

    fn context_with_two_cleanable_tasks() -> CommandContext<InMemoryRegistry> {
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

    #[test]
    fn operation_kernel_refuses_blocked_plan_without_running_commands() {
        let mut blocked_plan = CommandPlan::new("blocked");
        blocked_plan.blocked_reasons = vec!["not ready".to_string()];
        let mut runner = RecordingQueuedRunner::default();

        assert_eq!(
            execute_external_plan(&blocked_plan, true, &mut runner),
            Err(CommandError::PlanBlocked(vec!["not ready".to_string()]))
        );
        assert!(runner.commands.is_empty());
    }

    #[test]
    fn operation_kernel_requires_confirmation_before_running_risky_plan() {
        let mut confirmation_plan = CommandPlan::new("confirm");
        confirmation_plan.requires_confirmation = true;
        let mut runner = RecordingQueuedRunner::default();

        assert_eq!(
            execute_external_plan(&confirmation_plan, false, &mut runner),
            Err(CommandError::ConfirmationRequired)
        );
        assert!(runner.commands.is_empty());
    }

    #[test]
    fn operation_kernel_surfaces_nonzero_exit_after_running_the_failing_command() {
        let mut failing_plan = CommandPlan::new("failing");
        failing_plan
            .commands
            .push(CommandSpec::new("git", ["status"]));
        let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "fatal".to_string(),
        }]);

        assert_eq!(
            execute_external_plan(&failing_plan, true, &mut runner),
            Err(CommandError::CommandRun(
                crate::adapters::CommandRunError::NonZeroExit {
                    program: "git".to_string(),
                    status_code: 128,
                    stderr: "fatal".to_string(),
                    cwd: None,
                }
            ))
        );
        assert_eq!(runner.commands.len(), 1);
    }

    #[test]
    fn operation_kernel_returns_outputs_for_successful_plan() {
        let mut success_plan = CommandPlan::new("success");
        success_plan
            .commands
            .push(CommandSpec::new("git", ["status"]));
        success_plan.commands.push(CommandSpec::new("tmux", ["ls"]));
        let mut runner = RecordingQueuedRunner::new(vec![
            CommandOutput {
                status_code: 0,
                stdout: "ok".to_string(),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: "session".to_string(),
                stderr: String::new(),
            },
        ]);

        assert_eq!(
            execute_external_plan(&success_plan, true, &mut runner).unwrap(),
            vec![
                CommandOutput {
                    status_code: 0,
                    stdout: "ok".to_string(),
                    stderr: String::new(),
                },
                CommandOutput {
                    status_code: 0,
                    stdout: "session".to_string(),
                    stderr: String::new(),
                },
            ]
        );
        assert_eq!(runner.commands.len(), 2);
    }

    #[test]
    fn start_operation_plan_returns_task_intent_and_commands_without_mutating_registry() {
        let context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let (intent, plan) = plan_start_task_operation(&context, request).unwrap();

        assert_eq!(context.registry.list_tasks().len(), 0);
        assert_eq!(context.registry.list_events().len(), 0);
        assert_eq!(intent.id, TaskId::new("web/fix-login"));
        assert_eq!(intent.repo, "web");
        assert_eq!(intent.handle, "fix-login");
        assert_eq!(intent.title, "Fix login");
        assert_eq!(intent.branch, "ajax/fix-login");
        assert_eq!(intent.base_branch, "main");
        assert_eq!(
            intent.worktree_path,
            std::path::Path::new("/repo/web__worktrees/ajax-fix-login")
        );
        assert_eq!(intent.tmux_session, "ajax-web-fix-login");
        assert_eq!(intent.task_window, "task");
        assert_eq!(intent.selected_agent, AgentClient::Codex);
        assert_eq!(plan.title, "create task: Fix login");
        assert_eq!(plan.commands.len(), 5);
        assert!(crate::commands::is_git_worktree_add_command(
            &plan.commands[1]
        ));
        assert!(crate::commands::is_task_window_new_session_command(
            &plan.commands[2]
        ));
        assert_eq!(plan.commands[3].program, "sh");
        assert!(crate::commands::is_agent_send_keys_command(
            &plan.commands[4]
        ));
    }

    #[test]
    fn start_operation_execution_failure_preserves_intent_and_marks_provisioning_failed() {
        let mut context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let (intent, plan) = plan_start_task_operation(&context, request.clone()).unwrap();
        let mut runner = FirstCommandFailsRunner::default();

        let error = execute_start_task_operation(
            &mut context,
            &mut runner,
            &request,
            &plan,
            true,
            OpenMode::Attach,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
                status_code: 1,
                ..
            })
        ));
        let task = context.registry.get_task(&intent.id).unwrap();
        assert_eq!(task.intent(), intent);
        assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.metadata.get("start_failed_step").map(String::as_str),
            Some("worktree_created")
        );
        assert_eq!(
            task.metadata
                .get("operator_recommendation")
                .map(String::as_str),
            Some("retry ajax start after checking the failed provisioning step")
        );
        assert_eq!(runner.commands.len(), 1);
    }

    #[test]
    fn start_operation_records_receipts_for_successful_provisioning_steps() {
        let mut context = context();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let (intent, plan) = plan_start_task_operation(&context, request.clone()).unwrap();
        let mut runner = RecordingQueuedRunner::default();

        execute_start_task_operation(
            &mut context,
            &mut runner,
            &request,
            &plan,
            true,
            OpenMode::Attach,
        )
        .unwrap();

        let receipts = context.registry.step_receipts_for_task(&intent.id);
        let keys = receipts
            .iter()
            .map(|receipt| {
                (
                    receipt.operation,
                    receipt.step_key.as_str(),
                    receipt.target.as_str(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                (
                    TaskOperationKind::Start,
                    "worktree_created",
                    "/repo/web__worktrees/ajax-fix-login",
                ),
                (
                    TaskOperationKind::Start,
                    "task_session_created",
                    "ajax-web-fix-login",
                ),
                (
                    TaskOperationKind::Start,
                    "agent_command_sent",
                    "ajax-web-fix-login:task",
                ),
            ]
        );
    }

    #[test]
    fn task_command_operation_plans_use_operator_titles() {
        let context = context_with_reviewable_task();

        let cases = [
            (TaskCommandKind::Resume, "open task: web/fix-login"),
            (TaskCommandKind::Review, "diff task: web/fix-login"),
            (TaskCommandKind::Repair, "repair task: web/fix-login"),
            (TaskCommandKind::Ship, "merge task: web/fix-login"),
        ];

        for (kind, title) in cases {
            let plan =
                plan_task_command_operation(&context, kind, "web/fix-login", OpenMode::Attach)
                    .unwrap();

            assert_eq!(plan.title, title);
            assert!(
                !plan.commands.is_empty(),
                "{kind:?} should carry executable commands"
            );
        }
    }

    #[test]
    fn resume_operation_executes_plan_and_reports_state_change() {
        let mut context = context_with_reviewable_task();
        let resume_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Resume,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut resume_runner = RecordingQueuedRunner::new(vec![
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
        ]);

        let (resume_outputs, resume_state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Resume,
            "web/fix-login",
            &resume_plan,
            true,
            &mut resume_runner,
        )
        .unwrap();

        assert_eq!(resume_runner.commands.len(), 2);
        assert_eq!(resume_outputs.len(), 2);
        assert!(resume_state_changed);
    }

    #[test]
    fn review_operation_returns_diff_output_without_state_change() {
        let mut context = context_with_reviewable_task();
        let review_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Review,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut review_runner = RecordingQueuedRunner::new(vec![CommandOutput {
            status_code: 0,
            stdout: "diff stat".to_string(),
            stderr: String::new(),
        }]);

        let (review_outputs, review_state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Review,
            "web/fix-login",
            &review_plan,
            true,
            &mut review_runner,
        )
        .unwrap();

        assert_eq!(review_runner.commands.len(), 1);
        assert_eq!(review_outputs[0].stdout, "diff stat");
        assert!(!review_state_changed);
    }

    fn claude_waiting_context() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_reviewable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap();
        task.selected_agent = AgentClient::Claude;
        task.lifecycle_status = LifecycleStatus::Active;
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        context
    }

    #[test]
    fn successful_resume_records_attention_acknowledgment() {
        let mut context = claude_waiting_context();
        let plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Resume,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(
            plan.commands
                .iter()
                .map(|_| CommandOutput {
                    status_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
                .collect(),
        );

        execute_task_command_operation(
            &mut context,
            TaskCommandKind::Resume,
            "web/fix-login",
            &plan,
            true,
            &mut runner,
        )
        .unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert!(task.attention_acknowledged_at.is_some());
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
    }

    #[test]
    fn failed_resume_does_not_acknowledge_attention() {
        let mut context = claude_waiting_context();
        let plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Resume,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: "resume failed".to_string(),
        }]);

        let (_error, state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Resume,
            "web/fix-login",
            &plan,
            true,
            &mut runner,
        )
        .unwrap_err();

        assert!(!state_changed);
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.attention_acknowledged_at, None);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn review_operation_does_not_acknowledge_attention() {
        let mut context = claude_waiting_context();
        let plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Review,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(
            plan.commands
                .iter()
                .map(|_| CommandOutput {
                    status_code: 0,
                    stdout: "diff stat".to_string(),
                    stderr: String::new(),
                })
                .collect(),
        );

        execute_task_command_operation(
            &mut context,
            TaskCommandKind::Review,
            "web/fix-login",
            &plan,
            true,
            &mut runner,
        )
        .unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.attention_acknowledged_at, None);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn ship_task_operation_refreshes_git_evidence_before_merge_commands() {
        let mut context = context_with_reviewable_task();
        let ship_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Ship,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
            status_code: 0,
            stdout: "## ajax/fix-login\n M src/lib.rs\n".to_string(),
            stderr: String::new(),
        }]);

        let (error, state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Ship,
            "web/fix-login",
            &ship_plan,
            true,
            &mut runner,
        )
        .unwrap_err();

        assert!(!state_changed);
        assert!(matches!(error, CommandError::PlanBlocked(_)));
        assert_eq!(runner.commands.len(), 1);
        assert_eq!(runner.commands[0].program, "git");
        assert!(runner.commands[0].args.contains(&"status".to_string()));
    }

    #[test]
    fn ship_operation_marks_task_merged_on_success() {
        let mut context = context_with_reviewable_task();
        let ship_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Ship,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(vec![
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
        ]);

        let (outputs, state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Ship,
            "web/fix-login",
            &ship_plan,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(state_changed);
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Merged
        );
    }

    #[test]
    fn ship_operation_records_conflict_attention_on_merge_failure() {
        let mut context = context_with_reviewable_task();
        let ship_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Ship,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(vec![
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
                status_code: 1,
                stdout: String::new(),
                stderr: "Automatic merge failed; fix conflicts and then commit.".to_string(),
            },
        ]);

        let (error, _state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Ship,
            "web/fix-login",
            &ship_plan,
            true,
            &mut runner,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
                status_code: 1,
                ..
            })
        ));
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| (status.kind, status.summary.as_str())),
            Some((LiveStatusKind::CommandFailed, "merge failed"))
        );
    }

    #[test]
    fn repair_operation_promotes_task_to_reviewable_on_check_success() {
        let mut context = context_with_reviewable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::TestsFailed);
        let repair_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Repair,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(
            repair_plan
                .commands
                .iter()
                .map(|_| CommandOutput {
                    status_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
                .collect(),
        );

        let (outputs, state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Repair,
            "web/fix-login",
            &repair_plan,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(outputs.len(), repair_plan.commands.len());
        assert!(state_changed);
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
        assert!(!task.has_side_flag(SideFlag::TestsFailed));
        assert!(task.live_status.is_none());
    }

    #[test]
    fn repair_operation_records_tests_failed_on_check_failure() {
        let mut context = context_with_reviewable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;
        let repair_plan = plan_task_command_operation(
            &context,
            TaskCommandKind::Repair,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
            status_code: 42,
            stdout: String::new(),
            stderr: "tests failed".to_string(),
        }]);

        let (error, _state_changed) = execute_task_command_operation(
            &mut context,
            TaskCommandKind::Repair,
            "web/fix-login",
            &repair_plan,
            true,
            &mut runner,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
                status_code: 42,
                ..
            })
        ));
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| (status.kind, status.summary.as_str())),
            Some((LiveStatusKind::CommandFailed, "check failed"))
        );
    }

    #[test]
    fn drop_operation_plan_uses_fresh_observation_instead_of_cached_substrate() {
        let mut context = context_with_cleanable_task();
        let mut runner = QueuedRunner::new(vec![
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
                stdout: String::new(),
                stderr: String::new(),
            },
        ]);

        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        assert_eq!(operation.observation.tmux_session, ResourceState::Absent);
        assert_eq!(operation.observation.worktree, ResourceState::Absent);
        assert_eq!(operation.observation.branch, ResourceState::Absent);
    }

    #[test]
    fn drop_operation_does_not_remove_other_branch_at_expected_path() {
        let mut context = context_with_cleanable_task();
        let mut outputs = vec![
            output(0, "ajax-web-fix-login\n", ""),
            output(
                0,
                "worktree /repo/web__worktrees/ajax-fix-login\nbranch refs/heads/dependabot/pip/minor\n\n",
                "",
            ),
            output(0, "ajax/fix-login\n", ""),
        ];
        outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(completion, DropTaskCompletion::Removed);
        assert_eq!(outputs.len(), 2);
        assert!(runner.commands.iter().any(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "branch")
                && command.args.iter().any(|arg| arg == "ajax/fix-login")
        }));
        assert!(!runner.commands.iter().any(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "worktree")
                && command.args.iter().any(|arg| arg == "remove")
        }));
        assert!(!runner.commands.iter().any(|command| {
            command.program == "sh"
                && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
        }));
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
    }

    #[test]
    fn drop_execution_keeps_resource_specific_command_and_missing_rules() {
        let context = context_with_cleanable_task();

        let agent_decision = drop_op_execution_decision(
            &context,
            "web/fix-login",
            DropOp::EnsureAgentStopped,
            false,
        )
        .unwrap();
        assert!(matches!(agent_decision, DropExecutionDecision::InProcess));

        let worktree_unforced = drop_op_execution_decision(
            &context,
            "web/fix-login",
            DropOp::EnsureWorktreeAbsent,
            false,
        )
        .unwrap();
        assert!(matches!(
            worktree_unforced,
            DropExecutionDecision::Command(ref command)
                if command
                    == &crate::adapters::GitAdapter::new("git")
                        .remove_worktree("/repo/web", "/repo/web__worktrees/ajax-fix-login")
        ));

        let worktree_forced = drop_op_execution_decision(
            &context,
            "web/fix-login",
            DropOp::EnsureWorktreeAbsent,
            true,
        )
        .unwrap();
        assert!(matches!(
            worktree_forced,
            DropExecutionDecision::Command(ref command)
                if command.program == "sh"
                    && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
        ));

        let branch_unforced = drop_op_execution_decision(
            &context,
            "web/fix-login",
            DropOp::EnsureBranchAbsent,
            false,
        )
        .unwrap();
        assert!(matches!(
            branch_unforced,
            DropExecutionDecision::Command(ref command)
                if command.program == "git"
                    && command.args.iter().any(|arg| arg == "-d")
        ));

        let branch_forced =
            drop_op_execution_decision(&context, "web/fix-login", DropOp::EnsureBranchAbsent, true)
                .unwrap();
        assert!(matches!(
            branch_forced,
            DropExecutionDecision::Command(ref command)
                if command.program == "git"
                    && command.args.iter().any(|arg| arg == "-D")
        ));

        let tmux_decision = drop_op_execution_decision(
            &context,
            "web/fix-login",
            DropOp::EnsureTmuxSessionAbsent,
            false,
        )
        .unwrap();
        assert!(matches!(
            tmux_decision,
            DropExecutionDecision::Command(ref command)
                if command.program == "tmux"
                    && command.args.iter().any(|arg| arg == "kill-session")
        ));
    }

    #[test]
    fn drop_resource_catalog_preserves_receipt_policy_and_targets() {
        let task = context_with_cleanable_task()
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .clone();

        let receipts = [
            DropOp::EnsureAgentStopped,
            DropOp::EnsureWorktreeAbsent,
            DropOp::EnsureBranchAbsent,
            DropOp::EnsureTmuxSessionAbsent,
        ]
        .into_iter()
        .map(|op| {
            (
                op,
                op.step_key(),
                op.receipt_target(&task),
                op.records_observed_absent_receipt(),
            )
        })
        .collect::<Vec<_>>();

        assert_eq!(
            receipts,
            vec![
                (
                    DropOp::EnsureAgentStopped,
                    "agent_stopped",
                    "ajax-web-fix-login".to_string(),
                    false,
                ),
                (
                    DropOp::EnsureWorktreeAbsent,
                    "worktree_absent",
                    "/repo/web__worktrees/ajax-fix-login".to_string(),
                    true,
                ),
                (
                    DropOp::EnsureBranchAbsent,
                    "branch_absent",
                    "ajax/fix-login".to_string(),
                    true,
                ),
                (
                    DropOp::EnsureTmuxSessionAbsent,
                    "tmux_session_absent",
                    "ajax-web-fix-login".to_string(),
                    true,
                ),
            ]
        );
    }

    #[test]
    fn drop_operation_removes_failed_or_orphaned_tasks_when_resources_are_absent() {
        for lifecycle_status in [LifecycleStatus::Error, LifecycleStatus::Orphaned] {
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
            task.lifecycle_status = lifecycle_status;
            context.registry.create_task(task).unwrap();
            let mut outputs = absent_drop_observation_outputs();
            outputs.extend(absent_drop_observation_outputs());
            let mut runner = RecordingQueuedRunner::new(outputs);
            let operation =
                plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

            let (_outputs, completion) = execute_drop_task_operation(
                &mut context,
                "web/fix-login",
                operation,
                true,
                &mut runner,
            )
            .unwrap();

            assert_eq!(completion, DropTaskCompletion::Removed);
            assert!(
                context
                    .registry
                    .get_task(&TaskId::new("web/fix-login"))
                    .is_none(),
                "{lifecycle_status:?}"
            );
        }
    }

    #[test]
    fn confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("web/fix-login");
        {
            let task = context.registry.get_task_mut(&task_id).unwrap();
            task.add_side_flag(SideFlag::Dirty);
            if let Some(git_status) = task.git_status.as_mut() {
                git_status.dirty = true;
            }
        }
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
        outputs.extend([
            output(0, "", ""),
            output(0, "worktree /repo/web__worktrees/ajax-fix-login\n", ""),
            output(0, "", ""),
        ]);
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (command_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        let fast_remove = runner
            .commands
            .iter()
            .find(|command| {
                command.program == "sh"
                    && command.args.first().map(String::as_str) == Some("-c")
                    && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
            })
            .expect("fast remove command");
        assert_eq!(
            fast_remove.args[1],
            "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
        );
        assert_eq!(fast_remove.args[3], "/repo/web");
        assert_eq!(fast_remove.args[4], "/repo/web__worktrees/ajax-fix-login");
        assert!(fast_remove.args[5].starts_with("/repo/web__worktrees/.ajax-trash/fix-login-"));
        assert!(!runner.commands.iter().any(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "worktree")
                && command.args.iter().any(|arg| arg == "remove")
        }));
        assert_eq!(command_outputs.len(), 3);
        assert_eq!(completion, DropTaskCompletion::Removed);

        assert!(context.registry.get_task(&task_id).is_none());
    }

    #[test]
    fn unforced_dirty_drop_keeps_plain_git_worktree_remove() {
        let mut context = context_with_cleanable_task();
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        let git = crate::adapters::GitAdapter::new("git");
        let worktree_remove = runner
            .commands
            .iter()
            .find(|command| {
                command.program == "git"
                    && command.args.iter().any(|arg| arg == "worktree")
                    && command.args.iter().any(|arg| arg == "remove")
            })
            .expect("plain worktree remove");
        assert_eq!(
            worktree_remove,
            &git.remove_worktree("/repo/web", "/repo/web__worktrees/ajax-fix-login")
        );
        assert!(!runner.commands.iter().any(|command| {
            command.program == "sh"
                && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
        }));
        assert_eq!(completion, DropTaskCompletion::Removed);
    }

    #[test]
    fn fast_drop_mv_failure_marks_teardown_incomplete() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("web/fix-login");
        {
            let task = context.registry.get_task_mut(&task_id).unwrap();
            task.add_side_flag(SideFlag::Dirty);
            if let Some(git_status) = task.git_status.as_mut() {
                git_status.dirty = true;
            }
        }
        let mut outputs = present_drop_observation_outputs();
        outputs.push(output(1, "", "mv: cannot move: No such file or directory"));
        outputs.extend([output(0, "", ""), output(0, "", "")]);
        outputs.extend([
            output(0, "", ""),
            output(0, "worktree /repo/web__worktrees/ajax-fix-login\n", ""),
            output(0, "", ""),
        ]);
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap_err();

        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert_eq!(
            task.metadata
                .get("drop_failed_step_key")
                .map(String::as_str),
            Some("worktree_absent")
        );
        assert!(task
            .metadata
            .get("drop_failed_detail")
            .is_some_and(|detail| detail.contains("No such file or directory")));
    }
    #[test]
    fn drop_failure_keeps_task_and_tmux_when_worktree_remove_fails_before_session_kill() {
        let mut context = context_with_cleanable_task();
        let mut outputs = present_drop_observation_outputs();
        outputs.push(output(
            2,
            "",
            "error: failed to remove worktree: permission denied",
        ));
        outputs.extend([
            output(0, "ajax-web-fix-login\n", ""),
            output(
                0,
                "worktree /repo/web__worktrees/ajax-fix-login\nbranch refs/heads/ajax/fix-login\n\n",
                "",
            ),
            output(0, "ajax/fix-login\n", ""),
        ]);
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap_err();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("failed git step should leave task resumable");
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert_eq!(
            task.metadata.get("drop_failed_step").map(String::as_str),
            Some("remove worktree")
        );
        assert!(task
            .metadata
            .get("drop_failed_detail")
            .is_some_and(|detail| detail.contains("permission denied")));
        assert!(context
            .registry
            .events_for_task(&TaskId::new("web/fix-login"))
            .iter()
            .any(|event| event.message.contains("drop step failed: remove worktree")));
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
        assert!(!runner.commands.iter().any(|command| {
            command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
        }));
    }

    #[test]
    fn drop_failure_keeps_task_when_branch_remove_fails_after_worktree_removed() {
        let mut context = context_with_cleanable_task();
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([
            output(0, "", ""),
            output(0, "", ""),
            output(2, "", "error: refusing to delete checked out branch"),
            output(0, "ajax-web-fix-login\n", ""),
            output(0, "", ""),
            output(0, "ajax/fix-login\n", ""),
        ]);
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap_err();
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("branch-only cleanup should remain resumable");
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
    }

    #[test]
    fn drop_operation_resumes_from_receipts_after_partial_success() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("web/fix-login");
        context
            .registry
            .record_step_receipt(StepReceipt::succeeded(
                task_id.clone(),
                TaskOperationKind::Drop,
                "worktree_absent",
                "/repo/web__worktrees/ajax-fix-login",
                "{}",
            ))
            .unwrap();
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([
            output(0, "", ""),
            output(0, "", ""),
            output(0, "", ""),
            output(0, "", ""),
            output(0, "", ""),
            output(0, "", ""),
        ]);
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (command_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(command_outputs.len(), 2);
        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(!runner.commands.iter().any(|command| {
            command.program == "git"
                && command.args.contains(&"worktree".to_string())
                && command.args.contains(&"remove".to_string())
        }));
        assert!(runner.commands.iter().any(|command| {
            command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
        }));
    }

    #[test]
    fn drop_retry_repeats_receipted_step_when_fresh_observation_finds_resource_present() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("web/fix-login");
        context
            .registry
            .get_task_mut(&task_id)
            .unwrap()
            .lifecycle_status = LifecycleStatus::TeardownIncomplete;
        context
            .registry
            .get_task_mut(&task_id)
            .unwrap()
            .metadata
            .insert(
                "drop_failed_step_key".to_string(),
                "branch_absent".to_string(),
            );
        context
            .registry
            .record_step_receipt(StepReceipt::succeeded(
                task_id,
                TaskOperationKind::Drop,
                "branch_absent",
                "ajax/fix-login",
                "{}",
            ))
            .unwrap();
        let mut outputs = absent_drop_observation_outputs();
        outputs[2] = output(0, "ajax/fix-login\n", "");
        outputs.push(output(0, "", ""));
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(runner.commands.iter().any(|command| {
            command.program == "git"
                && command.args.contains(&"branch".to_string())
                && command.args.contains(&"ajax/fix-login".to_string())
        }));
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
    }

    #[test]
    fn drop_operation_records_remaining_resource_when_empty_plan_still_finishes_incomplete() {
        let mut context = context_with_cleanable_task();
        let mut outputs = absent_drop_observation_outputs();
        outputs.extend(vec![
            output(0, "", ""),
            output(0, "", ""),
            output(0, "ajax/fix-login\n", ""),
        ]);
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner)
            .expect("drop operation should plan");

        let (_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .expect("drop operation should complete with incomplete teardown");

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert!(matches!(
            completion,
            DropTaskCompletion::TeardownIncomplete {
                failed_step: DropOp::EnsureBranchAbsent,
                ..
            }
        ));
        assert_eq!(
            task.metadata.get("drop_failed_step").map(String::as_str),
            Some("delete branch")
        );
    }

    #[test]
    fn drop_completion_hard_deletes_task_when_final_observation_is_absent() {
        let mut context = context_with_cleanable_task();

        let completion = complete_drop_task_operation(
            &mut context,
            "web/fix-login",
            &crate::commands::DropObservation {
                agent: ResourceState::Absent,
                tmux_session: ResourceState::Absent,
                worktree: ResourceState::Absent,
                branch: ResourceState::Absent,
            },
        )
        .unwrap();

        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
    }

    #[test]
    fn drop_completion_marks_teardown_incomplete_when_resources_remain() {
        let mut context = context_with_cleanable_task();

        let completion = complete_drop_task_operation(
            &mut context,
            "web/fix-login",
            &crate::commands::DropObservation {
                agent: ResourceState::Absent,
                tmux_session: ResourceState::Absent,
                worktree: ResourceState::Absent,
                branch: ResourceState::Present,
            },
        )
        .unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert!(matches!(
            completion,
            DropTaskCompletion::TeardownIncomplete {
                failed_step: DropOp::EnsureBranchAbsent,
                detail,
            } if detail.contains("branch still present")
        ));
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert_eq!(
            task.metadata.get("drop_failed_step").map(String::as_str),
            Some("delete branch")
        );
        assert!(task
            .metadata
            .get("drop_latest_observation")
            .is_some_and(|observation| observation.contains("branch=Present")));
    }

    #[test]
    fn drop_operation_executes_teardown_and_completes_from_final_observation() {
        let mut context = context_with_cleanable_task();
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(outputs.len(), 3);
        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
        assert!(runner.commands.iter().any(|command| {
            command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
        }));
        assert!(runner.commands.iter().any(|command| {
            command.program == "git" && command.args.iter().any(|arg| arg == "worktree")
        }));
        assert!(runner.commands.iter().any(|command| {
            command.program == "git" && command.args.iter().any(|arg| arg == "branch")
        }));

        assert!(context
            .registry
            .step_receipts_for_task(&TaskId::new("web/fix-login"))
            .is_empty());
    }

    #[test]
    fn drop_operation_records_skipped_receipts_for_already_missing_resources() {
        let mut context = context_with_cleanable_task();
        let mut outputs = absent_drop_observation_outputs();
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

        assert!(context
            .registry
            .step_receipts_for_task(&TaskId::new("web/fix-login"))
            .is_empty());
    }

    #[test]
    fn drop_operation_treats_invalid_branch_delete_error_as_already_absent() {
        let mut context = context_with_cleanable_task();
        let mut outputs = present_drop_observation_outputs();
        outputs.extend([
            output(0, "", ""),
            output(
                128,
                "",
                "fatal: 'ajax/fix-login' is not a valid branch name",
            ),
            output(0, "", ""),
        ]);
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(outputs.len(), 3);
        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
    }

    #[test]
    fn sweep_cleanup_marks_teardown_incomplete_when_final_observation_still_finds_tmux() {
        let mut context = context_with_cleanable_task();
        let plan = crate::commands::clean_task_plan(&context, "web/fix-login").unwrap();
        let mut runner_outputs = crate::commands::sweep_trash_commands(&context)
            .iter()
            .map(|_| output(0, "", ""))
            .collect::<Vec<_>>();
        runner_outputs.push(output(0, "ajax-web-fix-login\n", ""));
        runner_outputs.extend(plan.commands.iter().map(|_| output(0, "", "")));
        let mut runner = RecordingQueuedRunner::new(runner_outputs);

        execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .expect("task should remain when tmux is still present");
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert!(task
            .metadata
            .get("drop_failed_detail")
            .is_some_and(|detail| detail.contains("tmux")));
    }

    #[test]
    fn tidy_still_projects_each_successful_cleanup_command() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("web/fix-login");
        {
            let task = context.registry.get_task_mut(&task_id).unwrap();
            task.agent_status = AgentRuntimeStatus::Running;
        }

        let trash_sweeps = crate::commands::sweep_trash_commands(&context);
        let plan = crate::commands::clean_task_plan(&context, "web/fix-login").unwrap();
        let mut runner_outputs: Vec<CommandOutput> =
            trash_sweeps.iter().map(|_| output(0, "", "")).collect();
        runner_outputs.push(output(1, "", "boom"));
        runner_outputs.extend(plan.commands.iter().map(|_| output(0, "", "")));
        runner_outputs.push(output(1, "", "unexpected git command"));
        runner_outputs.push(output(1, "", "unexpected git command"));
        let mut runner = RecordingQueuedRunner::new(runner_outputs);

        let (_outputs, state_changed) =
            execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        let task = context.registry.get_task(&task_id).unwrap();
        assert!(state_changed);
        assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists && !status.branch_exists));
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists));
        assert!(task
            .task_window_status
            .as_ref()
            .is_some_and(|status| !status.exists));
    }

    #[test]
    fn sweep_cleanup_removes_stale_trash_entries() {
        let mut context = context_with_cleanable_task();
        let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

        execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        let trash_sweep = runner
            .commands
            .iter()
            .find(|command| {
                command.program == "sh"
                    && command.args.first().map(String::as_str) == Some("-c")
                    && command.args.get(2).map(String::as_str) == Some("ajax-trash-sweep")
            })
            .expect("trash sweep command");
        assert_eq!(
            trash_sweep.args[1],
            "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi"
        );
        assert_eq!(trash_sweep.args[3], "/repo/web__worktrees/.ajax-trash");
    }

    #[test]
    fn sweep_cleanup_batches_repo_observations_across_candidates() {
        let mut context = context_with_two_cleanable_tasks();
        let candidates = crate::commands::sweep_cleanup_candidates(&context);
        assert_eq!(candidates.len(), 2);
        let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

        execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        let list_sessions = runner
            .commands
            .iter()
            .filter(|command| command.args.first().map(String::as_str) == Some("list-sessions"))
            .count();
        let worktree_lists = runner
            .commands
            .iter()
            .filter(|command| {
                command.program == "git"
                    && command.args.iter().any(|arg| arg == "worktree")
                    && command.args.iter().any(|arg| arg == "list")
            })
            .count();
        let branch_lists = runner
            .commands
            .iter()
            .filter(|command| {
                command.program == "git"
                    && command
                        .args
                        .iter()
                        .any(|arg| arg.contains("--format=%(refname:short)"))
            })
            .count();

        assert_eq!(list_sessions, 1, "shared tmux listing should run once");
        assert_eq!(
            worktree_lists, 1,
            "repo worktree observation should be reused"
        );
        assert_eq!(branch_lists, 1, "repo branch observation should be reused");
    }

    fn sweep_success_runner_outputs(
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

    #[test]
    fn sweep_cleanup_operation_executes_candidates_and_reports_partial_failure_state() {
        let mut context = context_with_two_cleanable_tasks();
        let candidates = crate::commands::sweep_cleanup_candidates(&context);
        let trash_sweeps = crate::commands::sweep_trash_commands(&context);
        let total_plan_commands: usize = candidates
            .iter()
            .map(|candidate| {
                crate::commands::clean_task_plan(&context, candidate)
                    .unwrap()
                    .commands
                    .len()
            })
            .sum();
        let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

        let (outputs, state_changed) =
            execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        assert_eq!(outputs.len(), total_plan_commands + trash_sweeps.len());
        assert!(state_changed);
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_none());
        assert!(context
            .registry
            .get_task(&TaskId::new("web/fix-sidebar"))
            .is_none());

        let mut context = context_with_two_cleanable_tasks();
        let candidates = crate::commands::sweep_cleanup_candidates(&context);
        let trash_sweeps = crate::commands::sweep_trash_commands(&context);
        let first_candidate_command_count =
            crate::commands::clean_task_plan(&context, &candidates[0])
                .unwrap()
                .commands
                .len();
        let mut outputs: Vec<CommandOutput> =
            trash_sweeps.iter().map(|_| output(0, "", "")).collect();
        outputs.push(output(0, "ajax-web-fix-login\n", ""));
        outputs.extend(
            (0..first_candidate_command_count.saturating_sub(1)).map(|_| output(0, "", "")),
        );
        outputs.push(output(2, "", "branch delete failed"));
        let mut runner = RecordingQueuedRunner::new(outputs);

        let (error, state_changed) =
            execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap_err();

        assert!(state_changed);
        assert!(matches!(
            error,
            CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
                status_code: 2,
                ..
            })
        ));
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .expect("first candidate should remain after partial failure")
                .lifecycle_status,
            LifecycleStatus::Cleanable
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-sidebar"))
                .expect("second candidate should remain untouched")
                .lifecycle_status,
            LifecycleStatus::Cleanable
        );
    }
}
