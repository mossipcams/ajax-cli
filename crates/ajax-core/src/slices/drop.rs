use crate::{
    adapters::{
        CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter,
        TmuxAdapter,
    },
    capability_policy,
    commands::{CommandError, CommandPlan},
    models::{
        LifecycleStatus, LiveStatusKind, OperatorAction, SideFlag, StepReceipt, StepReceiptStatus,
        Task, TaskConditionKind, TaskOperationKind,
    },
    recommended::{available_built_in_decision, blocked_built_in_decision, TaskActionDecision},
    registry::{Registry, RegistryEventKind},
    use_cases::CommandContext,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropTaskOperationPlan {
    pub confirmation_plan: CommandPlan,
    pub observation: crate::commands::DropObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DropTaskCompletion {
    Removed,
    TeardownIncomplete {
        failed_step: crate::commands::DropOp,
        detail: String,
    },
}

pub fn decision(task: &Task) -> TaskActionDecision {
    if invalid_task_requires_drop(task) {
        return available_built_in_decision(OperatorAction::Drop, "invalid_task", true);
    }

    let clean = capability_policy::clean_blocked_reasons(task);
    let remove = capability_policy::remove_blocked_reasons(task);
    if clean.is_empty() || remove.is_empty() {
        available_built_in_decision(OperatorAction::Drop, "drop", true)
    } else {
        blocked_built_in_decision(
            OperatorAction::Drop,
            first_block_reason(clean, remove),
            true,
        )
    }
}

pub fn resolve_execution_confirmation(
    task: &Task,
    confirmation_plan: &CommandPlan,
    confirmed: bool,
) -> Result<bool, CommandError> {
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(CommandError::PlanBlocked(
            confirmation_plan.blocked_reasons.clone(),
        ));
    }

    let resuming_incomplete =
        task.lifecycle_status == crate::models::LifecycleStatus::TeardownIncomplete;
    let cleanup_lifecycle = matches!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Merged | crate::models::LifecycleStatus::Cleanable
    );
    let facts = task.facts();
    let can_observe_before_confirmation =
        cleanup_lifecycle && !facts.dirty && !facts.conflicted && !facts.unpushed;
    let recent_merge_failure = task
        .latest_condition(TaskConditionKind::MergeFailed)
        .is_some_and(|condition| {
            task.git_status.as_ref().is_none_or(|status| {
                status.conflicted
                    || !task.git_observation.observed
                    || task.git_observation.observed_at <= condition.occurred_at
            })
        });

    if !confirmed
        && !resuming_incomplete
        && ((confirmation_plan.requires_confirmation && !can_observe_before_confirmation)
            || (cleanup_lifecycle && recent_merge_failure))
    {
        return Err(CommandError::ConfirmationRequired);
    }

    Ok(confirmed || resuming_incomplete || can_observe_before_confirmation)
}

pub(crate) fn invalid_task_requires_drop(task: &Task) -> bool {
    let facts = task.facts();
    facts.worktree_missing
        || facts.branch_missing
        || task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::WorktrunkMissing)
        || task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists)
        || task
            .worktrunk_status
            .as_ref()
            .is_some_and(|status| !status.exists || !status.points_at_expected_path)
        || task.live_status.as_ref().is_some_and(|live| {
            matches!(
                live.kind,
                LiveStatusKind::TmuxMissing | LiveStatusKind::WorktrunkMissing
            )
        })
}

fn first_block_reason(clean: Vec<String>, remove: Vec<String>) -> String {
    [clean, remove]
        .into_iter()
        .find_map(|reasons| reasons.into_iter().next())
        .unwrap_or_else(|| "drop is unavailable".to_string())
}

pub fn plan_confirmation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let clean_plan = crate::commands::clean_task_plan(context, qualified_handle)?;
    if clean_plan.blocked_reasons.is_empty() {
        Ok(clean_plan)
    } else {
        crate::commands::remove_task_plan(context, qualified_handle)
    }
}

pub fn plan_operation<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    runner: &mut impl crate::adapters::CommandRunner,
) -> Result<DropTaskOperationPlan, CommandError> {
    let confirmation_plan = plan_confirmation(context, qualified_handle)?;
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

    let observation = crate::commands::observe_drop_resources(context, &task, runner)?;

    Ok(DropTaskOperationPlan {
        confirmation_plan,
        observation,
    })
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    operation: DropTaskOperationPlan,
    confirmed: bool,
    runner: &mut impl crate::adapters::CommandRunner,
) -> Result<(Vec<crate::adapters::CommandOutput>, DropTaskCompletion), CommandError> {
    if !operation.confirmation_plan.blocked_reasons.is_empty() {
        return Err(CommandError::PlanBlocked(
            operation.confirmation_plan.blocked_reasons,
        ));
    }
    if operation.confirmation_plan.requires_confirmation && !confirmed {
        return Err(CommandError::ConfirmationRequired);
    }

    let cleanup_lifecycle = task_is_in_cleanup_lifecycle(context, qualified_handle)?;
    crate::commands::mark_task_removing(context, qualified_handle)?;
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
            crate::commands::DropOp::EnsureAgentStopped => {
                crate::commands::mark_drop_agent_stopped(context, qualified_handle)?;
                record_drop_step_event(context, qualified_handle, op)?;
                record_drop_step_receipt(
                    context,
                    qualified_handle,
                    op,
                    StepReceiptStatus::Succeeded,
                )?;
            }
            crate::commands::DropOp::EnsureTmuxSessionAbsent
            | crate::commands::DropOp::EnsureWorktreeAbsent
            | crate::commands::DropOp::EnsureBranchAbsent => {
                let command = drop_op_command(context, qualified_handle, op, force)?;
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
                crate::commands::mark_task_cleanup_step_completed(
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
    let final_observation = crate::commands::observe_drop_resources(context, &final_task, runner)?;
    let completion = complete(context, qualified_handle, &final_observation)?;

    Ok((outputs, completion))
}

pub fn complete<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    final_observation: &crate::commands::DropObservation,
) -> Result<DropTaskCompletion, CommandError> {
    let Some(incomplete_step) = crate::commands::plan_drop_from_observation(final_observation)
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

    crate::commands::mark_task_removing(context, qualified_handle)?;
    let detail = crate::commands::format_drop_remaining_resources_detail(final_observation);
    crate::commands::mark_task_teardown_incomplete(
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

fn unknown_observation() -> crate::commands::DropObservation {
    crate::commands::DropObservation {
        agent: crate::commands::ResourceState::Unknown,
        tmux_session: crate::commands::ResourceState::Unknown,
        worktree: crate::commands::ResourceState::Unknown,
        branch: crate::commands::ResourceState::Unknown,
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
    op: crate::commands::DropOp,
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
        crate::commands::DropOp::EnsureTmuxSessionAbsent => tmux.kill_session(&task.tmux_session),
        crate::commands::DropOp::EnsureWorktreeAbsent if force => {
            fast_remove_worktree(&repo_path, task)?
        }
        crate::commands::DropOp::EnsureWorktreeAbsent => {
            git.remove_worktree(&repo_path, &task.worktree_path.display().to_string())
        }
        crate::commands::DropOp::EnsureBranchAbsent if force => {
            git.force_delete_branch(&repo_path, &task.branch)
        }
        crate::commands::DropOp::EnsureBranchAbsent => git.delete_branch(&repo_path, &task.branch),
        crate::commands::DropOp::EnsureAgentStopped => {
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
    Ok(CommandSpec {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
                .to_string(),
            "ajax-fast-worktree-remove".to_string(),
            repo_path.to_string(),
            worktree_path,
            trash_path,
        ],
        cwd: None,
        mode: CommandMode::Capture,
        timeout: None,
    })
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
    let facts = task.facts();
    if cleanup_lifecycle {
        return Ok(facts.dirty || facts.conflicted);
    }
    Ok(facts.dirty
        || facts.conflicted
        || facts.unpushed
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.merged))
}

fn record_drop_step_event<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    op: crate::commands::DropOp,
) -> Result<(), CommandError> {
    let task_id = task(context, qualified_handle)?.id.clone();
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::SubstrateChanged,
            format!(
                "drop step completed: {}",
                crate::commands::drop_op_label(op)
            ),
        )
        .map_err(CommandError::Registry)
}

fn record_drop_step_failed_event<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    op: crate::commands::DropOp,
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
                crate::commands::drop_op_label(op)
            ),
        )
        .map_err(CommandError::Registry)
}

fn record_observed_absent_drop_receipts<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    observation: &crate::commands::DropObservation,
) -> Result<(), CommandError> {
    for (op, state) in [
        (
            crate::commands::DropOp::EnsureTmuxSessionAbsent,
            observation.tmux_session,
        ),
        (
            crate::commands::DropOp::EnsureWorktreeAbsent,
            observation.worktree,
        ),
        (
            crate::commands::DropOp::EnsureBranchAbsent,
            observation.branch,
        ),
    ] {
        if state == crate::commands::ResourceState::Absent {
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
    op: crate::commands::DropOp,
    status: StepReceiptStatus,
) -> Result<(), CommandError> {
    let task = task(context, qualified_handle)?.clone();
    context
        .registry
        .record_step_receipt(drop_step_receipt(&task, op, status))
        .map_err(CommandError::Registry)
}

fn drop_step_receipt(
    task: &Task,
    op: crate::commands::DropOp,
    status: StepReceiptStatus,
) -> StepReceipt {
    let (step_key, target) = match op {
        crate::commands::DropOp::EnsureAgentStopped => ("agent_stopped", task.tmux_session.clone()),
        crate::commands::DropOp::EnsureTmuxSessionAbsent => {
            ("tmux_session_absent", task.tmux_session.clone())
        }
        crate::commands::DropOp::EnsureWorktreeAbsent => {
            ("worktree_absent", task.worktree_path.display().to_string())
        }
        crate::commands::DropOp::EnsureBranchAbsent => ("branch_absent", task.branch.clone()),
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
    observation: &crate::commands::DropObservation,
) -> Result<Vec<crate::commands::DropOp>, CommandError> {
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
    Ok(crate::commands::plan_drop_from_observation_for_task(
        observation,
        &receipts,
    ))
}

fn receipt_matches_present_resource(
    step_key: &str,
    observation: &crate::commands::DropObservation,
) -> bool {
    matches!(
        (step_key, observation),
        (
            "tmux_session_absent",
            crate::commands::DropObservation {
                tmux_session: crate::commands::ResourceState::Present,
                ..
            }
        ) | (
            "worktree_absent",
            crate::commands::DropObservation {
                worktree: crate::commands::ResourceState::Present,
                ..
            }
        ) | (
            "branch_absent",
            crate::commands::DropObservation {
                branch: crate::commands::ResourceState::Present,
                ..
            }
        )
    )
}

fn mark_observed_drop_failure<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    failed_step: crate::commands::DropOp,
    failure_detail: Option<&str>,
    runner: &mut impl CommandRunner,
) -> Result<(), CommandError> {
    let task = task(context, qualified_handle)?.clone();
    let observation = crate::commands::observe_drop_resources(context, &task, runner)
        .unwrap_or_else(|_| unknown_observation());
    crate::commands::mark_task_teardown_incomplete(
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

#[cfg(test)]
mod tests {
    use super::resolve_execution_confirmation;
    use crate::{
        commands::{CommandError, CommandPlan},
        models::{AgentClient, LifecycleStatus, Task, TaskCondition, TaskId},
    };

    fn cleanable_task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Cleanable;
        task
    }

    #[test]
    fn drop_confirmation_respects_typed_merge_failure_without_side_flag() {
        let mut task = cleanable_task();
        task.record_condition(TaskCondition::merge_failed(std::time::SystemTime::now()));
        let mut plan = CommandPlan::new("clean task: web/fix-login");
        plan.requires_confirmation = true;

        assert_eq!(
            resolve_execution_confirmation(&task, &plan, false),
            Err(CommandError::ConfirmationRequired)
        );
    }
}
