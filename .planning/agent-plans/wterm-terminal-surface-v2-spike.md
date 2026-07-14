# Wterm Terminal Surface V2 Spike

## Scope

Add an experimental Web Cockpit terminal surface using `@wterm/dom` +
`@wterm/ghostty`, gated by a persistent Settings toggle. Ghostty remains the
default and fallback. Reuse `terminalConnection.ts`. Do not rewrite PTY/tmux/WS.

## Non-goals

- Production migration away from Ghostty
- xterm.js
- wterm built-in WebSocketTransport
- Porting Ghostty monkeypatches / zero-lag / private-API workarounds into wterm
- Refactoring `TerminalRawView.svelte`
- Rust protocol or architecture ownership changes

## Delegation decision

`Delegation decision: delegated via model-router` — frontend UI behavior →
`cursor-delegate` / `composer-2.5`.

## Task checklist

- [x] Add exact deps `@wterm/dom@0.3.0` and `@wterm/ghostty@0.3.0`
- [x] `terminalSurfaceSetting.ts` (+ tests): default off, persist, notify
- [x] Settings Experimental toggle UI (+ tests)
- [x] `WtermTerminalView.svelte` (+ mocked tests): I/O, resize, toolbar, cleanup, `data-terminal-engine="wterm"`
- [x] `TerminalSurfaceSelector.svelte` (+ tests): choose surface, single connection, dispose on switch, fallback
- [x] Wire `TaskDetail` through selector
- [x] Update `TERMINAL.md` ownership + iPhone bake-off checklist
- [x] Vite terminal chunk includes wterm packages
- [x] Verify: web:check, focused vitest, web:build

## Validation

```bash
npm install --save-exact @wterm/dom@0.3.0 @wterm/ghostty@0.3.0
npm run web:check
npm run web:test -- --run src/terminalSurfaceSetting.test.ts src/components/SettingsView.test.ts src/components/TerminalSurfaceSelector.test.ts src/components/WtermTerminalView.test.ts src/components/TerminalRawView.test.ts src/components/TaskDetail.test.ts
npm run web:build
```

### Results

- `npm install --save-exact @wterm/dom@0.3.0 @wterm/ghostty@0.3.0` — exit 0
- `npm run web:check` — exit 0 (0 svelte-check errors)
- focused vitest — exit 0 (192 tests passed)
- `npm run web:build` — exit 0

## Deviations

- `TerminalSurfaceSelector` init-failure fallback keeps the V2 setting enabled
  in localStorage (session-only `fallbackToGhostty`); operator can retry from
  Settings without re-enabling.
- Parent review: hide-keyboard control blurs focus (matches Ghostty) instead of
  permanently hiding the toolbar.
- Parent review: device checklist aligned to the requested 10 bake-off items.
- Parent gate re-ran `web:check`, focused vitest (192), and `web:build` — all exit 0.

## Approval

User authorized autonomous implementation of this experimental spike.
