PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Flip Web Cockpit Diff navigation gestures:

1. Task detail: swipe-left opens Diff (`onOpenDiff`); swipe-right goes back (`onBack` → dashboard/project).
2. Diff Review: swipe-right returns (`onBack`); swipe-left does not navigate.
3. Keep `onOpenDiff` / `onBack` in refs. No long-press.
4. Update unit tests, mobile-webkit e2e, and architecture comment.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskDetail.tsx`
- `crates/ajax-web/web/src/features/task/TaskDetail.test.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- `crates/ajax-web/web/src/shared/gestures/navigateSwipe.ts` (comments only)
- `crates/ajax-web/web/e2e/diff-review.test.ts`
- `crates/ajax-web/web/e2e/diff-review-swipe-repro.test.ts`
- `architecture.md`
- `.planning/agent-plans/diff-swipe-left-open.md`

## Forbidden changes

- Long-press reintroduction
- Changing navigateSwipe trigger/engage math
- Changing App.tsx routing
- Unrelated files, commits, pushes, branch changes
- Editing untracked `scripts/*` symlinks

## Acceptance

- TaskDetail: left swipe calls `onOpenDiff`; right swipe calls `onBack`; wrong direction does not call the wrong handler.
- DiffReview: right swipe calls `onBack`; left does not; hunk/chip targets still suppress back.
- Refs keep in-flight gestures alive across callback identity changes.
- e2e: swipe-left opens Diff; swipe-right from task detail does not open Diff.
- architecture.md says swipe-left opens Diff Review.

## Constraints

Smallest diff. Preserve touch capture / preventDefault over the terminal.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run navigateSwipe TaskDetail DiffReview
      expected: all pass
    - type: typecheck
      command: npm run web:check
      expected: exit 0
    - type: browser
      command: npm run web:smoke -- e2e/diff-review.test.ts e2e/diff-review-swipe-repro.test.ts
      expected: mobile-webkit left-open / right-not-open pass
  broader_checks: []
  reason: Focused unit + typecheck + mobile-webkit smoke cover gesture wiring.
```

## Stop if

Change would exceed ~400 lines, need App routing redesign, or require long-press.
