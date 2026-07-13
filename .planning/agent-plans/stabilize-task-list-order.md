# Stabilize dashboard task row order

## Scope

Stop task rows from swapping positions on every cockpit poll when
`last_activity_unix_secs` leapfrogs within the same status. Keep status-group
ordering; preserve relative order across polls so taps land on the intended row.

## Non-goals

- No poll-interval changes, no backend activity semantics, no inbox redesign.

## Delegation decision

`Delegation decision: not delegated because this is a small sticky-sort behavior
fix the parent is landing while coordinating the larger scale-to-fit
delegation; work order would exceed the diff.`

## Task checklist

- [x] Failing test: same-status activity leapfrog does not reorder when previous order is supplied
- [x] Implement sticky tie-break in `sortCards` + TaskList previous-order wiring
- [x] Verify focused state/TaskList tests

## Validation results

- RED: `state.test.ts` sticky leapfrog failed as expected (exit 1)
- GREEN: `state.test.ts` + `TaskList.test.ts` — 31 passed

## Delegation decision

`Delegation decision: not delegated because this is a small sticky-sort behavior
fix the parent is landing while coordinating the larger scale-to-fit
delegation; work order would exceed the diff.`

