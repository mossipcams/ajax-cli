use std::collections::BTreeMap;

use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, TmuxAdapter},
    commands::{self, CommandContext, CommandError},
    registry::Registry,
};

use crate::task_operations::drop_task::{complete_drop_task_operation, DropTaskCompletion};

pub fn execute_sweep_cleanup_operation<R: Registry>(
    context: &mut CommandContext<R>,
    confirmed: bool,
    runner: &mut impl CommandRunner,
    orphan_mode: Option<commands::OrphanGcMode>,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    let mut outputs = Vec::new();
    let mut state_changed = false;
    for command in commands::sweep_trash_commands(context) {
        let output = runner
            .run(&command)
            .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
        if output.status_code != 0 {
            return Err((
                CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: command.program.clone(),
                    status_code: output.status_code,
                    stderr: output.stderr,
                    cwd: command.cwd.clone(),
                }),
                state_changed,
            ));
        }
        outputs.push(output);
    }
    let candidates = commands::sweep_cleanup_candidates(context);
    let tmux = TmuxAdapter::new("tmux");
    let shared_sessions = runner
        .run(&tmux.list_sessions())
        .ok()
        .filter(|output| output.status_code == 0)
        .map(|output| output.stdout);
    let mut repo_observations = BTreeMap::<String, commands::RepoDropObservationCache>::new();

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

        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == *candidate)
            .cloned()
            .ok_or_else(|| (CommandError::TaskNotFound(candidate.clone()), state_changed))?;
        let repo_cache = repo_observations.entry(task.repo.clone()).or_default();
        let observation = commands::observe_drop_resources_with_cache(
            context,
            &task,
            runner,
            shared_sessions.as_deref(),
            repo_cache,
        )
        .map_err(|error| (error, state_changed))?;
        match complete_drop_task_operation(context, candidate, &observation)
            .map_err(|error| (error, state_changed))?
        {
            DropTaskCompletion::Removed | DropTaskCompletion::TeardownIncomplete { .. } => {
                state_changed = true;
            }
        }
    }

    if let Some(mode) = orphan_mode {
        let orphan_commands = commands::collect_orphan_gc_commands(context, runner, mode)
            .map_err(|error| (error, state_changed))?;
        if !orphan_commands.is_empty() && !confirmed {
            return Err((CommandError::ConfirmationRequired, state_changed));
        }
        for command in &orphan_commands {
            let output = runner
                .run(command)
                .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
            if output.status_code != 0 {
                return Err((
                    CommandError::CommandRun(CommandRunError::NonZeroExit {
                        program: command.program.clone(),
                        status_code: output.status_code,
                        stderr: output.stderr,
                        cwd: command.cwd.clone(),
                    }),
                    state_changed,
                ));
            }
            outputs.push(output);
        }
    }

    Ok((outputs, state_changed))
}
