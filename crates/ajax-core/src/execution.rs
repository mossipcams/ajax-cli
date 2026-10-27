pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionState {
    Pending,
    Running,
    Waiting,
    Completing,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl ExecutionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Completing => "completing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "waiting" => Some(Self::Waiting),
            "completing" => Some(Self::Completing),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidTransition {
    Disallowed(ExecutionState, ExecutionState),
}

pub fn transition(from: ExecutionState, to: ExecutionState) -> Result<(), InvalidTransition> {
    let allowed = matches!(
        (from, to),
        (
            ExecutionState::Pending,
            ExecutionState::Running | ExecutionState::Cancelled
        ) | (
            ExecutionState::Running,
            ExecutionState::Waiting
                | ExecutionState::Completing
                | ExecutionState::Failed
                | ExecutionState::Cancelled
                | ExecutionState::Interrupted
        ) | (
            ExecutionState::Waiting,
            ExecutionState::Running
                | ExecutionState::Failed
                | ExecutionState::Cancelled
                | ExecutionState::Interrupted
        ) | (
            ExecutionState::Completing,
            ExecutionState::Completed | ExecutionState::Failed | ExecutionState::Interrupted
        ) | (
            ExecutionState::Interrupted,
            ExecutionState::Pending | ExecutionState::Cancelled | ExecutionState::Failed
        )
    );
    if allowed {
        Ok(())
    } else {
        Err(InvalidTransition::Disallowed(from, to))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplayPolicy {
    Safe,
    #[default]
    Unsafe,
    Reconcile,
}

impl ReplayPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
            Self::Reconcile => "reconcile",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "safe" => Self::Safe,
            "reconcile" => Self::Reconcile,
            _ => Self::Unsafe,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    SessionStart,
    SessionRestore,
    StateQuery,
    HistoryFetch,
    PromptDispatch,
    PermissionAnswer,
    Cancel,
}

impl Operation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::SessionRestore => "session_restore",
            Self::StateQuery => "state_query",
            Self::HistoryFetch => "history_fetch",
            Self::PromptDispatch => "prompt_dispatch",
            Self::PermissionAnswer => "permission_answer",
            Self::Cancel => "cancel",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "session_start" => Some(Self::SessionStart),
            "session_restore" => Some(Self::SessionRestore),
            "state_query" => Some(Self::StateQuery),
            "history_fetch" => Some(Self::HistoryFetch),
            "prompt_dispatch" => Some(Self::PromptDispatch),
            "permission_answer" => Some(Self::PermissionAnswer),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

pub fn default_replay_policy(operation: Operation, supports_reconciliation: bool) -> ReplayPolicy {
    match operation {
        Operation::SessionStart | Operation::SessionRestore | Operation::StateQuery => {
            ReplayPolicy::Safe
        }
        Operation::HistoryFetch => ReplayPolicy::Safe,
        Operation::PromptDispatch if supports_reconciliation => ReplayPolicy::Reconcile,
        Operation::PromptDispatch => ReplayPolicy::Unsafe,
        Operation::PermissionAnswer | Operation::Cancel => ReplayPolicy::Unsafe,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnfinishedCheckpoint {
    pub operation: Option<Operation>,
    pub policy: ReplayPolicy,
    pub schema_version: u32,
    pub has_result: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptReason {
    UnsupportedSchema,
    UnknownOperation,
    UnsafeAmbiguous,
    ReconcileInconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    Cancel,
    ResumeFromResult,
    Replay,
    Reconcile,
    AdoptExternal,
    Interrupt(InterruptReason),
}

pub fn resolve_recovery(
    checkpoint: &UnfinishedCheckpoint,
    cancel_requested: bool,
) -> RecoveryAction {
    if checkpoint.schema_version > CHECKPOINT_SCHEMA_VERSION {
        return RecoveryAction::Interrupt(InterruptReason::UnsupportedSchema);
    }
    if checkpoint.has_result {
        return RecoveryAction::ResumeFromResult;
    }
    if cancel_requested {
        return RecoveryAction::Cancel;
    }
    let Some(operation) = checkpoint.operation else {
        return RecoveryAction::Interrupt(InterruptReason::UnknownOperation);
    };
    match (operation, checkpoint.policy) {
        (_, ReplayPolicy::Safe) => RecoveryAction::Replay,
        (_, ReplayPolicy::Reconcile) => RecoveryAction::Reconcile,
        _ => RecoveryAction::Interrupt(InterruptReason::UnsafeAmbiguous),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconcileOutcome {
    Executed,
    NotExecuted,
    Unknown,
}

pub fn resolve_reconciled(outcome: ReconcileOutcome) -> RecoveryAction {
    match outcome {
        ReconcileOutcome::Executed => RecoveryAction::AdoptExternal,
        ReconcileOutcome::NotExecuted => RecoveryAction::Replay,
        ReconcileOutcome::Unknown => {
            RecoveryAction::Interrupt(InterruptReason::ReconcileInconclusive)
        }
    }
}

pub fn children_allow_completion(children: &[ExecutionState]) -> bool {
    !children.iter().any(|state| !state.is_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [ExecutionState; 8] = [
        ExecutionState::Pending,
        ExecutionState::Running,
        ExecutionState::Waiting,
        ExecutionState::Completing,
        ExecutionState::Completed,
        ExecutionState::Failed,
        ExecutionState::Cancelled,
        ExecutionState::Interrupted,
    ];

    const ALLOWED: [[bool; 8]; 8] = [
        [false, true, false, false, false, false, true, false],
        [false, false, true, true, false, true, true, true],
        [false, true, false, false, false, true, true, true],
        [false, false, false, false, true, true, false, true],
        [false; 8],
        [false; 8],
        [false; 8],
        [true, false, false, false, false, true, true, false],
    ];

    #[test]
    fn transition_matches_the_allowed_table_exactly() {
        for (from_row, from) in ALL_STATES.into_iter().enumerate() {
            for (to_col, to) in ALL_STATES.into_iter().enumerate() {
                let expected = ALLOWED[from_row][to_col];
                let is_ok = transition(from, to).is_ok();
                assert_eq!(
                    is_ok, expected,
                    "transition {from:?} -> {to:?}: expected allowed={expected}, got ok={is_ok}"
                );
            }
        }
    }

    #[test]
    fn rejected_transitions_report_the_pair() {
        assert_eq!(
            transition(ExecutionState::Pending, ExecutionState::Pending),
            Err(InvalidTransition::Disallowed(
                ExecutionState::Pending,
                ExecutionState::Pending
            ))
        );
        assert_eq!(
            transition(ExecutionState::Completed, ExecutionState::Running),
            Err(InvalidTransition::Disallowed(
                ExecutionState::Completed,
                ExecutionState::Running
            ))
        );
    }

    #[test]
    fn execution_state_wire_names_round_trip() {
        let wire = [
            "pending",
            "running",
            "waiting",
            "completing",
            "completed",
            "failed",
            "cancelled",
            "interrupted",
        ];
        for (state, name) in ALL_STATES.iter().zip(wire) {
            assert_eq!(state.as_str(), name);
            assert_eq!(ExecutionState::parse(name), Some(*state));
        }
        assert_eq!(ExecutionState::parse("done"), None);
    }

    #[test]
    fn is_terminal_covers_completed_failed_cancelled_only() {
        for (state, name) in ALL_STATES
            .iter()
            .zip(["p", "r", "w", "c", "d", "f", "x", "i"])
        {
            let expected = matches!(
                state,
                ExecutionState::Completed | ExecutionState::Failed | ExecutionState::Cancelled
            );
            assert_eq!(state.is_terminal(), expected, "{name}");
        }
    }

    #[test]
    fn replay_policy_defaults_to_unsafe() {
        assert_eq!(ReplayPolicy::default(), ReplayPolicy::Unsafe);
        assert_eq!(ReplayPolicy::parse("safe"), ReplayPolicy::Safe);
        assert_eq!(ReplayPolicy::parse("reconcile"), ReplayPolicy::Reconcile);
        assert_eq!(ReplayPolicy::parse("unsafe"), ReplayPolicy::Unsafe);
    }

    #[test]
    fn unknown_replay_policy_strings_parse_to_unsafe() {
        for value in ["", "S", "SAFE", "safety", "reconciliation", "yes"] {
            assert_eq!(ReplayPolicy::parse(value), ReplayPolicy::Unsafe, "{value}");
        }
    }

    #[test]
    fn operation_wire_names_round_trip() {
        let wire = [
            ("session_start", Operation::SessionStart),
            ("session_restore", Operation::SessionRestore),
            ("state_query", Operation::StateQuery),
            ("history_fetch", Operation::HistoryFetch),
            ("prompt_dispatch", Operation::PromptDispatch),
            ("permission_answer", Operation::PermissionAnswer),
            ("cancel", Operation::Cancel),
        ];
        for (name, op) in wire {
            assert_eq!(op.as_str(), name);
            assert_eq!(Operation::parse(name), Some(op));
        }
        assert_eq!(Operation::parse("prompt_send"), None);
    }

    fn checkpoint(
        operation: Option<Operation>,
        policy: ReplayPolicy,
        schema_version: u32,
        has_result: bool,
    ) -> UnfinishedCheckpoint {
        UnfinishedCheckpoint {
            operation,
            policy,
            schema_version,
            has_result,
        }
    }

    #[test]
    fn default_replay_policy_follows_the_operation_table() {
        assert_eq!(
            default_replay_policy(Operation::SessionStart, true),
            ReplayPolicy::Safe
        );
        assert_eq!(
            default_replay_policy(Operation::SessionRestore, false),
            ReplayPolicy::Safe
        );
        assert_eq!(
            default_replay_policy(Operation::StateQuery, false),
            ReplayPolicy::Safe
        );
        assert_eq!(
            default_replay_policy(Operation::HistoryFetch, true),
            ReplayPolicy::Safe
        );
        assert_eq!(
            default_replay_policy(Operation::PromptDispatch, true),
            ReplayPolicy::Reconcile
        );
        assert_eq!(
            default_replay_policy(Operation::PromptDispatch, false),
            ReplayPolicy::Unsafe
        );
        assert_eq!(
            default_replay_policy(Operation::PermissionAnswer, true),
            ReplayPolicy::Unsafe
        );
        assert_eq!(
            default_replay_policy(Operation::Cancel, true),
            ReplayPolicy::Unsafe
        );
    }

    #[test]
    fn newer_schema_interrupts_even_when_a_result_exists() {
        let cp = checkpoint(
            Some(Operation::StateQuery),
            ReplayPolicy::Safe,
            CHECKPOINT_SCHEMA_VERSION + 1,
            true,
        );
        assert_eq!(
            resolve_recovery(&cp, false),
            RecoveryAction::Interrupt(InterruptReason::UnsupportedSchema)
        );
    }

    #[test]
    fn a_stored_result_is_never_replayed_even_when_policy_is_safe() {
        let cp = checkpoint(
            Some(Operation::StateQuery),
            ReplayPolicy::Safe,
            CHECKPOINT_SCHEMA_VERSION,
            true,
        );
        assert_eq!(
            resolve_recovery(&cp, false),
            RecoveryAction::ResumeFromResult
        );

        let cancelled_too = checkpoint(
            Some(Operation::PromptDispatch),
            ReplayPolicy::Safe,
            CHECKPOINT_SCHEMA_VERSION,
            true,
        );
        assert_eq!(
            resolve_recovery(&cancelled_too, true),
            RecoveryAction::ResumeFromResult
        );
    }

    #[test]
    fn cancel_requested_cancels_before_policy_resolution() {
        let cp = checkpoint(
            Some(Operation::StateQuery),
            ReplayPolicy::Safe,
            CHECKPOINT_SCHEMA_VERSION,
            false,
        );
        assert_eq!(resolve_recovery(&cp, true), RecoveryAction::Cancel);
    }

    #[test]
    fn unknown_operation_interrupts_before_policy_resolution() {
        let safe = checkpoint(None, ReplayPolicy::Safe, CHECKPOINT_SCHEMA_VERSION, false);
        assert_eq!(
            resolve_recovery(&safe, false),
            RecoveryAction::Interrupt(InterruptReason::UnknownOperation)
        );

        let unsafe_cp = checkpoint(None, ReplayPolicy::Unsafe, CHECKPOINT_SCHEMA_VERSION, false);
        assert_eq!(
            resolve_recovery(&unsafe_cp, false),
            RecoveryAction::Interrupt(InterruptReason::UnknownOperation)
        );
    }

    #[test]
    fn safe_policy_replays() {
        let cp = checkpoint(
            Some(Operation::SessionRestore),
            ReplayPolicy::Safe,
            CHECKPOINT_SCHEMA_VERSION,
            false,
        );
        assert_eq!(resolve_recovery(&cp, false), RecoveryAction::Replay);
    }

    #[test]
    fn reconcile_policy_reconciles() {
        let cp = checkpoint(
            Some(Operation::PromptDispatch),
            ReplayPolicy::Reconcile,
            CHECKPOINT_SCHEMA_VERSION,
            false,
        );
        assert_eq!(resolve_recovery(&cp, false), RecoveryAction::Reconcile);
    }

    #[test]
    fn unsafe_policy_interrupts_as_ambiguous() {
        for op in [
            Operation::PromptDispatch,
            Operation::PermissionAnswer,
            Operation::Cancel,
        ] {
            let cp = checkpoint(
                Some(op),
                ReplayPolicy::Unsafe,
                CHECKPOINT_SCHEMA_VERSION,
                false,
            );
            assert_eq!(
                resolve_recovery(&cp, false),
                RecoveryAction::Interrupt(InterruptReason::UnsafeAmbiguous)
            );
        }
    }

    #[test]
    fn reconcile_outcomes_resolve_to_actions() {
        assert_eq!(
            resolve_reconciled(ReconcileOutcome::Executed),
            RecoveryAction::AdoptExternal
        );
        assert_eq!(
            resolve_reconciled(ReconcileOutcome::NotExecuted),
            RecoveryAction::Replay
        );
        assert_eq!(
            resolve_reconciled(ReconcileOutcome::Unknown),
            RecoveryAction::Interrupt(InterruptReason::ReconcileInconclusive)
        );
    }

    #[test]
    fn children_allow_completion_requires_all_terminal() {
        let terminal = ExecutionState::Completed;
        assert!(children_allow_completion(&[]));
        assert!(children_allow_completion(&[
            terminal,
            ExecutionState::Failed
        ]));
        assert!(!children_allow_completion(&[
            terminal,
            ExecutionState::Interrupted
        ]));
        assert!(!children_allow_completion(&[ExecutionState::Running]));
    }
}
