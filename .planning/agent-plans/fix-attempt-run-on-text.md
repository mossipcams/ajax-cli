# Fix Attempts run-on text (`Running2h ago`)

## Scope

- Task details Attempts rows concatenate outcome and relative time with no
  space (`Running2h ago · in progress`) because JSX emits adjacent spans
  without a whitespace child.
- Add an explicit separator and a focused regression test.

## Non-goals

- No Annotation Debug → Display cleanup (separate concern unless requested).
- No Task details layout redesign.
- No backend / registry changes.

## Delegation decision

`Delegation decision: not delegated because smaller than the work order
needed to describe it (explicit whitespace between two spans + one assert).`

## Task checklist

### Task 1: Space between outcome and when

- [x] Test: assert Attempts `textContent` has a space between outcome and
  relative time (e.g. `/Running\s+\d/`) — failed first with
  `Running2h ago · in progress`
- [x] Impl: insert `{" "}` between `.attempt-outcome` and `.attempt-when`
- [x] Verify: focused `TaskMetaDetails` vitest — PASS (10)

## Validation

```bash
rtk npm run web:test -- --run src/features/task/TaskMetaDetails.test.tsx
```

## Deviations

- None.

## Validation ledger

- `rtk npm run web:test -- --run src/features/task/TaskMetaDetails.test.tsx`
  → FAIL once (expected), then PASS (10)
- `rtk npm run web:build` → PASS; dist now emits `" "` between outcome and when
