# TDD Implementation Packet — R9 wterm visualViewport debounce resize (iOS)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: when `visualViewport` changes (iOS keyboard animation /
address bar), refit/notify immediately on the local side as needed but
**debounce** PTY `sendResize` via existing `createRefitScheduler` +
`RESIZE_DEBOUNCE_MS`. Convert
`it.todo("refits immediately but debounces server resize when the visual viewport changes")`.

## Hard gates
- WtermTerminalView* only; mobile Safari WebKit
- Never TerminalRawView
- Reuse `createRefitScheduler` from `terminalRefit.ts` — do not reinvent debounce

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No 80-col floor (R6) / copy fallback in this round
- No TerminalRawView / terminalRefit.ts API changes unless proven necessary
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- `createRefitScheduler({ fit, sendResize })` — `scheduleDebounced()` fits next
  frame, debounces sendResize 100ms
- Ghostty test stubs `visualViewport` and dispatches resize events
- Wterm `reportResize` already respects keyboard freeze (R7) and expand intent (R15)

## Code anchors
- onMount: create scheduler; listen to `visualViewport` `resize`/`scroll`
  with `scheduleDebounced`; dispose on cleanup
- For wterm, `fit` can be a no-op (autoResize owns local grid) OR a light
  host measure — prefer no-op fit + debounced `reportResize(term.cols, term.rows)`
  so bursts collapse to one PTY SIGWINCH

## Test-first instructions
1. Convert R9 todo (mirror Ghostty ~973):
   - Stub visualViewport with addEventListener/dispatch
   - Mount; clear sendResize
   - Dispatch several visualViewport resize events in quick succession
   - Advance timers less than RESIZE_DEBOUNCE_MS → sendResize not yet flushed
     (or only fit path)
   - Advance past debounce → exactly one sendResize
2. Use fake timers carefully with rAF (repo already stubs rAF in beforeEach to
   run sync — read existing beforeEach; may need to adjust for this test only)
3. RED → GREEN

## Edit instructions
1. Import `createRefitScheduler` (and `RESIZE_DEBOUNCE_MS` if asserting in test).
2. Wire visualViewport listeners → `scheduleDebounced`.
3. Dispose scheduler + listeners on unmount.
4. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- R9 todo green
- Keyboard freeze still works (no spray during open if policy blocks —
  debounced sendResize should still call reportResize which gates)
- RED→GREEN; V2-only

## Stop conditions
- Editing TerminalRawView
- Implementing 80-col floor in same round
- Breaking R7/R8
