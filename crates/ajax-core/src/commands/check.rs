use super::lookup::find_task;
use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::CommandSpec,
    capability_policy,
    live::LiveStatusKind,
    models::{LifecycleStatus, LiveObservation, SideFlag, TaskCondition, TaskConditionKind},
    registry::Registry,
};

pub fn check_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("check task: {qualified_handle}"));
    let reasons = capability_policy::review_blocked_reasons(task);
    if !reasons.is_empty() {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let Some(test_command) = context
        .config
        .test_commands
        .iter()
        .find(|test_command| test_command.repo == task.repo)
    else {
        return Err(CommandError::PlanBlocked(vec![format!(
            "no test command configured for repo {}",
            task.repo
        )]));
    };
    plan.commands.push(
        CommandSpec::new("sh", ["-lc", test_command.command.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

pub fn mark_task_check_started<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TestsRunning,
            "check running",
        ));
        task.remove_side_flag(SideFlag::TestsFailed);
        task.clear_condition(TaskConditionKind::LatestCheckFailed);
    }
    Ok(())
}

pub fn mark_task_check_succeeded<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Active | LifecycleStatus::Waiting
    ) {
        context
            .registry
            .update_lifecycle(&task.id, LifecycleStatus::Reviewable)
            .map_err(CommandError::Registry)?;
    }
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.remove_side_flag(SideFlag::TestsFailed);
        task.clear_condition(TaskConditionKind::LatestCheckFailed);
        if task
            .live_status
            .as_ref()
            .is_some_and(|status| status.kind == LiveStatusKind::TestsRunning)
        {
            task.live_status = None;
        }
    }
    Ok(())
}

pub fn mark_task_check_failed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        task.add_side_flag(SideFlag::TestsFailed);
        task.record_condition(TaskCondition::latest_check_failed(
            std::time::SystemTime::now(),
        ));
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "check failed",
        ));
    }
    Ok(())
}
