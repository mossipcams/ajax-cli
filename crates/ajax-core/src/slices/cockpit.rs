use crate::{
    commands::{
        projection::{
            cockpit_projection as build_cockpit_projection, cockpit_summary, count_active_tasks,
            count_attention_items, count_lifecycle, is_cockpit_menu_task, is_visible_task,
            task_summary,
        },
        CommandError,
    },
    config::Config,
    models::{Annotation, LifecycleStatus, Task},
    output::{
        AnnotationItem, CockpitResponse, CockpitView, InboxResponse, InspectResponse, NextResponse,
        RepoSummary, ReposResponse, TasksResponse,
    },
    recommended::{evidence_label, operator_action},
    registry::Registry,
    use_cases::CommandContext,
};

pub fn list_repos<R: Registry>(context: &CommandContext<R>) -> ReposResponse {
    let all_tasks = context.registry.list_tasks();
    list_repos_from_tasks(&context.config, all_tasks.as_slice())
}

pub fn list_tasks<R: Registry>(context: &CommandContext<R>, repo: Option<&str>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    list_tasks_from_tasks(all_tasks.as_slice(), repo)
}

pub fn review_queue<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    review_queue_from_tasks(all_tasks.as_slice())
}

pub fn inspect_task<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<InspectResponse, CommandError> {
    let Some(task) = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
    else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    Ok(InspectResponse {
        task: task_summary(task),
        branch: task.branch.clone(),
        worktree_path: task.worktree_path.display().to_string(),
        tmux_session: task.tmux_session.clone(),
        flags: task
            .side_flags()
            .map(|flag| format!("{flag:?}"))
            .collect::<Vec<_>>(),
    })
}

pub fn inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    inbox_from_tasks(tasks.as_slice())
}

pub fn next<R: Registry>(context: &CommandContext<R>) -> NextResponse {
    NextResponse {
        item: inbox(context).items.into_iter().next(),
    }
}

pub fn status<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    list_tasks(context, None)
}

pub fn cockpit<R: Registry>(context: &CommandContext<R>) -> CockpitResponse {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let tasks = list_tasks_from_tasks(all_tasks.as_slice(), None);
    let review = review_queue_from_tasks(all_tasks.as_slice());
    let inbox = inbox_from_tasks(all_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks, &review, &inbox);
    let next = NextResponse {
        item: inbox.items.first().cloned(),
    };

    CockpitResponse {
        summary,
        repos,
        tasks,
        review,
        inbox,
        next,
    }
}

pub fn cockpit_view<R: Registry>(context: &CommandContext<R>) -> CockpitView {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let inbox = cockpit_inbox_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review, &inbox);
    let projection = build_cockpit_projection(all_tasks.as_slice(), summary);

    CockpitView {
        repos,
        cards: projection.cards,
        inbox,
    }
}

#[cfg(test)]
pub(crate) fn cockpit_projection<R: Registry>(
    context: &CommandContext<R>,
) -> crate::output::CockpitProjection {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let inbox = inbox_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review, &inbox);
    build_cockpit_projection(all_tasks.as_slice(), summary)
}

#[cfg(test)]
pub(crate) fn cockpit_inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    cockpit_inbox_from_tasks(tasks.as_slice())
}

fn list_repos_from_tasks(config: &Config, all_tasks: &[&Task]) -> ReposResponse {
    let repos = config
        .repos
        .iter()
        .map(|repo| {
            let repo_tasks = all_tasks
                .iter()
                .copied()
                .filter(|task| task.repo == repo.name && is_visible_task(task))
                .collect::<Vec<_>>();

            RepoSummary {
                name: repo.name.clone(),
                path: repo.path.display().to_string(),
                active_tasks: count_active_tasks(&repo_tasks),
                attention_items: count_attention_items(&repo_tasks),
                reviewable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Reviewable),
                cleanable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Cleanable),
            }
        })
        .collect();

    ReposResponse { repos }
}

fn list_tasks_from_tasks(tasks: &[&Task], repo: Option<&str>) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| repo.is_none_or(|repo_name| task.repo == repo_name))
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

fn review_queue_from_tasks(tasks: &[&Task]) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| {
            matches!(
                task.lifecycle_status,
                LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
            )
        })
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

fn inbox_from_tasks(tasks: &[&Task]) -> InboxResponse {
    let visible = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    InboxResponse {
        items: annotation_items_matching(visible.as_slice(), |_| true),
    }
}

fn cockpit_inbox_from_tasks(tasks: &[&Task]) -> InboxResponse {
    let visible = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    InboxResponse {
        items: cockpit_status_items(visible.as_slice()),
    }
}

fn annotation_items_matching(
    tasks: &[&Task],
    include: impl Fn(&Annotation) -> bool,
) -> Vec<AnnotationItem> {
    let mut items = tasks
        .iter()
        .copied()
        .filter_map(|task| {
            crate::commands::projection::annotations_for_task(task)
                .into_iter()
                .filter(|annotation| include(annotation))
                .min_by_key(|annotation| annotation.severity)
                .map(|annotation| annotation_item(task, annotation))
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
            .then_with(|| left.reason.cmp(&right.reason))
    });
    items
}

fn annotation_item(task: &Task, annotation: Annotation) -> AnnotationItem {
    AnnotationItem {
        task_id: task.id.clone(),
        task_handle: task.qualified_handle(),
        reason: evidence_label(&annotation.evidence).to_string(),
        severity: annotation.severity,
        action: operator_action(task).action,
    }
}

fn cockpit_status_items(tasks: &[&Task]) -> Vec<AnnotationItem> {
    let mut items = tasks
        .iter()
        .copied()
        .filter_map(|task| {
            let status = crate::ui_state::derive_operator_status(task);
            let severity = match status.status {
                crate::ui_state::TaskStatus::Waiting => 1,
                crate::ui_state::TaskStatus::Error => 2,
                crate::ui_state::TaskStatus::Running | crate::ui_state::TaskStatus::Idle => {
                    return None;
                }
            };
            Some(AnnotationItem {
                task_id: task.id.clone(),
                task_handle: task.qualified_handle(),
                reason: status
                    .explanation
                    .unwrap_or_else(|| status.status.as_str().to_string()),
                severity,
                action: operator_action(task).action,
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
    });
    items
}
