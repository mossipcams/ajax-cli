use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext, CommandError},
    models::RecommendedAction,
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;

use crate::{
    command_error,
    render::{render_execution_outputs, render_plan},
    task_arg, CliError, RenderedCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskCommandOperation {
    Open,
    Trunk,
    Check,
    Diff,
    Merge,
    Clean,
}

impl TaskCommandOperation {
    pub(crate) fn from_cli_subcommand(name: &str) -> Option<Self> {
        match name {
            "open" => Some(Self::Open),
            "trunk" => Some(Self::Trunk),
            "check" => Some(Self::Check),
            "diff" => Some(Self::Diff),
            "merge" => Some(Self::Merge),
            "clean" => Some(Self::Clean),
            _ => None,
        }
    }

    pub(crate) fn from_recommended_action(action: RecommendedAction) -> Option<Self> {
        match action {
            RecommendedAction::CheckTask | RecommendedAction::InspectTestOutput => {
                Some(Self::Check)
            }
            RecommendedAction::DiffTask | RecommendedAction::ReviewDiff => Some(Self::Diff),
            RecommendedAction::MergeTask => Some(Self::Merge),
            RecommendedAction::CleanTask => Some(Self::Clean),
            RecommendedAction::OpenWorktrunk => Some(Self::Trunk),
            RecommendedAction::OpenTask
            | RecommendedAction::InspectAgent
            | RecommendedAction::MonitorTask
            | RecommendedAction::ReviewBranch => Some(Self::Open),
            RecommendedAction::SelectProject
            | RecommendedAction::NewTask
            | RecommendedAction::Reconcile
            | RecommendedAction::InspectTask
            | RecommendedAction::Status => None,
        }
    }

    pub(crate) fn plan<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
    ) -> Result<commands::CommandPlan, CommandError> {
        match self {
            Self::Open => commands::open_task_plan(context, task, commands::OpenMode::Attach),
            Self::Trunk => commands::trunk_task_plan(context, task),
            Self::Check => commands::check_task_plan(context, task),
            Self::Diff => commands::diff_task_plan(context, task),
            Self::Merge => commands::merge_task_plan(context, task),
            Self::Clean => commands::clean_task_plan(context, task),
        }
    }

    pub(crate) fn apply_after_execute<R: Registry>(
        self,
        context: &mut CommandContext<R>,
        task: &str,
    ) -> Result<bool, CommandError> {
        match self {
            Self::Open => {
                commands::mark_task_opened(context, task)?;
                Ok(true)
            }
            Self::Merge => {
                commands::mark_task_merged(context, task)?;
                Ok(true)
            }
            Self::Clean => {
                commands::mark_task_removed(context, task)?;
                Ok(true)
            }
            Self::Trunk | Self::Check | Self::Diff => Ok(false),
        }
    }
}

pub(crate) fn render_task_command<R: CommandRunner>(
    operation: TaskCommandOperation,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let plan = operation.plan(context, task).map_err(command_error)?;
    if !subcommand.get_flag("execute") {
        return Ok(RenderedCommand {
            output: render_plan(plan, subcommand.get_flag("json"))?,
            state_changed: false,
        });
    }

    let outputs =
        commands::execute_plan(&plan, subcommand.get_flag("yes"), runner).map_err(command_error)?;
    let state_changed = operation
        .apply_after_execute(context, task)
        .map_err(command_error)?;
    Ok(RenderedCommand {
        output: render_execution_outputs(&outputs, None),
        state_changed,
    })
}
