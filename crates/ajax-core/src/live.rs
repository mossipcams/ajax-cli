use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

#[path = "live_application.rs"]
mod application;
#[path = "live_recognize.rs"]
mod recognize;
pub use crate::models::{AgentClient, LiveObservation, LiveStatusKind};
pub use application::{
    acknowledge_attention, apply_authoritative_observation, apply_authoritative_observation_at,
    apply_observation, apply_observation_at, apply_trusted_observation,
    apply_trusted_observation_at, has_pending_live_class_candidate, has_pending_running_candidate,
    has_pending_waiting_candidate, RUNNING_CANDIDATE_SINCE_KEY, WAITING_CANDIDATE_SINCE_KEY,
};

/// Freshness window for a Codex `working` hook value.
const CODEX_WORKING_FRESH_FOR: Duration = Duration::from_secs(20);
/// Freshness window for hook waiting/approval values and Claude hook states.
const HOOK_DEFAULT_FRESH_FOR: Duration = Duration::from_secs(120);
/// Freshness window for an active runtime-wrapper heartbeat.
const WRAPPER_RUNNING_FRESH_FOR: Duration = Duration::from_secs(30);

/// Wrapper terminal evidence decays on the same clock as hook completion
/// evidence: the wrapper only vouches for the process it supervised, and an
/// agent relaunched directly in the session must win via pane capture.
const WRAPPER_TERMINAL_FRESH_FOR: Duration = HOOK_DEFAULT_FRESH_FOR;
/// Freshness window for non-terminal structured provider lifecycle events.
const LIFECYCLE_FRESH_FOR: Duration = Duration::from_secs(30 * 60);
/// Terminal lifecycle events persist until superseded by newer evidence.
const LIFECYCLE_TERMINAL_FRESH_FOR: Duration = Duration::from_secs(365 * 24 * 3600);

/// Value-typed input for the pure per-agent hook freshness decision.
///
/// The decision is intentionally free of lifecycle, task mutation, and clock
/// access so callers can inject `now` and test it deterministically.
#[derive(Clone, Copy, Debug)]
pub struct HookDecisionInput<'a> {
    pub selected_agent: AgentClient,
    pub prior: Option<&'a LiveObservation>,
    pub value: &'a str,
    pub observed_at: SystemTime,
    pub acknowledged_at: Option<SystemTime>,
    pub now: SystemTime,
}

/// Result of [`decide_hook_observation`].
///
/// `applied` reports whether the hook produced eligible evidence. When it does
/// not, `observation` carries the preserved prior observation unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookDecision {
    pub applied: bool,
    pub observation: Option<LiveObservation>,
}

/// Decide whether a single hook value is eligible for the selected agent.
pub fn decide_hook_observation(input: HookDecisionInput<'_>) -> HookDecision {
    let preserved = HookDecision {
        applied: false,
        observation: input.prior.cloned(),
    };

    match hook_observation_if_eligible(
        input.selected_agent,
        input.value,
        input.observed_at,
        input.acknowledged_at,
        input.now,
    ) {
        Some(observation) => HookDecision {
            applied: true,
            observation: Some(observation),
        },
        None => preserved,
    }
}

/// Parse and freshness-check a hook value for the selected agent.
///
/// Returns the eligible observation, or `None` when the value is malformed,
/// the agent ignores hooks, the value is stale, or waiting/completion evidence
/// is at or before an acknowledgment.
fn hook_observation_if_eligible(
    agent: AgentClient,
    value: &str,
    observed_at: SystemTime,
    acknowledged_at: Option<SystemTime>,
    now: SystemTime,
) -> Option<LiveObservation> {
    if agent == AgentClient::Other {
        return None;
    }

    let observation = classify_agent_status_value(value)?;
    let window = hook_freshness_window(agent, observation.kind)?;
    if !within_window(now, observed_at, window) {
        return None;
    }

    if is_acknowledgeable_kind(observation.kind)
        && acknowledged_at.is_some_and(|acknowledged_at| observed_at <= acknowledged_at)
    {
        return None;
    }

    Some(observation)
}

fn hook_freshness_window(agent: AgentClient, kind: LiveStatusKind) -> Option<Duration> {
    match (agent, kind) {
        (AgentClient::Other, _) => None,
        (AgentClient::Codex, LiveStatusKind::AgentRunning) => Some(CODEX_WORKING_FRESH_FOR),
        (
            _,
            LiveStatusKind::AgentRunning
            | LiveStatusKind::WaitingForInput
            | LiveStatusKind::WaitingForApproval
            | LiveStatusKind::Done
            | LiveStatusKind::CommandFailed,
        ) => Some(HOOK_DEFAULT_FRESH_FOR),
        _ => None,
    }
}

/// Inclusive freshness check. A future `observed_at` is treated as fresh,
/// matching the existing clock-skew behavior and the agent-deck reference.
fn within_window(now: SystemTime, observed_at: SystemTime, window: Duration) -> bool {
    match now.duration_since(observed_at) {
        Ok(age) => age <= window,
        Err(_) => true,
    }
}

fn is_waiting_kind(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForInput | LiveStatusKind::WaitingForApproval
    )
}

fn is_acknowledgeable_kind(kind: LiveStatusKind) -> bool {
    is_waiting_kind(kind) || kind == LiveStatusKind::Done
}

/// Origin of an agent-status candidate considered by the status decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentEvidenceSource {
    /// Ajax launch-wrapper runtime snapshot: trusted process evidence.
    RuntimeWrapper,
    /// Hook-backed status file: observational hint.
    Hook,
    /// Structured provider lifecycle event from the `__agent-event` sink.
    Lifecycle,
}

/// A single agent-status candidate considered by [`select_status_observation`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusCandidate {
    pub source: AgentEvidenceSource,
    pub value: String,
    pub observed_at: SystemTime,
    /// When set, this candidate belongs to a delegated/child run. Session-level
    /// hooks leave this `None` (primary run).
    pub run_id: Option<String>,
    pub parent_run_id: Option<String>,
}

impl StatusCandidate {
    pub fn new(
        source: AgentEvidenceSource,
        value: impl Into<String>,
        observed_at: SystemTime,
    ) -> Self {
        Self {
            source,
            value: value.into(),
            observed_at,
            run_id: None,
            parent_run_id: None,
        }
    }

    pub fn with_run(mut self, run_id: impl Into<String>, parent_run_id: Option<String>) -> Self {
        self.run_id = Some(run_id.into());
        self.parent_run_id = parent_run_id;
        self
    }
}

/// Value-typed input for the multi-source status decision.
pub struct StatusDecisionInput<'a> {
    pub selected_agent: AgentClient,
    pub prior: Option<&'a LiveObservation>,
    pub acknowledged_at: Option<SystemTime>,
    pub now: SystemTime,
    pub candidates: &'a [StatusCandidate],
    /// Additional observations (pane projections, delegated child runs) merged
    /// into the reducer after wrapper/hook candidates.
    pub extra_observations: &'a [crate::agent_status::StatusObservation],
}

/// Result of [`select_status_observation`].
///
/// `applied` reports whether stronger evidence than the prior observation was
/// found. When it is `false`, `observation` carries the preserved prior and
/// `source` is `None`, so the caller may fall through to pane capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusDecision {
    pub applied: bool,
    pub observation: Option<LiveObservation>,
    pub source: Option<AgentEvidenceSource>,
    pub observed_at: Option<SystemTime>,
    /// True when no candidate applied because fresh waiting/completion evidence
    /// was suppressed by an acknowledgment. The caller should hold the current
    /// non-actionable state and skip pane capture rather than re-raise it.
    pub acknowledged_hold: bool,
    /// Wrapper heartbeat observed the agent process as alive. Informational
    /// only — never alone implies [`LiveStatusKind::AgentRunning`].
    pub process_alive: bool,
}

/// Select the strongest eligible agent-status observation.
///
/// Candidates are projected onto the conservative
/// [`crate::agent_status`] observation/run model. A wrapper `working`/
/// `starting` heartbeat is treated as process liveness only — it never selects
/// an agent-activity observation — and `done`/`failed` from the wrapper is
/// treated as a `ProcessExit` activity on the primary run. Hook values are
/// projected as `ProviderHook` observations. Missing-substrate priors are
/// preserved over any candidate (the reducer falls through to pane capture),
/// and fresh waiting/completion evidence suppressed by an acknowledgment is
/// reported via `acknowledged_hold` so the caller can hold the current state
/// without re-raising it.
pub fn select_status_observation(input: StatusDecisionInput<'_>) -> StatusDecision {
    let preserved = StatusDecision {
        applied: false,
        observation: input.prior.cloned(),
        source: None,
        observed_at: None,
        acknowledged_hold: false,
        process_alive: false,
    };

    if input
        .prior
        .is_some_and(|prior| prior.kind.is_missing_substrate())
    {
        return preserved;
    }

    let now = input.now;
    let mut process_alive = false;
    let mut observations: Vec<crate::agent_status::StatusObservation> = Vec::new();
    let mut acknowledged_hold = false;

    for candidate in input.candidates {
        match candidate.source {
            AgentEvidenceSource::RuntimeWrapper => {
                let Some(observation) = classify_agent_status_value(&candidate.value) else {
                    continue;
                };
                match observation.kind {
                    LiveStatusKind::Done | LiveStatusKind::CommandFailed => {
                        if !within_window(now, candidate.observed_at, WRAPPER_TERMINAL_FRESH_FOR) {
                            continue;
                        }
                        if is_acknowledgeable_kind(observation.kind)
                            && input
                                .acknowledged_at
                                .is_some_and(|ack| candidate.observed_at <= ack)
                        {
                            acknowledged_hold = true;
                            continue;
                        }
                        let Some(activity) = live_kind_to_activity(observation.kind) else {
                            continue;
                        };
                        observations.push(crate::agent_status::StatusObservation {
                            source: crate::agent_status::ObservationSource::ProcessExit,
                            observed_at: candidate.observed_at,
                            expires_at: candidate.observed_at + WRAPPER_TERMINAL_FRESH_FOR,
                            confidence: crate::agent_status::Confidence::High,
                            run_id: candidate
                                .run_id
                                .clone()
                                .unwrap_or_else(|| PRIMARY_RUN_ID.to_string()),
                            parent_run_id: candidate.parent_run_id.clone(),
                            kind: activity,
                        });
                    }
                    LiveStatusKind::AgentRunning
                    | LiveStatusKind::CommandRunning
                    | LiveStatusKind::TestsRunning => {
                        // A wrapper heartbeat confirms the process is alive,
                        // never that the agent is actively working. Activity
                        // must come from structured pane/hook/lifecycle.
                        if within_window(now, candidate.observed_at, WRAPPER_RUNNING_FRESH_FOR) {
                            process_alive = true;
                        }
                    }
                    _ => {}
                }
            }
            AgentEvidenceSource::Hook => {
                let Some(observation) = hook_observation_if_eligible(
                    input.selected_agent,
                    &candidate.value,
                    candidate.observed_at,
                    None,
                    now,
                ) else {
                    continue;
                };
                if is_acknowledgeable_kind(observation.kind)
                    && input
                        .acknowledged_at
                        .is_some_and(|ack| candidate.observed_at <= ack)
                {
                    acknowledged_hold = true;
                    continue;
                }
                let Some(activity) = live_kind_to_activity(observation.kind) else {
                    continue;
                };
                let window = hook_freshness_window(input.selected_agent, observation.kind)
                    .unwrap_or(HOOK_DEFAULT_FRESH_FOR);
                observations.push(crate::agent_status::StatusObservation {
                    source: crate::agent_status::ObservationSource::ProviderHook,
                    observed_at: candidate.observed_at,
                    expires_at: candidate.observed_at + window,
                    confidence: crate::agent_status::Confidence::High,
                    run_id: candidate
                        .run_id
                        .clone()
                        .unwrap_or_else(|| PRIMARY_RUN_ID.to_string()),
                    parent_run_id: candidate.parent_run_id.clone(),
                    kind: activity,
                });
            }
            AgentEvidenceSource::Lifecycle => {
                let Some(observation) = classify_agent_status_value(&candidate.value) else {
                    continue;
                };
                let (eligible, window) = match observation.kind {
                    LiveStatusKind::Done | LiveStatusKind::CommandFailed => {
                        (true, LIFECYCLE_TERMINAL_FRESH_FOR)
                    }
                    LiveStatusKind::AgentRunning
                    | LiveStatusKind::WaitingForInput
                    | LiveStatusKind::WaitingForApproval => {
                        let window = LIFECYCLE_FRESH_FOR;
                        (within_window(now, candidate.observed_at, window), window)
                    }
                    _ => continue,
                };
                if !eligible {
                    continue;
                }
                if is_acknowledgeable_kind(observation.kind)
                    && input
                        .acknowledged_at
                        .is_some_and(|ack| candidate.observed_at <= ack)
                {
                    acknowledged_hold = true;
                    continue;
                }
                let Some(activity) = live_kind_to_activity(observation.kind) else {
                    continue;
                };
                observations.push(crate::agent_status::StatusObservation {
                    source: crate::agent_status::ObservationSource::ProviderLifecycle,
                    observed_at: candidate.observed_at,
                    expires_at: candidate.observed_at + window,
                    confidence: crate::agent_status::Confidence::High,
                    run_id: candidate
                        .run_id
                        .clone()
                        .unwrap_or_else(|| PRIMARY_RUN_ID.to_string()),
                    parent_run_id: candidate.parent_run_id.clone(),
                    kind: activity,
                });
            }
        }
    }

    observations.extend_from_slice(input.extra_observations);
    drop_hooks_superseded_by_lifecycle(&mut observations, now);

    let projection = crate::agent_status::reduce_agent_status(crate::agent_status::ReduceInput {
        now,
        primary_run_id: PRIMARY_RUN_ID.to_string(),
        process_liveness: Some(crate::agent_status::ProcessLiveness {
            alive: process_alive,
            observed_at: now,
        }),
        observations: &observations,
    });

    status_decision_from_projection(
        projection,
        process_alive,
        acknowledged_hold,
        input.prior,
        now,
    )
}

/// Fresh lifecycle evidence on a run supersedes hook files so equal-timestamp
/// class disagreements never downgrade lifecycle into reducer conflict.
fn drop_hooks_superseded_by_lifecycle(
    observations: &mut Vec<crate::agent_status::StatusObservation>,
    now: SystemTime,
) {
    let lifecycle_runs: BTreeSet<String> = observations
        .iter()
        .filter(|observation| {
            observation.source == crate::agent_status::ObservationSource::ProviderLifecycle
                && observation.expires_at >= now
        })
        .map(|observation| observation.run_id.clone())
        .collect();
    if lifecycle_runs.is_empty() {
        return;
    }
    observations.retain(|observation| {
        observation.source != crate::agent_status::ObservationSource::ProviderHook
            || !lifecycle_runs.contains(&observation.run_id)
    });
}

/// Map a reducer projection onto the CLI/refresh [`StatusDecision`] surface.
fn status_decision_from_projection(
    projection: crate::agent_status::StatusProjection,
    process_alive: bool,
    acknowledged_hold: bool,
    prior: Option<&LiveObservation>,
    now: SystemTime,
) -> StatusDecision {
    let preserved = StatusDecision {
        applied: false,
        observation: prior.cloned(),
        source: None,
        observed_at: None,
        acknowledged_hold,
        process_alive,
    };

    if projection.phase == crate::agent_status::ParentPhase::Unknown {
        return preserved;
    }

    let agent_source = match projection.selected_source {
        Some(crate::agent_status::ObservationSource::ProcessExit) => {
            Some(AgentEvidenceSource::RuntimeWrapper)
        }
        Some(crate::agent_status::ObservationSource::ProviderHook) => {
            Some(AgentEvidenceSource::Hook)
        }
        Some(crate::agent_status::ObservationSource::ProviderLifecycle) => {
            Some(AgentEvidenceSource::Lifecycle)
        }
        _ => None,
    };

    StatusDecision {
        applied: true,
        observation: Some(projection.live),
        source: agent_source,
        observed_at: projection.selected_observed_at.or(Some(now)),
        acknowledged_hold: false,
        process_alive,
    }
}

/// Run id used by the [`live`] adapter when projecting wrapper/hook candidates
/// onto the conservative observation model.
const PRIMARY_RUN_ID: &str = "primary";

/// Map a presentation [`LiveStatusKind`] returned by `classify_*` helpers onto
/// the reducer's activity-only enum. Kinds that are not activity observations
/// return `None`.
fn live_kind_to_activity(kind: LiveStatusKind) -> Option<crate::agent_status::ActivityKind> {
    Some(match kind {
        LiveStatusKind::AgentRunning => crate::agent_status::ActivityKind::Working,
        LiveStatusKind::CommandRunning => crate::agent_status::ActivityKind::CommandRunning,
        LiveStatusKind::TestsRunning => crate::agent_status::ActivityKind::TestsRunning,
        LiveStatusKind::WaitingForInput => crate::agent_status::ActivityKind::WaitingInput,
        LiveStatusKind::WaitingForApproval => crate::agent_status::ActivityKind::WaitingApproval,
        LiveStatusKind::Done => crate::agent_status::ActivityKind::Done,
        LiveStatusKind::CommandFailed => crate::agent_status::ActivityKind::Failed,
        _ => return None,
    })
}

const STRUCTURED_PANE_FRESH_FOR: Duration = Duration::from_secs(60);
const GENERIC_PANE_FRESH_FOR: Duration = Duration::from_secs(15);

/// Project a visible-pane capture onto a single conservative
/// [`crate::agent_status::StatusObservation`].
///
/// Pane text is weak evidence. The only signals it may emit are
/// `Busy`, `IdlePrompt`, and `ApprovalPrompt`, all
/// positionally anchored to the visible screen bottom. Pane text never
/// asserts completion, failure, or stuck states — those belong to the
/// runtime wrapper exit snapshot, provider hooks/lifecycle events, and
/// git/`gh` substrate evidence. Busy chrome maps to a Low-confidence
/// `GenericPane` observation (which cannot alone assert `AgentRunning`
/// through the reducer); structured recognition (stream-json, anchored
/// prompt chrome) maps to Medium-confidence `StructuredPane`.
///
/// Returns `None` when the pane yields no hint, so neutral or unparseable
/// panes preserve prior credible state instead of overwriting it.
pub fn project_pane_activity(
    agent: AgentClient,
    pane: &str,
    now: SystemTime,
) -> Option<crate::agent_status::StatusObservation> {
    let (hint, recognition) = recognize::recognize_pane(agent, pane)?;

    let kind = match hint {
        recognize::PaneHint::Busy => crate::agent_status::ActivityKind::Working,
        recognize::PaneHint::IdlePrompt => crate::agent_status::ActivityKind::WaitingInput,
        recognize::PaneHint::ApprovalPrompt => crate::agent_status::ActivityKind::WaitingApproval,
    };
    let (source, confidence, fresh_for) = match (hint, recognition) {
        (recognize::PaneHint::Busy, recognize::Recognition::Chrome) => (
            crate::agent_status::ObservationSource::GenericPane,
            crate::agent_status::Confidence::Low,
            GENERIC_PANE_FRESH_FOR,
        ),
        _ => (
            crate::agent_status::ObservationSource::StructuredPane,
            crate::agent_status::Confidence::Medium,
            STRUCTURED_PANE_FRESH_FOR,
        ),
    };

    Some(crate::agent_status::StatusObservation {
        source,
        observed_at: now,
        expires_at: now + fresh_for,
        confidence,
        run_id: PRIMARY_RUN_ID.to_string(),
        parent_run_id: None,
        kind,
    })
}
pub fn classify_agent_status_value(value: &str) -> Option<LiveObservation> {
    match value.trim() {
        "starting" | "working" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "wait" => Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        )),
        "ask" => Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        )),
        "done" | "parked" => Some(LiveObservation::new(LiveStatusKind::Done, "done")),
        "failed" => Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "agent failed",
        )),
        _ => None,
    }
}

pub fn reduce_live_observation(
    current: Option<&LiveObservation>,
    next: LiveObservation,
) -> LiveObservation {
    let Some(current) = current else {
        return next;
    };

    if next.kind.is_missing_substrate() {
        return next;
    }

    if current.kind.is_missing_substrate() {
        return current.clone();
    }

    if should_keep_current_status(current.kind, next.kind) {
        return current.clone();
    }

    next
}

fn should_keep_current_status(current: LiveStatusKind, next: LiveStatusKind) -> bool {
    if current == LiveStatusKind::Done {
        return is_passive_observation(next);
    }

    if current == LiveStatusKind::TestsRunning {
        return matches!(
            next,
            LiveStatusKind::AgentRunning | LiveStatusKind::CommandRunning
        );
    }

    if is_waiting_status(current) {
        return is_passive_observation(next);
    }

    if is_failure_status(current) {
        return is_passive_observation(next);
    }

    false
}

fn is_waiting_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput
    )
}

fn is_failure_status(kind: LiveStatusKind) -> bool {
    matches!(
        kind,
        LiveStatusKind::AuthRequired
            | LiveStatusKind::RateLimited
            | LiveStatusKind::ContextLimit
            | LiveStatusKind::CiFailed
            | LiveStatusKind::CommandFailed
            | LiveStatusKind::Blocked
            | LiveStatusKind::MergeConflict
    )
}

fn is_passive_observation(kind: LiveStatusKind) -> bool {
    matches!(kind, LiveStatusKind::ShellIdle | LiveStatusKind::Unknown)
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
    };

    use super::{apply_observation, apply_observation_at, classify_agent_status_value};

    fn base_task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        )
    }

    #[test]
    fn pane_projection_maps_anchored_idle_prompt_to_waiting_input() {
        let now = std::time::SystemTime::now();
        let pane = "Here is my plan.\n\n❯\n  ⏵⏵ bypass permissions on (shift+tab to cycle)";

        let observation = super::project_pane_activity(AgentClient::Claude, pane, now)
            .expect("anchored idle prompt should project");

        assert_eq!(
            observation.kind,
            crate::agent_status::ActivityKind::WaitingInput
        );
        assert_eq!(
            observation.source,
            crate::agent_status::ObservationSource::StructuredPane
        );
        assert_eq!(
            observation.confidence,
            crate::agent_status::Confidence::Medium
        );
    }

    #[test]
    fn pane_projection_maps_anchored_permission_menu_to_waiting_approval() {
        let now = std::time::SystemTime::now();
        let pane = "Do you want to run this command?\n❯ 1. Yes\n  2. No\nEsc to cancel";

        let observation = super::project_pane_activity(AgentClient::Claude, pane, now)
            .expect("anchored permission menu should project");

        assert_eq!(
            observation.kind,
            crate::agent_status::ActivityKind::WaitingApproval
        );
        assert_eq!(
            observation.source,
            crate::agent_status::ObservationSource::StructuredPane
        );
    }

    /// The core false-positive regression: pane prose about failures,
    /// conflicts, CI, auth, limits, or completion is never live evidence.
    /// Those statuses belong to the wrapper exit snapshot, hooks, and
    /// git/`gh` substrate evidence.
    #[test]
    fn pane_projection_never_asserts_failure_stuck_or_completion() {
        let now = std::time::SystemTime::now();
        for pane in [
            "test result: FAILED\nCommand failed with exit code 1",
            "CONFLICT (content): merge conflict in a.rs",
            "Automatic merge failed; fix conflicts and then commit the result.",
            "ci failed on main",
            "check run failed: cargo test",
            "please login to continue",
            "rate limit exceeded; try again later",
            "context limit reached",
            "this is blocked, cannot continue",
            "all done, task complete",
            "✓ Successfully completed task",
            "test result: ok. 37 passed",
        ] {
            assert_eq!(
                super::project_pane_activity(AgentClient::Claude, pane, now),
                None,
                "pane prose {pane:?} must never project an activity observation"
            );
        }
    }

    #[test]
    fn pane_projection_treats_unanchored_prose_as_neutral() {
        let now = std::time::SystemTime::now();
        for pane in [
            "The user asked \"did you mean the parser?\" — checking both.",
            "I am thinking about running the tests next.",
            "Waiting for input. Press Enter to continue.",
            "compiling ajax-core v0.51.7\n    Finished dev profile",
        ] {
            assert_eq!(
                super::project_pane_activity(AgentClient::Claude, pane, now),
                None,
                "unanchored prose {pane:?} must stay neutral"
            );
        }
    }
    #[test]
    fn agent_status_values_map_to_live_observations() {
        for (value, expected) in [
            ("working", Some(LiveStatusKind::AgentRunning)),
            ("done", Some(LiveStatusKind::Done)),
            ("wait", Some(LiveStatusKind::WaitingForInput)),
            ("ask", Some(LiveStatusKind::WaitingForApproval)),
            ("parked", Some(LiveStatusKind::Done)),
            ("", None),
            ("nonsense", None),
        ] {
            let observation = classify_agent_status_value(value);

            assert_eq!(
                observation.map(|observation| observation.kind),
                expected,
                "{value:?}"
            );
        }
    }

    #[test]
    fn missing_resource_observations_clear_agent_running() {
        for status in [
            LiveStatusKind::WorktreeMissing,
            LiveStatusKind::TmuxMissing,
            LiveStatusKind::TaskWindowMissing,
        ] {
            let mut task = base_task();
            task.agent_status = AgentRuntimeStatus::Running;
            task.add_side_flag(SideFlag::AgentRunning);

            apply_observation(&mut task, LiveObservation::new(status, "resource missing"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Dead, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
        }
    }

    #[test]
    fn running_observation_does_not_override_missing_resources() {
        let mut task = base_task();
        task.add_side_flag(SideFlag::WorktreeMissing);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn recovered_missing_resource_can_accept_new_live_status() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WorktreeMissing, "worktree missing"),
        );
        task.remove_side_flag(SideFlag::WorktreeMissing);
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn done_observation_is_not_downgraded_by_shell_idle() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::Done, "done"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::ShellIdle, "shell idle"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn waiting_observation_is_not_downgraded_by_passive_terminal_evidence() {
        for status in [LiveStatusKind::ShellIdle, LiveStatusKind::Unknown] {
            let mut task = base_task();

            apply_observation(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
            );
            apply_observation(&mut task, LiveObservation::new(status, "passive evidence"));

            assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting, "{status:?}");
            assert!(task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(LiveStatusKind::WaitingForApproval),
                "{status:?}"
            );
        }
    }

    #[test]
    fn waiting_for_approval_is_cleared_by_resumed_activity() {
        use std::time::{Duration, UNIX_EPOCH};
        for status in [
            LiveStatusKind::AgentRunning,
            LiveStatusKind::CommandRunning,
            LiveStatusKind::TestsRunning,
        ] {
            let mut task = base_task();

            apply_observation_at(
                &mut task,
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
                UNIX_EPOCH + Duration::from_secs(100),
            );
            // First busy sample only stamps the running candidate.
            apply_observation_at(
                &mut task,
                LiveObservation::new(status, "activity resumed"),
                UNIX_EPOCH + Duration::from_secs(110),
            );
            assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting, "{status:?}");
            // Dwell-confirmed busy sample clears waiting.
            apply_observation_at(
                &mut task,
                LiveObservation::new(status, "activity resumed"),
                UNIX_EPOCH + Duration::from_secs(115),
            );

            assert_eq!(task.agent_status, AgentRuntimeStatus::Running, "{status:?}");
            assert!(!task.has_side_flag(SideFlag::NeedsInput), "{status:?}");
            assert!(task.has_side_flag(SideFlag::AgentRunning), "{status:?}");
            assert_eq!(
                task.live_status
                    .as_ref()
                    .map(|live_status| live_status.kind),
                Some(status),
                "{status:?}"
            );
        }
    }

    #[test]
    fn done_yields_to_new_busy_evidence() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(LiveStatusKind::Done, "done")),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn failure_yields_to_new_busy_evidence() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(
                LiveStatusKind::CommandFailed,
                "command failed",
            )),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn done_keeps_over_shell_idle_and_unknown() {
        for passive in [LiveStatusKind::ShellIdle, LiveStatusKind::Unknown] {
            let reduced = super::reduce_live_observation(
                Some(&LiveObservation::new(LiveStatusKind::Done, "done")),
                LiveObservation::new(passive, "passive"),
            );

            assert_eq!(reduced.kind, LiveStatusKind::Done, "{passive:?}");
        }
    }

    #[test]
    fn failed_observation_yields_to_later_busy_output() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandFailed, "command failed"),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CommandRunning, "command running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandRunning)
        );
    }

    #[test]
    fn blocked_observation_yields_to_later_busy_output() {
        let reduced = super::reduce_live_observation(
            Some(&LiveObservation::new(
                LiveStatusKind::Blocked,
                "manual intervention required",
            )),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn merge_conflict_flag_is_cleared_by_later_input_prompt() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "merge conflict needs attention",
            ),
        );
        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn live_lifecycle_updates_ignore_invalid_transition_edges() {
        let mut task = base_task();
        task.lifecycle_status = crate::models::LifecycleStatus::Error;

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        );

        assert_eq!(task.lifecycle_status, crate::models::LifecycleStatus::Error);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn ci_failed_observation_marks_task_blocked_and_tests_failed() {
        let mut task = base_task();

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CiFailed)
        );

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn active_live_observation_refreshes_activity_and_clears_stale() {
        let mut task = base_task();
        task.last_activity_at = std::time::SystemTime::UNIX_EPOCH;
        task.add_side_flag(SideFlag::Stale);

        apply_observation(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert!(!task.has_side_flag(SideFlag::Stale));
        assert!(task.last_activity_at > std::time::SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn live_projection_functions_do_not_mutate_lifecycle_or_substrate() {
        let task = base_task();
        let lifecycle_before = task.lifecycle_status;
        let git_before = task.git_status.clone();
        let tmux_before = task.tmux_status.clone();
        let task_before = task.task_window_status.clone();

        let projected = super::project_pane_activity(
            AgentClient::Claude,
            "Do you want to run this command?\n❯ 1. Yes\n  2. No\nEsc to cancel",
            std::time::SystemTime::now(),
        );
        let reduced = super::reduce_live_observation(
            task.live_status.as_ref(),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(
            projected.map(|observation| observation.kind),
            Some(crate::agent_status::ActivityKind::WaitingApproval)
        );
        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
        assert_eq!(task.lifecycle_status, lifecycle_before);
        assert_eq!(task.git_status, git_before);
        assert_eq!(task.tmux_status, tmux_before);
        assert_eq!(task.task_window_status, task_before);
    }
    fn hook_now() -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000)
    }

    fn decide(
        agent: AgentClient,
        prior: Option<&LiveObservation>,
        value: &str,
        observed_at: std::time::SystemTime,
    ) -> super::HookDecision {
        super::decide_hook_observation(super::HookDecisionInput {
            selected_agent: agent,
            prior,
            value,
            observed_at,
            acknowledged_at: None,
            now: hook_now(),
        })
    }

    #[test]
    fn codex_working_hook_is_fresh_through_twenty_seconds() {
        let now = hook_now();
        let decision = decide(
            AgentClient::Codex,
            None,
            "working",
            now - std::time::Duration::from_secs(20),
        );

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn codex_working_hook_is_stale_after_twenty_seconds() {
        let now = hook_now();
        let prior = LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
        let decision = decide(
            AgentClient::Codex,
            Some(&prior),
            "working",
            now - std::time::Duration::from_secs(20) - std::time::Duration::from_nanos(1),
        );

        assert!(!decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn codex_wait_hook_is_fresh_through_two_minutes() {
        let now = hook_now();
        let decision = decide(
            AgentClient::Codex,
            None,
            "wait",
            now - std::time::Duration::from_secs(120),
        );

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn codex_wait_hook_is_stale_after_two_minutes() {
        let now = hook_now();
        let prior = LiveObservation::new(LiveStatusKind::Done, "done");
        let decision = decide(
            AgentClient::Codex,
            Some(&prior),
            "wait",
            now - std::time::Duration::from_secs(120) - std::time::Duration::from_nanos(1),
        );

        assert!(!decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn claude_working_hook_uses_two_minute_window() {
        let now = hook_now();
        let fresh = decide(
            AgentClient::Claude,
            None,
            "working",
            now - std::time::Duration::from_secs(120),
        );
        assert!(fresh.applied);
        assert_eq!(
            fresh.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );

        let prior = LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
        let stale = decide(
            AgentClient::Claude,
            Some(&prior),
            "working",
            now - std::time::Duration::from_secs(120) - std::time::Duration::from_nanos(1),
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn hook_future_timestamp_is_treated_as_fresh() {
        let now = hook_now();
        let decision = decide(
            AgentClient::Codex,
            None,
            "working",
            now + std::time::Duration::from_secs(5),
        );

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn other_agent_ignores_hook_values() {
        let now = hook_now();
        let prior = LiveObservation::new(LiveStatusKind::Done, "done");
        for value in ["working", "wait", "ask"] {
            let decision = decide(AgentClient::Other, Some(&prior), value, now);

            assert!(!decision.applied, "{value}");
            assert_eq!(
                decision
                    .observation
                    .as_ref()
                    .map(|observation| observation.kind),
                Some(LiveStatusKind::Done),
                "{value}"
            );
        }
    }

    #[test]
    fn malformed_hook_values_preserve_prior_observation() {
        let now = hook_now();
        let prior = LiveObservation::new(LiveStatusKind::Done, "done");
        for value in ["", "   ", "unknown", "WAIT"] {
            let decision = decide(AgentClient::Codex, Some(&prior), value, now);

            assert!(!decision.applied, "{value:?}");
            assert_eq!(
                decision
                    .observation
                    .as_ref()
                    .map(|observation| observation.kind),
                Some(LiveStatusKind::Done),
                "{value:?}"
            );
        }
    }

    fn candidate(
        source: super::AgentEvidenceSource,
        value: &str,
        sub: std::time::Duration,
    ) -> super::StatusCandidate {
        super::StatusCandidate::new(source, value, hook_now() - sub)
    }

    fn select(
        agent: AgentClient,
        prior: Option<&LiveObservation>,
        candidates: &[super::StatusCandidate],
    ) -> super::StatusDecision {
        super::select_status_observation(super::StatusDecisionInput {
            selected_agent: agent,
            prior,
            acknowledged_at: None,
            now: hook_now(),
            candidates,
            extra_observations: &[],
        })
    }

    #[rstest::rstest]
    #[case(AgentClient::Claude, crate::live::AgentEvidenceSource::Hook, "wait")]
    #[case(AgentClient::Codex, crate::live::AgentEvidenceSource::Hook, "wait")]
    #[case(
        AgentClient::Codex,
        crate::live::AgentEvidenceSource::RuntimeWrapper,
        "done"
    )]
    fn acknowledged_waiting_and_completion_candidates_are_held_for_every_agent(
        #[case] agent: AgentClient,
        #[case] source: crate::live::AgentEvidenceSource,
        #[case] value: &str,
    ) {
        let now = hook_now();
        let observed_at = now - std::time::Duration::from_secs(2);
        let candidates = [super::StatusCandidate::new(source, value, observed_at)];

        let decision = super::select_status_observation(super::StatusDecisionInput {
            selected_agent: agent,
            prior: None,
            acknowledged_at: Some(now - std::time::Duration::from_secs(1)),
            now,
            candidates: &candidates,
            extra_observations: &[],
        });

        assert!(!decision.applied);
        assert!(decision.acknowledged_hold);
        assert_eq!(decision.observed_at, None);
    }

    #[test]
    fn selected_status_decision_returns_source_timestamp() {
        let now = hook_now();
        let observed_at = now - std::time::Duration::from_secs(1);
        let candidates = [super::StatusCandidate::new(
            super::AgentEvidenceSource::Hook,
            "wait",
            observed_at,
        )];

        let decision = super::select_status_observation(super::StatusDecisionInput {
            selected_agent: AgentClient::Codex,
            prior: None,
            acknowledged_at: None,
            now,
            candidates: &candidates,
            extra_observations: &[],
        });

        assert!(decision.applied);
        assert_eq!(decision.observed_at, Some(observed_at));
    }

    #[test]
    fn missing_substrate_outranks_wrapper_and_hook_activity() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let prior = LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux missing");
        let secs = std::time::Duration::from_secs(1);
        let candidates = [
            candidate(RuntimeWrapper, "working", secs),
            candidate(RuntimeWrapper, "done", secs),
            candidate(Hook, "working", secs),
        ];

        let decision = select(AgentClient::Codex, Some(&prior), &candidates);

        assert!(!decision.applied);
        assert!(decision.source.is_none());
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
    }

    #[test]
    fn fresh_wrapper_completion_outranks_newer_hook_working() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let candidates = [
            candidate(RuntimeWrapper, "done", std::time::Duration::from_secs(60)),
            candidate(Hook, "working", std::time::Duration::from_secs(1)),
        ];

        let decision = select(AgentClient::Codex, None, &candidates);

        assert!(decision.applied);
        assert_eq!(
            decision.source,
            Some(super::AgentEvidenceSource::RuntimeWrapper)
        );
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );
    }

    #[test]
    fn fresh_wrapper_failure_outranks_newer_hook_waiting() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let candidates = [
            candidate(RuntimeWrapper, "failed", std::time::Duration::from_secs(60)),
            candidate(Hook, "wait", std::time::Duration::from_secs(1)),
        ];

        let decision = select(AgentClient::Codex, None, &candidates);

        assert!(decision.applied);
        assert_eq!(
            decision.source,
            Some(super::AgentEvidenceSource::RuntimeWrapper)
        );
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }

    #[test]
    fn stale_wrapper_running_falls_through_to_fresh_hook() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let candidates = [
            candidate(
                RuntimeWrapper,
                "working",
                std::time::Duration::from_secs(31),
            ),
            candidate(Hook, "wait", std::time::Duration::from_secs(1)),
        ];

        let decision = select(AgentClient::Codex, None, &candidates);

        assert!(decision.applied);
        assert_eq!(decision.source, Some(super::AgentEvidenceSource::Hook));
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn wrapper_running_is_fresh_through_thirty_seconds() {
        use super::AgentEvidenceSource::RuntimeWrapper;
        let prior = LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");

        // A wrapper heartbeat is process liveness only: even while fresh, it
        // does not assert `AgentRunning`. The reducer falls through, so the
        // prior credible state is preserved and the caller may probe the pane.
        let fresh = select(
            AgentClient::Codex,
            Some(&prior),
            &[candidate(
                RuntimeWrapper,
                "working",
                std::time::Duration::from_secs(30),
            )],
        );
        assert!(!fresh.applied);
        assert_eq!(
            fresh.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );

        let stale = select(
            AgentClient::Codex,
            Some(&prior),
            &[super::StatusCandidate::new(
                RuntimeWrapper,
                "working",
                hook_now()
                    - std::time::Duration::from_secs(30)
                    - std::time::Duration::from_nanos(1),
            )],
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn wrapper_terminal_is_fresh_through_two_minutes() {
        use super::AgentEvidenceSource::RuntimeWrapper;
        let prior = LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");

        let fresh = select(
            AgentClient::Codex,
            Some(&prior),
            &[candidate(
                RuntimeWrapper,
                "done",
                std::time::Duration::from_secs(120),
            )],
        );
        assert!(fresh.applied);
        assert_eq!(
            fresh.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );

        let stale = select(
            AgentClient::Codex,
            Some(&prior),
            &[super::StatusCandidate::new(
                RuntimeWrapper,
                "done",
                hook_now()
                    - std::time::Duration::from_secs(120)
                    - std::time::Duration::from_nanos(1),
            )],
        );
        assert!(!stale.applied);
        assert_eq!(
            stale.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn stale_wrapper_terminal_falls_through_to_fresh_hook() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let candidates = [
            candidate(RuntimeWrapper, "done", std::time::Duration::from_secs(121)),
            candidate(Hook, "working", std::time::Duration::from_secs(1)),
        ];

        let decision = select(AgentClient::Codex, None, &candidates);

        assert!(decision.applied);
        assert_eq!(decision.source, Some(super::AgentEvidenceSource::Hook));
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn stale_acknowledged_wrapper_terminal_does_not_hold() {
        use super::AgentEvidenceSource::RuntimeWrapper;
        let now = hook_now();
        let observed_at = now - std::time::Duration::from_secs(121);
        let candidates = [super::StatusCandidate::new(
            RuntimeWrapper,
            "done",
            observed_at,
        )];

        let decision = super::select_status_observation(super::StatusDecisionInput {
            selected_agent: AgentClient::Claude,
            prior: None,
            acknowledged_at: Some(now - std::time::Duration::from_secs(1)),
            now,
            candidates: &candidates,
            extra_observations: &[],
        });

        assert!(!decision.applied);
        assert!(!decision.acknowledged_hold);
    }

    #[test]
    fn runtime_wrapper_wins_equal_timestamp_tie_with_hook() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        let secs = std::time::Duration::from_secs(1);
        for candidates in [
            vec![
                candidate(RuntimeWrapper, "done", secs),
                candidate(Hook, "working", secs),
            ],
            vec![
                candidate(Hook, "working", secs),
                candidate(RuntimeWrapper, "done", secs),
            ],
        ] {
            let decision = select(AgentClient::Codex, None, &candidates);

            assert!(decision.applied);
            assert_eq!(
                decision.source,
                Some(super::AgentEvidenceSource::RuntimeWrapper)
            );
            assert_eq!(
                decision.observation.map(|observation| observation.kind),
                Some(LiveStatusKind::Done)
            );
        }
    }

    #[test]
    fn busy_hook_wins_equal_timestamp_tie_with_waiting_hook() {
        use super::AgentEvidenceSource::Hook;
        let secs = std::time::Duration::from_secs(1);
        for candidates in [
            vec![
                candidate(Hook, "working", secs),
                candidate(Hook, "wait", secs),
                candidate(Hook, "ask", secs),
            ],
            vec![
                candidate(Hook, "ask", secs),
                candidate(Hook, "wait", secs),
                candidate(Hook, "working", secs),
            ],
            vec![
                candidate(Hook, "wait", secs),
                candidate(Hook, "ask", secs),
                candidate(Hook, "working", secs),
            ],
        ] {
            let decision = select(AgentClient::Codex, None, &candidates);

            assert!(decision.applied);
            assert_eq!(
                decision.observation.map(|observation| observation.kind),
                Some(LiveStatusKind::AgentRunning)
            );
        }
    }

    #[test]
    fn newest_malformed_entry_does_not_hide_older_valid_entry() {
        use super::AgentEvidenceSource::Hook;
        let candidates = [
            candidate(Hook, "garbage", std::time::Duration::from_secs(1)),
            candidate(Hook, "wait", std::time::Duration::from_secs(10)),
        ];

        let decision = select(AgentClient::Codex, None, &candidates);

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn all_ineligible_candidates_preserve_prior_credible_state() {
        use super::AgentEvidenceSource::{Hook, RuntimeWrapper};
        for prior_kind in [
            LiveStatusKind::Done,
            LiveStatusKind::WaitingForInput,
            LiveStatusKind::AgentRunning,
        ] {
            let prior = LiveObservation::new(prior_kind, "prior");
            let candidates = [
                candidate(
                    RuntimeWrapper,
                    "working",
                    std::time::Duration::from_secs(31),
                ),
                candidate(Hook, "wait", std::time::Duration::from_secs(121)),
                candidate(Hook, "garbage", std::time::Duration::from_secs(1)),
            ];

            let decision = select(AgentClient::Codex, Some(&prior), &candidates);

            assert!(!decision.applied, "{prior_kind:?}");
            assert!(decision.source.is_none(), "{prior_kind:?}");
            assert_eq!(
                decision.observation.map(|observation| observation.kind),
                Some(prior_kind),
                "{prior_kind:?}"
            );
        }
    }

    #[test]
    fn waiting_on_delegated_status_decision_applies() {
        let now = hook_now();
        let child = crate::agent_status::StatusObservation {
            source: crate::agent_status::ObservationSource::ProviderHook,
            observed_at: now - std::time::Duration::from_secs(1),
            expires_at: now + std::time::Duration::from_secs(120),
            confidence: crate::agent_status::Confidence::High,
            run_id: "child-1".to_string(),
            parent_run_id: Some("primary".to_string()),
            kind: crate::agent_status::ActivityKind::Working,
        };

        let decision = super::select_status_observation(super::StatusDecisionInput {
            selected_agent: AgentClient::Codex,
            prior: None,
            acknowledged_at: None,
            now,
            candidates: &[],
            extra_observations: &[child],
        });

        assert!(decision.applied);
        assert!(decision.source.is_none());
        assert_eq!(
            decision
                .observation
                .as_ref()
                .map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(
            decision
                .observation
                .as_ref()
                .is_some_and(|observation| observation.summary.contains("delegated")),
            "summary={:?}",
            decision
                .observation
                .as_ref()
                .map(|observation| &observation.summary)
        );
    }

    #[test]
    fn generic_pane_busy_alone_projects_unknown() {
        let now = hook_now();
        let pane_obs = super::project_pane_activity(AgentClient::Codex, "codex is working\n", now)
            .expect("busy chrome should project");
        assert_eq!(
            pane_obs.source,
            crate::agent_status::ObservationSource::GenericPane
        );
        assert_eq!(pane_obs.confidence, crate::agent_status::Confidence::Low);

        let projection =
            crate::agent_status::reduce_agent_status(crate::agent_status::ReduceInput {
                now,
                primary_run_id: "primary".to_string(),
                process_liveness: Some(crate::agent_status::ProcessLiveness {
                    alive: true,
                    observed_at: now,
                }),
                observations: &[pane_obs],
            });
        assert_eq!(projection.phase, crate::agent_status::ParentPhase::Unknown);
        assert_eq!(projection.live.kind, LiveStatusKind::Unknown);
    }

    #[test]
    fn structured_cursor_json_pane_can_project_running() {
        let now = hook_now();
        let pane_obs =
            super::project_pane_activity(AgentClient::Codex, r#"{"type":"thinking"}"#, now)
                .expect("cursor json should project");
        assert_eq!(
            pane_obs.source,
            crate::agent_status::ObservationSource::StructuredPane
        );
        assert_eq!(pane_obs.confidence, crate::agent_status::Confidence::Medium);

        let projection =
            crate::agent_status::reduce_agent_status(crate::agent_status::ReduceInput {
                now,
                primary_run_id: "primary".to_string(),
                process_liveness: Some(crate::agent_status::ProcessLiveness {
                    alive: true,
                    observed_at: now,
                }),
                observations: &[pane_obs],
            });
        assert_eq!(
            projection.phase,
            crate::agent_status::ParentPhase::ActivelyWorking
        );
        assert_eq!(projection.live.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn parent_done_hook_with_active_child_pane_run_is_not_fully_complete() {
        let now = hook_now();
        let candidates = [
            super::StatusCandidate::new(
                super::AgentEvidenceSource::Hook,
                "done",
                now - std::time::Duration::from_secs(1),
            ),
            super::StatusCandidate::new(
                super::AgentEvidenceSource::Hook,
                "working",
                now - std::time::Duration::from_secs(1),
            )
            .with_run("pane:ajax-web-fix-login:%1", Some("primary".to_string())),
        ];

        let decision = super::select_status_observation(super::StatusDecisionInput {
            selected_agent: AgentClient::Codex,
            prior: None,
            acknowledged_at: None,
            now,
            candidates: &candidates,
            extra_observations: &[],
        });

        assert!(decision.applied);
        assert_ne!(
            decision
                .observation
                .as_ref()
                .map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );
        assert_eq!(
            decision
                .observation
                .as_ref()
                .map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert!(
            decision
                .observation
                .as_ref()
                .is_some_and(|observation| observation.summary.contains("delegated")),
            "summary={:?}",
            decision
                .observation
                .as_ref()
                .map(|observation| &observation.summary)
        );
    }

    #[test]
    fn lifecycle_working_outranks_hook_wait() {
        use super::AgentEvidenceSource::{Hook, Lifecycle};
        let candidates = [
            candidate(Lifecycle, "working", std::time::Duration::ZERO),
            candidate(Hook, "wait", std::time::Duration::ZERO),
        ];

        let decision = select(AgentClient::Claude, None, &candidates);

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn lifecycle_ignores_selected_agent_other() {
        use super::AgentEvidenceSource::Lifecycle;
        let candidates = [candidate(Lifecycle, "working", std::time::Duration::ZERO)];

        let decision = select(AgentClient::Other, None, &candidates);

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn stale_nonterminal_lifecycle_does_not_assert_activity() {
        use super::AgentEvidenceSource::Lifecycle;
        let prior = LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
        let candidates = [candidate(
            Lifecycle,
            "working",
            std::time::Duration::from_secs(31 * 60),
        )];

        let decision = select(AgentClient::Codex, Some(&prior), &candidates);

        assert!(!decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[test]
    fn terminal_lifecycle_persists_and_wrapper_exit_outranks() {
        use super::AgentEvidenceSource::{Lifecycle, RuntimeWrapper};
        let lifecycle_only = [candidate(
            Lifecycle,
            "done",
            std::time::Duration::from_secs(2 * 3600),
        )];

        let decision = select(AgentClient::Codex, None, &lifecycle_only);

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );

        let with_wrapper = [
            candidate(Lifecycle, "done", std::time::Duration::from_secs(2 * 3600)),
            candidate(RuntimeWrapper, "failed", std::time::Duration::from_secs(1)),
        ];

        let decision = select(AgentClient::Codex, None, &with_wrapper);

        assert!(decision.applied);
        assert_eq!(
            decision.observation.map(|observation| observation.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }
}
