use crate::{
    adapters::{environment::local_branch_exists, CommandOutput, CommandRunner},
    commands::{
        self, CommandContext, CommandError, CommandPlan, NewTaskRequest, OpenMode,
        StartPlanObservation, StartProvisioningStep,
    },
    models::{StepReceipt, Task, TaskIntent, TaskOperationKind},
    registry::Registry,
    task_operations::kernel::execute_external_plan_with_success,
};

pub fn plan_start_task_operation<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
) -> Result<(TaskIntent, CommandPlan), CommandError> {
    // Derive the same handle the start planner would use, then form the
    // `ajax/<handle>` branch without re-implementing slugify. The repo/handle
    // identity is already public via `start_task_identity`.
    let branch = format!(
        "ajax/{}",
        commands::start_task_identity(&request.repo, &request.title)
            .as_str()
            .split_once('/')
            .map(|(_, handle)| handle)
            .unwrap_or_default()
    );
    let target_branch_exists = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo)
        .is_some_and(|repo| local_branch_exists(&repo.path, &branch));
    plan_start_task_operation_with_observation(
        context,
        request,
        StartPlanObservation {
            origin_fetch_age: None,
            target_branch_exists,
        },
    )
}

pub fn plan_start_task_operation_with_observation<R: Registry>(
    context: &CommandContext<R>,
    request: NewTaskRequest,
    observation: StartPlanObservation,
) -> Result<(TaskIntent, CommandPlan), CommandError> {
    let task = commands::task_from_new_request(context, &request)?;
    let plan = commands::new_task_plan_with_observation(context, request, &observation)?;

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
    execute_start_task_operation_with_checkpoint(
        context,
        runner,
        request,
        plan,
        confirmed,
        open_mode,
        |_| Ok(()),
    )
}

pub fn execute_start_task_operation_with_checkpoint<R: Registry>(
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
        match execute_external_plan_with_success(plan, confirmed, runner, |index, _, _| {
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
    outputs.extend(crate::task_operations::kernel::execute_external_plan(
        &open_plan, true, runner,
    )?);

    let task = context.registry.get_task(&task.id).cloned().unwrap_or(task);

    Ok((outputs, task))
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
            format!("{}:{}", task.tmux_session, task.task_window),
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
