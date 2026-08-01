# Undo #719; swipe-left opens Diff, swipe-right → dashboard

Mode: Behavior Change.
Status: in progress.

## Situation

#719 merged to main as squash `ea13fe3` with **wrong** directions
(swipe-right opens Diff). The corrective commit never landed.

## Product (confirmed)

- From task detail: swipe-**left** → Diff
- From task detail: swipe-**right** → dashboard/project (`onBack`)
- Diff Review: swipe-**right** → back to task (same back stack)
- Plain swipes; keep callback refs; no long-press

## Plan

1. Branch from `origin/main`
2. Revert `#719` (`ea13fe3`)
3. Re-apply plain swipe with **correct** directions (+ refs)
4. Open a new PR (do not revive #719)

## Delegation decision

`Delegation decision: not delegated because` revert + direction flip is
smaller than a packet round-trip and must stay aligned with an explicit
"undo that PR" request.

## Checklist

- [ ] Revert ea13fe3 on a clean branch from main
- [ ] Implement left→Diff / right→onBack (TaskDetail + DiffReview)
- [ ] Tests + e2e + architecture
- [ ] Validate; open new PR

## Validation

```bash
npm run web:test -- --run navigateSwipe TaskDetail DiffReview
npm run web:check
npm run web:smoke -- e2e/diff-review.test.ts e2e/diff-review-swipe-repro.test.ts
```
