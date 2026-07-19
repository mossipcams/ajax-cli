# TDD Implementation Packet — conservative agent status reducer

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Introduce a conservative internal agent-status observation/run model in
`ajax-core` that (1) separates `process_alive` from agent activity, (2) tracks
parent/child runs, (3) expires/stales observations with source+confidence, and
(4) derives existing `LiveStatusKind` without treating process liveness alone
as `AgentRunning`. Wire `live::select_status_observation` and wrapper candidate
eligibility through this reducer so idle live processes and stale pane/hook
evidence stop producing confident false positives.

## Allowed files

- `crates/ajax-core/src/agent_status.rs` (new)
- `crates/ajax-core/src/lib.rs`
- `crates/ajax-core/src/live.rs`
- `.planning/agent-plans/agent-status-conservative.md` (checklist only)

## Forbidden changes

- Do not add pane regex vocabulary or expand `pane_evidence` / `looks_like_*`
  needle lists.
- Do not edit `ajax-web`, `ajax-tui`, SQLite migrations, or `architecture.md`
  in this packet (docs are a later packet).
- Do not change `agent_runtime.rs` / `agent_status_cache.rs` / `runtime_refresh.rs`
  in this packet (adapter wiring is the next packet).
- Do not flatten new parent phases into extra public `LiveStatusKind` variants;
  encode parent phase in observation summary / internal enum and derive existing
  kinds.
- No formatting sweeps, renames outside anchors, drive-by cleanup.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- **Desired behavior:** User requires process_alive ≠ activity; observations with
  source/observed_at/expires_at/confidence/run_id/parent_run_id; evidence order
  exit → structured lifecycle → hook → structured pane → generic pane →
  liveness(info); Unknown when ambiguous; parent/child aggregation; no Done
  until non-detached descendants terminal.
- **Current false positive:** `live.rs` `eligible_candidate` maps wrapper
  `working` to `EvidenceTier::WrapperRunning` / `AgentRunning` (lines ~324–331);
  `classify_agent_status_value("working")` → `AgentRunning` (lines ~1031–1036);
  architecture.md documents wrapper heartbeat as activity.
- **Existing patterns:** `StatusCandidate` / `select_status_observation` /
  `EvidenceTier` in `live.rs`; `ObservationConfidence` already in
  `runtime.rs` (reuse if practical, or mirror a local Confidence in
  `agent_status` to avoid coupling); tests around `select()` helpers near
  line ~2280 in `live.rs`.
- **Architecture boundary:** Core owns status reduction; CLI adapters only
  supply candidates. This packet stays inside core reduction.

## Code anchors

`crates/ajax-core/src/live.rs`:

- `EvidenceTier` / `select_status_observation` / `eligible_candidate`
- `classify_agent_status_value`
- existing status decision tests module (~line 2280+)

`crates/ajax-core/src/lib.rs`: add `pub mod agent_status;`

Locked design (do not reopen):

1. New public API in `agent_status`:
   - `ObservationSource`: `ProcessExit | ProviderLifecycle | ProviderHook |
     StructuredPane | GenericPane | ProcessLiveness`
   - `Confidence`: `High | Medium | Low`
   - `StatusObservation { source, observed_at, expires_at, confidence, run_id,
     parent_run_id: Option<String>, kind: ActivityKind }`
   - `ActivityKind`: activity-only enum (`Working | WaitingInput |
     WaitingApproval | Done | Failed | CommandRunning | TestsRunning | …`) —
     **no ProcessAlive variant**
   - `ProcessLiveness { alive: bool, observed_at }` separate input
   - `ParentPhase`: `ActivelyWorking | WaitingOnDelegated |
     WaitingForUser | CompletedLocallyChildrenActive | FullyCompleted |
     Unknown`
   - `StatusProjection { live: LiveObservation /* derived */, phase: ParentPhase,
     process_alive: bool }`
   - `fn reduce_agent_status(input) -> StatusProjection`

2. Evidence precedence (lower rank wins among non-expired):  
   ProcessExit(0) < ProviderLifecycle(1) < ProviderHook(2) < StructuredPane(3)
   < GenericPane(4). ProcessLiveness never selects activity.

3. Freshness: drop if `now > expires_at`. Newer `observed_at` for same `run_id`
   wins over older regardless of source tier. Cross-run: prefer higher-tier
   fresh evidence; if two fresh High/Medium observations disagree on activity
   class (running vs waiting vs done) with different sources → `Unknown`.

4. Default TTLs when building observations inside `live` adapter helpers:
   - ProcessExit / Failed: 120s
   - ProviderHook working: Codex 20s / else 120s (preserve existing windows)
   - ProviderHook wait/ask/done: 120s
   - StructuredPane: 60s, Medium
   - GenericPane: 15s, Low
   - Wrapper Running: **liveness only**, not an activity observation

5. Run graph: observations carry `run_id` + optional `parent_run_id`.
   Detached children: `parent_run_id` absent and `run_id` not referenced as
   child — ignore for parent completion gating when marked detached via
   `parent_run_id: None` **and** `run_id != primary`. Primary run id default
   `"primary"`. Non-detached child = has `parent_run_id == primary` (or chain).
   Parent FullyCompleted only when primary activity is Done/Failed/Exit **and**
   every non-detached descendant is terminal (Done/Failed/Exit). If primary
   Done but child active → `CompletedLocallyChildrenActive` → derive
   `WaitingForInput` with summary `delegated runs still active` (not Done),
   so lifecycle trusted-Done must not fire from this projection.

6. Derive `LiveStatusKind`:
   - ActivelyWorking → AgentRunning
   - WaitingOnDelegated → WaitingForInput + summary `waiting on delegated runs`
   - WaitingForUser → WaitingForInput / WaitingForApproval from best evidence
   - CompletedLocallyChildrenActive → WaitingForInput + `delegated runs still active`
   - FullyCompleted → Done (or CommandFailed if failed)
   - Unknown / empty trustworthy activity (even if process_alive) →
     `LiveStatusKind::Unknown`

7. Adapt `select_status_observation` to:
   - treat `RuntimeWrapper` + `working`/`starting` as process_alive=true only
   - treat `RuntimeWrapper` + `done`/`failed` as ProcessExit activity on
     run_id `primary`
   - treat `Hook` values via existing classify map as ProviderHook observations
     on run_id `primary` unless candidate value encodes `run_id=` (not required)
   - call `reduce_agent_status`; map result into `StatusDecision`
   - preserve missing-substrate prior short-circuit and ack-hold behavior

## Test-first instructions

Add deterministic unit tests in `crates/ajax-core/src/agent_status.rs`
(`#[cfg(test)] mod tests`) covering all of:

1. `live_process_idle_prompt_is_not_agent_running` — process_alive=true, no
   activity obs → `Unknown` (not AgentRunning)
2. `live_process_waiting_approval_from_hook` — alive + fresh ask hook →
   WaitingForApproval
3. `stale_working_heartbeat_then_waiting_hook` — stale wrapper working
   (liveness) + fresh wait hook → WaitingForInput
4. `historical_pane_running_or_approval_text_is_low_confidence` — only
   GenericPane Low with historical “running”/“approval required” → Unknown
   (or expired Low ignored), never confident AgentRunning/Approval alone
5. `parent_waiting_on_one_active_child` — primary idle/done-local + child
   Working → WaitingOnDelegated / WaitingForInput summary delegated
6. `parent_complete_while_child_active` — primary Done + child Working →
   CompletedLocallyChildrenActive, derived kind ≠ Done
7. `mixed_children_running_failed_completed` — aggregate still waiting if any
   non-detached child non-terminal; FullyCompleted only when all terminal
8. `child_completion_then_parent_resumption` — child Done then primary Working
   → ActivelyWorking / AgentRunning
9. `orphaned_stale_delegated_run_expires` — expired child obs ignored; parent
   may FullyComplete
10. `conflicting_observations_time_and_confidence` — older High Working vs
    newer High Waiting same run → Waiting; equal-fresh contradictory sources →
    Unknown
11. `process_exit_beats_stale_active_pane` — ProcessExit Done + stale GenericPane
    Working → Done / FullyCompleted
12. `no_trustworthy_evidence_yields_unknown` — empty obs, alive or not → Unknown

RED command (must fail before production module exists / before reducer logic):

```bash
cargo test -p ajax-core --lib agent_status -- --nocapture
```

Expected: fail because module/tests/assertions not satisfied.

Also keep existing `live` status decision tests compiling; update only those
that asserted wrapper `working` → applied AgentRunning (they must now expect
not applied / Unknown / fallthrough). Run:

```bash
cargo test -p ajax-core --lib live::tests -- --nocapture
```

## Edit instructions

1. Create `agent_status.rs` with types + `reduce_agent_status` per locked design.
2. Export module from `lib.rs`.
3. Refactor `live.rs` `select_status_observation` / `eligible_candidate` to feed
   the reducer; wrapper running/starting = liveness only.
4. Update obsolete live tests that required wrapper-working → AgentRunning.
5. Check off completed items in the plan checklist for task 1.

## Verification commands

```bash
cargo test -p ajax-core --lib agent_status -- --nocapture
cargo test -p ajax-core --lib live::tests -- --nocapture
cargo check -p ajax-core
```

## Acceptance criteria

- All 12 named scenarios pass.
- Wrapper/`working` alone never yields `LiveStatusKind::AgentRunning`.
- Process exit outranks stale pane activity.
- Parent not FullyCompleted/Done while non-detached child active.
- Existing missing-substrate and ack-hold behaviors preserved.
- Diff limited to Allowed files.

## Stop conditions

- Need to edit `runtime_refresh`, CLI cache, web, or SQLite → stop; that is
  packet 2.
- Temptation to add regex needles → stop.
- Patch exceeds ~400 changed lines → stop and split.
- Existing live tests require broader behavior changes than wrapper-liveness
  retarget → stop and report.
