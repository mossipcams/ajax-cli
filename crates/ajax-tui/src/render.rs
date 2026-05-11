use ajax_core::cockpit::{CockpitSnapshot, CockpitTaskView};

pub fn render_snapshot(snapshot: &CockpitSnapshot) -> String {
    let mut lines = vec![
        "Ajax Cockpit".to_string(),
        format!("Repos: {}", snapshot.repos.len()),
        format!("Tasks: {}", snapshot.tasks.len()),
        "Task Statuses".to_string(),
    ];

    if snapshot.tasks.is_empty() {
        lines.push("no active tasks".to_string());
    } else {
        lines.extend(snapshot.tasks.iter().map(render_task));
    }

    lines.push("Attention".to_string());
    if snapshot.attention.is_empty() {
        lines.push("no tasks need attention".to_string());
    } else {
        lines.extend(
            snapshot
                .attention
                .iter()
                .map(|item| format!("{}: {} -> {:?}", item.task_handle, item.reason, item.action)),
        );
    }

    lines.join("\n")
}

fn render_task(task: &CockpitTaskView) -> String {
    format!(
        "{}/{}\t{:?}\t{}",
        task.repo, task.handle, task.status, task.title
    )
}
