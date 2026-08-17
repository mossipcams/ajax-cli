# Close open defects — 2026-08-17

**Mode:** Behavior change (bounded Web Cockpit defects)  
**Approval:** User asked to plan then delegate. Waves 1–3 dispatched without a further gate.  
**Tracking:** GitHub issues on `mossipcams/ajax-cli` only. Each fix PR must `Fixes #N`.

## Inventory (12 open)

| Issue | Sev | Cluster | This session |
| --- | --- | --- | --- |
| #810 slash-only `#/t/` handle | low | A routes | Wave 1 |
| #821 whitespace-only `#/p/%20` | low | A routes | Wave 1 |
| #811 nested `…/extra/diff` | low | A routes | Wave 1 |
| #859 nested `/diff/extra` | medium | A routes | Wave 1 |
| #855 late task-start overrides nav | medium | B start | Wave 2 |
| #861 404 task marks disconnected | medium | C 404 | Wave 3 |
| #908 empty main on cold 404 | low | C 404 | Wave 4 (after 3) |
| #860 trailing `/diff/` DiffReview chrome | low | C 404 / DiffReview | Wave 4 |
| #896 Resume 200 ok no-op | medium | D resume | later — product choice |
| #893 permission replay | high | E session | closed — already fixed by #894 |
| #877 sticky session composer | high | F iOS layout | later |
| #873 Test in Stable kills :8787 | high | G stable restart | later (#874 closed unmerged) |

## Non-goals

- Architecture / lifecycle / registry changes
- Mixing with the dirty model-router skill files already in this worktree
- Commits, pushes, or PRs until the parent reviews and the user asks
- Hiding Resume globally (task-open still POSTs resume to `mark_task_opened`)

## Wave 1 — `parseRoute` (#810, #821, #811, #859)

**Contract:** a task or project hash uses one encoded path segment. Literal extra `/` is a route suffix, not part of the handle. Encoded `%2F` in that one segment remains a valid `project/task` handle.

| Hash | Result |
| --- | --- |
| `#/t/web%2Ffix-login` | task `web/fix-login` |
| `#/t/web%2Ffix-login/diff` | diff |
| `#/t/web%2Ffix-login/diff/` | diff (already normalized) |
| `#/t/web%2Ffix-login/extra/diff` | dashboard (#811) |
| `#/t/web%2Ffix-login/diff/extra` | dashboard (#859) |
| `#/t//`, `#/t/%2F` | dashboard (#810) |
| `#/t/%20` | dashboard (whitespace handle) |
| `#/p/%20`, `#/p/%20%20` | dashboard (#821) |
| `#/p/my%20repo` | project `my repo` (unchanged) |

**Files:** `crates/ajax-web/web/src/shared/lib/routes.ts`, `routes.test.ts`  
**Verify:** `npm run web:test -- --run src/shared/lib/routes.test.ts` then `npm run web:check`

## Wave 2 — late Start must not steal the route (#855)

Settings unmounts New Task (`sheetAllowed` is dashboard/project only), but the in-flight `startTask` still calls `onOpenTask` → `openTask` after unmount.

**Fix:** mounted (or aborted) guard in `NewTaskSheet`. On success after unmount: still call `onCockpit` if a cockpit payload arrived; do **not** call `onOpenTask` / `onClose`.

**Files:** `NewTaskSheet.tsx`, `NewTaskSheet.test.tsx`  
**Verify:** focused NewTaskSheet tests including a delayed `startTask` + unmount before resolve.

## Wave 3 — task 404 is not a disconnect (#861)

`useTaskDetailResource` calls `applyConnectionError` for every `ApiError`, so `GET /api/tasks/:missing` → 404 becomes `disconnected: HTTP 404` and survives `history.back` until the next cockpit poll.

**Fix:** do not apply connection error for HTTP 404 (or other task-not-found). Keep 5xx / network / stale-session as connection failures. 404 still sets detail `status: "error"` so `TaskLoadError` renders.

**Files:** `useTaskDetailResource.ts`, `useTaskDetailResource.test.tsx`  
**Verify:** focused hook tests; do not weaken the existing 503 → disconnected App test.

## Later waves (not dispatched yet)

- **Wave 4:** #908 (prompt loading or error on cold missing-task; skeleton vs empty `main`) and #860 (DiffReview chrome for missing task). May touch `App.tsx` / DiffReview.
- **#896:** Web Resume uses `OpenMode::NoAttach` → empty plan → 200 + empty output. Do not change attach semantics. Prefer a visible non-success explanation and/or not offering Resume on a task the operator is already viewing. `resumeOnOpen` must keep working.
- **#893:** PR #894 merged (`Fixes #893`) but the issue is still open. Verify current `sessionReducer` + reconnect, then close or reopen with a remaining repro.
- **#877:** lift session composer out of `.session-thread` sticky scroller (iOS). High, layout-sensitive.
- **#873:** #874 closed unmerged. `schedule_test_in_stable` still `process::exit(0)` after wrapper spawn. High, process/lifecycle; needs its own plan.

## Checklist

- [x] T1 Wave 1 routes + tests (#810 #821 #811 #859)
- [x] T2 Wave 2 NewTaskSheet unmount guard (#855)
- [x] T3 Wave 3 skip connection error on task 404 (#861)
- [x] T4 Parent review of actual diffs; focused verification
- [ ] T5 Wave 4 / remaining issues after T4
- [ ] T6 PRs with `Fixes #N` when the user asks to ship

## Validation (T1–T3)

```
npm run web:test -- --run src/shared/lib/routes.test.ts src/features/task/NewTaskSheet.test.tsx src/features/task/useTaskDetailResource.test.tsx
# 63 passed (parent re-run)
```

Parent accepted the three diffs. They stay uncommitted until a ship request. Do not mix with dirty model-router skill files.

## Stop conditions

Stop and ask before changing task lifecycle, registry truth, Resume/NoAttach attach semantics, Test-in-Stable process model, or public operate contracts beyond returning a clearer Resume result.
