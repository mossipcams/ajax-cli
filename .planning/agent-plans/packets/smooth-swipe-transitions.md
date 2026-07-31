PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Implement finish-the-slide swipe page transitions for Web Cockpit:

1. Finger-follow drag up to viewport width (not 96px rubber-band).
2. Past trigger on release: animate off-screen (`±width`), then navigate.
3. Below trigger: spring back to 0.
4. One-shot CSS enter animation on the next outlet via short-lived direction flag.
5. Button/bottom-nav navigations stay instant (no enter flag).
6. Directions unchanged: Task left→Diff / right→onBack; Diff right→onBack.
7. Diff hunk/chip pans still block back gesture.

## Allowed files

- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.ts`
- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.test.ts`
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.ts` (new)
- `crates/ajax-web/web/src/shared/hooks/useSwipePageTransition.test.tsx` (new)
- `crates/ajax-web/web/src/shared/lib/swipeEnter.ts` (new — flag helpers)
- `crates/ajax-web/web/src/shared/lib/swipeEnter.test.ts` (new)
- `crates/ajax-web/web/src/features/task/TaskDetail.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- `crates/ajax-web/web/src/app/App.tsx`
- `crates/ajax-web/web/src/app/App.test.tsx` (only if needed for enter class)
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/e2e/diff-review.test.ts`
- `crates/ajax-web/web/e2e/diff-review-swipe-repro.test.ts`
- `architecture.md`
- `.planning/agent-plans/smooth-swipe-transitions.md`

## Forbidden changes

- Dual-mounted peek / View Transitions API
- Changing swipe directions or reintroducing long-press
- Animating bottom-nav / Settings / New-task sheet
- Commits, pushes, branch changes
- Editing untracked `scripts/*` symlinks

## Acceptance

- During drag, `translateX` follows finger beyond 96px up to page width.
- Commit: animate to `±width`, then fire nav callback (not before).
- Cancel: animate to 0; no navigation.
- Before swipe nav, set enter flag; App outlet gets `ajax-swipe-enter-left|right` and clears after animationend.
- Diff pan targets still suppress back.
- Existing unit + mobile-webkit smoke pass (timeouts may need +300ms for commit anim).

## Constraints

- Keep math pure in `navigateSwipe.ts` (pass width in).
- Callback refs so cockpit polls do not remount listeners.
- Smallest diff. Match existing hook patterns (`useSheetDrag` / `useSwipeReveal`).
- Commit transition ~220ms with `--ease-spring` / existing `--ease`.
- Module or sessionStorage flag for enter direction; clear on read in App.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run navigateSwipe useSwipePageTransition swipeEnter TaskDetail DiffReview
      expected: all pass
    - type: typecheck
      command: npm run web:check
      expected: exit 0
    - type: browser
      command: npm run web:smoke -- e2e/diff-review.test.ts e2e/diff-review-swipe-repro.test.ts
      expected: mobile-webkit pass with commit animation delay
  broader_checks: []
  reason: Focused unit + typecheck + existing smoke cover gesture wiring and route open.
```

## Stop if

Would exceed ~400 changed lines, need dual-mount stack, or change hash routing model.
