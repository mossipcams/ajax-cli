use crate::{
    capability_policy,
    models::{OperatorAction, Task},
    recommended::{available_built_in_decision, blocked_built_in_decision, TaskActionDecision},
};

pub fn decision(task: &Task) -> TaskActionDecision {
    if super::super::drop::invalid_task_requires_drop(task) {
        return blocked_built_in_decision(
            OperatorAction::Review,
            "task has missing substrate",
            false,
        );
    }
    capability_policy::review_blocked_reasons(task)
        .into_iter()
        .next()
        .map(|reason| blocked_built_in_decision(OperatorAction::Review, reason, false))
        .unwrap_or_else(|| available_built_in_decision(OperatorAction::Review, "review", false))
}
