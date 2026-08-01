# Packet: Back buttons reuse swipe exit+enter

PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

```yaml
dispatch_level: compact
estimated_changed_lines: 120
estimated_files: 6
```

## Goal

Task detail **← Back** and Diff Review **← Back** must finish like swipe-right:
slide the current page off to the right, set swipe-enter direction `"right"`,
then invoke the existing `onBack` navigation. Do not navigate instantly on click.

## Allowed files

- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.test.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- `architecture.md` (only the Web Cockpit sentence about swipe vs button nav)

## Forbidden changes

- App.tsx hash routing redesign, bottom-nav, open-task enter (T2), sheet dismiss (T3)
- ActionBar / Drop / ResultPanel
- Terminal gesture / Diff pan ignore rules beyond what Back needs
- Commits, pushes, branch changes
- Broad CSS refactors; reuse existing `--ease-spring` / `SWIPE_PAGE_COMMIT_MS`

## Code anchors

- `useSwipePageTransition.ts` `animateTo` (~63–90) + touch-end left/right commit (~127–141)
- `setSwipeEnterDirection` already called inside `animateTo` when direction is set
- `TaskDetail.tsx` Back: `onClick={() => onBack?.()}` (~59) — must use programmatic commit instead
- `DiffReview.tsx` Back: same pattern (~296)
- `architecture.md` ~660–662: “button and bottom-nav navigations stay instant”

## Implementation sketch

1. Extend `useSwipePageTransition` to return a stable `commit(direction: "left" | "right")` that runs the same `animateTo(offset, direction, thenNavigate)` path as a successful swipe (no-op if that side has no handler).
2. TaskDetail: Back click → `commit("right")` (requires `onRight` / onBack wired as today).
3. DiffReview: Back click → `commit("right")`.
4. Keep swipe behavior unchanged.
5. Amend architecture.md: swipe-parallel button Back uses the same exit+enter contract; other chrome navigations may remain instant (T2 owns open-task).

## Acceptance criteria

1. Clicking TaskDetail ← Back does **not** call `onBack` synchronously; after `SWIPE_PAGE_COMMIT_MS` (+ small slack), `onBack` fires once and `setSwipeEnterDirection` was called with `"right"`.
2. Clicking DiffReview ← Back same as (1).
3. Existing left/right swipe tests still pass (commit still leaves transform off-screen until unmount).
4. Double-click Back during settle does not double-navigate (ignore while settling/dragging, or equivalent guard).
5. `architecture.md` no longer claims all button navigations stay instant; wording matches swipe-parallel Back.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk npm run web:test -- --run src/shared/hooks/useSwipePageTransition.test.tsx src/features/task/TaskDetail.test.tsx src/features/diff/DiffReview.test.tsx
      expected: all pass including new Back-button commit tests
    - type: typecheck
      command: rtk npm run web:check
      expected: exit 0
  broader_checks: []
  reason: Behavior is hook + two Back wiring sites; focused unit tests cover sync vs animated commit.
```

## Stop conditions

- Fix seems to require App-level route transition stack
- Diff pan / terminal swipe ownership regresses
- Patch exceeds ~400 changed lines or Allowed files
- architecture.md edits beyond the one navigation sentence
