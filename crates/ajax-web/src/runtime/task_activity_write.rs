use crate::runtime::bridge::RuntimeBridge;
use crate::runtime::state::WebAppState;
use crate::slices::web_session::SessionActivity;
use ajax_core::{
    adapters::CommandRunner,
    models::Task,
    registry::{InMemoryRegistry, Registry as _},
};
use std::time::SystemTime;

impl<C, B> WebAppState<C, B>
where
    C: Clone + CommandRunner,
    B: Clone + RuntimeBridge<C>,
{
    pub(super) fn record_task_activity_in_place(
        &self,
        handle: &str,
        activity: SessionActivity,
    ) -> Result<bool, String> {
        let mut guard = self.shared();
        let Some(previous) = guard
            .context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == handle)
            .cloned()
        else {
            return Ok(false);
        };
        let task_id = previous.id.clone();
        crate::slices::web_session::record_session_activity(
            &mut guard.context,
            handle,
            activity,
            SystemTime::now(),
        )?;

        let shared = &mut *guard;
        let lifecycle_changed = shared
            .context
            .registry
            .get_task(&task_id)
            .is_some_and(|task| task.lifecycle_status != previous.lifecycle_status);
        if lifecycle_changed {
            restore_task(&mut shared.context.registry, previous);
            return Ok(false);
        }

        match shared
            .bridge
            .record_task_activity(&mut shared.context, &task_id)
        {
            Ok(()) => {
                shared.revision = shared.revision.saturating_add(1);
                shared.cockpit_cache = None;
                Ok(true)
            }
            Err(error) => {
                restore_task(&mut shared.context.registry, previous);
                Err(error.to_string())
            }
        }
    }
}

fn restore_task(registry: &mut InMemoryRegistry, task: Task) {
    if let Some(slot) = registry.get_task_mut(&task.id) {
        *slot = task;
    }
}
