use crate::{
    adapters::{CommandOutput, CommandRunner},
    commands::{self, CommandError, NewTaskRequest, OpenMode, StartPlanObservation},
    external_plan,
    models::{StepReceipt, Task, TaskIntent, TaskOperationKind},
    registry::Registry,
    use_cases::{CommandContext, CommandPlan},
};

pub fn plan<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
) -> Result<(TaskIntent, CommandPlan), CommandError> {
    plan_with_observation(
        context,
        request,
        StartPlanObservation {
            origin_fetch_age: None,
        },
    )
}

pub fn plan_with_observation<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
    observation: StartPlanObservation,
) -> Result<(TaskIntent, CommandPlan), CommandError> {
    let task = commands::task_from_new_request(context, &request)?;
    let plan = commands::new_task_plan_with_observation(context, request, &observation)?;

    Ok((task.intent(), plan))
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: &NewTaskRequest,
    plan: &CommandPlan,
    confirmed: bool,
    open_mode: OpenMode,
) -> Result<(Vec<CommandOutput>, Task), CommandError> {
    execute_with_checkpoint(context, runner, request, plan, confirmed, open_mode, |_| {
        Ok(())
    })
}

pub fn execute_with_checkpoint<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: &NewTaskRequest,
    plan: &CommandPlan,
    confirmed: bool,
    open_mode: OpenMode,
    mut checkpoint: impl FnMut(&CommandContext<R>) -> Result<(), CommandError>,
) -> Result<(Vec<CommandOutput>, Task), CommandError> {
    let task = commands::record_new_task(context, request)?;
    checkpoint(context)?;
    let external_outputs =
        match external_plan::execute_with_success(plan, confirmed, runner, |index, _, _| {
            if let Some(step) = commands::start_provisioning_step_for_command(
                plan.commands.get(index).expect("command index"),
            ) {
                commands::mark_new_task_provisioning_step_completed(context, &task.id, step)?;
                context
                    .registry
                    .record_step_receipt(start_step_receipt(&task, step))
                    .map_err(CommandError::Registry)?;
                checkpoint(context)?;
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
    let mut outputs = external_outputs;

    commands::mark_task_opened(context, &task.qualified_handle())?;
    let open_plan = commands::open_task_plan(context, &task.qualified_handle(), open_mode)?;
    outputs.extend(external_plan::execute(&open_plan, true, runner)?);

    let task = context.registry.get_task(&task.id).cloned().unwrap_or(task);

    Ok((outputs, task))
}

pub fn provision_with_checkpoint<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: &NewTaskRequest,
    plan: &CommandPlan,
    confirmed: bool,
    mut checkpoint: impl FnMut(&CommandContext<R>) -> Result<(), CommandError>,
) -> Result<(Vec<CommandOutput>, Task), CommandError> {
    let task = crate::commands::record_new_task(context, request)?;
    checkpoint(context)?;
    let outputs =
        match external_plan::execute_with_success(plan, confirmed, runner, |index, _, _| {
            if let Some(step) = crate::commands::start_provisioning_step_for_command(
                plan.commands.get(index).expect("command index"),
            ) {
                crate::commands::mark_new_task_provisioning_step_completed(
                    context, &task.id, step,
                )?;
                context
                    .registry
                    .record_step_receipt(start_step_receipt(&task, step))
                    .map_err(CommandError::Registry)?;
                checkpoint(context)?;
            }
            Ok(())
        }) {
            Ok(outputs) => outputs,
            Err(error @ CommandError::CommandRun(_)) => {
                let _ = crate::commands::mark_new_task_provisioning_failed(context, &task.id);
                return Err(error);
            }
            Err(error) => return Err(error),
        };

    let task = context.registry.get_task(&task.id).cloned().unwrap_or(task);
    Ok((outputs, task))
}

fn start_step_receipt(task: &Task, step: crate::commands::StartProvisioningStep) -> StepReceipt {
    let (step_key, target) = match step {
        crate::commands::StartProvisioningStep::WorktreeCreated => {
            ("worktree_created", task.worktree_path.display().to_string())
        }
        crate::commands::StartProvisioningStep::TaskSessionCreated => {
            ("task_session_created", task.tmux_session.clone())
        }
        crate::commands::StartProvisioningStep::AgentCommandSent => (
            "agent_command_sent",
            format!("{}:{}", task.tmux_session, task.worktrunk_window),
        ),
    };
    StepReceipt::succeeded(
        task.id.clone(),
        TaskOperationKind::Start,
        step_key,
        target,
        "{}",
    )
}
