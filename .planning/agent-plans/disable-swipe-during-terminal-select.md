# Disable page swipe during terminal double-tap select

## Scope

While double-tap-hold selection drag is active on the terminal, TaskDetail
page swipe (Diff / Back) must not engage.

## Non-goals

- Changing when swipe works on non-selecting terminal gestures
- Changing Diff Review ignore targets
- Full e2e swipe+select matrix

## Approach

1. Set/clear `document.documentElement.dataset.ajaxTerminalSelecting` when
   selection-drag arms / ends (in `mountTaskTerminalSession`).
2. `useSwipePageTransition` aborts an in-flight gesture on move/end when that
   flag is set (capture-phase swipe arms before terminal bubble selection).
3. Wire via a small shared helper on `terminalTouchSelection.ts`.
4. Unit test: swipe does not commit when selecting flag is set mid-gesture.

## Delegation decision

`Delegation decision: not delegated because change is smaller than a useful
work order (set flag + abort swipe on move; ~3 files, ~40 lines).`

## Checklist

- [x] Flag helpers + set/clear in mount
- [x] Abort swipe when selecting
- [x] Focused tests
- [x] Verify

## Validation

```bash
npm run web:test -- --run src/shared/hooks/useSwipePageTransition.test.tsx \
  src/features/task/TaskDetail.test.tsx \
  src/features/task/terminalTouchSelection.test.ts
# → 39 passed
```

`mountTaskTerminalSession.ts` LOC: 938 (≤1000).

Uncommitted; say if you want this on #752.