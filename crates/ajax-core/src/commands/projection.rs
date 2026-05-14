use crate::{
    attention::annotate,
    models::{Annotation, LifecycleStatus, OperatorAction, SideFlag, Task},
    operation::{task_operation_eligibility, TaskOperation},
    output::{
        CockpitNextStep, CockpitProjection, CockpitSummary, InboxResponse, ReposResponse, TaskCard,
        TaskSummary, TasksResponse,
    },
    recommended::operator_action,
    ui_state::derive_ui_state,
};

pub(super) fn cockpit_summary(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
    inbox: &InboxResponse,
) -> CockpitSummary {
    CockpitSummary {
        repos: repos.repos.len() as u32,
        tasks: tasks.tasks.len() as u32,
        active_tasks: repos.repos.iter().map(|repo| repo.active_tasks).sum(),
        attention_items: inbox
            .items
            .iter()
            .map(|item| item.task_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u32,
        reviewable_tasks: review.tasks.len() as u32,
        cleanable_tasks: repos.repos.iter().map(|repo| repo.cleanable_tasks).sum(),
    }
}

pub(super) fn count_lifecycle(tasks: &[&Task], status: LifecycleStatus) -> u32 {
    tasks
        .iter()
        .filter(|task| task.lifecycle_status == status)
        .count() as u32
}

pub(super) fn count_active_tasks(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .filter(|task| {
            task.lifecycle_status == LifecycleStatus::Active && !task.has_missing_substrate()
        })
        .count() as u32
}

pub(super) fn count_attention_items(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .filter(|task| !annotate(task).is_empty())
        .count() as u32
}

pub(super) fn is_visible_task(task: &Task) -> bool {
    task.lifecycle_status != LifecycleStatus::Removed
}

pub(super) fn task_summary(task: &Task) -> TaskSummary {
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        needs_attention: !annotate(task).is_empty(),
        live_status: task.live_status.clone(),
        actions: task_actions(task),
    }
}

fn task_actions(task: &Task) -> Vec<String> {
    if task.has_side_flag(SideFlag::TmuxMissing) || task.has_side_flag(SideFlag::WorktrunkMissing) {
        return vec![OperatorAction::Repair.as_str().to_string()];
    }

    let mut actions = [
        (TaskOperation::Open, OperatorAction::Resume),
        (TaskOperation::Merge, OperatorAction::Ship),
        (TaskOperation::Clean, OperatorAction::Drop),
        (TaskOperation::Remove, OperatorAction::Drop),
    ]
    .into_iter()
    .filter(|(operation, _)| task_operation_eligibility(task, *operation).is_allowed())
    .map(|(_, action)| action.as_str().to_string())
    .collect::<Vec<_>>();
    actions.dedup();
    actions
}

pub(super) fn task_card(task: &Task) -> TaskCard {
    let ui_state = derive_ui_state(task);
    let plan = operator_action(task);
    let annotations = annotations_for_task(task);
    TaskCard {
        id: task.id.clone(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        ui_state,
        lifecycle: task.lifecycle_status,
        annotations,
        primary_action: plan.action,
        available_actions: plan.available_actions,
        live_summary: task.live_status.as_ref().map(|live| live.summary.clone()),
    }
}

fn annotations_for_task(task: &Task) -> Vec<Annotation> {
    if task.annotations.is_empty() {
        annotate(task)
    } else {
        task.annotations.clone()
    }
}

pub(super) fn cockpit_projection(tasks: &[&Task], summary: CockpitSummary) -> CockpitProjection {
    let visible: Vec<&Task> = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect();
    let cards: Vec<TaskCard> = visible.iter().copied().map(task_card).collect();
    let next = cards
        .iter()
        .filter_map(|card| {
            card.annotations
                .iter()
                .min_by_key(|annotation| annotation.severity)
                .map(|annotation| (annotation.severity, card, annotation))
        })
        .min_by(
            |(left_severity, left_card, _), (right_severity, right_card, _)| {
                left_severity
                    .cmp(right_severity)
                    .then_with(|| left_card.qualified_handle.cmp(&right_card.qualified_handle))
            },
        )
        .map(|(_, card, annotation)| CockpitNextStep {
            task_id: card.id.clone(),
            task_handle: card.qualified_handle.clone(),
            ui_state: card.ui_state,
            action: annotation.suggests,
            reason: format!("{:?}", annotation.evidence),
        });
    CockpitProjection {
        counts: summary,
        cards,
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::{cockpit_projection, task_card};
    use crate::{
        lifecycle::{mark_active, mark_reviewable},
        models::{AgentClient, AnnotationKind, LifecycleStatus, OperatorAction, Task, TaskId},
        output::CockpitSummary,
    };

    fn task(handle: &str) -> Task {
        Task::new(
            TaskId::new(format!("task-{handle}")),
            "web",
            handle,
            format!("Task {handle}"),
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/{handle}"),
            format!("ajax-web-{handle}"),
            "worktrunk",
            AgentClient::Codex,
        )
    }

    fn summary() -> CockpitSummary {
        CockpitSummary {
            repos: 1,
            tasks: 1,
            active_tasks: 0,
            attention_items: 1,
            reviewable_tasks: 1,
            cleanable_tasks: 0,
        }
    }

    #[test]
    fn task_card_carries_annotations() {
        let mut task = task("review");
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.annotations = crate::attention::annotate(&task);

        let card = task_card(&task);

        assert_eq!(card.annotations.len(), 1);
        assert_eq!(card.annotations[0].kind, AnnotationKind::Reviewable);
        assert_eq!(card.primary_action, OperatorAction::Review);
    }

    #[test]
    fn cockpit_projection_drops_parallel_attention_list() {
        let mut task = task("review");
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.annotations = crate::attention::annotate(&task);
        let tasks = vec![&task];

        let projection = cockpit_projection(tasks.as_slice(), summary());

        assert_eq!(projection.cards[0].annotations.len(), 1);
        assert_eq!(projection.next.unwrap().action, OperatorAction::Review);
    }
}
