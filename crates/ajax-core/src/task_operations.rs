pub mod kernel {
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner},
        commands::{CommandError, CommandPlan},
    };

    pub fn execute_external_plan(
        plan: &CommandPlan,
        confirmed: bool,
        runner: &mut impl CommandRunner,
    ) -> Result<Vec<CommandOutput>, CommandError> {
        execute_external_plan_with_success(plan, confirmed, runner, |_, _, _| Ok(()))
    }

    pub fn execute_external_plan_with_success(
        plan: &CommandPlan,
        confirmed: bool,
        runner: &mut impl CommandRunner,
        mut on_success: impl FnMut(
            usize,
            &crate::adapters::CommandSpec,
            &CommandOutput,
        ) -> Result<(), CommandError>,
    ) -> Result<Vec<CommandOutput>, CommandError> {
        if !plan.blocked_reasons.is_empty() {
            return Err(CommandError::PlanBlocked(plan.blocked_reasons.clone()));
        }

        if plan.requires_confirmation && !confirmed {
            return Err(CommandError::ConfirmationRequired);
        }

        let mut outputs = Vec::new();
        for (index, command) in plan.commands.iter().enumerate() {
            let output = runner.run(command).map_err(CommandError::CommandRun)?;
            if output.status_code != 0 {
                return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: command.program.clone(),
                    status_code: output.status_code,
                    stderr: output.stderr,
                    cwd: command.cwd.clone(),
                }));
            }
            on_success(index, command, &output)?;
            outputs.push(output);
        }

        Ok(outputs)
    }
}

pub mod start {
    use crate::{
        adapters::{CommandOutput, CommandRunner},
        commands::{
            self, CommandContext, CommandError, CommandPlan, NewTaskRequest, OpenMode,
            StartProvisioningStep,
        },
        models::{StepReceipt, Task, TaskIntent, TaskOperationKind},
        registry::Registry,
        task_operations::kernel::execute_external_plan_with_success,
    };

    pub fn plan_start_task_operation<R: Registry>(
        context: &CommandContext<R>,
        request: NewTaskRequest,
    ) -> Result<(TaskIntent, CommandPlan), CommandError> {
        let task = commands::task_from_new_request(context, &request)?;
        let plan = commands::new_task_plan(context, request)?;

        Ok((task.intent(), plan))
    }

    pub fn execute_start_task_operation<R: Registry>(
        context: &mut CommandContext<R>,
        runner: &mut impl CommandRunner,
        request: &NewTaskRequest,
        plan: &CommandPlan,
        confirmed: bool,
        open_mode: OpenMode,
    ) -> Result<(Vec<CommandOutput>, Task), CommandError> {
        let task = commands::record_new_task(context, request)?;
        let external_outputs =
            match execute_external_plan_with_success(plan, confirmed, runner, |index, _, _| {
                if let Some(step) = start_step_for_command_index(plan, index) {
                    commands::mark_new_task_provisioning_step_completed(context, &task.id, step)?;
                    context
                        .registry
                        .record_step_receipt(start_step_receipt(&task, step))
                        .map_err(CommandError::Registry)?;
                }
                Ok(())
            }) {
                Ok(execution) => execution,
                Err(error @ CommandError::CommandRun(_)) => {
                    let _ = commands::mark_new_task_provisioning_failed(context, &task.id);
                    return Err(error);
                }
                Err(error) => return Err(error),
            };
        let mut outputs = plan
            .commands
            .iter()
            .zip(external_outputs)
            .filter_map(|(command, output)| {
                (!commands::is_new_task_husky_hook_command(command)).then_some(output)
            })
            .collect::<Vec<_>>();

        commands::mark_task_opened(context, &task.qualified_handle())?;
        let open_plan = commands::open_task_plan(context, &task.qualified_handle(), open_mode)?;
        outputs.extend(commands::execute_plan(&open_plan, true, runner)?);

        let task = context.registry.get_task(&task.id).cloned().unwrap_or(task);

        Ok((outputs, task))
    }

    fn start_step_for_command_index(
        plan: &CommandPlan,
        index: usize,
    ) -> Option<StartProvisioningStep> {
        if index == 0 {
            Some(StartProvisioningStep::WorktreeCreated)
        } else if index + 2 == plan.commands.len() {
            Some(StartProvisioningStep::TaskSessionCreated)
        } else if index + 1 == plan.commands.len() {
            Some(StartProvisioningStep::AgentCommandSent)
        } else {
            None
        }
    }

    fn start_step_receipt(task: &Task, step: StartProvisioningStep) -> StepReceipt {
        let (step_key, target) = match step {
            StartProvisioningStep::WorktreeCreated => {
                ("worktree_created", task.worktree_path.display().to_string())
            }
            StartProvisioningStep::TaskSessionCreated => {
                ("task_session_created", task.tmux_session.clone())
            }
            StartProvisioningStep::AgentCommandSent => (
                "agent_command_sent",
                format!("{}:{}", task.tmux_session, task.worktrunk_window),
            ),
        };

        StepReceipt::succeeded(
            task.id.clone(),
            TaskOperationKind::Start,
            step_key,
            target,
            serde_json::json!({
                "source": "command_result",
                "step": step_key,
            })
            .to_string(),
        )
    }
}

pub mod task_command {
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner},
        commands::{self, CommandContext, CommandError, CommandPlan, OpenMode},
        registry::Registry,
        task_operations::kernel::execute_external_plan,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum TaskCommandKind {
        Resume,
        Review,
        Repair,
        Ship,
    }

    pub fn plan_task_command_operation<R: Registry>(
        context: &CommandContext<R>,
        kind: TaskCommandKind,
        qualified_handle: &str,
        open_mode: OpenMode,
    ) -> Result<CommandPlan, CommandError> {
        Ok(match kind {
            TaskCommandKind::Resume => {
                commands::open_task_plan(context, qualified_handle, open_mode)?
            }
            TaskCommandKind::Review => {
                crate::slices::review::review_task_plan(context, qualified_handle)?
            }
            TaskCommandKind::Repair => repair_task_plan(context, qualified_handle, open_mode)?,
            TaskCommandKind::Ship => commands::merge_task_plan(context, qualified_handle)?,
        })
    }

    pub fn execute_task_command_operation<R: Registry>(
        context: &mut CommandContext<R>,
        kind: TaskCommandKind,
        qualified_handle: &str,
        plan: &CommandPlan,
        confirmed: bool,
        runner: &mut impl CommandRunner,
    ) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
        if kind == TaskCommandKind::Ship {
            return execute_ship_task_command_operation(
                context,
                plan,
                confirmed,
                runner,
                qualified_handle,
            );
        }
        if kind == TaskCommandKind::Repair {
            commands::mark_task_check_started(context, qualified_handle)
                .map_err(|error| (error, false))?;
        }
        let outputs = match execute_external_plan(plan, confirmed, runner) {
            Ok(execution) => execution,
            Err(error) if kind == TaskCommandKind::Repair => {
                commands::mark_task_check_failed(context, qualified_handle)
                    .map_err(|mark_error| (mark_error, true))?;
                return Err((error, true));
            }
            Err(error) => return Err((error, false)),
        };
        let state_changed = match kind {
            TaskCommandKind::Review => false,
            TaskCommandKind::Resume => {
                commands::mark_task_opened(context, qualified_handle)
                    .map_err(|error| (error, false))?;
                true
            }
            TaskCommandKind::Ship => {
                commands::mark_task_merged(context, qualified_handle)
                    .map_err(|error| (error, false))?;
                true
            }
            TaskCommandKind::Repair => {
                commands::mark_task_trunk_repaired(context, qualified_handle)
                    .map_err(|error| (error, true))?;
                commands::mark_task_check_succeeded(context, qualified_handle)
                    .map_err(|error| (error, true))?;
                true
            }
        };

        Ok((outputs, state_changed))
    }

    fn execute_ship_task_command_operation<R: Registry>(
        context: &mut CommandContext<R>,
        plan: &CommandPlan,
        confirmed: bool,
        runner: &mut impl CommandRunner,
        qualified_handle: &str,
    ) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
        if !plan.blocked_reasons.is_empty() {
            return Err((
                CommandError::PlanBlocked(plan.blocked_reasons.clone()),
                false,
            ));
        }
        if plan.requires_confirmation && !confirmed {
            return Err((CommandError::ConfirmationRequired, false));
        }

        let mut outputs = Vec::new();
        for (index, command) in plan.commands.iter().enumerate() {
            let output = runner
                .run(command)
                .map_err(|error| (CommandError::CommandRun(error), false))?;
            if output.status_code != 0 {
                let error = CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: command.program.clone(),
                    status_code: output.status_code,
                    stderr: output.stderr.clone(),
                    cwd: command.cwd.clone(),
                });
                let state_changed = if index > 0 {
                    commands::mark_task_merge_failed(
                        context,
                        qualified_handle,
                        merge_error_looks_conflicted(&error),
                    )
                    .map_err(|mark_error| (mark_error, true))?;
                    true
                } else {
                    false
                };
                return Err((error, state_changed));
            }
            outputs.push(output);
        }

        commands::mark_task_merged(context, qualified_handle).map_err(|error| (error, false))?;
        Ok((outputs, true))
    }

    fn merge_error_looks_conflicted(error: &CommandError) -> bool {
        matches!(
            error,
            CommandError::CommandRun(error) if command_run_error_looks_conflicted(error)
        )
    }

    fn command_run_error_looks_conflicted(error: &CommandRunError) -> bool {
        match error {
            CommandRunError::NonZeroExit { stderr, .. } => {
                stderr.to_ascii_lowercase().contains("conflict")
            }
            CommandRunError::SpawnFailed(_) | CommandRunError::MissingStatusCode => false,
        }
    }

    fn repair_task_plan<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
        open_mode: OpenMode,
    ) -> Result<CommandPlan, CommandError> {
        let mut plan =
            commands::trunk_task_plan_with_open_mode(context, qualified_handle, open_mode)?;
        plan.title = format!("repair task: {qualified_handle}");
        if let Ok(check_plan) = commands::check_task_plan(context, qualified_handle) {
            plan.commands.extend(check_plan.commands);
            plan.requires_confirmation |= check_plan.requires_confirmation;
            plan.blocked_reasons.extend(check_plan.blocked_reasons);
        }
        Ok(plan)
    }
}

pub mod drop_task {
    use crate::{
        adapters::{
            CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter, TmuxAdapter,
        },
        commands::{
            self, CommandContext, CommandError, CommandPlan, DropObservation, DropOp, ResourceState,
        },
        models::{
            LifecycleStatus, SideFlag, StepReceipt, StepReceiptStatus, Task, TaskOperationKind,
        },
        registry::{Registry, RegistryEventKind},
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DropTaskOperationPlan {
        pub confirmation_plan: CommandPlan,
        pub observation: DropObservation,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum DropTaskCompletion {
        Removed,
        TeardownIncomplete { failed_step: DropOp, detail: String },
    }

    pub fn plan_drop_task_operation<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        runner: &mut impl crate::adapters::CommandRunner,
    ) -> Result<DropTaskOperationPlan, CommandError> {
        let confirmation_plan = plan_drop_confirmation(context, qualified_handle)?;
        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == qualified_handle)
            .cloned()
            .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
        if !confirmation_plan.blocked_reasons.is_empty() {
            return Ok(DropTaskOperationPlan {
                confirmation_plan,
                observation: unknown_observation(),
            });
        }

        let observation = commands::observe_drop_resources(context, &task, runner)?;

        Ok(DropTaskOperationPlan {
            confirmation_plan,
            observation,
        })
    }

    pub fn plan_drop_confirmation<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
    ) -> Result<CommandPlan, CommandError> {
        let clean_plan = commands::clean_task_plan(context, qualified_handle)?;
        if clean_plan.blocked_reasons.is_empty() {
            Ok(clean_plan)
        } else {
            commands::remove_task_plan(context, qualified_handle)
        }
    }

    pub fn complete_drop_task_operation<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        final_observation: &DropObservation,
    ) -> Result<DropTaskCompletion, CommandError> {
        let Some(incomplete_step) = commands::plan_drop_from_observation(final_observation)
            .into_iter()
            .next()
        else {
            let task_id = context
                .registry
                .list_tasks()
                .into_iter()
                .find(|task| task.qualified_handle() == qualified_handle)
                .map(|task| task.id.clone())
                .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
            context
                .registry
                .delete_task(&task_id)
                .map_err(CommandError::Registry)?;
            return Ok(DropTaskCompletion::Removed);
        };

        commands::mark_task_removing(context, qualified_handle)?;
        let detail = commands::format_drop_remaining_resources_detail(final_observation);
        commands::mark_task_teardown_incomplete(
            context,
            qualified_handle,
            incomplete_step,
            final_observation,
            Some(&detail),
        )?;
        Ok(DropTaskCompletion::TeardownIncomplete {
            failed_step: incomplete_step,
            detail,
        })
    }

    pub fn execute_drop_task_operation<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        operation: DropTaskOperationPlan,
        confirmed: bool,
        runner: &mut impl CommandRunner,
    ) -> Result<(Vec<CommandOutput>, DropTaskCompletion), CommandError> {
        if !operation.confirmation_plan.blocked_reasons.is_empty() {
            return Err(CommandError::PlanBlocked(
                operation.confirmation_plan.blocked_reasons,
            ));
        }
        if operation.confirmation_plan.requires_confirmation && !confirmed {
            return Err(CommandError::ConfirmationRequired);
        }

        let cleanup_lifecycle = task_is_in_cleanup_lifecycle(context, qualified_handle)?;
        commands::mark_task_removing(context, qualified_handle)?;
        let force = drop_needs_force(
            context,
            qualified_handle,
            &operation.confirmation_plan,
            cleanup_lifecycle,
        )?;
        record_observed_absent_drop_receipts(context, qualified_handle, &operation.observation)?;
        let mut outputs = Vec::new();
        let drop_ops = planned_drop_ops(context, qualified_handle, &operation.observation)?;

        for op in drop_ops {
            match op {
                DropOp::EnsureAgentStopped => {
                    commands::mark_drop_agent_stopped(context, qualified_handle)?;
                    record_drop_step_event(context, qualified_handle, op)?;
                    record_drop_step_receipt(
                        context,
                        qualified_handle,
                        op,
                        StepReceiptStatus::Succeeded,
                    )?;
                }
                DropOp::EnsureTmuxSessionAbsent
                | DropOp::EnsureWorktreeAbsent
                | DropOp::EnsureBranchAbsent => {
                    let command = drop_op_command(context, qualified_handle, op, force)?;
                    let output = runner.run(&command).map_err(CommandError::CommandRun)?;
                    let already_missing = output.status_code != 0
                        && drop_cleanup_resource_is_already_missing(&command, &output);
                    if output.status_code != 0 && !already_missing {
                        let failure_detail = format!(
                            "{} exited with status {}: {}",
                            command.program, output.status_code, output.stderr
                        );
                        record_drop_step_failed_event(
                            context,
                            qualified_handle,
                            op,
                            &failure_detail,
                        )?;
                        let drop_error = CommandError::CommandRun(CommandRunError::NonZeroExit {
                            program: command.program.clone(),
                            status_code: output.status_code,
                            stderr: output.stderr.clone(),
                            cwd: command.cwd.clone(),
                        });
                        mark_observed_drop_failure(
                            context,
                            qualified_handle,
                            op,
                            Some(&failure_detail),
                            runner,
                        )?;
                        return Err(drop_error);
                    }
                    outputs.push(output);
                    commands::mark_task_cleanup_step_completed(
                        context,
                        qualified_handle,
                        &command,
                    )?;
                    record_drop_step_event(context, qualified_handle, op)?;
                    record_drop_step_receipt(
                        context,
                        qualified_handle,
                        op,
                        if already_missing {
                            StepReceiptStatus::SkippedObserved
                        } else {
                            StepReceiptStatus::Succeeded
                        },
                    )?;
                }
            }
        }

        let final_task = task(context, qualified_handle)?.clone();
        let final_observation = commands::observe_drop_resources(context, &final_task, runner)?;
        let completion =
            complete_drop_task_operation(context, qualified_handle, &final_observation)?;

        Ok((outputs, completion))
    }

    fn unknown_observation() -> DropObservation {
        DropObservation {
            agent: ResourceState::Unknown,
            tmux_session: ResourceState::Unknown,
            worktree: ResourceState::Unknown,
            branch: ResourceState::Unknown,
        }
    }

    fn task_is_in_cleanup_lifecycle<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
    ) -> Result<bool, CommandError> {
        Ok(matches!(
            task(context, qualified_handle)?.lifecycle_status,
            LifecycleStatus::Merged | LifecycleStatus::Cleanable
        ))
    }

    fn task<'a, R: Registry>(
        context: &'a CommandContext<R>,
        qualified_handle: &str,
    ) -> Result<&'a Task, CommandError> {
        context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == qualified_handle)
            .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))
    }

    fn drop_op_command<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
        op: DropOp,
        force: bool,
    ) -> Result<CommandSpec, CommandError> {
        let task = task(context, qualified_handle)?;
        let repo_path = context
            .config
            .repos
            .iter()
            .find(|repo| repo.name == task.repo)
            .map(|repo| repo.path.display().to_string())
            .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
        let git = GitAdapter::new("git");
        let tmux = TmuxAdapter::new("tmux");
        let command = match op {
            DropOp::EnsureTmuxSessionAbsent => tmux.kill_session(&task.tmux_session),
            DropOp::EnsureWorktreeAbsent if force => {
                git.force_remove_worktree(&repo_path, &task.worktree_path.display().to_string())
            }
            DropOp::EnsureWorktreeAbsent => {
                git.remove_worktree(&repo_path, &task.worktree_path.display().to_string())
            }
            DropOp::EnsureBranchAbsent if force => {
                git.force_delete_branch(&repo_path, &task.branch)
            }
            DropOp::EnsureBranchAbsent => git.delete_branch(&repo_path, &task.branch),
            DropOp::EnsureAgentStopped => {
                return Err(CommandError::PlanBlocked(vec![format!(
                    "drop op {op:?} does not have an external command"
                )]));
            }
        };
        Ok(command)
    }

    fn drop_needs_force<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
        confirmation_plan: &CommandPlan,
        cleanup_lifecycle: bool,
    ) -> Result<bool, CommandError> {
        if confirmation_plan.title.starts_with("remove task:") {
            return Ok(true);
        }
        let task = task(context, qualified_handle)?;
        if cleanup_lifecycle {
            return Ok(task.has_side_flag(SideFlag::Dirty)
                || task.has_side_flag(SideFlag::Conflicted)
                || task.git_status.as_ref().is_some_and(|status| {
                    status.dirty || status.untracked_files > 0 || status.conflicted
                }));
        }
        Ok(task.has_side_flag(SideFlag::Dirty)
            || task.has_side_flag(SideFlag::Conflicted)
            || task.has_side_flag(SideFlag::Unpushed)
            || task.git_status.as_ref().is_some_and(|status| {
                status.dirty
                    || status.untracked_files > 0
                    || status.conflicted
                    || status.unpushed_commits > 0
                    || !status.merged
            }))
    }

    fn record_drop_step_event<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        op: DropOp,
    ) -> Result<(), CommandError> {
        let task_id = task(context, qualified_handle)?.id.clone();
        context
            .registry
            .record_event(
                task_id,
                RegistryEventKind::SubstrateChanged,
                format!("drop step completed: {}", commands::drop_op_label(op)),
            )
            .map_err(CommandError::Registry)
    }

    fn record_drop_step_failed_event<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        op: DropOp,
        detail: &str,
    ) -> Result<(), CommandError> {
        let task_id = task(context, qualified_handle)?.id.clone();
        context
            .registry
            .record_event(
                task_id,
                RegistryEventKind::LifecycleChanged,
                format!(
                    "drop step failed: {}: {detail}",
                    commands::drop_op_label(op)
                ),
            )
            .map_err(CommandError::Registry)
    }

    fn record_observed_absent_drop_receipts<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        observation: &DropObservation,
    ) -> Result<(), CommandError> {
        for (op, state) in [
            (DropOp::EnsureTmuxSessionAbsent, observation.tmux_session),
            (DropOp::EnsureWorktreeAbsent, observation.worktree),
            (DropOp::EnsureBranchAbsent, observation.branch),
        ] {
            if state == ResourceState::Absent {
                record_drop_step_receipt(
                    context,
                    qualified_handle,
                    op,
                    StepReceiptStatus::SkippedObserved,
                )?;
            }
        }

        Ok(())
    }

    fn record_drop_step_receipt<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        op: DropOp,
        status: StepReceiptStatus,
    ) -> Result<(), CommandError> {
        let task = task(context, qualified_handle)?.clone();
        context
            .registry
            .record_step_receipt(drop_step_receipt(&task, op, status))
            .map_err(CommandError::Registry)
    }

    fn drop_step_receipt(task: &Task, op: DropOp, status: StepReceiptStatus) -> StepReceipt {
        let (step_key, target) = match op {
            DropOp::EnsureAgentStopped => ("agent_stopped", task.tmux_session.clone()),
            DropOp::EnsureTmuxSessionAbsent => ("tmux_session_absent", task.tmux_session.clone()),
            DropOp::EnsureWorktreeAbsent => {
                ("worktree_absent", task.worktree_path.display().to_string())
            }
            DropOp::EnsureBranchAbsent => ("branch_absent", task.branch.clone()),
        };

        StepReceipt::new(
            task.id.clone(),
            TaskOperationKind::Drop,
            step_key,
            target,
            status,
            serde_json::json!({
                "source": "command_result",
                "step": step_key,
            })
            .to_string(),
        )
    }

    fn planned_drop_ops<R: Registry>(
        context: &CommandContext<R>,
        qualified_handle: &str,
        observation: &DropObservation,
    ) -> Result<Vec<DropOp>, CommandError> {
        let task_id = task(context, qualified_handle)?.id.clone();
        let receipts = context
            .registry
            .step_receipts_for_task(&task_id)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        Ok(commands::plan_drop_from_observation_for_task(
            observation,
            &receipts,
        ))
    }

    fn mark_observed_drop_failure<R: Registry>(
        context: &mut CommandContext<R>,
        qualified_handle: &str,
        failed_step: DropOp,
        failure_detail: Option<&str>,
        runner: &mut impl CommandRunner,
    ) -> Result<(), CommandError> {
        let task = task(context, qualified_handle)?.clone();
        let observation =
            commands::observe_drop_resources(context, &task, runner).unwrap_or(DropObservation {
                agent: ResourceState::Unknown,
                tmux_session: ResourceState::Unknown,
                worktree: ResourceState::Unknown,
                branch: ResourceState::Unknown,
            });
        commands::mark_task_teardown_incomplete(
            context,
            qualified_handle,
            failed_step,
            &observation,
            failure_detail,
        )
    }

    fn drop_cleanup_resource_is_already_missing(
        command: &CommandSpec,
        output: &CommandOutput,
    ) -> bool {
        if output.status_code == 0 {
            return false;
        }

        let stderr = output.stderr.to_ascii_lowercase();
        if command.program == "tmux"
            && command
                .args
                .first()
                .is_some_and(|arg| arg == "kill-session")
        {
            return stderr.contains("can't find session")
                || stderr.contains("no server running")
                || stderr.contains("session not found");
        }

        if command.program == "git"
            && command.args.iter().any(|arg| arg == "worktree")
            && command.args.iter().any(|arg| arg == "remove")
        {
            return git_error_says_worktree_missing(&stderr);
        }

        command.program == "git"
            && command.args.iter().any(|arg| arg == "branch")
            && (command.args.iter().any(|arg| arg == "-d")
                || command.args.iter().any(|arg| arg == "-D"))
            && git_error_says_branch_missing(&stderr)
    }

    fn git_error_says_worktree_missing(stderr: &str) -> bool {
        stderr.contains("no such file or directory")
            || stderr.contains("is not a working tree")
            || stderr.contains("is not a worktree")
            || stderr.contains("does not exist")
    }

    fn git_error_says_branch_missing(stderr: &str) -> bool {
        stderr.contains("not found")
            || stderr.contains("not a branch")
            || stderr.contains("no such branch")
            || stderr.contains("not a valid branch name")
    }
}

pub mod sweep_cleanup {
    use crate::{
        adapters::{CommandOutput, CommandRunError, CommandRunner},
        commands::{self, CommandContext, CommandError},
        registry::Registry,
    };

    pub fn execute_sweep_cleanup_operation<R: Registry>(
        context: &mut CommandContext<R>,
        confirmed: bool,
        runner: &mut impl CommandRunner,
    ) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
        let mut outputs = Vec::new();
        let mut state_changed = false;
        let candidates = commands::sweep_cleanup_candidates(context);

        for candidate in &candidates {
            let plan = commands::clean_task_plan(context, candidate)
                .map_err(|error| (error, state_changed))?;
            if !plan.blocked_reasons.is_empty() {
                return Err((
                    CommandError::PlanBlocked(plan.blocked_reasons),
                    state_changed,
                ));
            }
            if plan.requires_confirmation && !confirmed {
                return Err((CommandError::ConfirmationRequired, state_changed));
            }

            for command in &plan.commands {
                let output = runner
                    .run(command)
                    .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
                if output.status_code != 0 {
                    return Err((
                        CommandError::CommandRun(CommandRunError::NonZeroExit {
                            program: command.program.clone(),
                            status_code: output.status_code,
                            stderr: output.stderr.clone(),
                            cwd: command.cwd.clone(),
                        }),
                        state_changed,
                    ));
                }
                outputs.push(output);
                state_changed |=
                    commands::mark_task_cleanup_step_completed(context, candidate, command)
                        .map_err(|error| (error, state_changed))?;
            }
            commands::mark_task_removed(context, candidate)
                .map_err(|error| (error, state_changed))?;
            state_changed = true;
        }

        Ok((outputs, state_changed))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::drop_task::{
        complete_drop_task_operation, execute_drop_task_operation, plan_drop_task_operation,
        DropTaskCompletion,
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
            AgentClient, GitStatus, LifecycleStatus, LiveStatusKind, SideFlag, Task, TaskId,
            TaskOperationKind, TmuxStatus, WorktrunkStatus,
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
            "worktrunk",
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
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
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
            "worktrunk",
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
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/repo/web__worktrees/ajax-fix-login",
        ));
        context.registry.create_task(task).unwrap();
        context
    }

    fn context_with_two_cleanable_tasks() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_cleanable_task();
        if let Some(task) = context.registry.get_task_mut(&TaskId::new("web/fix-login")) {
            task.tmux_status = None;
            task.worktrunk_status = None;
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
            "worktrunk",
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
    fn operation_kernel_handles_confirmation_blocking_nonzero_and_success() {
        let mut blocked_plan = CommandPlan::new("blocked");
        blocked_plan.blocked_reasons = vec!["not ready".to_string()];
        let mut runner = RecordingQueuedRunner::default();
        assert_eq!(
            execute_external_plan(&blocked_plan, true, &mut runner),
            Err(CommandError::PlanBlocked(vec!["not ready".to_string()]))
        );
        assert!(runner.commands.is_empty());

        let mut confirmation_plan = CommandPlan::new("confirm");
        confirmation_plan.requires_confirmation = true;
        assert_eq!(
            execute_external_plan(&confirmation_plan, false, &mut runner),
            Err(CommandError::ConfirmationRequired)
        );
        assert!(runner.commands.is_empty());

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

        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let error_state = ["Operation", "ErrorState"].concat();
        let execution = ["struct Operation", "Execution"].concat();
        let state_change_method = ["after", "_state", "_change"].concat();
        let wrapper_type = ["struct Operation", "Plan"].concat();
        let wrapper_constructor = ["Operation", "Plan::new"].concat();

        assert!(!source.contains(&error_state));
        assert!(!source.contains(&execution));
        assert!(!source.contains(&state_change_method));
        assert!(!source.contains(&wrapper_type));
        assert!(!source.contains(&wrapper_constructor));
    }

    #[test]
    fn start_operation_execution_uses_shared_operation_kernel() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let start_module = source
            .split("pub mod start")
            .nth(1)
            .and_then(|source| source.split("pub mod task_command").next())
            .unwrap();

        assert!(start_module.contains("execute_external_plan_with_success"));
        assert!(!start_module.contains("pub struct StartTaskOperationPlan"));
        assert!(!start_module.contains("operation.plan.requires_confirmation"));
        assert!(!start_module.contains("operation.plan.blocked_reasons"));
    }

    #[test]
    fn task_command_operation_returns_plain_execution_result() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let task_command_module = source
            .split("pub mod task_command")
            .nth(1)
            .and_then(|source| source.split("pub mod drop_task").next())
            .unwrap();

        assert!(!task_command_module.contains("pub struct TaskCommandOperationExecution"));
        assert!(!task_command_module.contains("pub struct TaskCommandOperationError"));
        assert!(!task_command_module.contains("fn operation_error"));
    }

    #[test]
    fn sweep_cleanup_operation_returns_plain_execution_result() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let sweep_cleanup_module = source
            .split("pub mod sweep_cleanup")
            .nth(1)
            .and_then(|source| source.split("#[cfg(test)]").next())
            .unwrap();
        let sweep_cleanup_plan = ["pub struct ", "SweepCleanupOperationPlan"].concat();

        assert!(!sweep_cleanup_module.contains(&sweep_cleanup_plan));
        assert!(!sweep_cleanup_module.contains("pub struct SweepCleanupOperationExecution"));
        assert!(!sweep_cleanup_module.contains("pub struct SweepCleanupOperationError"));
        assert!(!sweep_cleanup_module.contains("fn operation_error"));
        assert!(!sweep_cleanup_module.contains("plan_sweep_cleanup_operation"));
    }

    #[test]
    fn drop_operation_returns_plain_execution_result() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let drop_module = source
            .split("pub mod drop_task")
            .nth(1)
            .and_then(|source| source.split("pub mod sweep_cleanup").next())
            .unwrap();

        assert!(!drop_module.contains("pub struct DropTaskOperationExecution"));
        assert!(!drop_module.contains("-> Option<StepReceipt>"));
        assert!(!drop_module.contains("let Some(receipt)"));
    }

    #[test]
    fn drop_operation_plan_does_not_duplicate_confirmation_state() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let plan_fields = source
            .split("pub struct DropTaskOperationPlan")
            .nth(1)
            .and_then(|source| source.split("pub enum DropTaskCompletion").next())
            .unwrap();

        assert!(!plan_fields.contains("pub requires_confirmation"));
        assert!(!plan_fields.contains("pub blocked_reasons"));
        assert!(!plan_fields.contains("pub cleanup_lifecycle"));
        assert!(!plan_fields.contains("pub intent"));
        assert!(!plan_fields.contains("pub ops"));
    }

    #[test]
    fn operation_errors_use_plain_tuples_without_constructor_helpers() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let task_command_module = source
            .split("pub mod task_command")
            .nth(1)
            .and_then(|source| source.split("pub mod drop_task").next())
            .unwrap();
        let sweep_cleanup_module = source
            .split("pub mod sweep_cleanup")
            .nth(1)
            .and_then(|source| source.split("#[cfg(test)]").next())
            .unwrap();

        assert!(!task_command_module.contains("fn operation_error"));
        assert!(!sweep_cleanup_module.contains("fn operation_error"));
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
        assert_eq!(intent.worktrunk_window, "worktrunk");
        assert_eq!(intent.selected_agent, AgentClient::Codex);
        assert_eq!(plan.title, "create task: Fix login");
        assert_eq!(plan.commands.len(), 4);
        assert!(crate::commands::is_new_task_husky_hook_command(
            &plan.commands[1]
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
                    "ajax-web-fix-login:worktrunk",
                ),
            ]
        );
    }

    #[test]
    fn task_command_operation_plans_single_task_commands_without_derived_policy_fields() {
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

        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/task_operations.rs"),
        )
        .unwrap();
        let production_source = source.split("#[cfg(test)]").next().unwrap();
        let refresh_policy = ["TaskCommand", "RefreshPolicy"].concat();
        let post_execution = ["TaskCommand", "PostExecution"].concat();
        let task_command_plan = ["pub struct ", "TaskCommandOperationPlan"].concat();

        assert!(!production_source.contains(&task_command_plan));
        assert!(!source.contains(&refresh_policy));
        assert!(!source.contains(&post_execution));
    }

    #[test]
    fn resume_and_review_task_operations_execute_in_core_with_reducers() {
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

    #[test]
    fn ship_task_operation_marks_merged_or_records_merge_failure() {
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
    fn repair_task_operation_marks_check_success_or_failure_in_core() {
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
                "worktrunk",
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
    fn sweep_cleanup_operation_executes_candidates_and_reports_partial_failure_state() {
        let mut context = context_with_two_cleanable_tasks();
        let plan = crate::commands::sweep_cleanup_plan(&context);
        let mut runner =
            RecordingQueuedRunner::new(plan.commands.iter().map(|_| output(0, "", "")).collect());

        let (outputs, state_changed) =
            execute_sweep_cleanup_operation(&mut context, true, &mut runner).unwrap();

        assert_eq!(outputs.len(), plan.commands.len());
        assert!(state_changed);
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-sidebar"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );

        let mut context = context_with_two_cleanable_tasks();
        let candidates = crate::commands::sweep_cleanup_candidates(&context);
        let first_candidate_command_count =
            crate::commands::clean_task_plan(&context, &candidates[0])
                .unwrap()
                .commands
                .len();
        let mut outputs: Vec<CommandOutput> = (0..first_candidate_command_count)
            .map(|_| output(0, "", ""))
            .collect();
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
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-sidebar"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Cleanable
        );
    }
}
