# Harden swipe suppress for terminal select

## Problem

PR #756 set `ajaxTerminalSelecting` when selection-drag arms, but page swipe
listeners run in **capture** on the task root **before** the terminal bubble
handler. The second contact of a double-tap still arms swipe; horizontal drag
can navigate Diff/Back while highlighting.

## Fix

1. On first quick-tap end (pending double-tap), set a short-lived
   `ajaxTerminalDoubleTapPending` flag.
2. Swipe `touchstart`: ignore when selecting **or** (pending && target in
   terminal interaction surface) so the second contact never arms swipe.
3. When a single long-press matures (≥500ms, finger still down), set the
   selecting/lock flag so hold+drag cannot be stolen by swipe either.
4. Clear flags on every touch end/cancel/dispose path.

## Delegation decision

`Delegation decision: not delegated because focused follow-up to merged #756;
anchors known; change smaller than a full work order.`

## Checklist

- [x] Extend `terminalSelecting.ts` with pending + helpers
- [x] Wire mount + swipe hook
- [x] Tests
- [x] Focused vitest (17 passed)
- [ ] Commit / PR (awaiting user)

## Validation

```text
npm run web:test -- --run src/shared/hooks/useSwipePageTransition.test.tsx src/features/task/terminalTouchSelection.test.ts
→ 2 files, 17 tests passed
```
