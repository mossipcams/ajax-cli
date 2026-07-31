# Dashboard section-sticky task order

## Scope

Stop Active/Idle band rows from reshuffling on every cockpit poll when a task
stays in the same section. Sort by section (Active vs Idle) first; within a
section, preserve `previousOrder` sticky order. Only move a task when it
crosses Active ↔ Idle.

## Non-goals

- No backend status semantics, poll interval, Muster Bar fleetSegments, or
  badge/statusRank display changes.
- No redesign of Active/Idle bands.

## Root cause

`sortCards` compared fine-grained `statusRank` (`running` < `waiting` <
`error` < `idle`). Sticky `previousOrder` only applied when ranks were equal.
Agent status flaps (running ↔ waiting) therefore reordered rows inside Active
even though the UI section did not change.

## Delegation decision

`Delegation decision: delegated via model-router`

- Round 1 MiniMax (`opencode-go/minimax-m3`): FAILED — 429 monthly usage limit, empty diff → DISCARD
- Round 2 GLM (`opencode-go/glm-5.2`): FAILED — same OpenCode Go usage limit, empty diff → DISCARD
- Round 3 Cursor (`composer-2.5`): implemented; parent cleaned out-of-scope
  `.planning/router-runs/_tmp-report.yaml`; ACCEPT

## Task checklist

- [x] Test: same-section status flap (running ↔ waiting) keeps previous order
- [x] Test: Active ↔ Idle still promotes/demotes despite previous order
- [x] Implement section-first comparator in `sortCards`
- [x] Adjust existing sort tests for section-crossing wording
- [x] Parent verification: focused vitest for state + TaskList

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/shared/lib/state.test.ts src/features/task/TaskList.test.tsx
```

Result: 2 files, 42 tests passed (exit 0)

## Deviations

- OpenCode Go MiniMax/GLM unavailable (monthly usage limit); escalated to Cursor.
- Delegate wrote a temporary report under `.planning/router-runs/`; parent deleted
  it before ACCEPT (scope violation cleaned).

## Approval

Not required — Behavior Change, presentation-only list order.
