use crate::models::LifecycleStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleTransitionReason {
    Generic,
    Recovery,
    OperationResult,
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
    use super::{validate_lifecycle_transition, LifecycleTransitionReason};
    use crate::models::LifecycleStatus;
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
}
