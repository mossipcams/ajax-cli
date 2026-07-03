use crate::{
    attention::annotate,
    models::{Annotation, Evidence, LifecycleStatus, Task},
    output::{
        AnnotationItem, CockpitNextStep, CockpitProjection, CockpitSummary, InboxResponse,
        ReposResponse, TaskCard, TaskSummary, TasksResponse,
    },
    recommended::{available_operator_actions, evidence_label, operator_action},
    remediation::remediations_for_task,
    ui_state::derive_operator_status,
};

pub(super) fn cockpit_summary(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
) -> CockpitSummary {
    CockpitSummary {
        repos: repos.repos.len() as u32,
        tasks: tasks.tasks.len() as u32,
        active_tasks: repos.repos.iter().map(|repo| repo.active_tasks).sum(),
        attention_items: repos.repos.iter().map(|repo| repo.attention_items).sum(),
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
        .filter(|task| {
            is_cockpit_menu_task(task)
                && matches!(
                    derive_operator_status(task).status,
                    crate::ui_state::TaskStatus::Waiting | crate::ui_state::TaskStatus::Error
                )
        })
        .count() as u32
}

pub(super) fn is_visible_task(task: &Task) -> bool {
    task.lifecycle_status != LifecycleStatus::Removed
}

pub(super) fn is_cockpit_menu_task(task: &Task) -> bool {
    crate::ghost_task::is_cockpit_visible_task(task)
}

pub(super) fn task_summary(task: &Task) -> TaskSummary {
    let operator_status = derive_operator_status(task);
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        status: operator_status.status,
        status_explanation: operator_status.explanation.clone(),
        runtime_observation_error: task.runtime_projection.observation_error.clone(),
        needs_attention: !annotate(task).is_empty(),
        live_status: task.live_status.clone(),
        actions: task_actions(task),
    }
}

fn task_actions(task: &Task) -> Vec<String> {
    available_operator_actions(task)
        .into_iter()
        .map(|action| action.as_str().to_string())
        .collect()
}

pub(super) fn task_card(task: &Task) -> TaskCard {
    let operator_status = derive_operator_status(task);
    let plan = operator_action(task);
    let annotations = annotations_for_task(task);
    TaskCard {
        id: task.id.clone(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        status: operator_status.status,
        status_explanation: operator_status.explanation.clone(),
        lifecycle: task.lifecycle_status,
        annotations,
        primary_action: plan.action,
        available_actions: plan.available_actions,
        remediations: remediations_for_task(task),
    }
}

pub(super) fn inbox_from_cards(cards: &[TaskCard]) -> InboxResponse {
    let mut items = cards
        .iter()
        .filter_map(attention_item_from_card)
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
    });
    InboxResponse { items }
}

fn attention_item_from_card(card: &TaskCard) -> Option<AnnotationItem> {
    let annotation = card.annotations.first()?;
    Some(AnnotationItem {
        task_id: card.id.clone(),
        task_handle: card.qualified_handle.clone(),
        reason: attention_reason(card, annotation),
        severity: annotation.severity,
        action: card.primary_action,
    })
}

fn attention_reason(card: &TaskCard, annotation: &Annotation) -> String {
    if matches!(
        annotation.evidence,
        Evidence::Lifecycle(LifecycleStatus::Reviewable | LifecycleStatus::Mergeable)
    ) {
        if let Some(explanation) = card.status_explanation.clone() {
            return explanation;
        }
    }

    evidence_label(&annotation.evidence).to_string()
}

pub(super) fn annotations_for_task(task: &Task) -> Vec<Annotation> {
    annotate(task)
}

pub(super) fn cockpit_projection(tasks: &[&Task], summary: CockpitSummary) -> CockpitProjection {
    let visible: Vec<&Task> = tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect();
    let cards: Vec<TaskCard> = visible.iter().copied().map(task_card).collect();
    let next = inbox_from_cards(&cards)
        .items
        .first()
        .and_then(|item| {
            cards
                .iter()
                .find(|card| card.id == item.task_id)
                .map(|card| (item, card))
        })
        .map(|(item, card)| CockpitNextStep {
            task_id: item.task_id.clone(),
            task_handle: item.task_handle.clone(),
            status: card.status,
            status_explanation: card.status_explanation.clone(),
            action: item.action,
            reason: item.reason.clone(),
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
        models::{
            AgentClient, Annotation, AnnotationKind, Evidence, LifecycleStatus, LiveObservation,
            LiveStatusKind, OperatorAction, RuntimeObservationSource, SideFlag, Task, TaskId,
        },
        output::CockpitSummary,
        remediation::{FIX_CI, RESOLVE_MERGE_CONFLICTS},
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
            "task",
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
        assert!(
            card.available_actions.contains(&card.primary_action),
            "primary action {:?} must be in available_actions {:?}",
            card.primary_action,
            card.available_actions
        );
        assert_eq!(card.primary_action, OperatorAction::Resume);
    }

    #[test]
    fn cockpit_projection_drops_parallel_attention_list() {
        let mut task = task("review");
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.annotations = crate::attention::annotate(&task);
        let tasks = vec![&task];

        let projection = cockpit_projection(tasks.as_slice(), summary());

        assert_eq!(projection.cards[0].annotations.len(), 1);
        assert_eq!(projection.next.unwrap().action, OperatorAction::Resume);
    }

    #[test]
    fn cockpit_menu_visibility_matches_registry_ghost_classifier() {
        let mut broken = task("broken");
        broken.add_side_flag(SideFlag::WorktreeMissing);
        assert!(super::is_cockpit_menu_task(&broken));
        assert!(crate::ghost_task::is_cockpit_visible_task(&broken));

        let mut stale = task("stale");
        stale.add_side_flag(SideFlag::Stale);
        assert!(!super::is_cockpit_menu_task(&stale));
        assert!(!crate::ghost_task::is_cockpit_visible_task(&stale));
    }

    #[test]
    fn cockpit_projection_filters_stale_tasks_but_keeps_missing_substrate_tasks_visible() {
        let healthy = task("healthy");
        let mut stale = task("stale");
        stale.add_side_flag(SideFlag::Stale);
        let mut broken = task("broken");
        broken.add_side_flag(SideFlag::WorktreeMissing);
        let tasks = vec![&healthy, &stale, &broken];

        let projection = cockpit_projection(tasks.as_slice(), summary());

        let handles = projection
            .cards
            .iter()
            .map(|card| card.qualified_handle.as_str())
            .collect::<Vec<_>>();
        assert_eq!(handles, vec!["web/healthy", "web/broken"]);
    }

    #[test]
    fn cockpit_projection_keeps_incomplete_teardown_visible() {
        let mut removing = task("removing");
        removing.lifecycle_status = LifecycleStatus::Removing;
        let mut incomplete = task("incomplete");
        incomplete.lifecycle_status = LifecycleStatus::TeardownIncomplete;
        let mut removed = task("removed");
        removed.lifecycle_status = LifecycleStatus::Removed;
        let tasks = vec![&removing, &incomplete, &removed];

        let projection = cockpit_projection(tasks.as_slice(), summary());

        let handles = projection
            .cards
            .iter()
            .map(|card| card.qualified_handle.as_str())
            .collect::<Vec<_>>();
        assert_eq!(handles, vec!["web/removing", "web/incomplete"]);
    }

    #[test]
    fn task_card_explanation_comes_from_canonical_status_not_annotation_row() {
        let mut review_task = task("review");
        mark_active(&mut review_task).unwrap();
        mark_reviewable(&mut review_task).unwrap();
        review_task.annotations = crate::attention::annotate(&review_task);

        let card = task_card(&review_task);

        assert_eq!(card.status_explanation.as_deref(), Some("Ready for review"));
        assert_ne!(
            card.status_explanation.as_deref(),
            Some(card.annotations[0].row_label().as_str())
        );

        let mut running = task("running");
        mark_active(&mut running).unwrap();
        running.live_status = Some(LiveObservation::new(
            LiveStatusKind::TestsRunning,
            "tests running",
        ));

        let running_card = task_card(&running);

        assert_eq!(
            running_card.status_explanation.as_deref(),
            Some("Running tests")
        );
    }

    #[test]
    fn task_card_carries_canonical_status_and_explanation() {
        let mut task = task("waiting");
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "raw approval summary",
        ));

        let card = task_card(&task);

        assert_eq!(card.status, crate::ui_state::TaskStatus::Waiting);
        assert_eq!(
            card.status_explanation.as_deref(),
            Some("Waiting for approval")
        );
    }

    #[test]
    fn task_card_names_probe_failure_instead_of_showing_unknown_or_idle() {
        let mut task = task("probe-failed");
        mark_active(&mut task).unwrap();
        task.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        let card = task_card(&task);

        assert_eq!(
            card.status_explanation.as_deref(),
            Some("Status unavailable")
        );
        assert!(!card
            .status_explanation
            .as_deref()
            .is_some_and(|explanation| explanation.contains("unknown")));
    }

    #[test]
    fn current_empty_annotations_do_not_fall_back_to_stale_cached_annotations() {
        let mut task = task("running");
        mark_active(&mut task).unwrap();
        task.annotations = vec![Annotation::new(
            AnnotationKind::NeedsMe,
            Evidence::SideFlag(SideFlag::NeedsInput),
        )];

        let annotations = super::annotations_for_task(&task);

        assert!(annotations.is_empty(), "{annotations:?}");
        assert!(task_card(&task).annotations.is_empty());
    }

    #[test]
    fn task_card_includes_remediation_options_for_blocked_task() {
        let mut task = task("ci");
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);

        let card = task_card(&task);

        assert_eq!(card.remediations.len(), 1);
        assert_eq!(card.remediations[0].id, FIX_CI);
    }

    #[test]
    fn task_card_includes_resolve_merge_when_conflicted() {
        let mut task = task("merge");
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.add_side_flag(SideFlag::Conflicted);

        let card = task_card(&task);

        assert_eq!(card.remediations.len(), 1);
        assert_eq!(card.remediations[0].id, RESOLVE_MERGE_CONFLICTS);
    }

    #[test]
    fn stale_cached_annotations_do_not_override_current_primary_action() {
        let mut task = task("review");
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        task.annotations = vec![Annotation::new(
            AnnotationKind::NeedsMe,
            Evidence::SideFlag(SideFlag::NeedsInput),
        )];

        let card = task_card(&task);

        assert_eq!(card.status_explanation.as_deref(), Some("Ready for review"));
        assert!(
            card.available_actions.contains(&card.primary_action),
            "primary action {:?} must be in available_actions {:?}",
            card.primary_action,
            card.available_actions
        );
        assert_eq!(card.primary_action, OperatorAction::Resume);
        assert!(card
            .annotations
            .iter()
            .all(|annotation| annotation.kind != AnnotationKind::NeedsMe));
    }
}
