```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

After confirming Drop on a task detail page, navigating back to the dashboard before the undo window elapses must still commit the Drop API call (unless the operator taps Undo). Leaving the task must not silently cancel a confirmed Drop.

## Allowed files

- `crates/ajax-web/web/src/features/task/ActionBar.tsx`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx`
- `crates/ajax-web/web/src/shared/ui/ResultPanel.tsx`
- `crates/ajax-web/web/src/shared/ui/ResultPanel.test.tsx`

## Forbidden changes

- Do not change Drop confirmation UX (two-tap + toast Undo).
- Do not change `DROP_UNDO_MS` value.
- Do not edit backend operate/Drop Rust paths.
- Do not commit, push, merge, rebase, or change branches.
- Do not touch hotbar, terminal scroll, or CSS.

## Context evidence

1. **Desired behavior** — User: Drop from within a task should still drop after moving back to the dashboard.
2. **Source anchors** — `ActionBar.tsx` arms Drop with `dropTimerRef = setTimeout(commit, DROP_UNDO_MS)` and passes `{ onUndo, onCommit }` to App via `onResult`. Cleanup effect clears `dropTimerRef` on unmount (lines 60–65), cancelling commit when `TaskDetail` unmounts on back navigation. `ResultPanel.tsx` also commits via its own `setTimeout` calling `onCommit`, but the effect deps include `onCommit` and `onDismiss`; `App.tsx` passes `onDismiss={() => setResult(null)}` inline, so any App re-render resets that timer.
3. **Patterns** — `ActionBar.test.tsx` already covers delay/undo/dismiss with fake timers. `ResultPanel.test.tsx` covers `onCommit` after `DROP_UNDO_MS`. Extend those; do not invent a new toast system.
4. **Boundaries** — UI-only; Drop still goes through existing `postOperation`. Cockpit refresh via existing `onCockpit` in `run` when response includes cockpit.

## Code anchors

- `crates/ajax-web/web/src/features/task/ActionBar.tsx` — `useEffect` cleanup clearing `dropTimerRef`; `armDrop` / `commit` / `dropResolvedRef`
- `crates/ajax-web/web/src/shared/ui/ResultPanel.tsx` — auto-dismiss `useEffect` deps `[message, undoArmed, isError, onCommit, onDismiss]`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx` — existing Drop undo-window tests
- `crates/ajax-web/web/src/shared/ui/ResultPanel.test.tsx` — `auto-dismisses and calls onCommit after the undo window when armed`

## Test-first instructions

1. In `ActionBar.test.tsx`, add a test that:
   - Renders ActionBar with Drop + `onResult` + mocked `postOperation`
   - Confirms Drop (two taps)
   - Captures `onCommit` from `onResult` (optional) OR simply unmounts the component after confirm
   - Advances timers by `DROP_UNDO_MS`
   - Asserts `postOperation` was called with `action: "drop"` / `confirmed: true`
   - Name suggestion: `commits a pending Drop after unmount when the undo window elapses`

2. In `ResultPanel.test.tsx`, add a test that:
   - Renders ResultPanel with `onCommit` / `onDismiss` and undo-armed message
   - Rerenders with **new** `onCommit` / `onDismiss` function identities (same behavior)
   - Advances only `DROP_UNDO_MS` total from the first mount (not reset)
   - Asserts `onCommit` called once
   - Name suggestion: `keeps the undo commit timer across callback identity changes`

Red command:

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/features/task/ActionBar.test.tsx src/shared/ui/ResultPanel.test.tsx
```

Confirm the new tests fail for the expected reason before editing production code.

## Edit instructions

1. **ActionBar.tsx** — In the unmount cleanup, clear `confirmTimerRef` as today, but **do not** clear `dropTimerRef` when a Drop is still pending (`dropTimerRef.current` set and `!dropResolvedRef.current`). Allow the armed timeout to fire after unmount so `commit` → `run` still posts Drop. Guard `setRunningAction` / other setState in `run`/`undo`/`commit` with a mounted ref so unmounted setState is skipped; the API call must still run.

2. **ResultPanel.tsx** — Keep latest `onCommit` / `onDismiss` / `onUndo` in refs updated each render. Auto-dismiss `useEffect` must depend only on `message`, `undoArmed`, and `isError` (not callback identity), and invoke `onCommitRef.current` / `onDismissRef.current` when the timer fires. Manual Dismiss/Undo still call current refs.

3. Do not change public prop types.

## Verification commands

```bash
cd crates/ajax-web/web && npx vitest run --config vite.config.mts src/features/task/ActionBar.test.tsx src/shared/ui/ResultPanel.test.tsx
```

## Acceptance criteria

- Confirmed Drop still calls `postOperation` after ActionBar unmount once `DROP_UNDO_MS` elapses.
- Undo before elapsed still cancels (no API call).
- ResultPanel undo-armed timer does not reset when only callback identities change.
- Existing ActionBar / ResultPanel tests still pass.
- No edits outside allowed files.

## Stop conditions

- Need to change App.tsx Drop ownership or backend Drop semantics.
- New tests pass without production edits (false green).
- Diff touches hotbar, CSS, or terminal scroll files.
