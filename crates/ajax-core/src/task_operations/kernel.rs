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
