# AoE loop-shape pane reconcile (Ajax pipeline)

## Scope

Copy Agent of Empires’ **terminal status loop shape** into Ajax runtime refresh:
hooks/lifecycle remain primary; pane capture runs only on AoE’s reconcile gates
and may upgrade a mid-turn **Working** projection to Waiting when bottom-anchored
wait chrome is visible.

Keep Ajax’s JSONL → fold → `reduce_agent_status` → `derive_operator_status`
pipeline. Do **not** adopt AoE’s one-word sidecar as truth.

## Non-goals

- Full AoE Running/Idle pane detectors (B3 activity inference)
- Clearing stuck Waiting when chrome disappears (needs idle/running detector)
- Cursor `Notification(permission_*)` hook install (separate G1.4)
- ACP / structured-view paths
- Web UI changes

## Delegation decision

`Delegation decision: delegated via model-router` (initial loop shape)

Bugbot follow-ups (Unknown continue + Done idle-gate): `Delegation decision: not delegated because smaller than the work order — two focused regression fixes on the just-landed diff`

## Design (AoE gates → Ajax)

After `reduce_agent_status`, before applying live status:

| Gate | When | Pane action |
| --- | --- | --- |
| Running reconcile | `ParentPhase::ActivelyWorking` + Claude\|Codex\|Cursor | capture; `reconcile_wait_from_pane` (ungated) may override → Waiting |
| Idle reconcile | `FullyCompleted` + Claude + prior `AgentRunning` / `WaitingForApproval` / `WaitingForInput` (not soft `Done`) | capture; wait chrome → Waiting |
| Unknown fallback | `Unknown` + capability allows pane wait | existing `maybe_pane_wait` (gated) |
| Else | parked unambiguous Done/Idle/Error without gates | **no** capture |

Cursor is included in running reconcile (Ajax adaptation: no Cursor wait hooks yet).

## Architecture

Update `architecture.md` evidence precedence: structured lifecycle may be
pane-corrected for wait chrome under the gates above; pane still cannot invent
Running from chrome.

## Checklist

- [x] Failing tests: Working+permission pane → Waiting; Claude native still reconciles; stable Done skips capture; rewrite old never-override test
- [x] `pane_fallback::reconcile_wait_from_pane` (ungated hint → LiveObservation)
- [x] `runtime_refresh` AoE-shaped gates
- [x] `architecture.md` update
- [x] Focused `ajax-core` tests green
- [x] Parent validation

## Validation

```bash
cargo test -p ajax-core -- running_lifecycle_reconciles cursor_running_reconciles claude_running_reconciles fully_completed_claude_idle steady_state_refresh_skips_capture_pane reconcile_wait_from_pane_is_ungated pane_wait_observed
# → 7 passed (parent)
cargo test -p ajax-core pane_
# → 14 passed (parent)
cargo check -p ajax-core
# → ok
```

## Bugbot follow-ups (addressed)

- [x] High: Unknown phase always `continue` (never apply Unknown / clear live wait)
- [x] Medium: idle gate is actionable waits only (`AgentRunning` / approval / input), not soft `Done`
- [x] CLI/core fixture updates for Working capture-pane (command sequences + default non-wait chrome)

## Deviations

- GLM hit monthly usage limit (429); rerouted to Cursor Composer 2.5.
- Delegate report schema invalid; parent gated on diff + re-ran tests.
- Parent fixed `architecture.md` intro so it no longer says pane never overrides lifecycle.
- Husky verify surfaced remaining Working-capture test expectations; fixed before commit.
