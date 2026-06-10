use crate::models::{
    AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LifecycleStatus, LiveStatusKind,
    RuntimeHealth, SideFlag, SubstrateGap, Task,
};
use crate::ui_state::{derive_operator_status, TaskStatus};

pub fn annotate(task: &Task) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let operator_status = derive_operator_status(task);

    if task.runtime_projection.observation_error.is_some() {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(AnnotationKind::Broken, Evidence::RuntimeObservationFailed),
        );
    }

    if operator_status.status == TaskStatus::Waiting
        && matches!(
            task.agent_status,
            AgentRuntimeStatus::Waiting | AgentRuntimeStatus::Blocked
        )
    {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(
                AnnotationKind::NeedsMe,
                Evidence::AgentStatus(task.agent_status),
            ),
        );
    }

    if let Some(live_status) = task.live_status.as_ref() {
        if let Some(kind) = annotation_kind_for_live_status(live_status.kind) {
            push_collapsed_annotation(
                &mut annotations,
                Annotation::new(kind, Evidence::LiveStatus(live_status.kind)),
            );
        }
    }

    for flag in task.side_flags() {
        if let Some(kind) = annotation_kind_for_side_flag(flag) {
            let evidence = substrate_gap_for_side_flag(flag)
                .map(Evidence::Substrate)
                .unwrap_or(Evidence::SideFlag(flag));
            push_collapsed_annotation(&mut annotations, Annotation::new(kind, evidence));
        }
    }

    if let Some(gap) = substrate_gap_for_runtime_health(task.runtime_projection.health) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(AnnotationKind::Broken, Evidence::Substrate(gap)),
        );
    }

    if let Some(kind) = annotation_kind_for_agent_status(task.agent_status) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(kind, Evidence::Lifecycle(task.lifecycle_status)),
        );
    }

    if let Some(kind) = annotation_kind_for_lifecycle(task.lifecycle_status) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(kind, Evidence::Lifecycle(task.lifecycle_status)),
        );
    }

    if operator_status.status == TaskStatus::Error
        && !annotations
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::Broken)
    {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(
                AnnotationKind::Broken,
                Evidence::Lifecycle(task.lifecycle_status),
            ),
        );
    }

    annotations.retain(|annotation| match annotation.kind {
        AnnotationKind::NeedsMe => operator_status.status == TaskStatus::Waiting,
        AnnotationKind::Broken => operator_status.status == TaskStatus::Error,
        AnnotationKind::Reviewable | AnnotationKind::Cleanable => true,
    });

    annotations.sort_by_key(|annotation| annotation.severity);
    annotations
}

fn push_collapsed_annotation(annotations: &mut Vec<Annotation>, annotation: Annotation) {
    if let Some(existing) = annotations
        .iter_mut()
        .find(|existing| existing.kind == annotation.kind)
    {
        if evidence_preference(annotation.kind, &annotation.evidence)
            < evidence_preference(existing.kind, &existing.evidence)
        {
            *existing = annotation;
        }
    } else {
        annotations.push(annotation);
    }
}

fn evidence_preference(kind: AnnotationKind, evidence: &Evidence) -> u32 {
    match kind {
        AnnotationKind::NeedsMe => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::AgentStatus(_) => 1,
            Evidence::SideFlag(_) => 2,
            Evidence::Lifecycle(_) => 3,
            Evidence::Substrate(_) | Evidence::RuntimeObservationFailed => 4,
        },
        AnnotationKind::Broken => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::Substrate(_) => 1,
            Evidence::RuntimeObservationFailed => 2,
            Evidence::SideFlag(_) => 3,
            Evidence::AgentStatus(_) => 4,
            Evidence::Lifecycle(_) => 5,
        },
        AnnotationKind::Reviewable | AnnotationKind::Cleanable => match evidence {
            Evidence::Lifecycle(_) => 0,
            Evidence::LiveStatus(_) => 1,
            Evidence::AgentStatus(_) => 2,
            Evidence::SideFlag(_) => 3,
            Evidence::Substrate(_) | Evidence::RuntimeObservationFailed => 4,
        },
    }
}

fn annotation_kind_for_live_status(status: LiveStatusKind) -> Option<AnnotationKind> {
    match status {
        LiveStatusKind::WaitingForApproval
        | LiveStatusKind::WaitingForInput
        | LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit => Some(AnnotationKind::NeedsMe),
        LiveStatusKind::WorktreeMissing
        | LiveStatusKind::TmuxMissing
        | LiveStatusKind::WorktrunkMissing
        | LiveStatusKind::MergeConflict
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => Some(AnnotationKind::Broken),
        LiveStatusKind::Done => Some(AnnotationKind::Reviewable),
        LiveStatusKind::ShellIdle
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning
        | LiveStatusKind::AgentRunning
        | LiveStatusKind::Unknown => None,
        LiveStatusKind::CiFailed => Some(AnnotationKind::Broken),
    }
}

fn annotation_kind_for_side_flag(flag: SideFlag) -> Option<AnnotationKind> {
    match flag {
        SideFlag::NeedsInput => Some(AnnotationKind::NeedsMe),
        SideFlag::AgentDead => Some(AnnotationKind::Broken),
        SideFlag::TmuxMissing
        | SideFlag::WorktreeMissing
        | SideFlag::WorktrunkMissing
        | SideFlag::BranchMissing
        | SideFlag::Conflicted => Some(AnnotationKind::Broken),
        SideFlag::TestsFailed => Some(AnnotationKind::Broken),
        SideFlag::Dirty | SideFlag::AgentRunning | SideFlag::Stale | SideFlag::Unpushed => None,
    }
}

fn annotation_kind_for_agent_status(status: AgentRuntimeStatus) -> Option<AnnotationKind> {
    match status {
        AgentRuntimeStatus::Waiting => Some(AnnotationKind::NeedsMe),
        AgentRuntimeStatus::Dead => Some(AnnotationKind::Broken),
        AgentRuntimeStatus::Blocked => Some(AnnotationKind::Broken),
        AgentRuntimeStatus::NotStarted
        | AgentRuntimeStatus::Running
        | AgentRuntimeStatus::Done
        | AgentRuntimeStatus::Unknown => None,
    }
}

fn annotation_kind_for_lifecycle(status: LifecycleStatus) -> Option<AnnotationKind> {
    match status {
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable => {
            Some(AnnotationKind::Reviewable)
        }
        LifecycleStatus::Merged | LifecycleStatus::Cleanable => Some(AnnotationKind::Cleanable),
        LifecycleStatus::Created
        | LifecycleStatus::Provisioning
        | LifecycleStatus::Active
        | LifecycleStatus::Waiting
        | LifecycleStatus::Removing
        | LifecycleStatus::TeardownIncomplete
        | LifecycleStatus::Removed
        | LifecycleStatus::Orphaned
        | LifecycleStatus::Error => None,
    }
}

fn substrate_gap_for_side_flag(flag: SideFlag) -> Option<SubstrateGap> {
    match flag {
        SideFlag::WorktreeMissing => Some(SubstrateGap::WorktreeMissing),
        SideFlag::TmuxMissing => Some(SubstrateGap::TmuxMissing),
        SideFlag::WorktrunkMissing => Some(SubstrateGap::WorktrunkMissing),
        SideFlag::BranchMissing => Some(SubstrateGap::BranchMissing),
        _ => None,
    }
}

fn substrate_gap_for_runtime_health(health: RuntimeHealth) -> Option<SubstrateGap> {
    match health {
        RuntimeHealth::MissingWorktree => Some(SubstrateGap::WorktreeMissing),
        RuntimeHealth::MissingSession => Some(SubstrateGap::TmuxMissing),
        RuntimeHealth::MissingTaskWindow | RuntimeHealth::WrongTaskWindowPath => {
            Some(SubstrateGap::WorktrunkMissing)
        }
        RuntimeHealth::Healthy | RuntimeHealth::Unobservable => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::lifecycle::{mark_active, mark_cleanable, mark_merged, mark_reviewable};
    use crate::models::{
        AgentClient, AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LiveObservation,
        LiveStatusKind, OperatorAction, RuntimeObservationSource, SideFlag, SubstrateGap, Task,
        TaskId,
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

    fn claude_active_task() -> Task {
        let mut task = task_with_flags("ack", &[]);
        task.selected_agent = AgentClient::Claude;
        mark_active(&mut task).unwrap();
        task
    }

    fn ack_at() -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(500)
    }

    #[test]
    fn acknowledged_claude_waiting_has_no_needs_me_annotation() {
        let mut task = claude_active_task();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
        );
        crate::live::acknowledge_attention(&mut task, ack_at());

        let annotations = super::annotate(&task);

        assert!(!annotations
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::NeedsMe));
    }

    #[test]
    fn new_waiting_after_acknowledgment_restores_needs_me_annotation() {
        let mut task = claude_active_task();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
        );
        crate::live::acknowledge_attention(&mut task, ack_at());
        crate::live::apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        );

        let annotations = super::annotate(&task);

        assert_eq!(
            annotations
                .iter()
                .filter(|annotation| annotation.kind == AnnotationKind::NeedsMe)
                .count(),
            1
        );
    }

    #[test]
    fn acknowledgment_does_not_remove_broken_or_reviewable_annotations() {
        let mut conflict = claude_active_task();
        crate::live::apply_observation(
            &mut conflict,
            LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        );
        crate::live::acknowledge_attention(&mut conflict, ack_at());
        assert!(super::annotate(&conflict)
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::Broken));

        let mut reviewable = claude_active_task();
        mark_reviewable(&mut reviewable).unwrap();
        crate::live::acknowledge_attention(&mut reviewable, ack_at());
        assert!(super::annotate(&reviewable)
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::Reviewable));
    }

    #[test]
    fn dead_agent_error_outranks_waiting_evidence() {
        let mut task = task_with_flags("blocked", &[SideFlag::NeedsInput, SideFlag::AgentDead]);
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));

        let annotations = super::annotate(&task);

        assert_eq!(
            annotations,
            vec![Annotation::new(
                AnnotationKind::Broken,
                Evidence::SideFlag(SideFlag::AgentDead),
            )]
        );
        assert_eq!(annotations[0].suggests, OperatorAction::Repair);
    }

    #[test]
    fn annotate_emits_broken_for_missing_substrate() {
        let task = task_with_flags("broken", &[SideFlag::WorktreeMissing]);

        let annotations = super::annotate(&task);

        assert_eq!(
            annotations,
            vec![Annotation::new(
                AnnotationKind::Broken,
                Evidence::Substrate(SubstrateGap::WorktreeMissing),
            )]
        );
    }

    #[test]
    fn annotate_emits_broken_for_runtime_probe_failure() {
        let mut task = task_with_flags("probe-failed", &[]);
        task.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        let annotations = super::annotate(&task);

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].kind, AnnotationKind::Broken);
        assert_eq!(annotations[0].suggests, OperatorAction::Repair);
    }

    #[test]
    fn blocked_agent_is_broken_without_lifecycle_error() {
        let mut task = task_with_flags("blocked", &[]);
        mark_active(&mut task).unwrap();
        task.agent_status = AgentRuntimeStatus::Blocked;

        let annotations = super::annotate(&task);

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].kind, AnnotationKind::Broken);
        assert_eq!(
            task.lifecycle_status,
            crate::models::LifecycleStatus::Active
        );
    }

    #[test]
    fn annotate_emits_reviewable_when_lifecycle_reviewable() {
        let mut task = task_with_flags("review", &[]);
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();

        let annotations = super::annotate(&task);

        assert_eq!(
            annotations,
            vec![Annotation::new(
                AnnotationKind::Reviewable,
                Evidence::Lifecycle(crate::models::LifecycleStatus::Reviewable),
            )]
        );
    }

    #[test]
    fn annotate_emits_cleanable_when_lifecycle_cleanable() {
        let task = cleanable_task("clean");

        let annotations = super::annotate(&task);

        assert_eq!(
            annotations,
            vec![Annotation::new(
                AnnotationKind::Cleanable,
                Evidence::Lifecycle(crate::models::LifecycleStatus::Cleanable),
            )]
        );
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
