# Xterm Terminal Surface V2 Spike

## Scope

Wire experimental `@xterm/xterm` + `@xterm/addon-fit` as Terminal Surface V2
behind the existing Dev settings toggle (`ajax.terminal.surfaceV2`). Ghostty
remains default when the flag is off. Reuse `connectTaskTerminal`. Replace the
current V2 unavailable banner with `XtermTerminalView`.

## Non-goals

- Production migration away from Ghostty
- Porting Ghostty zero-lag, selection-manager casts, private API patches, or full gesture stack
- Extra xterm addons beyond Fit
- Server-side CRLF history conversion
- Auth / registry / CLI / Rust PTY changes
- Commit / push / branch changes

## Delegation decision

`Delegation decision: delegated via model-router` — frontend UI multi-file spike
→ `cursor-delegate` / `composer-2.5` with READY packet
`.planning/packets/xterm-terminal-surface-v2-spike.md`.

## Task checklist

- [x] Exact deps `@xterm/xterm@6.0.0` + `@xterm/addon-fit@0.11.0`
- [x] Failing selector/preload/XtermTerminalView tests first
- [x] `XtermTerminalView.svelte` (+ mocked tests): I/O, resize, toolbar, cleanup, `data-terminal-engine="xterm"`
- [x] Selector mounts xterm when V2 on; error+Retry on init failure; no Ghostty auto-fallback while flag on
- [x] Preload xterm chunk when V2 on (never Ghostty while V2 on)
- [x] Vite terminal chunk includes `@xterm` + `XtermTerminalView`
- [x] Settings copy + TERMINAL.md bake-off + architecture one-liner
- [x] Parent: web:check, focused vitest, web:build

## Phase B design (locked)

| Piece | Behavior |
| --- | --- |
| Deps | Exact `@xterm/xterm@6.0.0`, `@xterm/addon-fit@0.11.0`; CSS `@xterm/xterm/css/xterm.css` |
| View | Terminal + FitAddon; `connectTaskTerminal`; `onData`→`sendInput`; WS→`term.write`; fit→`sendResize` with `MIN_TERMINAL_COLS`; dispose on unmount |
| Selector | Dynamic import `XtermTerminalView`; `onInitFailure` → error banner + Retry; no Ghostty while flag on |
| Preload | V2 on → preload XtermTerminalView only |
| Docs | TERMINAL.md ownership + 10-item iPhone bake-off; architecture says experimental engine is xterm behind Dev settings |

## Deviations

- `sessionStorage` last-error write on init failure (matches existing Dev settings debug panel; not in packet but preserves prior wterm behavior)

## Validation

```bash
npm install --save-exact @xterm/xterm@6.0.0 @xterm/addon-fit@0.11.0
cd crates/ajax-web/web && npx vitest run \
  src/components/XtermTerminalView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts \
  src/components/SettingsView.test.ts \
  src/terminalSurfaceSetting.test.ts \
  src/components/TaskDetail.test.ts \
  src/components/TerminalRawView.test.ts
npm run web:check && npm run web:build
```

## Results

### RED (pre-implementation)

```bash
cd crates/ajax-web/web && npx vitest run \
  src/components/XtermTerminalView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts
# 3 failed files, 6 failed tests (XtermTerminalView missing; selector still shows unavailable banner; preload returns [])
```

### GREEN (focused)

```bash
cd crates/ajax-web/web && npx vitest run \
  src/components/XtermTerminalView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts
# 3 passed, 17 passed
```

### VERIFY — parent Review Gate

- Focused vitest (7 files): **202 passed**
- `npm run web:check`: **0 errors/warnings**
- `npm run web:build`: **pass** (`terminal.js` ~1.61 MB with xterm)
- `rg @wterm`: **clean**
- GATE: **ACCEPT**
