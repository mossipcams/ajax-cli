use super::lookup::find_task;
use super::{CommandContext, CommandError, CommandPlan, OpenMode};
use crate::{
    adapters::TmuxAdapter,
    models::{LiveStatusKind, RuntimeHealth, RuntimeObservationSource, Task},
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    registry::Registry,
};

pub fn open_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("open task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Open)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    if task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists || !status.branch_exists)
        || task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists)
        || task
            .task_window_status
            .as_ref()
            .is_some_and(|status| !status.exists || !status.points_at_expected_path)
    {
        plan.blocked_reasons
            .push("task has missing substrate".to_string());
        return Ok(plan);
    }

    if task.runtime_projection.health == RuntimeHealth::Unobservable
        && task.runtime_projection.source != RuntimeObservationSource::Unknown
        && !live_attention_allows_resume(task)
    {
        plan.blocked_reasons
            .push("runtime state is unobservable; refresh before resume".to_string());
        return Ok(plan);
    }

    if matches!(mode, OpenMode::NoAttach) {
        return Ok(plan);
    }

    let tmux = TmuxAdapter::new("tmux");
    plan.commands
        .push(tmux.select_window(&task.tmux_session, &task.task_window));
    match mode {
        OpenMode::Attach => plan
            .commands
            .push(tmux.attach_window(&task.tmux_session, &task.task_window)),
        OpenMode::SwitchClient => plan
            .commands
            .push(tmux.switch_client_to_window(&task.tmux_session, &task.task_window)),
        OpenMode::NoAttach => unreachable!("handled above"),
    };

    Ok(plan)
}

fn live_attention_allows_resume(task: &Task) -> bool {
    task.live_status.as_ref().is_some_and(|live_status| {
        matches!(
            live_status.kind,
            LiveStatusKind::Blocked
                | LiveStatusKind::WaitingForApproval
                | LiveStatusKind::WaitingForInput
                | LiveStatusKind::RateLimited
                | LiveStatusKind::AuthRequired
                | LiveStatusKind::ContextLimit
                | LiveStatusKind::CommandFailed
        )
    })
}

pub fn mark_task_opened<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    mark_task_opened_at(context, qualified_handle, std::time::SystemTime::now())
}

/// Clock-injected acknowledgment used by `mark_task_opened` and tests.
///
/// Opening a task records an attention acknowledgment without changing
/// lifecycle. The acknowledgment reducer clears Claude waiting attention while
/// leaving Codex waiting actionable, then cached annotations are refreshed.
pub fn mark_task_opened_at<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    now: std::time::SystemTime,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    if let Some(task) = context.registry.get_task_mut(&task_id) {
        crate::live::acknowledge_attention(task, now);
        task.annotations = crate::attention::annotate(task);
    }
    Ok(())
}
