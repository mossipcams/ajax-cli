# Smooth swipe page transitions

Mode: Behavior Change.
Status: complete.
Approval: user approved attached plan.

## Scope

Finish-the-slide transitions for task ↔ Diff ↔ dashboard swipes.

## Non-goals

Dual-page peek, bottom-nav/Settings animation, direction changes, long-press.

## Delegation decision

`Delegation decision: delegated via model-router` → `cursor-delegate` /
`composer-2.5`. Parent Review Gate ACCEPTed (report schema FAILED; verification
passed). Parent fixed: clear enter class on non-swipe nav; skip reset after
commit navigate.

## Checklist

- [x] Gesture math: trigger vs full-width visual + commit offset
- [x] `useSwipePageTransition` + TaskDetail / DiffReview
- [x] Enter animation flag + CSS + App outlet class
- [x] Tests + smoke + architecture note

## Validation

```bash
npm run web:test -- --run navigateSwipe useSwipePageTransition swipeEnter TaskDetail DiffReview  # pass
npm run web:check                                                                              # pass
npm run web:smoke -- e2e/diff-review.test.ts e2e/diff-review-swipe-repro.test.ts               # 3 pass
```
