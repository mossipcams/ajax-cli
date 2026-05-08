use crate::models::{AgentRuntimeStatus, AttentionItem, LifecycleStatus, SideFlag, Task};

pub fn derive_attention_items(tasks: &[Task]) -> Vec<AttentionItem> {
    let mut items = tasks
        .iter()
        .flat_map(attention_items_for_task)
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
            .then_with(|| left.reason.cmp(&right.reason))
    });

    items
}

fn attention_items_for_task(task: &Task) -> Vec<AttentionItem> {
    let mut items = Vec::new();

    for flag in task.side_flags() {
        let (reason, priority, recommended_action) = attention_for_flag(flag);
        items.push(AttentionItem {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            reason: reason.to_string(),
            priority,
            recommended_action: recommended_action.to_string(),
        });
    }

    if task.lifecycle_status == LifecycleStatus::Cleanable {
        items.push(AttentionItem {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            reason: "task is cleanable".to_string(),
            priority: 80,
            recommended_action: "clean task".to_string(),
        });
    }

    if let Some((reason, priority, recommended_action)) =
        attention_for_agent_status(task.agent_status)
    {
        items.push(AttentionItem {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            reason: reason.to_string(),
            priority,
            recommended_action: recommended_action.to_string(),
        });
    }

    items
}

fn attention_for_flag(flag: SideFlag) -> (&'static str, u32, &'static str) {
    match flag {
        SideFlag::NeedsInput => ("agent needs input", 10, "open task"),
        SideFlag::TestsFailed => ("tests failed", 15, "inspect test output"),
        SideFlag::WorktrunkMissing => ("worktrunk missing", 20, "repair worktrunk"),
        SideFlag::TmuxMissing => ("tmux session missing", 25, "repair task"),
        SideFlag::WorktreeMissing => ("worktree missing", 30, "repair task"),
        SideFlag::BranchMissing => ("branch missing", 35, "repair task"),
        SideFlag::Conflicted => ("git conflicts detected", 40, "open task"),
        SideFlag::AgentDead => ("agent appears dead", 45, "inspect agent"),
        SideFlag::Dirty => ("worktree is dirty", 50, "review diff"),
        SideFlag::Unpushed => ("branch has unpushed work", 55, "review branch"),
        SideFlag::Stale => ("task is stale", 60, "inspect task"),
        SideFlag::AgentRunning => ("agent is running", 90, "monitor task"),
    }
}

fn attention_for_agent_status(
    status: AgentRuntimeStatus,
) -> Option<(&'static str, u32, &'static str)> {
    match status {
        AgentRuntimeStatus::Waiting => Some(("agent is waiting", 10, "open task")),
        AgentRuntimeStatus::Blocked => Some(("agent is blocked", 12, "inspect agent")),
        AgentRuntimeStatus::Dead => Some(("agent appears dead", 45, "inspect agent")),
        AgentRuntimeStatus::NotStarted
        | AgentRuntimeStatus::Running
        | AgentRuntimeStatus::Done
        | AgentRuntimeStatus::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, AttentionItem, LifecycleStatus, SideFlag, Task, TaskId,
    };

    fn task_with_flags(handle: &str, flags: &[SideFlag]) -> Task {
        let mut task = Task::new(
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
        );

        for flag in flags {
            task.add_side_flag(*flag);
        }

        task
    }

    #[test]
    fn attention_items_are_structured_and_prioritized() {
        let mut cleanable = task_with_flags("merged-task", &[]);
        cleanable.lifecycle_status = LifecycleStatus::Cleanable;
        let waiting = task_with_flags("needs-input", &[SideFlag::NeedsInput]);
        let broken = task_with_flags("broken", &[SideFlag::WorktrunkMissing]);

        let items = super::derive_attention_items(&[cleanable, broken, waiting]);

        assert_eq!(
            items,
            vec![
                AttentionItem {
                    task_id: TaskId::new("task-needs-input"),
                    task_handle: "web/needs-input".to_string(),
                    reason: "agent needs input".to_string(),
                    priority: 10,
                    recommended_action: "open task".to_string(),
                },
                AttentionItem {
                    task_id: TaskId::new("task-broken"),
                    task_handle: "web/broken".to_string(),
                    reason: "worktrunk missing".to_string(),
                    priority: 20,
                    recommended_action: "repair worktrunk".to_string(),
                },
                AttentionItem {
                    task_id: TaskId::new("task-merged-task"),
                    task_handle: "web/merged-task".to_string(),
                    reason: "task is cleanable".to_string(),
                    priority: 80,
                    recommended_action: "clean task".to_string(),
                },
            ]
        );
    }

    #[test]
    fn blocked_agent_status_creates_attention_item() {
        let mut task = task_with_flags("blocked-agent", &[]);
        task.agent_status = AgentRuntimeStatus::Blocked;

        let items = super::derive_attention_items(&[task]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "agent is blocked");
        assert_eq!(items[0].priority, 12);
        assert_eq!(items[0].recommended_action, "inspect agent");
    }
}
