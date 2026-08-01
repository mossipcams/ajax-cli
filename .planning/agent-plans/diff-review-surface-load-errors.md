# Diff Review: surface load errors (follow-up to pipe drain)

Mode: Behavior Change.
Parent PR: https://github.com/mossipcams/ajax-cli/pull/731

## Root cause vs error handling

- **Root cause (already in PR):** timed Capture pipe deadlock in `process.rs`.
- **Error handling (this follow-up):** Diff Review must not pretend PR list
  failures never happened, and must not say “Loading pull requests…” while
  `/diff` is in flight.

## Delegation decision

`Delegation decision: not delegated because` GLM pi-delegate hit usage limit
(429); Cursor already landed the process fix; this is a bounded DiffReview UX
follow-up on the same PR (R-STOP escalate exhausted for opencode lanes).

## Scope

- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- rebuild `web/dist` via verify/husky if needed

## Task checklist

- [x] Loading phases: pull-requests vs diff copy
- [x] When PR list fails but local/PR diff succeeds, show non-fatal warning with reason
- [x] Keep hard error when diff itself fails (existing)
- [x] Tests updated
- [ ] Validate + push to #731

## Validation

```bash
npm run web:test -- --run DiffReview  # 15 passed
npm run web:check
npm run web:build
```
