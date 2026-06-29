use crate::{
    adapters::{
        CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter, TmuxAdapter,
    },
    commands::{
        self, CommandContext, CommandError, CommandPlan, DropObservation, DropOp, ResourceState,
    },
    models::{LifecycleStatus, SideFlag, StepReceipt, StepReceiptStatus, Task, TaskOperationKind},
    registry::{Registry, RegistryEventKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropTaskOperationPlan {
    pub confirmation_plan: CommandPlan,
    pub observation: DropObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DropExecutionDecision {
    InProcess,
    Command(CommandSpec),
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
        match drop_op_execution_decision(context, qualified_handle, op, force)? {
            DropExecutionDecision::InProcess => {
                commands::mark_drop_agent_stopped(context, qualified_handle)?;
                record_drop_step_event(context, qualified_handle, op)?;
                record_drop_step_receipt(
                    context,
                    qualified_handle,
                    op,
                    StepReceiptStatus::Succeeded,
                )?;
            }
            DropExecutionDecision::Command(command) => {
                let output = runner.run(&command).map_err(CommandError::CommandRun)?;
                let already_missing = output.status_code != 0
                    && drop_cleanup_resource_is_already_missing(&command, &output);
                if output.status_code != 0 && !already_missing {
                    let failure_detail = format!(
                        "{} exited with status {}: {}",
                        command.program, output.status_code, output.stderr
                    );
                    record_drop_step_failed_event(context, qualified_handle, op, &failure_detail)?;
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
                commands::mark_task_cleanup_step_completed(context, qualified_handle, &command)?;
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
    let completion = complete_drop_task_operation(context, qualified_handle, &final_observation)?;

    Ok((outputs, completion))
}

pub(super) fn drop_op_execution_decision<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    op: DropOp,
    force: bool,
) -> Result<DropExecutionDecision, CommandError> {
    Ok(match op {
        DropOp::EnsureAgentStopped => DropExecutionDecision::InProcess,
        DropOp::EnsureTmuxSessionAbsent
        | DropOp::EnsureWorktreeAbsent
        | DropOp::EnsureBranchAbsent => {
            DropExecutionDecision::Command(drop_op_command(context, qualified_handle, op, force)?)
        }
    })
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
        DropOp::EnsureWorktreeAbsent if force => fast_remove_worktree(&repo_path, task)?,
        DropOp::EnsureWorktreeAbsent => {
            git.remove_worktree(&repo_path, &task.worktree_path.display().to_string())
        }
        DropOp::EnsureBranchAbsent if force => git.force_delete_branch(&repo_path, &task.branch),
        DropOp::EnsureBranchAbsent => git.delete_branch(&repo_path, &task.branch),
        DropOp::EnsureAgentStopped => {
            return Err(CommandError::PlanBlocked(vec![format!(
                "drop op {op:?} does not have an external command"
            )]));
        }
    };
    Ok(command)
}

fn fast_remove_worktree(repo_path: &str, task: &Task) -> Result<CommandSpec, CommandError> {
    let worktree_path = task.worktree_path.display().to_string();
    let trash_path = fast_remove_trash_path(task)?;
    Ok(CommandSpec::new(
        "sh",
        [
            "-c",
            "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }",
            "ajax-fast-worktree-remove",
            repo_path,
            &worktree_path,
            &trash_path,
        ],
    ))
}

fn fast_remove_trash_path(task: &Task) -> Result<String, CommandError> {
    let parent = task
        .worktree_path
        .parent()
        .ok_or_else(|| CommandError::TaskNotFound(task.qualified_handle()))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| CommandError::CommandRun(CommandRunError::SpawnFailed(error.to_string())))?
        .as_nanos();
    Ok(parent
        .join(".ajax-trash")
        .join(format!("{}-{nanos}", task.handle))
        .display()
        .to_string())
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
    for op in commands::DROP_TEARDOWN_ORDER {
        if op.records_observed_absent_receipt()
            && op.observed_state(observation) == ResourceState::Absent
        {
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
    let step_key = op.step_key();

    StepReceipt::new(
        task.id.clone(),
        TaskOperationKind::Drop,
        step_key,
        op.receipt_target(task),
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
    let task = task(context, qualified_handle)?;
    let mut receipts = context
        .registry
        .step_receipts_for_task(&task.id)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(failed_step_key) = task.metadata.get("drop_failed_step_key") {
        receipts.retain(|receipt| {
            receipt.step_key != *failed_step_key
                || !receipt_matches_present_resource(receipt.step_key.as_str(), observation)
        });
    }
    Ok(commands::plan_drop_from_observation_for_task(
        observation,
        &receipts,
    ))
}

fn receipt_matches_present_resource(step_key: &str, observation: &DropObservation) -> bool {
    commands::DROP_TEARDOWN_ORDER
        .into_iter()
        .find(|op| op.step_key() == step_key)
        .is_some_and(|op| op.observed_state(observation) == ResourceState::Present)
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

fn drop_cleanup_resource_is_already_missing(command: &CommandSpec, output: &CommandOutput) -> bool {
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
