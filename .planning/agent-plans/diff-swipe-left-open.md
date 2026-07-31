# Diff swipe directions: left opens, right backs

Mode: Behavior Change.
Status: complete.

## Product decision (corrected)

- **Task detail:** swipe-**left** opens Diff Review; swipe-**right** calls `onBack` (dashboard / project).
- **Diff Review:** swipe-**right** returns to task; swipe-**left** does not navigate.
- Plain swipes (no long-press). Keep `onOpenDiff` / `onBack` in refs.

## Non-goals

- Reintroduce long-press.
- Change Diff PR/hunk pan ownership.
- Change hash routes or App wiring beyond existing `onBack` / `onOpenDiff`.

## Delegation decision

`Delegation decision: delegated via model-router` → `cursor-delegate` / `composer-2.5`
(Report schema FAILED; parent Review Gate ACCEPTed on delta + verification.)

## Checklist

- [x] Flip TaskDetail: left → open Diff, right → onBack (+ onBackRef)
- [x] Flip DiffReview: right → onBack
- [x] Unit + e2e + architecture comments
- [x] Validate and push to #719

## Validation

```bash
npm run web:test -- --run navigateSwipe TaskDetail DiffReview  # pass (transaction)
npm run web:check                                              # pass
npm run web:smoke -- e2e/diff-review.test.ts e2e/diff-review-swipe-repro.test.ts  # 3 pass
```
