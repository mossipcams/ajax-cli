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
            RecommendedAction::MergeTask => Some(Self::Merge),
            RecommendedAction::CleanTask => Some(Self::Clean),
            RecommendedAction::OpenTask => Some(Self::Open),
            RecommendedAction::SelectProject
            | RecommendedAction::NewTask
            | RecommendedAction::Status => None,
        }
    }

    pub(crate) fn plan<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
    ) -> Result<commands::CommandPlan, CommandError> {
        self.plan_with_open_mode(context, task, commands::OpenMode::Attach)
    }

    pub(crate) fn plan_with_open_mode<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
        open_mode: commands::OpenMode,
    ) -> Result<commands::CommandPlan, CommandError> {
        match self {
            Self::Open => commands::open_task_plan(context, task, open_mode),
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

    pub(crate) fn returns_to_cockpit_after_execute(self) -> bool {
        matches!(self, Self::Check | Self::Diff | Self::Merge | Self::Clean)
    }
}

pub(crate) fn render_task_command<R: CommandRunner>(
    operation: TaskCommandOperation,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    open_mode: commands::OpenMode,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let plan = operation
        .plan_with_open_mode(context, task, open_mode)
        .map_err(command_error)?;
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
