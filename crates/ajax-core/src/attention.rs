use crate::models::{
    AgentRuntimeStatus, AttentionItem, LifecycleStatus, LiveStatusKind, RecommendedAction,
    SideFlag, Task,
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
        if let Some(existing) = deduplicated
            .iter_mut()
            .find(|existing| equivalent_attention_item(existing, &item))
        {
            if item.priority < existing.priority {
                *existing = item;
            }
        } else {
            deduplicated.push(item);
        }
    }

    deduplicated
}

fn equivalent_attention_item(left: &AttentionItem, right: &AttentionItem) -> bool {
    left.task_id == right.task_id
        && left.recommended_action == right.recommended_action
        && (left.reason == right.reason
            || (operator_waiting_reason(&left.reason) && operator_waiting_reason(&right.reason)))
}

fn operator_waiting_reason(reason: &str) -> bool {
    matches!(
        reason,
        "agent needs input" | "agent is waiting" | "waiting for approval" | "waiting for input"
    )
}

fn attention_items_for_task(task: &Task) -> Vec<AttentionItem> {
    let mut items = Vec::new();

    for flag in task.side_flags() {
        if flag == SideFlag::AgentRunning {
            continue;
        }
        let (reason, priority, recommended_action) = attention_for_flag(flag);
        items.push(AttentionItem {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            reason: reason.to_string(),
            priority,
            recommended_action: recommended_action.as_str().to_string(),
        });
    }

    if task.lifecycle_status == LifecycleStatus::Cleanable {
        items.push(AttentionItem {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            reason: "task is cleanable".to_string(),
            priority: 80,
            recommended_action: RecommendedAction::CleanTask.as_str().to_string(),
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
                recommended_action: recommended_action.as_str().to_string(),
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
            recommended_action: recommended_action.as_str().to_string(),
        });
    }

    items
}

fn attention_for_flag(flag: SideFlag) -> (&'static str, u32, RecommendedAction) {
    match flag {
        SideFlag::NeedsInput => ("agent needs input", 10, RecommendedAction::OpenTask),
        SideFlag::TestsFailed => ("tests failed", 15, RecommendedAction::OpenTask),
        SideFlag::WorktrunkMissing => ("worktrunk missing", 20, RecommendedAction::OpenTrunk),
        SideFlag::TmuxMissing => ("tmux session missing", 25, RecommendedAction::OpenTrunk),
        SideFlag::WorktreeMissing => ("worktree missing", 30, RecommendedAction::OpenTask),
        SideFlag::BranchMissing => ("branch missing", 35, RecommendedAction::OpenTask),
        SideFlag::Conflicted => ("git conflicts detected", 40, RecommendedAction::OpenTask),
        SideFlag::AgentDead => ("agent appears dead", 45, RecommendedAction::OpenTask),
        SideFlag::Dirty => ("worktree is dirty", 50, RecommendedAction::OpenTask),
        SideFlag::Unpushed => ("branch has unpushed work", 55, RecommendedAction::OpenTask),
        SideFlag::Stale => ("task is stale", 60, RecommendedAction::OpenTask),
        SideFlag::AgentRunning => ("agent is running", 90, RecommendedAction::OpenTask),
    }
}

fn attention_for_live_status(
    status: LiveStatusKind,
) -> Option<(&'static str, u32, RecommendedAction)> {
    match status {
        LiveStatusKind::WaitingForApproval => {
            Some(("waiting for approval", 5, RecommendedAction::OpenTask))
        }
        LiveStatusKind::WaitingForInput => {
            Some(("waiting for input", 6, RecommendedAction::OpenTask))
        }
        LiveStatusKind::AuthRequired => {
            Some(("authentication required", 7, RecommendedAction::OpenTask))
        }
        LiveStatusKind::RateLimited => Some(("rate limited", 8, RecommendedAction::OpenTask)),
        LiveStatusKind::ContextLimit => {
            Some(("context limit reached", 9, RecommendedAction::OpenTask))
        }
        LiveStatusKind::MergeConflict => Some((
            "merge conflict needs attention",
            10,
            RecommendedAction::OpenTask,
        )),
        LiveStatusKind::CommandFailed => Some(("command failed", 15, RecommendedAction::OpenTask)),
        LiveStatusKind::CiFailed => Some(("ci failed", 11, RecommendedAction::OpenTask)),
        LiveStatusKind::Blocked => Some(("agent is blocked", 12, RecommendedAction::OpenTask)),
        LiveStatusKind::WorktreeMissing => {
            Some(("worktree missing", 30, RecommendedAction::OpenTask))
        }
        LiveStatusKind::TmuxMissing => {
            Some(("tmux session missing", 25, RecommendedAction::OpenTrunk))
        }
        LiveStatusKind::WorktrunkMissing => {
            Some(("worktrunk missing", 20, RecommendedAction::OpenTrunk))
        }
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
) -> Option<(&'static str, u32, RecommendedAction)> {
    match status {
        AgentRuntimeStatus::Waiting => Some(("agent is waiting", 10, RecommendedAction::OpenTask)),
        AgentRuntimeStatus::Blocked => Some(("agent is blocked", 12, RecommendedAction::OpenTask)),
        AgentRuntimeStatus::Dead => Some(("agent appears dead", 45, RecommendedAction::OpenTask)),
        AgentRuntimeStatus::NotStarted
        | AgentRuntimeStatus::Running
        | AgentRuntimeStatus::Done
        | AgentRuntimeStatus::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::lifecycle::{mark_active, mark_cleanable, mark_merged, mark_reviewable};
    use crate::models::{
        AgentClient, AgentRuntimeStatus, AttentionItem, LiveObservation, LiveStatusKind, SideFlag,
        Task, TaskId,
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

    fn cleanable_task(handle: &str) -> Task {
        let mut task = task_with_flags(handle, &[]);
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_cleanable(&mut task).unwrap();
        task
    }

    #[test]
    fn attention_items_are_structured_and_prioritized() {
        let cleanable = cleanable_task("merged-task");
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
                    recommended_action: "open worktrunk".to_string(),
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
        assert_eq!(items[0].recommended_action, "open task");
    }

    #[test]
    fn equivalent_waiting_attention_collapses_to_one_open_task_item() {
        let mut task = task_with_flags("tech-debt", &[SideFlag::NeedsInput]);
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));

        let items = super::derive_attention_items(&[task]);

        assert_eq!(
            items,
            vec![AttentionItem {
                task_id: TaskId::new("task-tech-debt"),
                task_handle: "web/tech-debt".to_string(),
                reason: "waiting for approval".to_string(),
                priority: 5,
                recommended_action: "open task".to_string(),
            }]
        );
    }

    #[test]
    fn unrelated_open_task_reasons_remain_distinct() {
        let task = task_with_flags(
            "conflicted-input",
            &[SideFlag::NeedsInput, SideFlag::Conflicted],
        );

        let items = super::derive_attention_items(&[task]);

        assert_eq!(
            items
                .iter()
                .map(|item| item.reason.as_str())
                .collect::<Vec<_>>(),
            vec!["agent needs input", "git conflicts detected"]
        );
    }

    #[test]
    fn missing_worktree_attention_is_visible() {
        let mut task = task_with_flags("missing-worktree", &[SideFlag::WorktreeMissing]);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WorktreeMissing,
            "worktree missing",
        ));

        let items = super::derive_attention_items(&[task]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "worktree missing");
        assert_eq!(items[0].priority, 30);
        assert_eq!(items[0].recommended_action, "open task");
    }

    #[test]
    fn live_missing_tmux_attention_is_visible_without_side_flag() {
        let mut task = task_with_flags("missing-tmux", &[]);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TmuxMissing,
            "tmux session missing",
        ));

        let items = super::derive_attention_items(&[task]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "tmux session missing");
        assert_eq!(items[0].priority, 25);
    }

    #[test]
    fn matching_flag_and_live_missing_evidence_deduplicates_attention() {
        let mut task = task_with_flags("missing-tmux", &[SideFlag::TmuxMissing]);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TmuxMissing,
            "tmux session missing",
        ));

        let items = super::derive_attention_items(&[task]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "tmux session missing");
    }

    #[test]
    fn broken_resource_flags_create_attention() {
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

        assert_eq!(
            items
                .iter()
                .map(|item| item.reason.as_str())
                .collect::<Vec<_>>(),
            vec![
                "worktrunk missing",
                "tmux session missing",
                "worktree missing",
                "branch missing",
            ]
        );
    }

    #[test]
    fn missing_resources_suppress_stale_agent_running_item() {
        let task = task_with_flags(
            "broken-running",
            &[SideFlag::WorktreeMissing, SideFlag::AgentRunning],
        );

        let items = super::derive_attention_items(&[task]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "worktree missing");
    }

    #[test]
    fn agent_running_flag_alone_does_not_need_operator_attention() {
        let task = task_with_flags("running-agent", &[SideFlag::AgentRunning]);

        let items = super::derive_attention_items(&[task]);

        assert!(
            items.is_empty(),
            "agent running is operational state, not operator attention: {items:?}"
        );
    }

    #[test]
    fn specific_running_live_status_suppresses_generic_running_attention() {
        let mut task = task_with_flags("tech-debt", &[SideFlag::AgentRunning]);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TestsRunning,
            "tests running",
        ));

        let items = super::derive_attention_items(&[task]);

        assert!(
            items.iter().all(|item| item.reason != "agent is running"),
            "specific live status should be enough without generic monitor attention: {items:?}"
        );
    }

    #[test]
    fn deriving_attention_does_not_mutate_task_lifecycle() {
        let task = cleanable_task("read-only");
        let before = task.lifecycle_status;

        let _items = super::derive_attention_items(std::slice::from_ref(&task));

        assert_eq!(task.lifecycle_status, before);
    }

    #[test]
    fn attention_module_does_not_assign_lifecycle_status() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/attention.rs"),
        )
        .unwrap();
        let forbidden_assignment = [".lifecycle", "_status ="].concat();
        let permitted_equality = [".lifecycle", "_status =="].concat();

        assert!(!source.lines().any(
            |line| line.contains(&forbidden_assignment) && !line.contains(&permitted_equality)
        ));
    }
}
