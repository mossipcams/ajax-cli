use crate::models::{
    AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LifecycleStatus, LiveStatusKind,
    SideFlag, SubstrateGap, Task,
};

pub fn annotate(task: &Task) -> Vec<Annotation> {
    let mut annotations = Vec::new();

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
            Evidence::SideFlag(_) => 1,
            Evidence::Lifecycle(_) => 2,
            Evidence::Substrate(_) => 3,
        },
        AnnotationKind::Broken => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::Substrate(_) => 1,
            Evidence::SideFlag(_) => 2,
            Evidence::Lifecycle(_) => 3,
        },
        AnnotationKind::Reviewable | AnnotationKind::Cleanable => match evidence {
            Evidence::Lifecycle(_) => 0,
            Evidence::LiveStatus(_) => 1,
            Evidence::SideFlag(_) => 2,
            Evidence::Substrate(_) => 3,
        },
    }
}

fn annotation_kind_for_live_status(status: LiveStatusKind) -> Option<AnnotationKind> {
    match status {
        LiveStatusKind::WaitingForApproval
        | LiveStatusKind::WaitingForInput
        | LiveStatusKind::AuthRequired
        | LiveStatusKind::RateLimited
        | LiveStatusKind::ContextLimit
        | LiveStatusKind::CommandFailed
        | LiveStatusKind::Blocked => Some(AnnotationKind::NeedsMe),
        LiveStatusKind::WorktreeMissing
        | LiveStatusKind::TmuxMissing
        | LiveStatusKind::WorktrunkMissing
        | LiveStatusKind::MergeConflict => Some(AnnotationKind::Broken),
        LiveStatusKind::Done => Some(AnnotationKind::Reviewable),
        LiveStatusKind::ShellIdle
        | LiveStatusKind::CommandRunning
        | LiveStatusKind::TestsRunning
        | LiveStatusKind::AgentRunning
        | LiveStatusKind::CiFailed
        | LiveStatusKind::Unknown => None,
    }
}

fn annotation_kind_for_side_flag(flag: SideFlag) -> Option<AnnotationKind> {
    match flag {
        SideFlag::NeedsInput | SideFlag::AgentDead => Some(AnnotationKind::NeedsMe),
        SideFlag::TmuxMissing
        | SideFlag::WorktreeMissing
        | SideFlag::WorktrunkMissing
        | SideFlag::BranchMissing
        | SideFlag::Conflicted => Some(AnnotationKind::Broken),
        SideFlag::Dirty
        | SideFlag::AgentRunning
        | SideFlag::TestsFailed
        | SideFlag::Stale
        | SideFlag::Unpushed => None,
    }
}

fn annotation_kind_for_agent_status(status: AgentRuntimeStatus) -> Option<AnnotationKind> {
    match status {
        AgentRuntimeStatus::Waiting | AgentRuntimeStatus::Blocked | AgentRuntimeStatus::Dead => {
            Some(AnnotationKind::NeedsMe)
        }
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

#[cfg(test)]
mod tests {
    use crate::lifecycle::{mark_active, mark_cleanable, mark_merged, mark_reviewable};
    use crate::models::{
        AgentClient, AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LiveObservation,
        LiveStatusKind, OperatorAction, SideFlag, SubstrateGap, Task, TaskId,
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
    fn annotate_collapses_blocker_evidence_into_needs_me() {
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
                AnnotationKind::NeedsMe,
                Evidence::LiveStatus(LiveStatusKind::WaitingForApproval),
            )]
        );
        assert_eq!(annotations[0].suggests, OperatorAction::Resume);
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
