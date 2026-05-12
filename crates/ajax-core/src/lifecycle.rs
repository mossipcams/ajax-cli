use crate::models::{LifecycleStatus, Task};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleTransitionReason {
    Generic,
    Recovery,
    OperationResult,
    ForceRemove,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleTransitionError {
    pub from: LifecycleStatus,
    pub to: LifecycleStatus,
    pub reason: LifecycleTransitionReason,
}

pub fn validate_lifecycle_transition(
    from: LifecycleStatus,
    to: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> Result<(), LifecycleTransitionError> {
    if from == to
        || generic_transition_allowed(from, to)
        || named_transition_allowed(from, to, reason)
    {
        return Ok(());
    }

    Err(LifecycleTransitionError { from, to, reason })
}

pub fn transition_lifecycle(
    task: &mut Task,
    status: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> Result<(), LifecycleTransitionError> {
    validate_lifecycle_transition(task.lifecycle_status, status, reason)?;
    task.lifecycle_status = status;
    Ok(())
}

pub fn hydrate_lifecycle_status(task: &mut Task, status: LifecycleStatus) {
    task.lifecycle_status = status;
}

pub fn mark_provisioning(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Provisioning,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_active(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Active,
        LifecycleTransitionReason::Generic,
    )
}

pub fn restore_active(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Active,
        LifecycleTransitionReason::Restore,
    )
}

pub fn mark_waiting(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Waiting,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_reviewable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Reviewable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_mergeable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Mergeable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_merged(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Merged,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_cleanable(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Cleanable,
        LifecycleTransitionReason::Generic,
    )
}

pub fn mark_removed(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Removed,
        LifecycleTransitionReason::Generic,
    )
}

pub fn force_mark_removed(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Removed,
        LifecycleTransitionReason::ForceRemove,
    )
}

pub fn mark_error(task: &mut Task) -> Result<(), LifecycleTransitionError> {
    transition_lifecycle(
        task,
        LifecycleStatus::Error,
        LifecycleTransitionReason::Generic,
    )
}

fn named_transition_allowed(
    from: LifecycleStatus,
    to: LifecycleStatus,
    reason: LifecycleTransitionReason,
) -> bool {
    matches!(
        (reason, from, to),
        (
            LifecycleTransitionReason::Recovery | LifecycleTransitionReason::OperationResult,
            LifecycleStatus::Error,
            LifecycleStatus::Active | LifecycleStatus::Reviewable
        ) | (
            LifecycleTransitionReason::ForceRemove,
            LifecycleStatus::Created
                | LifecycleStatus::Provisioning
                | LifecycleStatus::Active
                | LifecycleStatus::Waiting
                | LifecycleStatus::Reviewable
                | LifecycleStatus::Mergeable
                | LifecycleStatus::Merged
                | LifecycleStatus::Cleanable
                | LifecycleStatus::Orphaned
                | LifecycleStatus::Error,
            LifecycleStatus::Removed
        ) | (
            LifecycleTransitionReason::Restore,
            LifecycleStatus::Removed,
            LifecycleStatus::Active
        )
    )
}

fn generic_transition_allowed(from: LifecycleStatus, to: LifecycleStatus) -> bool {
    matches!(
        (from, to),
        (LifecycleStatus::Created, LifecycleStatus::Provisioning)
            | (LifecycleStatus::Created, LifecycleStatus::Active)
            | (LifecycleStatus::Created, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Created, LifecycleStatus::Error)
            | (LifecycleStatus::Provisioning, LifecycleStatus::Active)
            | (LifecycleStatus::Provisioning, LifecycleStatus::Error)
            | (LifecycleStatus::Active, LifecycleStatus::Waiting)
            | (LifecycleStatus::Active, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Active, LifecycleStatus::Error)
            | (LifecycleStatus::Waiting, LifecycleStatus::Active)
            | (LifecycleStatus::Waiting, LifecycleStatus::Reviewable)
            | (LifecycleStatus::Waiting, LifecycleStatus::Error)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Mergeable)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Merged)
            | (LifecycleStatus::Reviewable, LifecycleStatus::Error)
            | (LifecycleStatus::Mergeable, LifecycleStatus::Merged)
            | (LifecycleStatus::Mergeable, LifecycleStatus::Error)
            | (LifecycleStatus::Merged, LifecycleStatus::Cleanable)
            | (LifecycleStatus::Merged, LifecycleStatus::Removed)
            | (LifecycleStatus::Merged, LifecycleStatus::Error)
            | (LifecycleStatus::Cleanable, LifecycleStatus::Removed)
            | (LifecycleStatus::Cleanable, LifecycleStatus::Error)
            | (LifecycleStatus::Orphaned, LifecycleStatus::Error)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        mark_active, mark_cleanable, mark_error, mark_merged, mark_provisioning, mark_removed,
        mark_reviewable, restore_active, transition_lifecycle, validate_lifecycle_transition,
        LifecycleTransitionReason,
    };
    use crate::models::{AgentClient, LifecycleStatus, Task, TaskId};
    use rstest::rstest;

    #[rstest]
    #[case(LifecycleStatus::Created, LifecycleStatus::Provisioning)]
    #[case(LifecycleStatus::Provisioning, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Provisioning, LifecycleStatus::Error)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Waiting)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Reviewable)]
    #[case(LifecycleStatus::Waiting, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Waiting, LifecycleStatus::Reviewable)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Mergeable)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Mergeable, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Cleanable)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::Cleanable, LifecycleStatus::Removed)]
    fn generic_lifecycle_transition_matrix_allows_valid_edges(
        #[case] from: LifecycleStatus,
        #[case] to: LifecycleStatus,
    ) {
        assert!(
            validate_lifecycle_transition(from, to, LifecycleTransitionReason::Generic).is_ok(),
            "{from:?} -> {to:?}"
        );
    }

    #[rstest]
    #[case(LifecycleStatus::Created, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Active, LifecycleStatus::Merged)]
    #[case(LifecycleStatus::Reviewable, LifecycleStatus::Removed)]
    #[case(LifecycleStatus::Mergeable, LifecycleStatus::Cleanable)]
    #[case(LifecycleStatus::Merged, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Cleanable, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Removed, LifecycleStatus::Active)]
    #[case(LifecycleStatus::Error, LifecycleStatus::Active)]
    fn generic_lifecycle_transition_matrix_blocks_invalid_edges(
        #[case] from: LifecycleStatus,
        #[case] to: LifecycleStatus,
    ) {
        assert!(
            validate_lifecycle_transition(from, to, LifecycleTransitionReason::Generic).is_err(),
            "{from:?} -> {to:?}"
        );
    }

    #[rstest]
    #[case(LifecycleStatus::Active, LifecycleTransitionReason::Recovery)]
    #[case(LifecycleStatus::Reviewable, LifecycleTransitionReason::Recovery)]
    #[case(LifecycleStatus::Active, LifecycleTransitionReason::OperationResult)]
    #[case(
        LifecycleStatus::Reviewable,
        LifecycleTransitionReason::OperationResult
    )]
    fn error_lifecycle_recovery_requires_named_transition_reason(
        #[case] to: LifecycleStatus,
        #[case] reason: LifecycleTransitionReason,
    ) {
        assert!(
            validate_lifecycle_transition(
                LifecycleStatus::Error,
                to,
                LifecycleTransitionReason::Generic
            )
            .is_err(),
            "generic Error -> {to:?} should stay blocked"
        );
        assert!(
            validate_lifecycle_transition(LifecycleStatus::Error, to, reason).is_ok(),
            "{reason:?} Error -> {to:?}"
        );
    }

    fn task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        )
    }

    #[test]
    fn transition_lifecycle_mutates_only_after_validating_edge() {
        let mut task = task();

        transition_lifecycle(
            &mut task,
            LifecycleStatus::Provisioning,
            LifecycleTransitionReason::Generic,
        )
        .unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
    }

    #[test]
    fn rejected_transition_leaves_lifecycle_unchanged() {
        let mut task = task();

        let result = transition_lifecycle(
            &mut task,
            LifecycleStatus::Merged,
            LifecycleTransitionReason::Generic,
        );

        assert!(result.is_err());
        assert_eq!(task.lifecycle_status, LifecycleStatus::Created);
    }

    #[test]
    fn removed_task_cannot_be_reactivated_without_explicit_restore_transition() {
        let mut task = task();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_removed(&mut task).unwrap();

        let result = mark_active(&mut task);

        assert!(result.is_err());
        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);

        restore_active(&mut task).unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    }

    #[test]
    fn named_helpers_apply_expected_lifecycle_transitions() {
        let mut task = task();

        mark_provisioning(&mut task).unwrap();
        mark_active(&mut task).unwrap();
        mark_reviewable(&mut task).unwrap();
        mark_merged(&mut task).unwrap();
        mark_cleanable(&mut task).unwrap();
        mark_removed(&mut task).unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
    }

    #[test]
    fn evidence_driven_operation_result_can_recover_error_to_reviewable() {
        let mut task = task();
        mark_error(&mut task).unwrap();

        transition_lifecycle(
            &mut task,
            LifecycleStatus::Reviewable,
            LifecycleTransitionReason::OperationResult,
        )
        .unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    }

    #[test]
    fn production_code_does_not_assign_lifecycle_status_outside_authority_module() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut violations = Vec::new();

        for entry in std::fs::read_dir(&src_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("lifecycle.rs") {
                continue;
            }

            let source = std::fs::read_to_string(&path).unwrap();
            let production_source = source
                .split("\n#[cfg(test)]")
                .next()
                .unwrap_or(source.as_str());

            for (index, line) in production_source.lines().enumerate() {
                let trimmed = line.trim_start();
                let is_assignment = trimmed.starts_with("lifecycle_status =")
                    || line.contains(".lifecycle_status =");
                let is_equality = trimmed.starts_with("lifecycle_status ==")
                    || line.contains(".lifecycle_status ==");
                if is_assignment && !is_equality {
                    violations.push(format!(
                        "{}:{}:{line}",
                        path.strip_prefix(&src_dir).unwrap().display(),
                        index + 1
                    ));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "lifecycle writes must go through ajax_core::lifecycle:\n{}",
            violations.join("\n")
        );
    }
}
