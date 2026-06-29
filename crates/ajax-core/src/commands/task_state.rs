use crate::{
    live::LiveStatusKind,
    models::{LifecycleStatus, LiveObservation, SideFlag, TaskId},
    registry::{Registry, RegistryError},
};

pub(crate) fn mark_task_check_started<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    let Some(task) = registry.get_task_mut(task_id) else {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    };
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::TestsRunning,
        "check running",
    ));
    task.remove_side_flag(SideFlag::TestsFailed);
    Ok(())
}

pub(crate) fn mark_task_check_succeeded<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    let Some(task) = registry.get_task_mut(task_id) else {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    };
    task.remove_side_flag(SideFlag::TestsFailed);
    if task
        .live_status
        .as_ref()
        .is_some_and(|status| status.kind == LiveStatusKind::TestsRunning)
    {
        task.live_status = None;
    }
    Ok(())
}

pub(crate) fn mark_task_check_failed<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    let Some(task) = registry.get_task_mut(task_id) else {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    };
    task.add_side_flag(SideFlag::TestsFailed);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CommandFailed,
        "check failed",
    ));
    Ok(())
}

pub(crate) fn mark_task_merged<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    let Some(task) = registry.get_task_mut(task_id) else {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    };
    task.remove_side_flag(SideFlag::Conflicted);
    if task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CommandFailed && status.summary == "merge failed"
    }) {
        task.live_status = None;
    }
    Ok(())
}

pub(crate) fn mark_task_merge_failed<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
    conflicted: bool,
) -> Result<(), RegistryError> {
    let Some(task) = registry.get_task_mut(task_id) else {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    };
    if conflicted {
        task.add_side_flag(SideFlag::Conflicted);
    }
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CommandFailed,
        "merge failed",
    ));
    Ok(())
}

pub(crate) fn update_check_lifecycle<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    registry.update_lifecycle(task_id, LifecycleStatus::Reviewable)
}

pub(crate) fn update_merge_lifecycle<R: Registry>(
    registry: &mut R,
    task_id: &TaskId,
) -> Result<(), RegistryError> {
    registry.update_lifecycle(task_id, LifecycleStatus::Merged)
}
