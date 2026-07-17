# React migration S4 — New-task sheet + FullscreenLayer

Source of truth: `docs/react-migration-plan.md` §S4.

Status: in progress on branch `ajax/react-s4` (from `origin/main` after S3 #575 merged).

Delegation decision: delegated via model-router.

## Decision (recorded)

**Bespoke sheet** — do **not** introduce shadcn/Radix `Sheet`/`Drawer`. Radix portals to `body` and would escape `--app-height` band pinning. Keep `FullscreenLayer` + form markup parity.

## Scope

- Port `FullscreenLayer` → React (`children` via props).
- Port `sheetDragAction` → `useSheetDrag` (same passive touch flags); keep pure `sheetDrag.ts`.
- Port `NewTaskSheet` → React; island-swap App consumer.
- Move FullscreenLayer + NewTaskSheet scoped CSS into `styles.css` verbatim.
- Delete Svelte files + `sheetDragAction` (+test); update `keyboardBandPin.test.ts` / source-contract tests to read CSS from `styles.css`.

## Non-goals

- No `viewport.ts` changes.
- No shadcn dependency.
- No terminal / TaskDetail / shell migration.
- Compact Test-in-Dev remains stashed.

## Task checklist

- [x] **Task 1 — FullscreenLayer + useSheetDrag + NewTaskSheet + App swap (single implement round).**
  - Test first: `useSheetDrag` unit tests; port `NewTaskSheet.test.ts` → RTL; update keyboardBandPin fullscreen source to `styles.css`.
  - Implement: React components + hook; App island; CSS move; deletions.
  - Verify: focused unit + `actions` new-task e2e + `layout-scroll` sheet band e2e.

## Validation results

Task 1 (delegate round):

```bash
# RED (missing implementations)
npm run web:test -- --run crates/ajax-web/web/src/react/useSheetDrag.test.tsx crates/ajax-web/web/src/components/NewTaskSheet.test.tsx
# EXIT_CODE: 1 — Failed to resolve import "./useSheetDrag" / "./NewTaskSheet"

# GREEN (focused unit)
npm run web:test -- --run crates/ajax-web/web/src/react/useSheetDrag.test.tsx crates/ajax-web/web/src/components/NewTaskSheet.test.tsx crates/ajax-web/web/src/components/keyboardBandPin.test.ts crates/ajax-web/web/src/components/App.test.ts
# EXIT_CODE: 0 — 59 passed

npm run web:check
# EXIT_CODE: 0 — svelte-check found 0 errors

npm run web:smoke -- crates/ajax-web/web/e2e/actions.test.ts crates/ajax-web/web/e2e/layout-scroll.test.ts
# EXIT_CODE: 0 — 32 passed (new-task cancel/start + sheet band green)
```

- [x] **Task 2 — full automated validation.**
  - PASS — parent 2026-07-17: web:test **327**; smoke **115**; ajax-web **159**; verify **1628**; serviceWorker **0**.

- [x] **Task 3 — on-device + PR.**
  - Matt: keyboard band, drag-dismiss, submit — **validated 2026-07-17**.
  - PR: https://github.com/mossipcams/ajax-cli/pull/577 (opened 2026-07-17). CI/review/merge/baseline-restore still pending.

## Escalate instead of guessing

- Any `viewport.ts` edit.
- Any e2e weakening.
- Introducing Radix portal / shadcn Sheet.

## Deviations

- Added explicit `(handle: string)` type on `onOpenTask` in `App.svelte` ReactIsland props to satisfy svelte-check.
