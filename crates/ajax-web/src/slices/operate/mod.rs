//! Browser-submitted operator actions.

use ajax_core::{
    adapters::{
        environment::{local_branch_exists, origin_fetch_age},
        CommandOutput, CommandRunError, CommandRunner,
    },
    commands::{self, BranchAdoptionPlan, CommandContext, CommandError, NewTaskRequest, OpenMode},
    models::{LifecycleStatus, OperatorAction, SideFlag},
    registry::Registry,
    remediation::{self, RemediationError},
    task_operations::{
        drop_task::{
            execute_drop_task_operation, plan_drop_confirmation, plan_drop_task_operation,
            DropTaskCompletion,
        },
        operator_dispatch::{
            execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
        },
        start::plan_start_task_operation_with_observation,
    },
};

use crate::adapters::skills::resolve_skill_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperateRequest {
    pub task_handle: String,
    pub action: String,
    pub confirmed: bool,
    pub branch_adoption: Option<BranchAdoptionPlan>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StartTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub orchestration_chat: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperateOutcome {
    pub state_changed: bool,
    pub output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperateError {
    UnknownAction(String),
    UnsupportedCapability(&'static str),
    Command(CommandError, bool),
}

pub fn operate<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: OperateRequest,
) -> Result<OperateOutcome, OperateError> {
    let action = request.action.clone();
    let task = request.task_handle.clone();
    tracing::info!(
        target: "ajax_web",
        action = %action,
        task = %task,
        "operate"
    );

    let result = operate_inner(context, runner, request);

    match &result {
        Ok(_) => tracing::info!(
            target: "ajax_web",
            action = %action,
            task = %task,
            outcome = "ok",
            "operate"
        ),
        Err(error) => tracing::warn!(
            target: "ajax_web",
            action = %action,
            task = %task,
            outcome = "err",
            error = %format_operate_error(error),
            "operate"
        ),
    }

    result
}

fn operate_inner<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: OperateRequest,
) -> Result<OperateOutcome, OperateError> {
    if remediation::is_remediation_action(&request.action) {
        return run_remediation(context, runner, &request.task_handle, &request.action);
    }

    let Some(action) = OperatorAction::from_label(&request.action) else {
        return Err(OperateError::UnknownAction(request.action));
    };

    match action {
        OperatorAction::Drop => execute_drop(context, runner, &request.task_handle, true),
        OperatorAction::Start => Err(OperateError::UnsupportedCapability(
            "start uses the dedicated Web Cockpit new-task operation",
        )),
        OperatorAction::Review
        | OperatorAction::Ship
        | OperatorAction::Repair
        | OperatorAction::Resume => {
            let kind = task_command_kind(action)?;
            execute_task_command(context, runner, kind, &request)
        }
    }
}

/// Test convenience: `start_task_with_checkpoint` with a noop checkpoint.
/// Production callers (ajax-cli) always supply a real checkpoint.
#[cfg(test)]
pub fn start_task<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
) -> Result<OperateOutcome, OperateError> {
    start_task_with_checkpoint(context, runner, request, |_| Ok(()))
}

pub fn start_task_with_checkpoint<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
    checkpoint: impl FnMut(&CommandContext<R>) -> Result<(), ajax_core::commands::CommandError>,
) -> Result<OperateOutcome, OperateError> {
    let repo = request.repo.clone();
    let agent = request.agent.clone();
    tracing::info!(
        target: "ajax_web",
        repo = %repo,
        agent = %agent,
        request_id = %request.request_id,
        "start task"
    );

    let result = start_task_with_checkpoint_inner(context, runner, request, checkpoint);

    match &result {
        Ok(_) => tracing::info!(
            target: "ajax_web",
            repo = %repo,
            agent = %agent,
            outcome = "ok",
            "start task"
        ),
        Err(error) => tracing::warn!(
            target: "ajax_web",
            repo = %repo,
            agent = %agent,
            outcome = "err",
            error = %format_operate_error(error),
            "start task"
        ),
    }

    result
}

fn start_task_with_checkpoint_inner<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
    checkpoint: impl FnMut(&CommandContext<R>) -> Result<(), ajax_core::commands::CommandError>,
) -> Result<OperateOutcome, OperateError> {
    if request.title.trim().is_empty() {
        return Err(OperateError::UnsupportedCapability(
            "start requires a non-empty task title",
        ));
    }
    if !supported_start_agent(&request.agent) {
        return Err(OperateError::UnsupportedCapability("unsupported agent"));
    }
    if request.orchestration_chat && request.agent != "cursor" {
        return Err(OperateError::UnsupportedCapability(
            "orchestration chat requires the cursor agent",
        ));
    }

    let core_request = NewTaskRequest {
        repo: request.repo,
        title: request.title,
        agent: request.agent,
        agent_start: if request.orchestration_chat {
            commands::AgentStartMode::PreparedSession
        } else {
            commands::AgentStartMode::InteractiveCli
        },
    };
    let observation = start_plan_observation(context, &core_request);
    let (_intent, plan) =
        plan_start_task_operation_with_observation(context, core_request.clone(), observation)
            .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    ajax_core::task_operations::start::execute_start_task_operation_with_checkpoint(
        context,
        runner,
        &core_request,
        &plan,
        confirmed,
        OpenMode::NoAttach,
        checkpoint,
    )
    .map_err(|error| OperateError::Command(error, true))?;

    Ok(OperateOutcome {
        state_changed: true,
        output: format!("started task: {}", core_request.title),
    })
}

fn start_plan_observation<R: Registry>(
    context: &CommandContext<R>,
    request: &NewTaskRequest,
) -> commands::StartPlanObservation {
    let repo = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo);
    let origin_fetch_age = repo.and_then(|repo| origin_fetch_age(&repo.path));
    let branch = format!(
        "ajax/{}",
        commands::start_task_identity(&request.repo, &request.title)
            .as_str()
            .split_once('/')
            .map(|(_, handle)| handle)
            .unwrap_or_default()
    );
    let target_branch_exists = repo.is_some_and(|repo| local_branch_exists(&repo.path, &branch));

    commands::StartPlanObservation {
        origin_fetch_age,
        target_branch_exists,
    }
}

/// Single agent allowlist for web task starts; the route pre-check and the
/// slice validation must never disagree.
pub fn supported_start_agent(agent: &str) -> bool {
    matches!(agent, "codex" | "claude" | "cursor" | "pi")
}

fn execute_task_command<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    kind: TaskCommandKind,
    request: &OperateRequest,
) -> Result<OperateOutcome, OperateError> {
    let task_handle = &request.task_handle;
    if matches!(kind, TaskCommandKind::Review | TaskCommandKind::Repair) {
        let _ = commands::refresh_git_substrate_evidence(context, runner);
    }

    let open_mode = if matches!(kind, TaskCommandKind::Resume | TaskCommandKind::Repair) {
        OpenMode::NoAttach
    } else {
        OpenMode::Attach
    };
    let mut plan = plan_task_command_operation(context, kind, task_handle, open_mode)
        .map_err(|error| OperateError::Command(error, false))?;
    if kind == TaskCommandKind::Repair {
        if let Some(request_adoption) = &request.branch_adoption {
            plan.branch_adoption = Some(request_adoption.clone());
        }
    }
    let confirmed = task_command_confirmed(kind, request, &plan);
    let (outputs, state_changed) =
        execute_task_command_operation(context, kind, task_handle, &plan, confirmed, runner)
            .map_err(|(error, state_changed)| OperateError::Command(error, state_changed))?;

    Ok(OperateOutcome {
        state_changed,
        output: format_execution_outputs(&outputs),
    })
}

fn task_command_confirmed(
    kind: TaskCommandKind,
    request: &OperateRequest,
    plan: &commands::CommandPlan,
) -> bool {
    if kind == TaskCommandKind::Repair && plan.branch_adoption.is_some() {
        return request.branch_adoption.is_some() && request.confirmed;
    }
    if plan.requires_confirmation {
        request.confirmed
    } else {
        true
    }
}

fn execute_drop<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    confirmed: bool,
) -> Result<OperateOutcome, OperateError> {
    let confirmation_plan = plan_drop_confirmation(context, task_handle)
        .map_err(|error| OperateError::Command(error, false))?;
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(OperateError::Command(
            CommandError::PlanBlocked(confirmation_plan.blocked_reasons),
            false,
        ));
    }

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| {
            OperateError::Command(CommandError::TaskNotFound(task_handle.to_string()), false)
        })?;

    let resuming_incomplete = task.lifecycle_status == LifecycleStatus::TeardownIncomplete;
    let can_observe_before_confirmation = matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) && !task.has_side_flag(SideFlag::Dirty)
        && !task.has_side_flag(SideFlag::Conflicted)
        && !task.has_side_flag(SideFlag::Unpushed)
        && task.git_status.as_ref().is_none_or(|status| {
            !status.dirty && !status.conflicted && status.unpushed_commits == 0
        });

    if confirmation_plan.requires_confirmation
        && !confirmed
        && !resuming_incomplete
        && !can_observe_before_confirmation
    {
        return Err(OperateError::Command(
            CommandError::ConfirmationRequired,
            false,
        ));
    }

    let operation = plan_drop_task_operation(context, task_handle, runner)
        .map_err(|error| OperateError::Command(error, false))?;
    let operation_confirmed = confirmed || resuming_incomplete || can_observe_before_confirmation;
    let (outputs, completion) =
        execute_drop_task_operation(context, task_handle, operation, operation_confirmed, runner)
            .map_err(|error| OperateError::Command(error, true))?;

    let output = match completion {
        DropTaskCompletion::Removed => {
            if outputs.is_empty() {
                format!("removed task: {task_handle}")
            } else {
                format_execution_outputs(&outputs)
            }
        }
        DropTaskCompletion::TeardownIncomplete {
            failed_step,
            detail,
        } => {
            return Err(OperateError::Command(
                CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: "drop".to_string(),
                    status_code: 1,
                    stderr: ajax_core::commands::format_drop_teardown_incomplete_message(
                        task_handle,
                        failed_step,
                        &detail,
                    ),
                    cwd: None,
                }),
                true,
            ));
        }
    };

    Ok(OperateOutcome {
        state_changed: true,
        output,
    })
}

fn task_command_kind(action: OperatorAction) -> Result<TaskCommandKind, OperateError> {
    match action {
        OperatorAction::Review => Ok(TaskCommandKind::Review),
        OperatorAction::Ship => Ok(TaskCommandKind::Ship),
        OperatorAction::Repair => Ok(TaskCommandKind::Repair),
        OperatorAction::Resume => Ok(TaskCommandKind::Resume),
        OperatorAction::Start | OperatorAction::Drop => Err(OperateError::UnsupportedCapability(
            "action is handled by a dedicated web operation",
        )),
    }
}

pub fn format_execution_outputs(outputs: &[CommandOutput]) -> String {
    outputs
        .iter()
        .filter_map(|output| {
            let stdout = output.stdout.trim();
            let stderr = output.stderr.trim();
            match (stdout.is_empty(), stderr.is_empty()) {
                (true, true) => None,
                (false, true) => Some(stdout.to_string()),
                (true, false) => Some(stderr.to_string()),
                (false, false) => Some(format!("{stdout}\n{stderr}")),
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn run_remediation<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    remediation_id: &str,
) -> Result<OperateOutcome, OperateError> {
    let skill_name = match remediation_id {
        remediation::FIX_CI => "gh-fix-ci",
        remediation::RESOLVE_MERGE_CONFLICTS => "resolve-merge-conflicts",
        _ => return Err(OperateError::UnknownAction(remediation_id.to_string())),
    };
    let skill_path = resolve_skill_path(skill_name).ok_or(OperateError::UnsupportedCapability(
        "required agent skill is not installed on the companion host",
    ))?;
    let outcome = remediation::execute_remediation(
        context,
        runner,
        task_handle,
        remediation_id,
        &skill_path.display().to_string(),
    )
    .map_err(remediation_error_to_operate)?;
    Ok(OperateOutcome {
        state_changed: outcome.state_changed,
        output: outcome.output,
    })
}

fn remediation_error_to_operate(error: RemediationError) -> OperateError {
    match error {
        RemediationError::UnknownRemediation(id) => OperateError::UnknownAction(id),
        RemediationError::TaskNotFound(handle) => {
            OperateError::Command(CommandError::TaskNotFound(handle), false)
        }
        RemediationError::UnsupportedCapability(message) => {
            OperateError::UnsupportedCapability(message)
        }
        RemediationError::CommandRun(message) => OperateError::Command(
            CommandError::CommandRun(ajax_core::adapters::CommandRunError::SpawnFailed(message)),
            false,
        ),
    }
}

pub fn format_operate_error(error: &OperateError) -> String {
    match error {
        OperateError::UnknownAction(action) => format!("unknown action: {action}"),
        OperateError::UnsupportedCapability(message) => (*message).to_string(),
        OperateError::Command(error, _) => format_command_error(error),
    }
}

pub fn operate_error_code(error: &OperateError) -> &'static str {
    match error {
        OperateError::UnknownAction(_) => "unknown_action",
        OperateError::UnsupportedCapability(message) => {
            if message.to_ascii_lowercase().contains("terminal") {
                "needs_terminal"
            } else {
                "unsupported_action"
            }
        }
        OperateError::Command(command_error, _) => match command_error {
            CommandError::TaskNotFound(_) => "task_not_found",
            CommandError::ConfirmationRequired => "confirmation_required",
            CommandError::PlanBlocked(_) => "conflict",
            _ => "command_failed",
        },
    }
}

fn format_command_error(error: &CommandError) -> String {
    match error {
        CommandError::ConfirmationRequired => {
            "confirmation required — tap again to confirm".to_string()
        }
        CommandError::PlanBlocked(reasons) => reasons.join("; "),
        CommandError::TaskNotFound(handle) => format!("task not found: {handle}"),
        CommandError::RepoNotFound(repo) => format!("repo not found: {repo}"),
        CommandError::Registry(error) => error.to_string(),
        CommandError::CommandRun(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests;
