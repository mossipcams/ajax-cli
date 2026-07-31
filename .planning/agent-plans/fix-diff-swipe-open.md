# Fix Diff Review swipe-right open

Mode: Small Fix.
Status: in progress.

## Diagnosis

1. `App` passes inline `onOpenDiff={() => ...}` so TaskDetail's touch `useEffect`
   rebinds on every cockpit poll and resets the in-flight long-press/swipe.
2. Long-press only armed on the first `touchmove` after the hold window; finger
   jitter >8px during hold cancelled arming on device.

## Delegation decision

`Delegation decision: not delegated because` focused regression with a known
root cause (callback identity + arming); smaller than a packet round-trip.

## Fix

- Keep `onOpenDiff` in a ref; mount touch listeners once.
- Arm with `setTimeout(NAVIGATE_LONG_PRESS_MS)`; cancel on early move/end.
- Reset swipe origin when the timer fires; raise cancel jitter to 16px.
- Mirror timer arming on DiffReview back gesture.

## Validation

```bash
npm run web:test -- --run navigateSwipe TaskDetail DiffReview
npm run web:check
```
