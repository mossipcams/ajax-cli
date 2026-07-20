# Pane stuck-status regression (post agent-status-conservative)

## Scope

Restore the six actionable pane statuses dropped by the conservative status
redesign (#591): `AuthRequired`, `RateLimited`, `ContextLimit`, `Blocked`,
`MergeConflict`, `CiFailed`.

## Non-goals

- No new `ActivityKind` variants and no reducer changes — these kinds are not
  agent *activity*, they are stuck states.
- No change to the false-`AgentRunning` fix: the fallback must never emit
  `AgentRunning`, `CommandRunning`, `TestsRunning`, or `Done`.
- Not fixing the `WaitingInput` low-confidence asymmetry (separate, raised).

## Diagnosis

`runtime_refresh.rs:448` replaced `classify_agent_pane` with
`project_pane_activity`. That funnels through `live_kind_to_activity`
(`live.rs:403`), which returns `None` for the six kinds above, so the refresh
loop hits `continue` and the status is never applied. Verified empirically
against real pane text — all six produce no observation.

`ui_state.rs:161-174` still maps them to operator explanations and they pass
`is_actionable_attention`, so these are notification-firing states that can no
longer be reached from the tmux polling path.

## Approval

Explicit user request: "add the failing test first, then fix it".

## Design

Add `live::project_pane_stuck_status(agent, pane) -> Option<LiveObservation>`
returning only kinds where `live_kind_to_activity` is `None` **and**
`LiveStatusKind::class()` is `Waiting` or `Error`. Derived, not a hand-kept
list, so it cannot drift from the activity mapping.

`runtime_refresh` calls it on the `project_pane_activity` → `None` branch.

## Task checklist

- [x] Test (red): inline `#[cfg(test)]` in `live.rs` — each of the six pane
      texts yields the matching kind via `project_pane_stuck_status`
      (`pane_stuck_states_survive_the_activity_projection`)
- [x] Test (red): busy/idle/done panes yield `None` (no `AgentRunning` leak)
      (`pane_stuck_states_never_project_activity_or_completion`)
- [x] Implement `project_pane_stuck_status`
- [x] Wire the `None` branch in `runtime_refresh.rs`
- [x] Verify: `cargo test -p ajax-core`, `cargo test -p ajax-cli`

## Notes

ajax-core tests must stay inline `#[cfg(test)]` — `lifecycle.rs:393` treats a
standalone sibling `tests.rs` as production code.

## Validation

- Red confirmed before implementing: `Blocked` asserted `left: None`.
- `cargo test -p ajax-core` — 828 passed (826 + 2 new).
- `cargo test -p ajax-cli` — 309 + 11 + 14 passed.
- `cargo clippy --all-targets -- -D warnings` — clean. `cargo fmt --all` applied.
- Throwaway probe mirroring the `runtime_refresh` branch: all six stuck states
  now `APPLY`; historical approval prompt, busy chrome, and idle shell still
  skip without fabricating state (redesign guarantees intact).

## Deviations

None. Note: a live busy pane (`esc to interrupt`) alone still projects
`Unknown` — that is the redesign's intended conservatism (busy chrome is
GenericPane/Low; activity must come from hook/wrapper), not a regression from
this change.

## Round 2 — notification alignment

Restoring the six states put them back on the notification path, so the
attention model had to be re-checked. Probing `take_attention_transition_at`
with real `Task` objects found two misalignments:

1. **Error-class bypassed the dwell gate.** `defers_unconfirmed_waiting` only
   covered `LiveStatusClass::Waiting`, but `is_actionable_attention` returns
   `true` unconditionally for `TaskStatus::Error`. So `Blocked` /
   `MergeConflict` / `CiFailed` fired a phone ping on a *single* unconfirmed
   pane sample, while `RateLimited` / `AuthRequired` / `ContextLimit` correctly
   waited 4s.
2. **Unbounded scrollback.** `classify_recent_evidence` scans the whole pane in
   reverse, so a stuck line 60 lines up still notified.

### Changes

- [x] Test (red): `error_class_pane_evidence_is_dwell_gated_like_waiting`
      (3 rstest cases) — silent on first sample, one ping after dwell
- [x] Test (green, guard): `trusted_error_evidence_is_not_dwell_gated` —
      wrapper/hook failures stay immediate
- [x] Test (red): `stale_scrollback_stuck_lines_are_not_live_evidence`
- [x] `defers_unconfirmed_waiting` → `defers_unconfirmed_attention`, now covers
      `Waiting | Error`. Trusted/authoritative paths still bypass.
- [x] `project_pane_stuck_status` bounded to the last `BUSY_WINDOW` meaningful
      lines (reuses the existing recency constant rather than inventing one)

### Verified behavior

All six states now uniform — silent at t0, exactly one ping after the 4s dwell,
deduped thereafter:

```
blocked/rate limited/auth required/
context limit/merge conflict/ci failed  t0=None  t+5s=ONE ping  t+10s=None
buried-in-scrollback blocked            no projection (silent)
```

Delegated-waiting suppression (`is_delegated_waiting_summary`) confirmed intact
and already covered by `delegated_waiting_does_not_notify` /
`delegated_still_active_does_not_notify`.

`cargo test --workspace` — 833 + 309 + 205 + 161 + 127 + 14 + 11 all pass.
`cargo clippy --all-targets -- -D warnings` clean.

## Follow-up not in scope

1. `is_untrustworthy_low_pane` (`agent_status.rs:284`) degrades `Working` and
   `WaitingApproval` at Low confidence but not `WaitingInput`, so
   low-confidence scrollback can still assert a confident `WaitingForInput`
   and fire a notification. Raised with the user; not fixed here.

2. **Prose false positives remain.** The needles are substring matches, so a
   line like "the PR is blocked on review, continuing now" still projects
   `Blocked` and (after dwell) notifies. This is pre-existing needle-vocabulary
   weakness — `classify_agent_pane` behaved identically before the redesign —
   and tightening it was an explicit non-goal of the original plan ("No new
   pane regex vocabulary"). The dwell gate does not help here because the prose
   persists across samples. Needs a deliberate needle-tightening pass.
