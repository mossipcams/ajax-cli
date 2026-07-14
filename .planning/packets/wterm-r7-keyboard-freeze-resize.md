# TDD Implementation Packet — R7 wterm keyboard freeze PTY resize (iOS Safari)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: while iOS soft keyboard is open (`isKeyboardOpen()` /
`html.keyboard-open`), withhold PTY `sendResize` so wterm `autoResize` local
grid changes do not SIGWINCH tmux mid-animation. Convert
`it.todo("freezes the local grid while the keyboard is open so it stays in lockstep with the PTY")`
into a passing test.

Reuse `createTerminalLayoutPolicy` + `isKeyboardOpen` (same truth as Ghostty).
Do **not** implement flush-on-close in this round (R8).

## Hard gates
- `ajax.terminal.surfaceV2` / `WtermTerminalView*` only
- Mobile Safari WebKit first — this is the iOS keyboard lockstep core
- Never edit `TerminalRawView.svelte`

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No flush-on-close yet (leave that todo)
- No safe-area / snap / fullscreen focus changes
- No `viewport.ts` / `terminalLayoutPolicy.ts` API changes unless a tiny
  export is proven necessary (prefer consume existing APIs)
- No commit/push/branch

## Context evidence
- Graphify: `NOT_REQUIRED`
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- `isKeyboardOpen()` in `viewport.ts` reads `html.keyboard-open`
- `createTerminalLayoutPolicy().setKeyboardOpen(open)` → `allowPtyResize: !keyboardOpen || intent`
- Ghostty `TerminalRawView.svelte` `sendResize` gates on `decision.allowPtyResize`
- Wterm today: `onResize: (cols, rows) => reportResize(cols, rows)` always sends

## Code anchors
- `reportResize` / `onResize` in `WtermTerminalView.svelte`
- Create `layoutPolicy = createTerminalLayoutPolicy()` in `onMount`, dispose on cleanup
- Gate: `const decision = layoutPolicy.setKeyboardOpen(isKeyboardOpen()); if (!decision.allowPtyResize) return;` before `connection.sendResize`

## Test-first instructions
1. Convert the freeze todo into a real test:
   - Mount view; wait for connection
   - Add `keyboard-open` to `document.documentElement`
   - Clear `sendResize` mock; fire `termOnResize!(40, 20)` (or whatever the mock exposes)
   - Expect `sendResize` **not** called
   - Remove `keyboard-open`; fire resize again → **is** called (optional in this round; can wait for R8 if flush differs — at minimum assert withhold while open)
2. RED: `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`

## Edit instructions
1. Import `isKeyboardOpen` from `../viewport` and `createTerminalLayoutPolicy` from `../terminalLayoutPolicy`.
2. Gate `reportResize` with layout policy as above.
3. Dispose policy on unmount if it has `dispose()`.
4. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Freeze todo green; flush todo still todo
- V2-only; RED→GREEN proven

## Stop conditions
- Editing TerminalRawView or viewport hysteresis constants
- Implementing flush + safe-area + snap in the same round
