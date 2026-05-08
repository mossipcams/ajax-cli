#![deny(unsafe_op_in_unsafe_fn)]

use ajax_core::output::{InboxResponse, ReposResponse, TasksResponse};

pub fn render_cockpit(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
    inbox: &InboxResponse,
) -> String {
    let mut lines = vec![
        "Ajax Cockpit".to_string(),
        format!("Repos: {}", repos.repos.len()),
        format!("Tasks: {}", tasks.tasks.len()),
        format!("Review: {}", review.tasks.len()),
        "Inbox".to_string(),
    ];

    if inbox.items.is_empty() {
        lines.push("no tasks need attention".to_string());
    } else {
        lines.extend(inbox.items.iter().map(|item| {
            format!(
                "{}: {} -> {}",
                item.task_handle, item.reason, item.recommended_action
            )
        }));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::render_cockpit;
    use ajax_core::{
        models::{AttentionItem, TaskId},
        output::{InboxResponse, RepoSummary, ReposResponse, TaskSummary, TasksResponse},
    };

    #[test]
    fn cockpit_renders_backend_snapshot() {
        let repos = ReposResponse {
            repos: vec![RepoSummary {
                name: "web".to_string(),
                path: "/Users/matt/projects/web".to_string(),
                active_tasks: 1,
                reviewable_tasks: 0,
                cleanable_tasks: 0,
                broken_tasks: 0,
            }],
        };
        let tasks = TasksResponse {
            tasks: vec![TaskSummary {
                id: "task-1".to_string(),
                qualified_handle: "web/fix-login".to_string(),
                title: "Fix login".to_string(),
                lifecycle_status: "Active".to_string(),
                needs_attention: true,
            }],
        };
        let inbox = InboxResponse {
            items: vec![AttentionItem {
                task_id: TaskId::new("task-1"),
                task_handle: "web/fix-login".to_string(),
                reason: "agent needs input".to_string(),
                priority: 10,
                recommended_action: "open task".to_string(),
            }],
        };

        let rendered = render_cockpit(&repos, &tasks, &tasks, &inbox);

        assert!(rendered.contains("Ajax Cockpit"));
        assert!(rendered.contains("Repos: 1"));
        assert!(rendered.contains("Review: 1"));
        assert!(rendered.contains("web/fix-login: agent needs input -> open task"));
    }
}
