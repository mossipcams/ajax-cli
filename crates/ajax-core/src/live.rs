use std::time::{Duration, SystemTime};

#[path = "live_application.rs"]
mod application;
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
        }
    }

    observations.extend_from_slice(input.extra_observations);

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

/// Project pane text onto a single conservative [`crate::agent_status::StatusObservation`].
///
/// Structured recognition (Cursor stream-json, agent-specific prompts) is
/// Medium confidence. Generic busy chrome and heuristic needles are Low
/// confidence and cannot alone assert running/approval through the reducer.
/// Returns `None` when the pane is empty or yields no activity kind.
pub fn project_pane_activity(
    agent: AgentClient,
    pane: &str,
    now: SystemTime,
) -> Option<crate::agent_status::StatusObservation> {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lines = meaningful_lines(trimmed);

    for line in lines.iter().rev() {
        if let Some(observation) = classify_cursor_stream_json_line(line) {
            return pane_status_observation(
                observation,
                crate::agent_status::ObservationSource::StructuredPane,
                crate::agent_status::Confidence::Medium,
                STRUCTURED_PANE_FRESH_FOR,
                now,
            );
        }
    }

    if let Some(observation) = classify_agent_prompt(agent, &lines) {
        return pane_status_observation(
            observation,
            crate::agent_status::ObservationSource::StructuredPane,
            crate::agent_status::Confidence::Medium,
            STRUCTURED_PANE_FRESH_FOR,
            now,
        );
    }

    if has_recent_busy_indicator(agent, &lines) {
        return pane_status_observation(
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
            crate::agent_status::ObservationSource::GenericPane,
            crate::agent_status::Confidence::Low,
            GENERIC_PANE_FRESH_FOR,
            now,
        );
    }

    if let Some(observation) = classify_recent_evidence(&lines) {
        if observation.kind == LiveStatusKind::Unknown
            || observation.kind == LiveStatusKind::ShellIdle
        {
            return None;
        }
        // Current-line approval/input prompts are structured UI recognition.
        // Historical scrollback of the same wording stays low-confidence generic.
        let on_current_line = lines.last().is_some_and(|line| {
            classify_line_evidence(line)
                .as_ref()
                .is_some_and(|current| current.kind == observation.kind)
        });
        let (source, confidence, fresh_for) = match (observation.kind, on_current_line) {
            (LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput, true) => (
                crate::agent_status::ObservationSource::StructuredPane,
                crate::agent_status::Confidence::Medium,
                STRUCTURED_PANE_FRESH_FOR,
            ),
            _ => (
                crate::agent_status::ObservationSource::GenericPane,
                crate::agent_status::Confidence::Low,
                GENERIC_PANE_FRESH_FOR,
            ),
        };
        return pane_status_observation(observation, source, confidence, fresh_for, now);
    }

    None
}

/// Project pane text onto an actionable *stuck* status — a state that blocks
/// the agent but is not agent activity, so [`project_pane_activity`] cannot
/// carry it (there is deliberately no [`crate::agent_status::ActivityKind`] for
/// these).
///
/// Membership is derived, never hand-listed: a kind qualifies when
/// [`live_kind_to_activity`] cannot express it *and* its
/// [`crate::models::LiveStatusClass`] is `Waiting` or `Error`. That yields
/// exactly `AuthRequired`, `RateLimited`, `ContextLimit`, `Blocked`,
/// `MergeConflict`, and `CiFailed`, and it cannot drift from the activity
/// mapping the way a duplicated list would.
///
/// Returns `None` for activity, completion, missing-substrate, and neutral
/// panes, so this can never re-assert `AgentRunning` from pane text alone.
pub fn project_pane_stuck_status(agent: AgentClient, pane: &str) -> Option<LiveObservation> {
    // Only the pane tail counts as live evidence. A stuck line buried under
    // fresh output describes a condition the agent has already moved past, and
    // acting on it fires an actionable notification about nothing. Reuses
    // `BUSY_WINDOW` so "recent enough to be live" means one thing across every
    // pane projection here.
    let lines = meaningful_lines(pane.trim());
    let tail = lines[lines.len().saturating_sub(BUSY_WINDOW)..].join("\n");

    let observation = classify_agent_pane(agent, &tail);
    if live_kind_to_activity(observation.kind).is_some() {
        return None;
    }
    match observation.kind.class() {
        crate::models::LiveStatusClass::Waiting | crate::models::LiveStatusClass::Error => {
            Some(observation)
        }
        _ => None,
    }
}

fn pane_status_observation(
    observation: LiveObservation,
    source: crate::agent_status::ObservationSource,
    confidence: crate::agent_status::Confidence,
    fresh_for: Duration,
    now: SystemTime,
) -> Option<crate::agent_status::StatusObservation> {
    let kind = live_kind_to_activity(observation.kind)?;
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

pub fn classify_pane(pane: &str) -> LiveObservation {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return LiveObservation::new(LiveStatusKind::Unknown, "pane is empty");
    }

    let lines = meaningful_lines(trimmed);
    if looks_like_idle_codex_prompt(&lines) {
        return LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input");
    }

    classify_recent_evidence(&lines)
        .unwrap_or_else(|| LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state"))
}

/// Agent-aware pane classification.
///
/// Recent busy indicators win over stale prompts, then agent-specific prompts,
/// then explicit failure/completion evidence, and finally a passive/unknown
/// fallback so the reducer can preserve prior credible state. `classify_pane`
/// remains the generic compatibility entry point.
pub fn classify_agent_pane(agent: AgentClient, pane: &str) -> LiveObservation {
    let trimmed = pane.trim();
    if trimmed.is_empty() {
        return LiveObservation::new(LiveStatusKind::Unknown, "pane is empty");
    }

    let lines = meaningful_lines(trimmed);

    if has_recent_busy_indicator(agent, &lines) {
        return LiveObservation::new(LiveStatusKind::AgentRunning, "agent running");
    }

    if let Some(observation) = classify_agent_prompt(agent, &lines) {
        return observation;
    }

    classify_recent_evidence(&lines)
        .unwrap_or_else(|| LiveObservation::new(LiveStatusKind::Unknown, "unknown terminal state"))
}

const BUSY_WINDOW: usize = 8;

fn has_recent_busy_indicator(_agent: AgentClient, lines: &[&str]) -> bool {
    lines
        .iter()
        .rev()
        .take(BUSY_WINDOW)
        .any(|line| looks_like_active_agent_status(line) || looks_like_claude_busy(line))
}

fn looks_like_claude_busy(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("to interrupt") || (line.contains('…') && lower.contains("tokens)"))
}

fn classify_agent_prompt(agent: AgentClient, lines: &[&str]) -> Option<LiveObservation> {
    match agent {
        AgentClient::Claude => classify_claude_prompt(lines),
        AgentClient::Codex => classify_codex_prompt(lines),
        AgentClient::Other => None,
    }
    .or_else(|| match agent {
        AgentClient::Claude => classify_codex_prompt(lines),
        AgentClient::Codex => classify_claude_prompt(lines),
        AgentClient::Other => {
            classify_claude_prompt(lines).or_else(|| classify_codex_prompt(lines))
        }
    })
}

fn classify_claude_prompt(lines: &[&str]) -> Option<LiveObservation> {
    if looks_like_claude_permission(lines) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));
    }

    if looks_like_claude_idle_prompt(lines) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }

    None
}

fn looks_like_claude_idle_prompt(lines: &[&str]) -> bool {
    if lines
        .last()
        .is_some_and(|line| line.trim() == "❯" || line.trim() == ">")
    {
        return true;
    }

    const IDLE_WINDOW: usize = 8;
    let recent = &lines[lines.len().saturating_sub(IDLE_WINDOW)..];
    let has_bare_prompt = recent.iter().any(|line| matches!(line.trim(), "❯" | ">"));
    let has_strong_chrome = recent.iter().any(|line| is_strong_claude_chrome_line(line));

    has_bare_prompt && has_strong_chrome
}

fn is_strong_claude_chrome_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("bypass permissions") || lower.contains("shift+tab") {
        return true;
    }

    let trimmed = line.trim();
    trimmed.contains('│')
        && (trimmed.contains('%') || trimmed.contains('█') || trimmed.contains('░'))
}

fn looks_like_claude_permission(lines: &[&str]) -> bool {
    let recent: Vec<&str> = lines.iter().rev().take(BUSY_WINDOW).copied().collect();
    let has_choice_marker = recent.iter().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('❯') {
            return false;
        }
        !trimmed.trim_start_matches('❯').trim().is_empty()
    });
    let has_cue = recent.iter().any(|line| {
        let lower = line.to_ascii_lowercase();
        if lower.contains("shift+tab") || lower.contains("bypass permissions") {
            return false;
        }
        contains_any(
            &lower,
            &[
                "run this command?",
                "do you want",
                "allow",
                "approve",
                "permission",
                "proceed?",
                "esc to cancel",
            ],
        )
    });
    has_choice_marker && has_cue
}

fn classify_codex_prompt(lines: &[&str]) -> Option<LiveObservation> {
    if looks_like_idle_codex_prompt(lines) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaneEvidence {
    Completion,
    ApprovalPrompt,
    InputPrompt,
    AuthRequired,
    RateLimited,
    ContextLimit,
    Blocked,
    MergeConflict,
    CiFailed,
    CommandFailed,
    CommandRunning,
    AgentRunning,
    TestsRunning,
}

impl PaneEvidence {
    fn observation(self) -> LiveObservation {
        match self {
            Self::Completion => LiveObservation::new(LiveStatusKind::Done, "done"),
            Self::ApprovalPrompt => {
                LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval")
            }
            Self::InputPrompt => {
                LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input")
            }
            Self::AuthRequired => {
                LiveObservation::new(LiveStatusKind::AuthRequired, "authentication required")
            }
            Self::RateLimited => LiveObservation::new(LiveStatusKind::RateLimited, "rate limited"),
            Self::ContextLimit => {
                LiveObservation::new(LiveStatusKind::ContextLimit, "context limit reached")
            }
            Self::Blocked => LiveObservation::new(LiveStatusKind::Blocked, "blocked"),
            Self::MergeConflict => LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "merge conflict needs attention",
            ),
            Self::CiFailed => LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
            Self::CommandFailed => {
                LiveObservation::new(LiveStatusKind::CommandFailed, "command failed")
            }
            Self::CommandRunning => {
                LiveObservation::new(LiveStatusKind::CommandRunning, "command running")
            }
            Self::AgentRunning => {
                LiveObservation::new(LiveStatusKind::AgentRunning, "agent running")
            }
            Self::TestsRunning => {
                LiveObservation::new(LiveStatusKind::TestsRunning, "tests running")
            }
        }
    }
}

fn classify_recent_evidence(lines: &[&str]) -> Option<LiveObservation> {
    if lines
        .last()
        .is_some_and(|line| looks_like_shell_prompt(line))
    {
        if let Some(line) = lines.iter().rev().nth(1).copied() {
            if let Some(observation) = classify_line_evidence(line) {
                return Some(observation);
            }
        }

        return Some(LiveObservation::new(
            LiveStatusKind::ShellIdle,
            "shell idle",
        ));
    }

    lines.iter().rev().copied().find_map(classify_line_evidence)
}

fn classify_line_evidence(line: &str) -> Option<LiveObservation> {
    classify_cursor_stream_json_line(line)
        .or_else(|| pane_evidence(line).map(PaneEvidence::observation))
}

fn pane_evidence(line: &str) -> Option<PaneEvidence> {
    let lower = line.to_ascii_lowercase();

    if is_completion_line(&lower) {
        return Some(PaneEvidence::Completion);
    }

    if contains_any(
        &lower,
        &[
            "do you want to proceed",
            "approve to proceed",
            "allow command",
            "approval request",
            "y/n",
            "[y/n]",
        ],
    ) {
        return Some(PaneEvidence::ApprovalPrompt);
    }

    if contains_any(
        &lower,
        &[
            "please login",
            "please log in",
            "log in to",
            "login to continue",
            "authenticate",
            "auth required",
        ],
    ) {
        return Some(PaneEvidence::AuthRequired);
    }

    if contains_any(
        &lower,
        &["rate limit", "too many requests", "try again later"],
    ) {
        return Some(PaneEvidence::RateLimited);
    }

    if contains_any(&lower, &["context limit", "token limit", "context length"]) {
        return Some(PaneEvidence::ContextLimit);
    }

    if contains_any(
        &lower,
        &["blocked", "cannot continue", "manual intervention required"],
    ) {
        return Some(PaneEvidence::Blocked);
    }

    if contains_any(
        &lower,
        &[
            "merge conflict",
            "conflict (",
            "automatic merge failed",
            "fix conflicts",
        ],
    ) {
        return Some(PaneEvidence::MergeConflict);
    }

    if contains_any(
        &lower,
        &[
            "ci failed",
            "github actions failed",
            "check run failed",
            "workflow failed",
            "failing checks",
        ],
    ) {
        return Some(PaneEvidence::CiFailed);
    }

    if contains_any(
        &lower,
        &[
            "waiting for input",
            "what kind of ",
            "what do you want me to",
            "what you want me to do",
            "send me the problem",
            "did you mean",
            "specific task",
            "press enter",
            "continue?",
            "enter your choice",
            "select an option",
        ],
    ) {
        return Some(PaneEvidence::InputPrompt);
    }

    if contains_any(
        &lower,
        &[
            "test result: failed",
            "command failed",
            "exit code",
            "nonzeroexit",
            "failed with",
        ],
    ) {
        return Some(PaneEvidence::CommandFailed);
    }

    if contains_any(
        &lower,
        &["running command", "executing command", "$ cargo", "$ npm"],
    ) {
        return Some(PaneEvidence::CommandRunning);
    }

    if looks_like_active_agent_status(line) {
        return Some(PaneEvidence::AgentRunning);
    }

    if contains_any(
        &lower,
        &[
            "cargo test",
            "cargo nextest",
            "npm test",
            "pnpm test",
            "yarn test",
            "pytest",
            "rspec",
            "running test",
            "running 0 tests",
            "running ",
        ],
    ) {
        return Some(PaneEvidence::TestsRunning);
    }

    None
}

fn meaningful_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn looks_like_idle_codex_prompt(lines: &[&str]) -> bool {
    let recent_lines: Vec<_> = lines.iter().rev().take(4).copied().collect();

    recent_lines.iter().any(|line| line.starts_with('›'))
        && lines
            .last()
            .is_some_and(|line| line.starts_with("gpt-") && line.contains("~/"))
        && !recent_lines
            .iter()
            .any(|line| looks_like_active_agent_status(line))
}

fn looks_like_active_agent_status(line: &str) -> bool {
    contains_any(
        &line.to_ascii_lowercase(),
        &[
            "codex is working",
            "claude is working",
            "cursor agent",
            "cursor is working",
            "background terminal running",
            "thinking",
            "waiting for background terminal",
            "working (",
            "working on your task",
            "running tool",
            "using tool",
        ],
    )
}

fn classify_cursor_stream_json_line(line: &str) -> Option<LiveObservation> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let value = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    let event_type = value.get("type")?.as_str()?.to_ascii_lowercase();

    match event_type.as_str() {
        "system" if value.get("subtype").and_then(serde_json::Value::as_str) == Some("init") => {
            Some(LiveObservation::new(
                LiveStatusKind::AgentRunning,
                "agent running",
            ))
        }
        "thinking" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "tool_call" => classify_cursor_tool_call_line(&value),
        "assistant" => cursor_assistant_observation(&value),
        "result" => classify_cursor_result_line(&value),
        "request" => classify_cursor_request_line(&value),
        "status" => classify_cursor_status_line(&value),
        _ => None,
    }
}

fn classify_cursor_tool_call_line(value: &serde_json::Value) -> Option<LiveObservation> {
    if let Some(status) = value.get("status").and_then(serde_json::Value::as_str) {
        return match status {
            "running" | "in_progress" => Some(cursor_tool_observation(&cursor_tool_name(value))),
            "error" | "failed" => Some(LiveObservation::new(
                LiveStatusKind::CommandFailed,
                "command failed",
            )),
            _ => None,
        };
    }

    match value.get("subtype").and_then(serde_json::Value::as_str) {
        Some("started") => value
            .get("tool_call")
            .map(|tool_call| cursor_tool_observation(&cursor_nested_tool_name(tool_call))),
        Some("completed") => None,
        _ => None,
    }
}

fn cursor_tool_observation(label: &str) -> LiveObservation {
    let fallback = LiveObservation::new(
        LiveStatusKind::CommandRunning,
        format!("tool running: {label}"),
    );
    let observation = classify_pane(label);
    if observation.kind == LiveStatusKind::Unknown {
        fallback
    } else {
        observation
    }
}

fn classify_cursor_result_line(value: &serde_json::Value) -> Option<LiveObservation> {
    if value.get("is_error").and_then(serde_json::Value::as_bool) == Some(true)
        || value.get("subtype").and_then(serde_json::Value::as_str) == Some("error")
    {
        return Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "command failed",
        ));
    }

    if value.get("subtype").and_then(serde_json::Value::as_str) == Some("success")
        || value.get("is_error").and_then(serde_json::Value::as_bool) == Some(false)
    {
        return Some(LiveObservation::new(LiveStatusKind::Done, "done"));
    }

    None
}

fn classify_cursor_request_line(value: &serde_json::Value) -> Option<LiveObservation> {
    let prompt = value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("prompt").and_then(serde_json::Value::as_str))
        .or_else(|| value.get("text").and_then(serde_json::Value::as_str))
        .unwrap_or("waiting for operator input");

    if cursor_mentions_approval(prompt) {
        Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ))
    } else {
        Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ))
    }
}

fn classify_cursor_status_line(value: &serde_json::Value) -> Option<LiveObservation> {
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();

    match status.as_str() {
        "RUNNING" | "CREATING" => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        "FINISHED" => Some(LiveObservation::new(LiveStatusKind::Done, "done")),
        "ERROR" | "CANCELLED" | "EXPIRED" => Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "command failed",
        )),
        _ => None,
    }
}

fn cursor_assistant_observation(value: &serde_json::Value) -> Option<LiveObservation> {
    let text = cursor_assistant_text(value)?;
    if text.trim_end().ends_with('?') && !cursor_mentions_approval(&text) {
        return Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }

    let observation = classify_pane(&text);
    if observation.kind == LiveStatusKind::Unknown {
        Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        ))
    } else {
        Some(observation)
    }
}

fn cursor_assistant_text(value: &serde_json::Value) -> Option<String> {
    let content = value.get("message")?.get("content")?.as_array()?;
    let text = content
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(serde_json::Value::as_str) != Some("text") {
                return None;
            }
            block.get("text").and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("");

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn cursor_tool_name(value: &serde_json::Value) -> String {
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tool");

    if let Some(command) = value
        .get("args")
        .and_then(|args| args.get("command").or_else(|| args.get("cmd")))
        .and_then(serde_json::Value::as_str)
    {
        return format!("{name}: {command}");
    }

    if let Some(path) = value
        .get("args")
        .and_then(|args| args.get("path"))
        .and_then(serde_json::Value::as_str)
    {
        return format!("{name} {path}");
    }

    name.to_string()
}

fn cursor_nested_tool_name(tool_call: &serde_json::Value) -> String {
    if let Some(read) = tool_call.get("readToolCall") {
        if let Some(path) = read
            .get("args")
            .and_then(|args| args.get("path"))
            .and_then(serde_json::Value::as_str)
        {
            return format!("read {path}");
        }
        return "read".to_string();
    }

    if let Some(shell) = tool_call
        .get("bashToolCall")
        .or_else(|| tool_call.get("shellToolCall"))
    {
        if let Some(command) = shell
            .get("args")
            .and_then(|args| args.get("command").or_else(|| args.get("cmd")))
            .and_then(serde_json::Value::as_str)
        {
            return format!("shell: {command}");
        }
        return "shell".to_string();
    }

    tool_call
        .as_object()
        .and_then(|fields| fields.keys().next())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "tool".to_string())
}

fn cursor_mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "approval required",
            "requires approval",
            "waiting for approval",
            "allow command",
            "proceed?",
            "do you want to proceed",
            "approve to proceed",
            "y/n",
            "[y/n]",
        ],
    )
}

fn is_completion_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "test result: ok",
            "tests passed",
            "all pre-pr checks passed",
            "successfully completed",
            "task complete",
            "all done",
        ],
    ) || lower.trim_matches(|character: char| !character.is_ascii_alphanumeric()) == "done"
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

pub fn reduce_agent_status_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Option<LiveObservation> {
    values
        .into_iter()
        .filter_map(|value| {
            classify_agent_status_value(value).map(|observation| {
                let priority = agent_status_priority(observation.kind);
                (priority, observation)
            })
        })
        .max_by_key(|(priority, _observation)| *priority)
        .map(|(_priority, observation)| observation)
}

fn agent_status_priority(kind: LiveStatusKind) -> u8 {
    match kind {
        LiveStatusKind::AgentRunning => 5,
        LiveStatusKind::WaitingForInput => 4,
        LiveStatusKind::WaitingForApproval => 3,
        LiveStatusKind::Done => 2,
        _ => 0,
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

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_shell_prompt(text: &str) -> bool {
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            let line = line.trim_end();
            line.ends_with('%') || line.ends_with('$') || line.ends_with('#')
        })
}

#[cfg(test)]
mod tests {
    use crate::models::{
        AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
    };

    use super::{
        apply_observation, apply_observation_at, classify_agent_status_value, classify_pane,
        project_pane_stuck_status, reduce_agent_status_values,
    };

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

    /// Regression: the conservative status redesign routed pane capture through
    /// `project_pane_activity`, which drops every kind `live_kind_to_activity`
    /// cannot express. That silently removed six actionable, notification-firing
    /// stuck states from the tmux polling path.
    #[test]
    fn pane_stuck_states_survive_the_activity_projection() {
        for (pane, expected) in [
            (
                "starting work\nthis is blocked, cannot continue\n",
                LiveStatusKind::Blocked,
            ),
            (
                "calling api\nrate limit exceeded\n",
                LiveStatusKind::RateLimited,
            ),
            (
                "running\nplease login to continue\n",
                LiveStatusKind::AuthRequired,
            ),
            (
                "working\ncontext limit reached\n",
                LiveStatusKind::ContextLimit,
            ),
            (
                "git merge\nCONFLICT (content): merge conflict in a.rs\n",
                LiveStatusKind::MergeConflict,
            ),
            ("pushed\nci failed on main\n", LiveStatusKind::CiFailed),
        ] {
            let observed = project_pane_stuck_status(AgentClient::Claude, pane);
            assert_eq!(
                observed.map(|observation| observation.kind),
                Some(expected),
                "pane {pane:?} should project the {expected:?} stuck state"
            );
        }
    }

    /// The redesign's thesis is that historical pane text is not live evidence.
    /// A stuck line buried in scrollback, with the agent visibly producing
    /// output since, must not project a stuck state — otherwise it fires an
    /// actionable notification about a condition that already resolved.
    #[test]
    fn stale_scrollback_stuck_lines_are_not_live_evidence() {
        let buried = format!(
            "this is blocked, cannot continue\n{}",
            "ordinary working output\n".repeat(60)
        );
        assert_eq!(
            project_pane_stuck_status(AgentClient::Claude, &buried),
            None,
            "a stuck line buried under fresh output is not live evidence"
        );

        // Still recent enough to be live: the same line near the pane tail.
        let recent = "ordinary working output\nthis is blocked, cannot continue\n";
        assert_eq!(
            project_pane_stuck_status(AgentClient::Claude, recent).map(|o| o.kind),
            Some(LiveStatusKind::Blocked),
            "a stuck line at the pane tail is live evidence"
        );
    }

    /// The stuck-state fallback must never reopen the false-positive hole the
    /// redesign closed: activity and terminal kinds stay the reducer's job.
    #[test]
    fn pane_stuck_states_never_project_activity_or_completion() {
        for pane in [
            "esc to interrupt",
            "Running tests…  (1234 tokens)",
            "some ordinary output\n$ ",
            "",
            "Do you want to proceed? [y/n]",
            "all done, task complete",
        ] {
            assert_eq!(
                project_pane_stuck_status(AgentClient::Claude, pane),
                None,
                "pane {pane:?} must not yield a stuck state"
            );
        }
    }

    #[test]
    fn pane_classifier_detects_agent_attention_states() {
        for (pane, expected) in [
            (
                "Do you want to proceed? y/n",
                LiveStatusKind::WaitingForApproval,
            ),
            (
                "Waiting for input. Press Enter to continue.",
                LiveStatusKind::WaitingForInput,
            ),
            ("Please login to continue", LiveStatusKind::AuthRequired),
            (
                "rate limit exceeded; try again later",
                LiveStatusKind::RateLimited,
            ),
            ("context limit reached", LiveStatusKind::ContextLimit),
            (
                "CONFLICT (content): merge conflict in src/lib.rs",
                LiveStatusKind::MergeConflict,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_codex_clarification_prompts_as_waiting_for_input() {
        for pane in [
            "\
› Math

⚠ Heads up, you have less than 25% of your weekly limit left.

• What kind of math do you want to work on? Send me the problem, equation, or
  topic.

› Use /skills to list available skills",
            "\
› trst

⚠ Heads up, you have less than 25% of your weekly limit left.

• I’m not sure what you want me to do with “trst”. Did you mean “test”, or is
  there a specific task in this repo you want me to handle?

› Use /skills to list available skills",
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, LiveStatusKind::WaitingForInput, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_conflict_and_ci_failure_evidence() {
        for (pane, expected) in [
            (
                "Automatic merge failed; fix conflicts and then commit the result.",
                LiveStatusKind::MergeConflict,
            ),
            (
                "CONFLICT (modify/delete): src/lib.rs deleted in HEAD and modified in feature",
                LiveStatusKind::MergeConflict,
            ),
            ("CI failed for this branch", LiveStatusKind::CiFailed),
            (
                "GitHub Actions failed: test.yml / build",
                LiveStatusKind::CiFailed,
            ),
            ("check run failed: cargo test", LiveStatusKind::CiFailed),
            ("workflow failed after 3m", LiveStatusKind::CiFailed),
            (
                "There are failing checks on the PR",
                LiveStatusKind::CiFailed,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_cursor_stream_json_and_activity() {
        for (pane, expected) in [
            (
                r#"{"type":"system","subtype":"init","agent":"cursor"}"#,
                LiveStatusKind::AgentRunning,
            ),
            (r#"{"type":"thinking"}"#, LiveStatusKind::AgentRunning),
            (
                r#"{"type":"tool_call","call_id":"1","name":"grep","status":"running"}"#,
                LiveStatusKind::CommandRunning,
            ),
            (
                r#"{"type":"tool_call","subtype":"started","call_id":"1","tool_call":{"readToolCall":{"args":{"path":"src/lib.rs"}}}}"#,
                LiveStatusKind::CommandRunning,
            ),
            (
                r#"{"type":"status","status":"RUNNING"}"#,
                LiveStatusKind::AgentRunning,
            ),
            (
                r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#,
                LiveStatusKind::Done,
            ),
            (
                r#"{"type":"result","subtype":"error","is_error":true,"result":"auth failed"}"#,
                LiveStatusKind::CommandFailed,
            ),
            (
                r#"{"type":"request","request_id":"req-1","message":"Allow command?"}"#,
                LiveStatusKind::WaitingForApproval,
            ),
            (
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Which branch should I use?"}]}}"#,
                LiveStatusKind::WaitingForInput,
            ),
            (
                "cursor agent --print --output-format stream-json fix tests",
                LiveStatusKind::AgentRunning,
            ),
            (
                concat!(
                    r#"{"type":"thinking"}"#,
                    "\n",
                    r#"{"type":"tool_call","call_id":"1","name":"grep","status":"running"}"#,
                ),
                LiveStatusKind::CommandRunning,
            ),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_detects_runtime_states() {
        for (pane, expected) in [
            (
                "cargo test --all-features\nrunning 37 tests",
                LiveStatusKind::TestsRunning,
            ),
            ("running command: npm test", LiveStatusKind::CommandRunning),
            ("test result: ok. 37 passed", LiveStatusKind::Done),
            (
                "codex is working on your task",
                LiveStatusKind::AgentRunning,
            ),
            (
                "Command failed with exit code 101",
                LiveStatusKind::CommandFailed,
            ),
            ("✓ Successfully completed task", LiveStatusKind::Done),
            ("matt@host project % ", LiveStatusKind::ShellIdle),
            ("", LiveStatusKind::Unknown),
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, expected, "{pane}");
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
    fn agent_status_values_reduce_by_tmux_agent_status_priority() {
        let observation = reduce_agent_status_values(["done", "wait", "working", "ask", "parked"]);

        assert_eq!(
            observation.map(|observation| observation.kind),
            Some(LiveStatusKind::AgentRunning)
        );

        let observation = reduce_agent_status_values(["parked", "done", "ask"]);

        assert_eq!(
            observation.map(|observation| observation.kind),
            Some(LiveStatusKind::WaitingForApproval)
        );
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_running_history() {
        let pane = "\
The targeted checks pass. I’m continuing the cherry-pick now.
The rebased commit is created. I’m running the full pre-PR parity script now.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_later_success_over_stale_failure_history() {
        let pane = "\
Earlier command failed with exit code 101.
I fixed the issue and reran the full suite.
All pre-PR checks passed.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::Done);
    }

    #[test]
    fn pane_classifier_uses_final_prompt_over_stale_approval_history() {
        let pane = "\
Do you want to proceed? y/n
Approved and continued.
No more work is running.
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
    }

    #[test]
    fn pane_classifier_treats_plan_approval_prompt_as_waiting_for_approval() {
        let pane = "\
Task 1: Badge accessibility + duplication cleanup

- Test to write: add failing Vitest coverage.
- Code to implement: extract a small internal badge-rendering helper.
- Verify: run rtk npm test -- badges.test.ts.

Plan ready. Approve to proceed.";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn pane_classifier_treats_idle_codex_prompt_as_waiting_for_input() {
        let pane = "\
› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn pane_classifier_treats_codex_working_prompt_as_agent_running() {
        let pane = "\
› Improve documentation in @filename

• codex is working

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn pane_classifier_treats_codex_working_status_prompt_as_agent_running() {
        let pane = "\
• Working (3m 00s • esc to interrupt) · 1 background terminal running · /ps to…

› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn pane_classifier_treats_codex_background_terminal_status_as_agent_running() {
        for pane in [
            "\
1 background terminal running · /ps to view · /stop to close

› Write tests for @filename

  gpt-5.5 high fast · ~/Desktop/Projects/autodoctor__worktrees/ajax-false-positive",
            "\
• Waiting for background terminal (20m 21s • esc to interrupt) · 1 background …

› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci",
        ] {
            let observation = classify_pane(pane);

            assert_eq!(observation.kind, LiveStatusKind::AgentRunning, "{pane}");
        }
    }

    #[test]
    fn pane_classifier_does_not_treat_negative_done_phrasing_as_complete() {
        let pane = "The task is not done yet; running cargo test now";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::TestsRunning);
    }

    #[test]
    fn pane_classifier_uses_current_failure_over_stale_success_history() {
        let pane = "\
All pre-PR checks passed.
Later validation found a regression.
Command failed with exit code 101
matt@Matts-MacBook-Pro ajax-tech-debt %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::CommandFailed);
    }

    #[test]
    fn pane_classifier_does_not_treat_login_task_text_as_auth_required() {
        let pane = "\
Task: Fix login form alignment
Review the button spacing.
matt@Matts-MacBook-Pro ajax-fix-login %";

        let observation = classify_pane(pane);

        assert_eq!(observation.kind, LiveStatusKind::ShellIdle);
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

        let classified = classify_pane("Do you want to proceed? y/n\n");
        let reduced = super::reduce_live_observation(
            task.live_status.as_ref(),
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        );

        assert_eq!(classified.kind, LiveStatusKind::WaitingForApproval);
        assert_eq!(reduced.kind, LiveStatusKind::AgentRunning);
        assert_eq!(task.lifecycle_status, lifecycle_before);
        assert_eq!(task.git_status, git_before);
        assert_eq!(task.tmux_status, tmux_before);
        assert_eq!(task.task_window_status, task_before);
    }

    use super::classify_agent_pane;

    const CLAUDE_TRAILING_STATUS_BAR: &str = "\
──────────────────────────────────
❯
──────────────────────────────────
Fable 5 │ ajax-statuses ██░░░░░░░░ 23%
⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents";

    #[test]
    fn claude_idle_pane_with_footer_advisories_classifies_waiting_for_input() {
        let pane = "\
Earlier we discussed the merge conflict in the PR comments.

The user said they were done reviewing the implementation.

pending, so merge the PR whenever you're ready.
─────────────────────────────────────────────────────────────────
❯
─────────────────────────────────────────────────────────────────
Fable 5 │ Python Dead Code & Constants (8/18) │ ajax-ux ███░…
⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents
                         ~265k uncached · /clear to start fresh
                         new task? /clear to save 266.7k tokens";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
        assert_ne!(observation.kind, LiveStatusKind::MergeConflict);
        assert_ne!(observation.kind, LiveStatusKind::Done);
        assert_ne!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn bare_prompt_without_claude_chrome_does_not_classify_waiting_for_input() {
        let pane = "\
$ cargo test -p ajax-core
running 1 test
test live::tests::example ... ok

❯
total 8
drwxr-xr-x  3 matt  staff   96 Jul  7 06:00 .";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_ne!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn claude_idle_pane_with_trailing_status_bar_classifies_waiting_for_input() {
        let pane = format!(
            "We should resolve the merge conflict before shipping.\n\
             The user said they were done reviewing the plan.\n\
             {CLAUDE_TRAILING_STATUS_BAR}"
        );

        let observation = classify_agent_pane(AgentClient::Claude, &pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn claude_permission_menu_with_trailing_status_bar_classifies_waiting_for_approval() {
        let pane = format!(
            "Do you want to proceed?\n\
             ❯ 1. Yes\n\
               2. No\n\
             {CLAUDE_TRAILING_STATUS_BAR}"
        );

        let observation = classify_agent_pane(AgentClient::Claude, &pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn claude_busy_pane_with_trailing_status_bar_classifies_agent_running() {
        let pane = format!(
            "✢ Flummoxing… (6m 14s · ↓ 14.3k tokens)\n\
             {CLAUDE_TRAILING_STATUS_BAR}"
        );

        let observation = classify_agent_pane(AgentClient::Claude, &pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn claude_busy_indicator_beats_stale_permission_prompt() {
        let pane = "\
Run this command?
❯ Yes
  No
ctrl+c to interrupt";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn claude_spinner_beats_stale_idle_prompt() {
        let pane = "\
❯

✢ Clauding… (53s · ↓ 749 tokens)";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn claude_permission_dialog_is_waiting_for_approval() {
        let pane = "\
Run this command?
❯ Yes
  No
Esc to cancel";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForApproval);
    }

    #[test]
    fn claude_standalone_prompt_is_waiting_for_input() {
        let pane = "\
Here is my plan.

❯";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn codex_working_status_beats_visible_composer_prompt() {
        let pane = "\
› Fix the tests

• Working (12s · esc to interrupt)

  gpt-5.5 high · ~/Desktop/Projects/ajax";

        let observation = classify_agent_pane(AgentClient::Codex, pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn codex_idle_composer_is_waiting_for_input() {
        let pane = "\
› Improve documentation in @filename

  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti";

        let observation = classify_agent_pane(AgentClient::Codex, pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn agent_specific_prompt_markers_cross_classify_when_registered_agent_differs() {
        let claude_prompt = "\
Here is my plan.

❯";
        let codex_composer = "\
› Fix the tests

  gpt-5.5 high · ~/Desktop/Projects/ajax";

        let claude_as_codex = classify_agent_pane(AgentClient::Codex, claude_prompt);
        let codex_as_claude = classify_agent_pane(AgentClient::Claude, codex_composer);

        assert_eq!(claude_as_codex.kind, LiveStatusKind::WaitingForInput);
        assert_eq!(codex_as_claude.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn codex_selected_task_with_claude_busy_pane_classifies_agent_running() {
        let pane = "\
✶ Scurrying… (3m 9s · ↓ 7.8k tokens)";

        let observation = classify_agent_pane(AgentClient::Codex, pane);

        assert_eq!(observation.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn codex_selected_task_with_claude_idle_prompt_classifies_waiting_for_input() {
        let pane = "\
We should resolve the merge conflict before shipping.

The user said they were done reviewing the plan.

❯";

        let observation = classify_agent_pane(AgentClient::Codex, pane);

        assert_eq!(observation.kind, LiveStatusKind::WaitingForInput);
    }

    #[test]
    fn other_agent_pane_uses_claude_and_codex_prompt_shapes() {
        let claude_busy = "\
✢ Clauding… (53s · ↓ 749 tokens)";
        let claude_prompt = "\
Here is my plan.

❯";
        let codex_composer = "\
› Fix the tests

  gpt-5.5 high · ~/Desktop/Projects/ajax";

        assert_eq!(
            classify_agent_pane(AgentClient::Other, claude_busy).kind,
            LiveStatusKind::AgentRunning
        );
        assert_eq!(
            classify_agent_pane(AgentClient::Other, claude_prompt).kind,
            LiveStatusKind::WaitingForInput
        );
        assert_eq!(
            classify_agent_pane(AgentClient::Other, codex_composer).kind,
            LiveStatusKind::WaitingForInput
        );
    }

    #[test]
    fn ambiguous_redraw_returns_unknown_for_reducer_fallback() {
        let pane = "\
┌──────────────┐
│ Output panel │
├──────────────┤
The quick brown fox jumps over the lazy dog.
Nothing actionable to report here.";

        let observation = classify_agent_pane(AgentClient::Claude, pane);

        assert_eq!(observation.kind, LiveStatusKind::Unknown);
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
}
