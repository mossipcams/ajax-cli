use crate::models::{
    AgentRuntimeStatus, AttentionItem, LifecycleStatus, LiveStatusKind, SideFlag, Task,
};

pub fn derive_attention_items(tasks: &[Task]) -> Vec<AttentionItem> {
    let mut items = tasks
        .iter()
        .flat_map(attention_items_for_task)
        .collect::<Vec<_>>();

    items = deduplicate_attention_items(items);

    items.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
            .then_with(|| left.reason.cmp(&right.reason))
    });

    items
}

fn deduplicate_attention_items(items: Vec<AttentionItem>) -> Vec<AttentionItem> {
    let mut deduplicated: Vec<AttentionItem> = Vec::new();

    for item in items {
        if let Some(existing) = deduplicated.iter_mut().find(|existing| {
            existing.task_id == item.task_id
                && existing.reason == item.reason
                && existing.recommended_action == item.recommended_action
        }) {
            if item.priority < existing.priority {
                *existing = item;
            }
        } else {
            deduplicated.push(item);
        }
    }

    deduplicated
}

fn attention_items_for_task(task: &Task) -> Vec<AttentionItem> {
    if task_has_missing_substrate(task) {
        return Vec::new();
    }

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

    if let Some(live_status) = task.live_status.as_ref() {
        if let Some((reason, priority, recommended_action)) =
            attention_for_live_status(live_status.kind)
        {
            items.push(AttentionItem {
                task_id: task.id.clone(),
                task_handle: task.qualified_handle(),
                reason: reason.to_string(),
                priority,
                recommended_action: recommended_action.to_string(),
            });
        }
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

fn task_has_missing_substrate(task: &Task) -> bool {
    task.side_flags().any(missing_substrate_flag)
        || task
            .live_status
            .as_ref()
            .is_some_and(|live_status| missing_substrate_live_status(live_status.kind))
}

fn missing_substrate_flag(flag: SideFlag) -> bool {
    matches!(
        flag,
        SideFlag::WorktrunkMissing
            | SideFlag::TmuxMissing
            | SideFlag::WorktreeMissing
            | SideFlag::BranchMissing
    )
}

fn missing_substrate_live_status(status: LiveStatusKind) -> bool {
    matches!(
        status,
        LiveStatusKind::WorktreeMissing
            | LiveStatusKind::TmuxMissing
            | LiveStatusKind::WorktrunkMissing
    )
}

fn attention_for_flag(flag: SideFlag) -> (&'static str, u32, &'static str) {
    match flag {
        SideFlag::NeedsInput => ("agent needs input", 10, "open task"),
        SideFlag::TestsFailed => ("tests failed", 15, "inspect test output"),
        SideFlag::WorktrunkMissing => ("worktrunk missing", 20, "inspect task"),
        SideFlag::TmuxMissing => ("tmux session missing", 25, "inspect task"),
        SideFlag::WorktreeMissing => ("worktree missing", 30, "inspect task"),
        SideFlag::BranchMissing => ("branch missing", 35, "inspect task"),
        SideFlag::Conflicted => ("git conflicts detected", 40, "open task"),
        SideFlag::AgentDead => ("agent appears dead", 45, "inspect agent"),
        SideFlag::Dirty => ("worktree is dirty", 50, "review diff"),
        SideFlag::Unpushed => ("branch has unpushed work", 55, "review branch"),
        SideFlag::Stale => ("task is stale", 60, "inspect task"),
        SideFlag::AgentRunning => ("agent is running", 90, "monitor task"),
    }
}

fn attention_for_live_status(status: LiveStatusKind) -> Option<(&'static str, u32, &'static str)> {
    match status {
        LiveStatusKind::WaitingForApproval => Some(("waiting for approval", 5, "open task")),
        LiveStatusKind::WaitingForInput => Some(("waiting for input", 6, "open task")),
        LiveStatusKind::AuthRequired => Some(("authentication required", 7, "open task")),
        LiveStatusKind::RateLimited => Some(("rate limited", 8, "inspect agent")),
        LiveStatusKind::ContextLimit => Some(("context limit reached", 9, "inspect agent")),
        LiveStatusKind::MergeConflict => Some(("merge conflict needs attention", 10, "open task")),
        LiveStatusKind::CommandFailed => Some(("command failed", 15, "inspect agent")),
        LiveStatusKind::Blocked => Some(("agent is blocked", 12, "inspect agent")),
        LiveStatusKind::WorktreeMissing
        | LiveStatusKind::TmuxMissing
        | LiveStatusKind::WorktrunkMissing => None,
        LiveStatusKind::ShellIdle
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning
        | LiveStatusKind::AgentRunning
        | LiveStatusKind::Done
        | LiveStatusKind::Unknown => None,
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
        AgentClient, AgentRuntimeStatus, AttentionItem, LifecycleStatus, LiveObservation,
        LiveStatusKind, SideFlag, Task, TaskId,
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

    #[test]
    fn missing_resource_attention_is_suppressed() {
        let mut task = task_with_flags("missing-worktree", &[SideFlag::WorktreeMissing]);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WorktreeMissing,
            "worktree missing",
        ));

        let items = super::derive_attention_items(&[task]);

        assert!(items.is_empty());
    }

    #[test]
    fn broken_resource_flags_do_not_create_attention() {
        let task = task_with_flags(
            "broken",
            &[
                SideFlag::WorktrunkMissing,
                SideFlag::TmuxMissing,
                SideFlag::WorktreeMissing,
                SideFlag::BranchMissing,
            ],
        );

        let items = super::derive_attention_items(&[task]);

        assert!(items.is_empty());
    }

    #[test]
    fn missing_resources_suppress_stale_agent_running_item() {
        let task = task_with_flags(
            "broken-running",
            &[SideFlag::WorktreeMissing, SideFlag::AgentRunning],
        );

        let items = super::derive_attention_items(&[task]);

        assert!(items.is_empty());
    }
}
