# Double-tap-hold terminal copy selection

## Scope

Add a Web Cockpit terminal gesture: **double-tap, then hold and drag** to
highlight a text range and use the existing Copy overlay.

## Non-goals

- Changing single long-press word select
- Changing single long-press → directional arrow drag
- Changing pinch-zoom, paste, clipboard helpers, or backend/PTY contracts
- Native iOS loupe / system selection handles
- Architecture or registry changes

## Desired behavior

1. Quick tap, then a second tap nearby within a short window starts a
   selection-drag gesture.
2. On that second contact, select the word under the finger (same word rules as
   today's long-press).
3. While still holding, drag extends the xterm selection from the anchor cell to
   the current cell via `term.select(...)`.
4. Lift keeps the selection and shows the existing Copy control.
5. While selection-drag is active, do **not** arm directional arrow keys and do
   **not** send PTY input for the drag.
6. Single-finger long-press (≥500ms) without a preceding quick tap still
   word-selects on lift; post-hold drag still becomes directional arrows.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Create READY compact packet
- [x] Route and dispatch implementation
- [x] Review delegate diff against acceptance
- [x] Run parent validation (unit + focused e2e / existing long-press tests)
- [x] Update this ledger with results

## Validation

Parent-run results (2026-08-04):

```bash
npm --prefix crates/ajax-web/web exec -- vitest run \
  src/features/task/terminalTouchSelection.test.ts \
  src/features/task/TaskTerminal.test.ts
# → 42 passed

npm run web:smoke -- e2e/terminal-behavior.test.ts \
  -g 'long press on known output text selects word|double tap hold drag selects range'
# → 2 passed (mobile-webkit)
```

`mountTaskTerminalSession.ts` LOC: 929 (≤1000).

## Deviations

- Round 1 delegate transport failed with sandbox `Operation not permitted` after writing
  files; work was recovered from the tree and completed via Cursor resume.
- Resume returned a non-schema report; parent re-ran verification and ACCEPTed.
- E2e helper uses one-evaluate re-measure after a focus tap to avoid layout-shift
  stale coordinates (required for green double-tap test).

## Approval

Not required — Behavior Change, user asked to implement.

## PR gate

- `npm run verify` — passed (exit 0) before commit/PR
- Commit goes through husky (`web:build` + verify + release build/install)
