---
context: default
slug: acp-status-in-task-truth
status: implemented 2026-08-22
approval: user-directed ("Do that", 2026-08-22) — option A investigated and found unnecessary
last_updated: 2026-08-22
---

# ACP run-state on the task page

## The gap (verified)

A provisioned chat task running an ACP turn reads `Running` in the chat live
head and `Waiting`/`Idle` everywhere else — task-page pill, details sheet,
dashboard card, TUI cockpit, `ajax status`.

Traced:

- `BrowserTaskCard.status` / `BrowserTaskDetail.status` come from
  `commands::cockpit_view` (`ajax-web/src/slices/cockpit/mod.rs:91,175`).
- That resolves through `ui_state::derive_task_status`
  (`ajax-core/src/ui_state.rs:133`), a pure function of `Task`.
- `Task.agent_status` / `SideFlag::AgentRunning` — the inputs that produce
  `run("Agent working")` at `ui_state.rs:192-196` — are written only by
  `live_application::apply_live_observation`
  (`ajax-core/src/live_application.rs:88-97`).
- The only production producer of `LiveObservation` is the supervisor's pane
  classifier (`ajax-supervisor/src/status.rs`), which reads tmux and has no
  concept of an ACP session.
- The ACP host touches the registry exactly once, read-only, to plan an attach
  (`ajax-web/src/slices/web_session/mod.rs:373-395`). It never reports back.

So ACP run-state has no path into task truth. The chat route hides the header
pill (`TaskWorkspace.tsx:179`) because the live head is the only surface that
knows.

## Proposed change

The ACP session host becomes a second evidence producer, on the same contract
the supervisor already uses. No derivation change, no new browser authority.

1. `TaskSession` reports transitions as `LiveObservation`s:
   - turn accepted → `LiveStatusKind::AgentRunning`
   - permission / elicitation pending → `LiveStatusKind::WaitingForApproval`
   - `turn_end` → `LiveStatusKind::Done` (or `Blocked` on error)
2. `derive_task_status` is untouched: `AgentRunning` already maps to
   `agent_status = Running` + `SideFlag::AgentRunning` → `run("Agent working")`,
   and `WaitingForApproval` already maps to an actionable `Waiting`.
3. Every surface — dashboard card, task-page pill, details sheet, TUI, CLI,
   notifier — becomes correct at once, because they all read the same
   projection.
4. `showStatusPill` on the chat route can then stay false or be re-enabled on
   evidence rather than on a guess; that is a follow-up, not part of this.

## Outcome

Implemented as described, **without** the arbitration rule. The hazard below was
tested rather than assumed: `refresh_preserves_acp_evidence_on_a_provisioned_task`
(`ajax-core/src/runtime_refresh/tests/suite_3.rs`) drives a full refresh against a
provisioned task carrying ACP evidence with a runner reporting an idle shell, and
the task keeps `Running`. Core already preserves prior evidence when the agent
projection has nothing trustworthy to say, so no producer needed removing. The
test stays as the lock.

Residual risk: an ACP-launched harness that also writes native session logs could
produce a `FullyCompleted` projection phase mid-turn and reconcile the task to
idle. Not observed; would show up as a chat task flipping to Idle while working.

## The hazard that would have needed arbitration

Two producers would write `agent_status` for one task. A provisioned task still
has a tmux session, and the supervisor classifies an idle pane as
`LiveStatusKind::ShellIdle`, which flips `Running` → `Dead`
(`live_application.rs:139-148`). Without an ownership rule, an actively working
ACP task would oscillate between `Running` and `Agent unavailable` at the
monitor's cadence.

Options, needing a decision:

- **A.** Pane evidence is ignored for tasks with `skip_interactive_agent()` —
  provisioned tasks have no agent pane to classify, so the pane classifier has
  nothing true to say about them.
- **B.** Last-writer-wins with a freshness window, like
  `PROCESS_LIVENESS_FRESH_FOR`.
- **C.** A distinct `LiveStatusKind` for ACP evidence so the two never share a
  field.

A is the simplest and most defensible: it removes a producer that is already
guessing rather than adding arbitration.

## Blast radius

`ajax-core` (evidence ownership rule), `ajax-web` (session host emits),
`ajax-supervisor` (skips provisioned tasks under A). Snapshot and status tests
across `ajax-cli`, `ajax-tui`, and `ajax-web`. Notifications change too:
`actionable` waiting phone-pings, so a pending ACP permission would start
pinging — probably desirable, definitely a behavior change to confirm.

## Not doing without a green light

This changes what task status *means* for provisioned tasks and touches the
CLI and TUI, which AGENTS.md lists as a stop condition. The web-only
alternative — showing the ACP state on the header pill while the chat route is
mounted — was rejected: it fixes nothing outside the one route that already
knows, and re-introduces the head/pill duplication removed in the conversation
flow work.
