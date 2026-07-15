# TDD Implementation Packet: Xterm Terminal Surface V2 Spike

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Add an experimental Web Cockpit terminal surface using `@xterm/xterm@6.0.0` +
`@xterm/addon-fit@0.11.0`, gated by existing Dev settings **Terminal Surface V2**
(`ajax.terminal.surfaceV2`, default off). Ghostty remains the production path
when the flag is off. Reuse `connectTaskTerminal` from `terminalConnection.ts`.
On xterm init failure: show error + Retry; do **not** auto-fallback to Ghostty
while the flag stays on. Replace the current “no engine” unavailable banner.

## 3. Allowed files

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/vite.config.mts`
- `crates/ajax-web/web/TERMINAL.md`
- `architecture.md` (one short sentence only about experimental xterm behind Dev settings)
- `crates/ajax-web/web/src/components/XtermTerminalView.svelte` (new)
- `crates/ajax-web/web/src/components/XtermTerminalView.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.svelte`
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.test.ts`
- `crates/ajax-web/web/src/components/SettingsView.svelte`
- `crates/ajax-web/web/src/components/SettingsView.test.ts` (only if copy assertions need update)
- `crates/ajax-web/web/src/terminalPreload.ts`
- `crates/ajax-web/web/src/terminalPreload.test.ts`
- `crates/ajax-web/web/dist/*` (via `npm run web:build` only)
- `.planning/agent-plans/xterm-terminal-surface-v2-spike.md` (checklist/results)
- `.planning/packets/xterm-terminal-surface-v2-spike.md` (this file)

## 4. Forbidden changes

- Do not rewrite Rust PTY, tmux, WebSocket protocol, assets, or runtime routes
- Do not refactor `TerminalRawView.svelte` / Ghostty behavior
- Do not remove Dev settings layout, Surface V2 toggle, or `terminalSurfaceSetting.ts`
- Do not port Ghostty zero-lag overlay, selection-manager casts, private APIs, or full gesture stack
- Do not add xterm addons beyond `@xterm/addon-fit`
- Do not re-add `@wterm/*` or server CRLF history conversion
- Do not commit, push, merge, rebase, or change branches
- Do not touch unrelated files or drive-by formatting

## 5. Context evidence

### Graphify
`NOT_REQUIRED`: confined to Web Cockpit frontend terminal experiment; Ghostty
remains product default per `architecture.md` / `TERMINAL.md`.

### Serena
`NOT_REQUIRED`: anchors from direct source reads.

### ast-grep / code anchors
- Selector currently forces `SURFACE_V2_UNAVAILABLE` when `v2Enabled` — replace
  with dynamic `import("./XtermTerminalView.svelte")` + `onInitFailure`
- `terminalPreload.ts` returns `Promise.resolve([])` when V2 on — change to
  preload `XtermTerminalView` only
- `connectTaskTerminal(handle, events)` → `sendInput`, `sendResize`, `dispose`;
  `onOutput(text)` for PTY bytes decoded to string
- `MIN_TERMINAL_COLS` from `terminalGeometry.ts` (= 80)
- Vite `manualChunks`: add `/node_modules/@xterm/` and
  `/components/XtermTerminalView.svelte` to the `terminal` chunk (keep
  `terminalSurfaceSetting` out of that chunk)
- First wterm spike (git `5d43c98`) is the chrome reference: status line,
  CONTROL_KEYS toolbar (Esc/Tab/⌃C/arrows/Ctrl/Paste/hide-blur),
  `data-testid="task-terminal-panel"`, dispose on unmount — adapt to xterm APIs
- Pin exact: `"@xterm/xterm": "6.0.0"`, `"@xterm/addon-fit": "0.11.0"`
- CSS: `import "@xterm/xterm/css/xterm.css"`

## 6. Code anchors

### XtermTerminalView.svelte
Props: `{ handle: string; onInitFailure?: (message: string) => void }`

Must:
1. Host with `data-testid="task-terminal-panel"` and `data-terminal-engine="xterm"`
2. `new Terminal(...)`, `loadAddon(new FitAddon())`, `open(host)`, `fit()`;
   on constructor/open failure call `onInitFailure(message)` and do not open WS
3. `connectTaskTerminal` only after successful open
4. `onOutput` → `term.write(text)`
5. `term.onData` → `connection.sendInput` (with local Ctrl-arm toolbar modify
   like first wterm spike)
6. ResizeObserver / window resize → `fitAddon.fit()` then
   `sendResize(Math.max(term.cols, MIN_TERMINAL_COLS), term.rows)`
7. Mobile control-key toolbar (duplicate CONTROL_KEYS/ctrl-arm locally; do not
   extract from TerminalRawView)
8. Status line for connecting/reconnecting/unavailable + reconnect button
9. Unmount: `connection.dispose()`, `term.dispose()`, clear timers/observers
10. Keep spike intentionally smaller than Ghostty (no zero-lag, no private APIs)

### TerminalSurfaceSelector.svelte
When V2 on:
```svelte
{#key `${handle}:${remountToken}`}
  {#await import("./XtermTerminalView.svelte") then { default: XtermTerminalView }}
    <XtermTerminalView {handle} onInitFailure={handleInitFailure} />
  {/await}
{/key}
```
- `handleInitFailure(message)` sets `initError` and shows existing
  `data-testid="terminal-surface-v2-error"` banner with Retry
- When `initError` set, do not keep mounting a broken term (same pattern as
  post-wterm-stabilize: show error only, or remount on Retry)
- V2 off: Ghostty dynamic import unchanged
- Keep `display: contents`
- Never mount Ghostty and xterm together

### Preload
```ts
export function preloadXtermTerminalView(): Promise<unknown> {
  return import("./components/XtermTerminalView.svelte");
}
export function warmTerminalAssets(): Promise<unknown[]> {
  if (isTerminalSurfaceV2Enabled()) {
    return Promise.all([preloadXtermTerminalView()]);
  }
  return Promise.all([preloadGhosttyRuntime(), preloadTerminalView()]);
}
```

### Settings
Update note to: experimental xterm.js terminal for mobile bake-off.

### Docs
- `TERMINAL.md`: ownership row for XtermTerminalView; packages pinned; bake-off
  checklist (10 items: open, backspace hold, keyboard open/close, paste, select
  copy, scroll during output, rotate, Codex/Claude in tmux, alt-screen, toggle
  back to Ghostty)
- `architecture.md`: TaskDetail → TerminalSurfaceSelector; default Ghostty;
  experimental Surface V2 mounts xterm when enabled

## 7. Test-first instructions

Order:

### A. `XtermTerminalView.test.ts` (new; mock `@xterm/xterm`, `@xterm/addon-fit`, connection)
- `data-terminal-engine="xterm"`
- PTY `onOutput` reaches `term.write`
- `onData` reaches `sendInput`
- fit/resize reports `sendResize` with cols ≥ `MIN_TERMINAL_COLS`
- unmount calls `connection.dispose` and `term.dispose`
- init/open throw → `onInitFailure` called; no `connectTaskTerminal`

Red first (file exists with assertions; component missing or stub) then green.

### B. `TerminalSurfaceSelector.test.ts`
- Mock xterm view module or `@xterm/*` so V2 on mounts
  `[data-terminal-engine="xterm"]`
- Remove “no engine” unavailable expectations
- V2 off → Ghostty only
- Switch disposes previous connection
- Never both engines
- Init failure → error banner; Ghostty not loaded while flag on

### C. `terminalPreload.test.ts`
- V2 on → does not load Ghostty; preloads xterm view (assert import path or
  spy that warm results length 1 / not empty)

Run:
```bash
cd crates/ajax-web/web && npx vitest run \
  src/components/XtermTerminalView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts
```
Prove RED before production implementation for the new view tests.

## 8. Edit instructions

1. `npm install --save-exact @xterm/xterm@6.0.0 @xterm/addon-fit@0.11.0`
2. Write failing tests (section 7)
3. Implement `XtermTerminalView.svelte`
4. Wire selector + preload + vite chunks + Settings copy
5. Update TERMINAL.md + architecture one-liner
6. `npm run web:build`
7. Check off `.planning/agent-plans/xterm-terminal-surface-v2-spike.md`

## 9. Verification commands

```bash
cd /Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-revert
cd crates/ajax-web/web && npx vitest run \
  src/components/XtermTerminalView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts \
  src/components/SettingsView.test.ts \
  src/terminalSurfaceSetting.test.ts \
  src/components/TaskDetail.test.ts \
  src/components/TerminalRawView.test.ts
npm run web:check && npm run web:build
rg -n '@wterm' package.json package-lock.json crates/ajax-web/web/src || true
```

## 10. Acceptance criteria

- Flag off → Ghostty path unchanged (TerminalRawView tests green)
- Flag on → `data-terminal-engine="xterm"`; I/O + resize + cleanup covered
- Init failure → Surface V2 error + Retry; Ghostty not auto-mounted
- Exact xterm pins; no wterm deps
- web:check / focused vitest / web:build pass
- Docs updated; bake-off checklist present (unchecked)

## 11. Stop conditions

- Need Rust/PTY changes to make xterm work
- Temptation to port Ghostty zero-lag / private APIs
- web:build fails due to chunk cycles — stop and report (do not put
  `terminalSurfaceSetting` into the terminal chunk)
