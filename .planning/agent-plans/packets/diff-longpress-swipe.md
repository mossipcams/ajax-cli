PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Change Diff Review navigation gestures so accidental quick swipes do nothing, and opening Diff requires a long-press followed by a right swipe.

## Scope

### Allowed
- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.ts`
- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.test.ts`
- `crates/ajax-web/web/src/features/task/TaskDetail.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx` (only if swipe-back tests exist or need updating)
- `crates/ajax-web/web/e2e/diff-review.test.ts`
- `architecture.md` (one-line gesture note only)
- `.planning/agent-plans/diff-longpress-swipe.md`

### Forbidden
- Core/Rust/DTO changes
- Signal/noise ranking changes
- Dashboard swipe-reveal restore
- Unrelated gesture modules (sheetDrag, pullToRefresh, TaskTerminal long-press selection)

## Acceptance

1. Task detail quick horizontal swipe left or right (no long-press) does **not** call `onOpenDiff` and does not leave a stuck drag transform.
2. Task detail: hold finger still ≥ `NAVIGATE_LONG_PRESS_MS` (export 450–500ms from navigateSwipe; move > ~8px before mature cancels arming), then swipe **right** past existing trigger → `onOpenDiff` once.
3. Long-press then swipe **left** on task detail does **not** open Diff.
4. Diff Review: quick swipe left/right does **not** call `onBack`.
5. Diff Review: long-press then swipe **left** past trigger → `onBack` (mirror exit). Back button still works.
6. Unit tests updated; e2e uses long-press + right drag helper.
7. `architecture.md` Diff Review entry sentence updated from swipe-left to long-press + swipe-right.

## Constraints

- Reuse existing `navigateSwipe*` math for distance/lock; only gate commitment on long-press arming + direction.
- Capture-phase listeners on TaskDetail stay (must work over terminal).
- Do not steal Diff hunk/PR-strip pans (`isDiffPanGestureTarget` still skips).
- Smallest diff.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run navigateSwipe TaskDetail DiffReview
      expected: pass
    - type: typecheck
      command: npm run web:check
      expected: pass
  broader_checks: []
  reason: Gesture behavior is covered by unit tests; e2e helper updated for smoke.
```

## Stop if

- Need to change terminal long-press selection semantics
- Patch exceeds ~400 lines or Allowed files

## Code anchors

- `TaskDetail.tsx` touch listeners ~L49–99 (`committedLeft` → must become long-press + right)
- `DiffReview.tsx` `onTouchEnd` currently `direction === "right" → onBack`
- `navigateSwipe.ts` trigger constants
- `TaskDetail.test.tsx` left-swipe cases ~L116–137
- `e2e/diff-review.test.ts` `touchDragLeft`

## Edit instructions

1. Export `NAVIGATE_LONG_PRESS_MS` and `NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX` from `navigateSwipe.ts` (optional tiny `longPressArmed(startedAt, now, movedPx)` helper for testability).
2. TaskDetail: track `pressStartedAt` + cancel on early move; only after armed, run `navigateSwipeMove` / visual drag; on end commit only if armed && direction === `"right"`.
3. DiffReview: same arming; commit only if armed && direction === `"left"` for back.
4. Rewrite unit/e2e tests accordingly (quick swipe asserts no-op; armed right opens; armed left backs from Diff).
