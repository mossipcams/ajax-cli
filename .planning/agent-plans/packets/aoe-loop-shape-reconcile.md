PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Adopt AoE’s terminal status **loop shape** inside Ajax refresh: when lifecycle
projects `ActivelyWorking` for Claude/Codex/Cursor, capture the pane and if
bottom-anchored wait chrome is recognized, override live status to Waiting.
Also Claude `FullyCompleted` after prior Running/Waiting may upgrade to Waiting
on wait chrome. Keep Unknown capability-gated fallback. Do not invent Running
from pane. Do not replace the JSONL/reducer pipeline.

## Allowed files

- `crates/ajax-core/src/pane_fallback.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `architecture.md`
- `.planning/agent-plans/aoe-loop-shape-reconcile.md`

## Forbidden changes

- Do not add AoE-style `/tmp` status sidecar or replace `AgentStatusFiles` JSONL.
- Do not change `ui_state`, `attention`, web, CLI hooks install, or `agent_event` translation.
- Do not add full Running/Idle pane activity detectors.
- Do not clear Waiting when chrome disappears.
- Do not edit unrelated crates or reformat the repo.

## Context evidence

Desired behavior: AoE `update_status_with_metadata_inner` (non-ACP) reads hook
status then reconciles Running (claude/codex) against pane before trusting it.
Ajax today only pane-captures on `ParentPhase::Unknown` and
`pane_evidence_never_overrides_lifecycle_observation` asserts Working+permission
pane stays AgentRunning with zero captures — that is the bug for mid-turn waits.

Existing patterns: `pane_fallback::recognize_wait_hint`, `maybe_pane_wait`
(capability-gated), `CLAUDE_PERMISSION_MENU` + `PermissionMenuRunner` tests at
`runtime_refresh.rs:2090+`.

Architecture: core owns task truth; reconcile must happen before/at apply so
`derive_operator_status` / notify / web keep working unchanged.

## Code anchors

- `crates/ajax-core/src/runtime_refresh.rs:384-407` — Unknown-only pane branch to extend into AoE gates
- `crates/ajax-core/src/runtime_refresh.rs:2151-2172` — test `pane_evidence_never_overrides_lifecycle_observation` must flip to running-reconcile behavior
- `crates/ajax-core/src/runtime_refresh.rs:2175-2189` — stable Working skip-capture test must be rewritten (Working+Codex now captures; assert no status change when pane has no wait chrome, or use FullyCompleted/Done lifecycle for zero-capture steady state)
- `crates/ajax-core/src/pane_fallback.rs:42-64` — add ungated `reconcile_wait_from_pane` beside gated `maybe_pane_wait`
- `crates/ajax-core/src/agent_status.rs:161-168` — `ParentPhase` variants for gates
- `architecture.md:147-171` — update precedence text for gated pane correction of Working

## Test-first instructions

1. Rewrite/add in `runtime_refresh.rs` tests:

   - `running_lifecycle_reconciles_to_waiting_on_permission_pane`: Codex (default)
     task + `lifecycle_obs(Working, …)` + `PermissionMenuRunner` → after refresh,
     `live_status.kind == WaitingForApproval`, capture-pane count == 1.

   - `cursor_running_reconciles_on_cursor_permission_chrome`: Cursor agent + Working
     lifecycle + Cursor-shaped pane (`> Allow` / `Deny` / `enter to select`) →
     `WaitingForApproval`.

   - `claude_running_reconciles_despite_native_wait_capability`: Claude + Working +
     `CLAUDE_PERMISSION_MENU` → Waiting (proves ungated reconcile, not `maybe_pane_wait`).

   - `fully_completed_claude_idle_reconciles_to_waiting_on_permission_pane`: Claude +
     `lifecycle_obs(Done/…)` mapping to FullyCompleted + prior `live_status`
     AgentRunning + permission pane → WaitingForApproval.

   - Replace `pane_evidence_never_overrides_lifecycle_observation` with the running
     reconcile assertion (or delete and rely on the new test).

   - Rewrite `steady_state_refresh_skips_capture_pane_when_agent_cache_is_stable` so
     a **non-gated** phase (e.g. Working is gated — use a Done/FullyCompleted task
     with no prior Running/Waiting for Claude, or Codex FullyCompleted) still
     asserts capture-pane == 0.

2. Add unit test in `pane_fallback.rs`: `reconcile_wait_from_pane` returns
   WaitingForApproval for Claude permission chrome; `maybe_pane_wait` still None
   for Claude on same pane.

3. RED: run
   `cargo test -p ajax-core running_lifecycle_reconciles_to_waiting_on_permission_pane -- --nocapture`
   and confirm failure before production edit.

## Edit instructions

1. In `pane_fallback.rs`, add:

```rust
pub fn reconcile_wait_from_pane(agent: AgentClient, visible_pane: &str) -> Option<LiveObservation> {
    match recognize_wait_hint(agent, visible_pane)? {
        PaneWaitHint::WaitingPermission => Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        )),
        PaneWaitHint::WaitingQuestion => Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        )),
    }
}
```

2. In `runtime_refresh.rs`, replace the Unknown-only block with AoE-shaped gates:

   - If `ActivelyWorking` && agent in {Claude, Codex, Cursor}: capture; if
     `reconcile_wait_from_pane` yields Some, use that as the observation to apply
     (observed_at = now), then fall through to normal apply path (or apply inline
     like today’s Unknown branch). Prefer one shared apply path.
   - Else if `FullyCompleted` && Claude && prior live was Waiting-class or
     AgentRunning: same capture + reconcile override.
   - Else if `Unknown` && `profile_allows_any_pane_wait_fallback`: existing
     `maybe_pane_wait` path.
   - Else: no capture; apply projection as today.

   Respect attention acknowledgment: do not apply Waiting override when
   `observed_at <= attention_acknowledged_at` (same rule as existing waiting hold).

3. Update `architecture.md` so pane may correct Working→Waiting under these gates;
   pane still never invents AgentRunning; Unknown fallback remains capability-gated.

4. Check off plan checklist items as you go.

## Verification commands

```bash
cargo test -p ajax-core running_lifecycle_reconciles_to_waiting_on_permission_pane -- --nocapture
cargo test -p ajax-core cursor_running_reconciles_on_cursor_permission_chrome -- --nocapture
cargo test -p ajax-core claude_running_reconciles_despite_native_wait_capability -- --nocapture
cargo test -p ajax-core fully_completed_claude_idle_reconciles_to_waiting_on_permission_pane -- --nocapture
cargo test -p ajax-core pane_ -- --nocapture
cargo test -p ajax-core steady_state_refresh_skips_capture_pane_when_agent_cache_is_stable -- --nocapture
cargo check -p ajax-core
```

## Acceptance criteria

- Mid-turn Working + permission pane → Waiting for Claude, Codex, and Cursor.
- Claude native capability no longer blocks running reconcile.
- Parked non-gated tasks still skip capture-pane.
- No JSONL/sidecar redesign; notify/web continue to consume projected Waiting.
- architecture.md documents the loop-shape gates.

## Stop conditions

- Need full Running pane detector to make a test pass.
- Diff exceeds ~400 lines or touches forbidden files.
- Waiting-clear-on-Esc required for green (out of scope).
