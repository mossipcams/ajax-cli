# Status rework follow-up fixes (#678 defects)

Fix-forward on defects found reviewing the shipped native-hook status rework
(PR #678, `d3f8212`, on `main`, unreleased as of `ajax-cli-v0.55.1`).

**Delegation decision: not delegated** because this session's harness
instructions bar spawning agents/workflows unless the user requests them, and
the user asked directly for the fix. I hold full review context on these code
paths from the preceding review pass.

## Scope

Only defects where the code diverges from its own approved design
(`.planning/agent-plans/native-hook-status-architecture.md`), or destroys state
that design never authorized.

1. `CiPending` masks an unacknowledged operator attention gate and clears
   `SideFlag::NeedsInput`.
2. No staleness on the GitHub override — a merged task projects
   `Running "CI running"` forever.
3. `architecture.md` drift written during #678: process liveness listed as a
   live precedence tier, and equal-timestamp cross-source conflict described as
   reachable.

### Non-goals

- No new `Task` field for GitHub status. The plan's cleaner "GitHub as a
  separate projector input" would need schema + persistence work; out of scope
  for a defect fix.
- No emitter for non-primary `AJAX_RUN_ID`. Task 3b makes the *consumer* honest
  (per-run grouping, aggregation exercised by test), but nothing in production
  writes a delegated run yet — `agent_runtime.rs:120` still hardcodes
  `"primary"`. Wiring subagent runs is a feature, not this fix.

## Deviation from approved design (needs sign-off)

Plan §6 is a first-match table where row 6 (pending CI → Running "CI running")
sits above row 10 (AgentPhase Waiting → Waiting), so displaying CI over a
waiting agent is *approved*. Task 1 narrows that: pending CI no longer overrides
an **unacknowledged `WaitingForApproval` / `WaitingForInput`**.

Rationale: §6 ranks display only and never addresses notification. Because
`attention.rs` clears the notify candidate for any `Running` status, honouring
row 6 literally means an approval gate is both invisible and unnotified for as
long as CI runs. Every other status still yields to CI.

## Tasks

- [x] **1. `CiPending` must not mask an operator attention gate**
  - Test: `github_pending_checks_do_not_mask_unacknowledged_attention_gate`
    (`runtime_refresh.rs`) — `WaitingForApproval` + `NeedsInput` survives
    `CiChecksObservation::Pending`; acknowledged waits and non-gate statuses
    still yield to CI.
  - Impl: `can_apply_github_override` refuses an unacknowledged attention gate;
    drop the `NeedsInput` clear from the `CiPending` arm in
    `live_application.rs` (GitHub CI state does not own the agent's flags).
  - Verify: `cargo nextest run -p ajax-core`
- [x] **2. GitHub CI evidence must not outlive its probe**
  - Test: `github_ci_evidence_is_cleared_when_probing_stops`
    (`runtime_refresh.rs`) — a `Merged` task holding `CiPending` clears it;
    `Unobservable` clears a pending status.
  - Impl: clear GitHub-owned CI evidence in `refresh_github_check_evidence`
    when lifecycle no longer probes; `Unobservable` clears `CiPending`.
  - Verify: `cargo nextest run -p ajax-core`
- [x] **3. Close the architectural drift (code, not prose)**

  First attempt edited `architecture.md` down to match the implementation.
  **Wrong direction** — AGENTS.md makes `architecture.md` the source of truth,
  so the drift is fixed by making the code true and the doc edits were reverted.

  - **3a. Process liveness becomes a real precedence tier 3.**
    - Test: `liveness_expires_after_its_thirty_second_window` (`agent_status.rs`)
      and `live_process_without_native_events_is_idle_not_unknown`
      (`ui_state.rs`).
    - Impl: added `PROCESS_LIVENESS_FRESH_FOR` (30s, the window the doc always
      claimed but nothing enforced — `ProcessLiveness::observed_at` was never
      read) and `ProcessLiveness::is_fresh_at`; `reduce_agent_status` now
      derives `process_alive` from it. Refresh stamps/clears
      `ui_state::AGENT_PROCESS_ALIVE_KEY` before the `Unknown` bail, and
      `has_no_status_evidence` respects it. A provably live process is now
      `Idle`, not `Unknown`, and still never `AgentRunning`.
    - The marker is metadata (like `ci_checks_probed_at`) so no schema
      migration, and refresh owns freshness so `derive_operator_status` stays a
      pure projection with no notion of "now".
  - **3b. Run-graph aggregation actually runs.**
    - Test: `delegated_run_events_do_not_move_the_primary_phase`
      (`agent_status_cache.rs`) — parent `turn_settled` + child `turn_started`
      in one log yields a `Done` primary and a `Working` child, reducing to
      `ParentPhase::CompletedLocallyChildrenActive`.
    - Impl: `ParsedEnvelope` gains `run_id`/`parent_run_id` (`serde(default)`,
      so pre-existing log lines still parse and fold into primary);
      `group_envelopes_by_run` groups the log per run and emits one observation
      per run instead of folding every run into `PRIMARY_RUN_ID`.
  - **3c.** Reverted the doc downgrades; `architecture.md` now documents tier 3
    with its constant and the per-run grouping, both of which are true.

## Validation

| Command | Result |
| --- | --- |
| `cargo fmt --check` | pass (after one `cargo fmt`) |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo nextest run --all-features` | pass — 1692 run, 1692 passed, 0 failed (was 1687; +5 new) |

Both new tests were confirmed failing first, for the intended assertion:
`github_pending_checks_do_not_mask_unacknowledged_attention_gate` at "pending CI
must not overwrite an unacknowledged approval gate", and
`github_ci_evidence_is_cleared_when_probing_stops` at the `Unobservable` clear
(the terminal-lifecycle half already passed — `clear_github_ci_evidence`
already covered `is_github_owned_ci`).

**Live verification incomplete.** Per memory `status-live-verification` the
suite is not proof — it passed with these defects present. Ran
`ajax-cli --profile dev --state <copy of ~/.ajax-dev/ajax.db> tasks`: the binary
renders correctly, and the single task present exercises
`github_probe_is_retired` via `WorktreeMissing` (projects `Error - Worktree
missing`, not a stale "CI running"). But that DB holds **one** task in an error
state, so the CI-pending-vs-attention-gate path was not observed against a real
PR. Worth re-checking against a task with live CI before release.

## Deviations discovered during execution

- Task 1's `can_apply_github_override` gate keys on
  `attention_acknowledged_at` vs `live_status_observed_at`, reusing the same
  comparison `ui_state::live_evidence_is_acknowledged` already uses, rather
  than inventing a second freshness rule.
- Task 2 clears on terminal lifecycle inside `refresh_github_check_evidence`
  rather than at the lifecycle transition, keeping every CI rule in one module.
