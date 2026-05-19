use super::lookup::find_task;
use super::trunk::trunk_task_plan_with_open_mode;
use super::{CommandContext, CommandError, CommandPlan, OpenMode};
use crate::{
    adapters::TmuxAdapter,
    models::SideFlag,
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    registry::Registry,
};

pub fn open_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let needs_trunk_repair = task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::WorktrunkMissing)
        || task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists)
        || task
            .worktrunk_status
            .as_ref()
            .is_some_and(|status| !status.exists || !status.points_at_expected_path);
    let has_non_tmux_missing_substrate = task.has_side_flag(SideFlag::WorktreeMissing)
        || task.has_side_flag(SideFlag::BranchMissing)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists || !status.branch_exists);
    if needs_trunk_repair && !has_non_tmux_missing_substrate {
        return trunk_task_plan_with_open_mode(context, qualified_handle, mode);
    }

    let mut plan = CommandPlan::new(format!("open task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Open)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let tmux = TmuxAdapter::new("tmux");
    plan.commands
        .push(tmux.select_window(&task.tmux_session, &task.worktrunk_window));
    match mode {
        OpenMode::Attach => plan
            .commands
            .push(tmux.attach_window(&task.tmux_session, &task.worktrunk_window)),
        OpenMode::SwitchClient => plan
            .commands
            .push(tmux.switch_client_to_window(&task.tmux_session, &task.worktrunk_window)),
    };

    Ok(plan)
}

pub fn mark_task_opened<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let _ = find_task(context, qualified_handle)?;
    Ok(())
}
