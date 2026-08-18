# Plan: Ajax chat Drop confirm and dismiss

## Scope

Restore Drop on `#/session/<handle>` (Ajax chat) so it matches the terminal
task page: usable Confirm toast, then Drop execute, then dashboard dismiss.

GitHub: [#947](https://github.com/mossipcams/ajax-cli/issues/947).

Non-goals: dashboard inline Drop pointer intercept (#926), git
`core.bare`/`core.worktree` Drop failure (#941), ACP
`TaskSessionDirectory::drop_session` on operate Drop (follow-up unless the UI
path still fails after this change).

## Root causes (inspected)

1. Session head ActionBar filters `destructive`, so Drop exists only in the
   details sheet (`FullscreenLayer` z-index 50). App `ResultPanel` is z-index
   40 and is covered; a second Drop tap is click-latched.
2. `stillOnDroppedTask`, the drop leave latch, and
   `cancelPendingConfirmOnRouteChange` treat only `task` and `diff` as staying
   on the dropped task. `#/session/<handle>` looks like a leave, so confirm can
   cancel and successful Drop never `go(#/)`.
3. SessionChat ActionBars do not receive `pendingConfirmAction` /
   `onCancelPendingConfirm` (TaskDetail does).

## Approach

- Treat `session` as a task surface wherever Drop confirm/leave/dismiss decide
  “still on this handle”.
- Close the session details sheet when Drop confirm arms so the existing
  ResultPanel is usable. Do not raise ResultPanel above NewTaskSheet.
- Pass pending-confirm props through SessionChat ActionBars.
- Sit the result toast on the session home-indicator band (nav is hidden).

## Approval

User reported the defect and wants it fixed — authorized to implement.

## Task checklist

### T1 — session is still-on-dropped-task

- [x] `task` | `diff` | `session` + same handle in leave latch,
      `stillOnDroppedTask`, and confirm-cancel
- [x] Staying on `#/session/<handle>` keeps confirm; leaving cancels

### T2 — usable Confirm from session details

- [x] Opening Drop from the details sheet closes the sheet (or otherwise
      exposes Confirm) without raising ResultPanel over NewTaskSheet
- [x] SessionChat ActionBars get `pendingConfirmAction` /
      `onCancelPendingConfirm`
- [x] Session result-panel bottom offset when nav is hidden

### T3 — regression tests (#947)

- [x] App-level: Drop from `#/session/<handle>` shows Confirm, Confirm posts
      drop after the undo window, dismisses to dashboard
- [x] Confirm remains while staying on the same session handle
- [x] Navigating away from the session during confirm cancels (no POST)

## Validation

```bash
rtk npm run web:test -- --run src/app/App.drop-confirm.test.tsx src/features/session/SessionChat.test.tsx
```

Result: pass — `npx vitest --config crates/ajax-web/web/vite.config.mts --run src/app/App.drop-confirm.test.tsx src/features/session/SessionChat.test.tsx` (38 tests, 0 failures). (`rtk npm run web:test` equivalent after `npm install`.)

## Deviations

None.
