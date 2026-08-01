PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Stop Diff Review from painting a stale PR diff when the operator taps chips
quickly.

`DiffReview.tsx` load effect (~129-167) only uses a `cancelled` boolean. An
older `fetchTaskDiff` can resolve after a newer `selectedPr` / `handle` change
and call `setState` with the wrong hunks/chips.

Use a monotonic `loadSeq` (or AbortController) so only the latest in-flight
load may update state.

## Allowed files

- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`

## Forbidden changes

- Backend Diff routes / `run_optimistic` / ajax-core diff_review
- Swipe / navigateSwipe / useSwipePageTransition changes
- Hybrid PR→local fallback UI work (separate packet)
- Commits, pushes, branch changes

## Acceptance

1. Rapid `selectedPr` (or handle) changes cannot apply an older response after a newer load has started.
2. Focused unit/component test simulates overlapping fetches (resolve older after newer) and asserts the rendered/ready state matches the latest selection.
3. Existing DiffReview load / error / ready tests still pass.

## Constraints

- Smallest state-guard; prefer loadSeq over new libraries.
- Soft-fail PR list behavior stays as today.
- Estimated scope ≤ ~60 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run DiffReview
      expected: new stale-load test passes; existing DiffReview tests pass
  reason: Component test is the right proof for out-of-order async setState.
```

## Stop if

- Fix requires backend API changes
- Patch would exceed ~400 changed lines
