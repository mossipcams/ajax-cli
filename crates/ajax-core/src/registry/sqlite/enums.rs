use super::super::{RegistryEventKind, RegistrySnapshotError};
use crate::models::{
    AgentClient, AgentRuntimeStatus, LifecycleStatus, LiveStatusKind, RuntimeHealth,
    RuntimeObservationSource, SideFlag, StepReceiptStatus, TaskOperationKind,
};

/// Generates a paired encoder/decoder for an enum whose persisted label is its
/// variant name. Keeps both directions in sync from a single variant list.
macro_rules! string_codec {
    ($to:ident, $from:ident, $ty:ty, $label:literal, [$($variant:ident),+ $(,)?]) => {
        pub(crate) fn $to(value: $ty) -> &'static str {
            match value {
                $(<$ty>::$variant => stringify!($variant),)+
            }
        }

        pub(crate) fn $from(value: &str) -> Result<$ty, RegistrySnapshotError> {
            $(if value == stringify!($variant) {
                return Ok(<$ty>::$variant);
            })+
            Err(RegistrySnapshotError::Decode(format!(
                concat!("unknown ", $label, ": {}"),
                value
            )))
        }
    };
}

string_codec!(
    agent_client_name,
    parse_agent_client,
    AgentClient,
    "agent client",
    [Claude, Codex, Cursor, Pi, Other,]
);

string_codec!(
    lifecycle_status_name,
    parse_lifecycle_status,
    LifecycleStatus,
    "lifecycle status",
    [
        Created,
        Provisioning,
        Active,
        Waiting,
        Reviewable,
        Mergeable,
        Merged,
        Cleanable,
        Removing,
        TeardownIncomplete,
        Removed,
        Orphaned,
        Error,
    ]
);

string_codec!(
    agent_runtime_status_name,
    parse_agent_runtime_status,
    AgentRuntimeStatus,
    "agent runtime status",
    [NotStarted, Running, Waiting, Blocked, Dead, Done, Unknown,]
);

pub(crate) fn side_flag_name(value: SideFlag) -> &'static str {
    match value {
        SideFlag::Dirty => "Dirty",
        SideFlag::AgentRunning => "AgentRunning",
        SideFlag::AgentDead => "AgentDead",
        SideFlag::NeedsInput => "NeedsInput",
        SideFlag::TestsFailed => "TestsFailed",
        SideFlag::TmuxMissing => "TmuxMissing",
        SideFlag::WorktreeMissing => "WorktreeMissing",
        SideFlag::TaskWindowMissing => "TaskWindowMissing",
        SideFlag::BranchMissing => "BranchMissing",
        SideFlag::Stale => "Stale",
        SideFlag::Conflicted => "Conflicted",
        SideFlag::Unpushed => "Unpushed",
    }
}

pub(crate) fn parse_side_flag(value: &str) -> Result<SideFlag, RegistrySnapshotError> {
    match value {
        "Dirty" => Ok(SideFlag::Dirty),
        "AgentRunning" => Ok(SideFlag::AgentRunning),
        "AgentDead" => Ok(SideFlag::AgentDead),
        "NeedsInput" => Ok(SideFlag::NeedsInput),
        "TestsFailed" => Ok(SideFlag::TestsFailed),
        "TmuxMissing" => Ok(SideFlag::TmuxMissing),
        "WorktreeMissing" => Ok(SideFlag::WorktreeMissing),
        "TaskWindowMissing" => Ok(SideFlag::TaskWindowMissing),
        "BranchMissing" => Ok(SideFlag::BranchMissing),
        "Stale" => Ok(SideFlag::Stale),
        "Conflicted" => Ok(SideFlag::Conflicted),
        "Unpushed" => Ok(SideFlag::Unpushed),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown side flag: {value}"
        ))),
    }
}

pub(crate) fn live_status_kind_name(value: LiveStatusKind) -> &'static str {
    match value {
        LiveStatusKind::WorktreeMissing => "WorktreeMissing",
        LiveStatusKind::TmuxMissing => "TmuxMissing",
        LiveStatusKind::TaskWindowMissing => "TaskWindowMissing",
        LiveStatusKind::ShellIdle => "ShellIdle",
        LiveStatusKind::CommandRunning => "CommandRunning",
        LiveStatusKind::TestsRunning => "TestsRunning",
        LiveStatusKind::AgentRunning => "AgentRunning",
        LiveStatusKind::WaitingForApproval => "WaitingForApproval",
        LiveStatusKind::WaitingForInput => "WaitingForInput",
        LiveStatusKind::Blocked => "Blocked",
        LiveStatusKind::RateLimited => "RateLimited",
        LiveStatusKind::AuthRequired => "AuthRequired",
        LiveStatusKind::MergeConflict => "MergeConflict",
        LiveStatusKind::CiFailed => "CiFailed",
        LiveStatusKind::CiPending => "CiPending",
        LiveStatusKind::ContextLimit => "ContextLimit",
        LiveStatusKind::CommandFailed => "CommandFailed",
        LiveStatusKind::Done => "Done",
        LiveStatusKind::Unknown => "Unknown",
    }
}

pub(crate) fn parse_live_status_kind(value: &str) -> Result<LiveStatusKind, RegistrySnapshotError> {
    match value {
        "WorktreeMissing" => Ok(LiveStatusKind::WorktreeMissing),
        "TmuxMissing" => Ok(LiveStatusKind::TmuxMissing),
        "TaskWindowMissing" => Ok(LiveStatusKind::TaskWindowMissing),
        "ShellIdle" => Ok(LiveStatusKind::ShellIdle),
        "CommandRunning" => Ok(LiveStatusKind::CommandRunning),
        "TestsRunning" => Ok(LiveStatusKind::TestsRunning),
        "AgentRunning" => Ok(LiveStatusKind::AgentRunning),
        "WaitingForApproval" => Ok(LiveStatusKind::WaitingForApproval),
        "WaitingForInput" => Ok(LiveStatusKind::WaitingForInput),
        "Blocked" => Ok(LiveStatusKind::Blocked),
        "RateLimited" => Ok(LiveStatusKind::RateLimited),
        "AuthRequired" => Ok(LiveStatusKind::AuthRequired),
        "MergeConflict" => Ok(LiveStatusKind::MergeConflict),
        "CiPending" => Ok(LiveStatusKind::CiPending),
        "CiFailed" => Ok(LiveStatusKind::CiFailed),
        "ContextLimit" => Ok(LiveStatusKind::ContextLimit),
        "CommandFailed" => Ok(LiveStatusKind::CommandFailed),
        "Done" => Ok(LiveStatusKind::Done),
        "Unknown" => Ok(LiveStatusKind::Unknown),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown live status kind: {value}"
        ))),
    }
}

pub(crate) fn parse_runtime_health(value: &str) -> Result<RuntimeHealth, RegistrySnapshotError> {
    RuntimeHealth::from_label(value)
        .ok_or_else(|| RegistrySnapshotError::Decode(format!("unknown runtime health: {value}")))
}

pub(crate) fn parse_runtime_observation_source(
    value: &str,
) -> Result<RuntimeObservationSource, RegistrySnapshotError> {
    RuntimeObservationSource::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown runtime observation source: {value}"))
    })
}

string_codec!(
    registry_event_kind_name,
    parse_registry_event_kind,
    RegistryEventKind,
    "registry event kind",
    [TaskCreated, LifecycleChanged, SubstrateChanged, UserNote,]
);

pub(crate) fn parse_task_operation_kind(
    value: &str,
) -> Result<TaskOperationKind, RegistrySnapshotError> {
    TaskOperationKind::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown task operation kind: {value}"))
    })
}

pub(crate) fn parse_step_receipt_status(
    value: &str,
) -> Result<StepReceiptStatus, RegistrySnapshotError> {
    StepReceiptStatus::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown step receipt status: {value}"))
    })
}
