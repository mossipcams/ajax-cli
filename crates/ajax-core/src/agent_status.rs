//! Conservative internal agent-status observation/run model.
//!
//! This module owns the pure status-reduction layer that maps observational
//! candidates onto a `StatusProjection`. The projection keeps
//! [`crate::models::LiveStatusKind`] as a presentation projection while
//! separating process liveness from agent activity, tracking parent/child
//! runs, and expiring/staling observations with source+confidence so idle
//! live processes and stale pane/hook evidence no longer produce confident
//! false positives.
//!
//! See `.planning/agent-plans/agent-status-conservative.md`.

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::time::SystemTime;

use crate::models::{LiveObservation, LiveStatusKind};

/// Internal live summary when the parent is idle/absent and a child is active.
pub const SUMMARY_WAITING_ON_DELEGATED: &str = "waiting on delegated runs";
/// Internal live summary when the primary run is terminal but children remain.
pub const SUMMARY_DELEGATED_STILL_ACTIVE: &str = "delegated runs still active";

/// Operator-facing explanation for [`SUMMARY_WAITING_ON_DELEGATED`].
pub const EXPLANATION_WAITING_ON_DELEGATED: &str = "Waiting on delegated runs";
/// Operator-facing explanation for [`SUMMARY_DELEGATED_STILL_ACTIVE`].
pub const EXPLANATION_DELEGATED_STILL_ACTIVE: &str = "Delegated runs still active";

/// True when a live/operator waiting summary means the parent is blocked on
/// children rather than on the operator. These must not set `NeedsInput` or
/// fire attention webhooks.
pub fn is_delegated_waiting_summary(summary: &str) -> bool {
    operator_explanation_for_summary(summary).is_some()
}

/// Map an internal delegated summary onto the operator explanation string.
pub fn operator_explanation_for_summary(summary: &str) -> Option<&'static str> {
    let trimmed = summary.trim();
    if trimmed.eq_ignore_ascii_case(SUMMARY_WAITING_ON_DELEGATED)
        || trimmed.eq_ignore_ascii_case(EXPLANATION_WAITING_ON_DELEGATED)
    {
        Some(EXPLANATION_WAITING_ON_DELEGATED)
    } else if trimmed.eq_ignore_ascii_case(SUMMARY_DELEGATED_STILL_ACTIVE)
        || trimmed.eq_ignore_ascii_case(EXPLANATION_DELEGATED_STILL_ACTIVE)
    {
        Some(EXPLANATION_DELEGATED_STILL_ACTIVE)
    } else {
        None
    }
}

/// Origin of an agent-status observation, ordered by evidence precedence
/// (lower [`ObservationSource::rank`] wins among non-expired observations).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ObservationSource {
    /// Terminal process exit / fatal runtime error: authoritative.
    ProcessExit,
    /// Structured native lifecycle event folded from the canonical JSONL log.
    ProviderLifecycle,
    /// Process liveness — informational, never selects activity.
    ProcessLiveness,
}

impl ObservationSource {
    pub const fn rank(self) -> u8 {
        match self {
            Self::ProcessExit => 0,
            Self::ProviderLifecycle => 1,
            Self::ProcessLiveness => 2,
        }
    }
}

/// Confidence of an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Activity-only classification carried by a `StatusObservation`. There is
/// deliberately **no** process-alive variant: liveness is supplied via
/// [`ProcessLiveness`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ActivityKind {
    Working,
    WaitingInput,
    WaitingApproval,
    Done,
    Failed,
    CommandRunning,
    TestsRunning,
}

/// Coarse activity class used for conflict detection between fresh
/// observations of the same run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ActivityClass {
    Running,
    Waiting,
    Terminal,
}

impl ActivityKind {
    pub const fn class(self) -> ActivityClass {
        match self {
            Self::Working | Self::CommandRunning | Self::TestsRunning => ActivityClass::Running,
            Self::WaitingInput | Self::WaitingApproval => ActivityClass::Waiting,
            Self::Done | Self::Failed => ActivityClass::Terminal,
        }
    }
}

/// A single observational sample for one agent run. `expires_at` is the
/// absolute time past which the observation is considered stale and dropped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusObservation {
    pub source: ObservationSource,
    pub observed_at: SystemTime,
    pub expires_at: SystemTime,
    pub confidence: Confidence,
    pub run_id: String,
    pub parent_run_id: Option<String>,
    pub kind: ActivityKind,
}

/// Separately-supplied process liveness. Never alone implies activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLiveness {
    pub alive: bool,
    pub observed_at: SystemTime,
}

/// Parent-side phase derived from primary activity plus non-detached child
/// aggregation. Encoded separately from [`LiveStatusKind`] so the existing
/// presentation enum is not expanded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParentPhase {
    ActivelyWorking,
    WaitingOnDelegated,
    WaitingForUser,
    CompletedLocallyChildrenActive,
    FullyCompleted,
    Unknown,
}

/// Result of [`reduce_agent_status`]. `live` is the derived presentation
/// observation; `phase` carries the richer internal phase; `process_alive`
/// reports the separate liveness input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusProjection {
    pub live: LiveObservation,
    pub phase: ParentPhase,
    pub process_alive: bool,
    pub selected_observed_at: Option<SystemTime>,
    pub selected_source: Option<ObservationSource>,
}

/// Pure reducer input.
pub struct ReduceInput<'a> {
    pub now: SystemTime,
    pub primary_run_id: String,
    pub process_liveness: Option<ProcessLiveness>,
    pub observations: &'a [StatusObservation],
}

/// Reduced result for a single run id, used internally for parent/child
/// resolution.
enum RunActivityResult {
    Conflict,
    Activity {
        kind: ActivityKind,
        source: ObservationSource,
        observed_at: SystemTime,
    },
}

/// Resolve the activity of a single run id among fresh observations.
///
/// Per run: drop expired samples, finding the newest `observed_at`. A
/// `ProcessExit` observation at the newest timestamp always wins (it is the
/// authoritative terminal event; ProcessExit outranks ongoing activity
/// evidence of equal-freshness tier (a stale heartbeat cannot disprove an
/// actual exit). Tier-strict precedence applies across non-expired
/// observations: lower [`ObservationSource::rank`] wins regardless of
/// observed_at, tiebroken by newest `observed_at` then activity state rank
/// (busy beats waiting beats approval). When two *non-ProcessExit* fresh
/// observations share the same `observed_at` but come from *different*
/// sources and disagree on activity class, the run is marked
/// [`RunActivityResult::Conflict`] and the reducer projects `Unknown`.
fn select_run_activity<'a>(
    fresh: &'a [&'a StatusObservation],
    run_id: &str,
) -> Option<RunActivityResult> {
    let run_obs: Vec<&StatusObservation> = fresh
        .iter()
        .copied()
        .filter(|o| o.run_id == run_id)
        .collect();
    if run_obs.is_empty() {
        return None;
    }

    // Conflict check among equal-timestamp, non-ProcessExit observations with
    // differing source tiers and conflicting activity classes. ProcessExit
    // is authoritative and never participates in the conflict projection.
    let by_observed_at = {
        let mut buckets: Vec<(&SystemTime, Vec<&StatusObservation>)> = Vec::new();
        for obs in &run_obs {
            if let Some(slot) = buckets.iter_mut().find(|(at, _)| *at == &obs.observed_at) {
                slot.1.push(obs);
            } else {
                buckets.push((&obs.observed_at, vec![obs]));
            }
        }
        buckets
    };
    for (_, slot) in by_observed_at {
        let non_exit: Vec<&StatusObservation> = slot
            .iter()
            .copied()
            .filter(|o| o.source != ObservationSource::ProcessExit)
            .collect();
        let distinct_classes: BTreeSet<ActivityClass> =
            non_exit.iter().map(|o| o.kind.class()).collect();
        let distinct_sources: BTreeSet<ObservationSource> =
            non_exit.iter().map(|o| o.source).collect();
        if distinct_classes.len() > 1 && distinct_sources.len() > 1 {
            return Some(RunActivityResult::Conflict);
        }
    }

    // Tier-strict precedence: lowest rank wins, then newest observed_at, then
    // activity state rank (busy beats waiting beats approval).
    let chosen = run_obs
        .iter()
        .copied()
        .min_by_key(|o| {
            (
                o.source.rank(),
                Reverse(o.observed_at),
                reversal(state_rank(o.kind)),
            )
        })
        .expect("non-empty run observations");
    Some(RunActivityResult::Activity {
        kind: chosen.kind,
        source: chosen.source,
        observed_at: chosen.observed_at,
    })
}

fn state_rank(kind: ActivityKind) -> u8 {
    match kind {
        ActivityKind::Working | ActivityKind::CommandRunning | ActivityKind::TestsRunning => 3,
        ActivityKind::WaitingInput => 2,
        ActivityKind::WaitingApproval => 1,
        ActivityKind::Done | ActivityKind::Failed => 0,
    }
}

/// Invert the state rank so `min_by_key` prefers the highest state rank on
/// tiebreak. `u8::MAX - rank` keeps a stable, deterministic ordering and
/// pairs with [`Reverse<SystemTime>`] for newest-observed_at tiebreaks.
fn reversal(rank: u8) -> u8 {
    u8::MAX - rank
}

/// True when the run is currently active (still running) per the resolved
/// activity.
fn is_run_active(activity: &Option<RunActivityResult>) -> bool {
    matches!(
        activity,
        Some(RunActivityResult::Activity {
            kind,
            ..
        }) if matches!(kind.class(), ActivityClass::Running | ActivityClass::Waiting)
    ) || matches!(activity, Some(RunActivityResult::Conflict))
}

/// Reduce observations + process liveness into a `StatusProjection`.
pub fn reduce_agent_status(input: ReduceInput<'_>) -> StatusProjection {
    let now = input.now;
    let primary = input.primary_run_id.as_str();
    let process_alive = input.process_liveness.map(|p| p.alive).unwrap_or(false);

    let fresh: Vec<&StatusObservation> = input
        .observations
        .iter()
        .filter(|o| o.expires_at >= now)
        .collect();

    let primary_activity = select_run_activity(&fresh, primary);

    let child_ids: BTreeSet<String> = fresh
        .iter()
        .filter(|o| o.run_id != primary && o.parent_run_id.as_deref() == Some(primary))
        .map(|o| o.run_id.clone())
        .collect();
    let child_activities: Vec<Option<RunActivityResult>> = child_ids
        .iter()
        .map(|rid| select_run_activity(&fresh, rid))
        .collect();
    let any_child_non_terminal = child_activities.iter().any(is_run_active);

    let unknown = || StatusProjection {
        live: LiveObservation::new(LiveStatusKind::Unknown, "unknown agent status"),
        phase: ParentPhase::Unknown,
        process_alive,
        selected_observed_at: None,
        selected_source: None,
    };

    match primary_activity {
        None => {
            if any_child_non_terminal {
                StatusProjection {
                    live: LiveObservation::new(
                        LiveStatusKind::WaitingForInput,
                        SUMMARY_WAITING_ON_DELEGATED,
                    ),
                    phase: ParentPhase::WaitingOnDelegated,
                    process_alive,
                    selected_observed_at: None,
                    selected_source: None,
                }
            } else {
                unknown()
            }
        }
        Some(RunActivityResult::Conflict) => unknown(),
        Some(RunActivityResult::Activity {
            kind,
            source,
            observed_at,
        }) => match kind.class() {
            ActivityClass::Running => StatusProjection {
                live: LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
                phase: ParentPhase::ActivelyWorking,
                process_alive,
                selected_observed_at: Some(observed_at),
                selected_source: Some(source),
            },
            ActivityClass::Waiting => {
                let (live_kind, summary) = match kind {
                    ActivityKind::WaitingApproval => {
                        (LiveStatusKind::WaitingForApproval, "waiting for approval")
                    }
                    _ => (LiveStatusKind::WaitingForInput, "waiting for input"),
                };
                StatusProjection {
                    live: LiveObservation::new(live_kind, summary),
                    phase: ParentPhase::WaitingForUser,
                    process_alive,
                    selected_observed_at: Some(observed_at),
                    selected_source: Some(source),
                }
            }
            ActivityClass::Terminal => {
                if any_child_non_terminal {
                    StatusProjection {
                        live: LiveObservation::new(
                            LiveStatusKind::WaitingForInput,
                            SUMMARY_DELEGATED_STILL_ACTIVE,
                        ),
                        phase: ParentPhase::CompletedLocallyChildrenActive,
                        process_alive,
                        selected_observed_at: Some(observed_at),
                        selected_source: Some(source),
                    }
                } else if kind == ActivityKind::Failed {
                    StatusProjection {
                        live: LiveObservation::new(LiveStatusKind::CommandFailed, "agent failed"),
                        phase: ParentPhase::FullyCompleted,
                        process_alive,
                        selected_observed_at: Some(observed_at),
                        selected_source: Some(source),
                    }
                } else {
                    StatusProjection {
                        live: LiveObservation::new(LiveStatusKind::Done, "done"),
                        phase: ParentPhase::FullyCompleted,
                        process_alive,
                        selected_observed_at: Some(observed_at),
                        selected_source: Some(source),
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{
        reduce_agent_status, ActivityKind, Confidence, ObservationSource, ParentPhase,
        ProcessLiveness, ReduceInput, StatusObservation,
    };
    use crate::models::LiveStatusKind;

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(10_000)
    }
    const PRIMARY: &str = "primary";

    fn obs(
        source: ObservationSource,
        kind: ActivityKind,
        age_secs: u64,
        ttl_secs: u64,
    ) -> StatusObservation {
        let observed_at = now() - Duration::from_secs(age_secs);
        StatusObservation {
            source,
            observed_at,
            expires_at: observed_at + Duration::from_secs(ttl_secs),
            confidence: Confidence::High,
            run_id: PRIMARY.to_string(),
            parent_run_id: None,
            kind,
        }
    }

    fn obs_with_run(
        source: ObservationSource,
        kind: ActivityKind,
        age_secs: u64,
        ttl_secs: u64,
        run_id: &str,
        parent_run_id: Option<&str>,
    ) -> StatusObservation {
        let observed_at = now() - Duration::from_secs(age_secs);
        StatusObservation {
            source,
            observed_at,
            expires_at: observed_at + Duration::from_secs(ttl_secs),
            confidence: Confidence::High,
            run_id: run_id.to_string(),
            parent_run_id: parent_run_id.map(str::to_string),
            kind,
        }
    }

    fn reduce(process_alive: bool, observations: &[StatusObservation]) -> super::StatusProjection {
        reduce_agent_status(ReduceInput {
            now: now(),
            primary_run_id: PRIMARY.to_string(),
            process_liveness: Some(ProcessLiveness {
                alive: process_alive,
                observed_at: now(),
            }),
            observations,
        })
    }

    fn reduce_no_liveness(observations: &[StatusObservation]) -> super::StatusProjection {
        reduce_agent_status(ReduceInput {
            now: now(),
            primary_run_id: PRIMARY.to_string(),
            process_liveness: None,
            observations,
        })
    }

    #[test]
    fn live_process_idle_prompt_is_not_agent_running() {
        let projection = reduce(true, &[]);

        assert_eq!(projection.phase, ParentPhase::Unknown);
        assert_eq!(projection.live.kind, LiveStatusKind::Unknown);
        assert_ne!(projection.live.kind, LiveStatusKind::AgentRunning);
        assert!(projection.process_alive);
    }

    #[test]
    fn live_process_waiting_approval_from_hook() {
        let obs = obs(
            ObservationSource::ProviderLifecycle,
            ActivityKind::WaitingApproval,
            1,
            120,
        );
        let projection = reduce(true, &[obs]);

        assert_eq!(projection.phase, ParentPhase::WaitingForUser);
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn stale_wrapper_heartbeat_then_waiting_hook() {
        // Stale wrapper working is supplied as liveness only, not activity.
        // A fresh wait hook then wins the activity projection.
        let observations = [obs(
            ObservationSource::ProviderLifecycle,
            ActivityKind::WaitingInput,
            1,
            120,
        )];
        let projection = reduce(true, &observations);

        assert_eq!(projection.phase, ParentPhase::WaitingForUser);
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn parent_waiting_on_one_active_child() {
        // Primary has no activity observation; one non-detached child Working.
        let child = obs_with_run(
            ObservationSource::ProviderLifecycle,
            ActivityKind::Working,
            1,
            120,
            "child-1",
            Some(PRIMARY),
        );
        let projection = reduce(true, &[child]);

        assert_eq!(projection.phase, ParentPhase::WaitingOnDelegated);
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn parent_complete_while_child_active() {
        // Primary Done + non-detached child Working → CompletedLocallyChildrenActive,
        // derived kind must not be Done.
        let primary = obs(ObservationSource::ProcessExit, ActivityKind::Done, 1, 120);
        let child = obs_with_run(
            ObservationSource::ProviderLifecycle,
            ActivityKind::Working,
            1,
            120,
            "child-1",
            Some(PRIMARY),
        );
        let projection = reduce(true, &[primary, child]);

        assert_eq!(
            projection.phase,
            ParentPhase::CompletedLocallyChildrenActive
        );
        assert_ne!(projection.live.kind, LiveStatusKind::Done);
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn mixed_children_running_failed_completed() {
        // Primary Done + children Working/Failed/Done → still not fully
        // complete because the Working child remains non-terminal.
        let primary = obs(ObservationSource::ProcessExit, ActivityKind::Done, 1, 120);
        let running = obs_with_run(
            ObservationSource::ProviderLifecycle,
            ActivityKind::Working,
            1,
            120,
            "child-running",
            Some(PRIMARY),
        );
        let failed = obs_with_run(
            ObservationSource::ProcessExit,
            ActivityKind::Failed,
            1,
            120,
            "child-failed",
            Some(PRIMARY),
        );
        let done = obs_with_run(
            ObservationSource::ProcessExit,
            ActivityKind::Done,
            1,
            120,
            "child-done",
            Some(PRIMARY),
        );
        let projection = reduce(
            true,
            &[primary.clone(), running, failed.clone(), done.clone()],
        );

        assert_eq!(
            projection.phase,
            ParentPhase::CompletedLocallyChildrenActive
        );
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForInput);

        // With all children terminal, the parent FullyCompletes.
        let projection = reduce(true, &[primary, failed, done]);

        assert_eq!(projection.phase, ParentPhase::FullyCompleted);
        assert_eq!(projection.live.kind, LiveStatusKind::Done);
    }

    #[test]
    fn child_completion_then_parent_resumption() {
        // After a child completes, the primary resumes Working → ActivelyWorking.
        let primary = obs(
            ObservationSource::ProviderLifecycle,
            ActivityKind::Working,
            1,
            120,
        );
        let child = obs_with_run(
            ObservationSource::ProcessExit,
            ActivityKind::Done,
            1,
            120,
            "child-1",
            Some(PRIMARY),
        );
        let projection = reduce(true, &[primary, child]);

        assert_eq!(projection.phase, ParentPhase::ActivelyWorking);
        assert_eq!(projection.live.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn orphaned_stale_delegated_run_expires() {
        // Primary Done + child Working but the child observation has expired.
        // The expired child is dropped, allowing the parent to FullyComplete.
        let primary = obs(ObservationSource::ProcessExit, ActivityKind::Done, 1, 120);
        let expired_child = {
            let observed_at = now() - Duration::from_secs(200);
            StatusObservation {
                source: ObservationSource::ProviderLifecycle,
                observed_at,
                expires_at: observed_at + Duration::from_secs(120),
                confidence: Confidence::High,
                run_id: "child-1".to_string(),
                parent_run_id: Some(PRIMARY.to_string()),
                kind: ActivityKind::Working,
            }
        };
        let projection = reduce(true, &[primary, expired_child]);

        assert_eq!(projection.phase, ParentPhase::FullyCompleted);
        assert_eq!(projection.live.kind, LiveStatusKind::Done);
    }

    #[test]
    fn conflicting_observations_time_and_confidence() {
        // Older High Working vs newer High Waiting, same run → newer wins.
        let older = obs(
            ObservationSource::ProviderLifecycle,
            ActivityKind::Working,
            10,
            120,
        );
        let newer = obs(
            ObservationSource::ProviderLifecycle,
            ActivityKind::WaitingInput,
            1,
            120,
        );
        let projection = reduce(false, &[older, newer]);

        assert_eq!(projection.phase, ParentPhase::WaitingForUser);
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForInput);

        // Equal-timestamp disagreement within the single structured lifecycle
        // source resolves by activity-state rank (busy beats waiting) rather
        // than projecting Unknown: cross-source conflict is unreachable now
        // that the only structured source is the folded native lifecycle.
        let at = |age: u64| now() - Duration::from_secs(age);
        let working = StatusObservation {
            source: ObservationSource::ProviderLifecycle,
            observed_at: at(1),
            expires_at: at(1) + Duration::from_secs(120),
            confidence: Confidence::High,
            run_id: PRIMARY.to_string(),
            parent_run_id: None,
            kind: ActivityKind::Working,
        };
        let waiting = StatusObservation {
            source: ObservationSource::ProviderLifecycle,
            observed_at: at(1),
            expires_at: at(1) + Duration::from_secs(120),
            confidence: Confidence::High,
            run_id: PRIMARY.to_string(),
            parent_run_id: None,
            kind: ActivityKind::WaitingInput,
        };
        let projection = reduce(false, &[working, waiting]);

        assert_eq!(projection.phase, ParentPhase::ActivelyWorking);
        assert_eq!(projection.live.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn process_exit_beats_stale_hook() {
        // ProcessExit Done + expired lifecycle Working → Done.
        let exit = obs(ObservationSource::ProcessExit, ActivityKind::Done, 5, 120);
        let stale_hook = {
            let observed_at = now() - Duration::from_secs(200);
            StatusObservation {
                source: ObservationSource::ProviderLifecycle,
                observed_at,
                expires_at: observed_at + Duration::from_secs(120),
                confidence: Confidence::High,
                run_id: PRIMARY.to_string(),
                parent_run_id: None,
                kind: ActivityKind::Working,
            }
        };
        let projection = reduce(false, &[exit, stale_hook]);

        assert_eq!(projection.phase, ParentPhase::FullyCompleted);
        assert_eq!(projection.live.kind, LiveStatusKind::Done);
    }

    #[test]
    fn no_trustworthy_evidence_yields_unknown() {
        // Empty observations, alive or not → Unknown.
        let projection = reduce(true, &[]);
        assert_eq!(projection.phase, ParentPhase::Unknown);
        assert_eq!(projection.live.kind, LiveStatusKind::Unknown);

        let projection = reduce_no_liveness(&[]);
        assert_eq!(projection.phase, ParentPhase::Unknown);
        assert_eq!(projection.live.kind, LiveStatusKind::Unknown);
    }
}
