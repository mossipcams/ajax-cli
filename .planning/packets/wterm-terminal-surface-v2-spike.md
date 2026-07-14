# TDD Implementation Packet: Wterm Terminal Surface V2 Spike

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Add an experimental Web Cockpit terminal surface (`@wterm/dom` + `@wterm/ghostty`)
behind a Settings toggle labeled **Terminal Surface V2**. Default remains Ghostty.
Reuse `connectTaskTerminal` from `terminalConnection.ts`. Do not use wterm's
`WebSocketTransport`. Fall back to Ghostty if wterm init fails. Preserve all
Ghostty behavior when the experiment is off.

## 3. Allowed files

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/vite.config.mts`
- `crates/ajax-web/web/TERMINAL.md`
- `crates/ajax-web/web/src/terminalSurfaceSetting.ts` (new)
- `crates/ajax-web/web/src/terminalSurfaceSetting.test.ts` (new)
- `crates/ajax-web/web/src/components/SettingsView.svelte`
- `crates/ajax-web/web/src/components/SettingsView.test.ts`
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte` (new)
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.svelte` (new)
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.test.ts` (new)
- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- `.planning/agent-plans/wterm-terminal-surface-v2-spike.md`

## 4. Forbidden changes

- Do not rewrite Rust PTY, tmux, or WebSocket protocol code
- Do not refactor `TerminalRawView.svelte` (leave Ghostty path unchanged)
- Do not use `@wterm` `WebSocketTransport` or any wterm built-in WS client
- Do not migrate to xterm.js
- Do not port Ghostty monkeypatches, zero-lag overlay, selection-manager casts, or private-API workarounds into wterm
- Do not create a custom VT parser/renderer
- Do not change `architecture.md` in this spike
- Do not touch unrelated files, formatting sweeps, or drive-by cleanup
- Do not commit, push, merge, rebase, or change branches

## 5. Context evidence

### Graphify
`NOT_REQUIRED`: spike is confined to Web Cockpit frontend modules already
documented in `TERMINAL.md` / `architecture.md` Web Cockpit terminal section;
no cross-crate ownership change.

### Serena
`NOT_REQUIRED`: anchors collected via direct source reads below.

### ast-grep / code anchors
- `TaskDetail.svelte` L67-70 currently dynamic-imports `TerminalRawView`
- `SettingsView.svelte` has sections but no Experimental toggles yet
- `terminalConnection.ts` exports `connectTaskTerminal` with `sendInput`,
  `sendResize` (`JSON.stringify({ type: "resize", cols, rows })`), `dispose`
- `TerminalRawView.svelte` sets `data-terminal-engine="ghostty"` (or placeholder)
- Persistence pattern: `localStorage` keys like `ajax.terminal.fontSize` in
  `terminalGeometry.ts` (try/catch for Safari private mode)
- Deps live in repo-root `package.json`; Ghostty is `ghostty-web` github pin;
  pin wterm exactly as `"@wterm/dom": "0.3.0"` and `"@wterm/ghostty": "0.3.0"`
- Vite `manualChunks` routes ghostty-web + TerminalRawView + `/web/src/terminal`
  into `terminal.js`; include `@wterm` the same way
- Published API:
  - `import { WTerm } from "@wterm/dom"`
  - `import "@wterm/dom/css"`
  - `import { GhosttyCore } from "@wterm/ghostty"`
  - `const core = await GhosttyCore.load(); const term = new WTerm(el, { core, onData, onResize, autoResize }); await term.init();`
  - `term.write(data); term.focus(); term.destroy();`

## 6. Code anchors

### Setting module (new)
- Storage key: `ajax.terminal.surfaceV2`
- Values: absent/`"false"` → off; `"true"` → on
- API:
  - `isTerminalSurfaceV2Enabled(): boolean` (default false; catch localStorage errors)
  - `setTerminalSurfaceV2Enabled(enabled: boolean): void`
  - `subscribeTerminalSurfaceV2(listener: (enabled: boolean) => void): () => void`
  - Emit on set; also listen to `storage` events for cross-tab if cheap

### Settings UI
Add section after Diagnostics:

```html
<div class="settings-section">
  <h3>Experimental</h3>
  <label>… Terminal Surface V2 …</label>
  <p class="settings-note">Use the experimental DOM-rendered terminal optimized for mobile browsers.</p>
</div>
```

Use a checkbox or switch with `data-testid="setting-terminal-surface-v2"`.
Available on desktop and mobile (no media hide).

### WtermTerminalView.svelte
Props: `{ handle: string; onInitFailure?: (message: string) => void }`

Must:
1. Mount host with `data-testid="task-terminal-panel"` and `data-terminal-engine="wterm"`
2. Load GhosttyCore + WTerm; on failure call `onInitFailure` and stop (do not open a second WS)
3. Call `connectTaskTerminal(handle, …)` only after successful `term.init()`
4. `onOutput` → `term.write(text)`
5. WTerm `onData` → `connection.sendInput(data)`
6. WTerm `onResize` / autoResize → `connection.sendResize(cols, rows)` using existing protocol
7. Enforce `MIN_TERMINAL_COLS` from `terminalGeometry.ts` when reporting cols if width would go below floor (document-only if pan cannot be matched)
8. Include the existing mobile control-key toolbar (Esc/Tab/⌃C/arrows/Ctrl/Paste/hide) — duplicate the small CONTROL_KEYS/ctrl-arm logic locally; do not extract from TerminalRawView
9. Status line for connecting/reconnecting/unavailable + reconnect button (mirror Ghostty status chrome lightly)
10. On unmount: `connection.dispose()`, `term.destroy()`, clear timers/listeners/observers
11. Prefer wterm native DOM input/selection; do not add Ghostty-style selection monkeypatches
12. If scroll-follow is awkward, keep wterm native scroll; document upstream gaps in TERMINAL.md rather than recreating Ghostty complexity

### TerminalSurfaceSelector.svelte
Props: `{ handle: string }`

Responsibilities only:
- Read setting (+ subscribe)
- `{#key ...}` remount so switching disposes previous surface/connection
- When V2 on: render `WtermTerminalView`; on `onInitFailure` force Ghostty and show error via existing toast/status convention (`onResult` not available here — use an inline status banner with `data-testid="terminal-surface-fallback-error"`)
- When V2 off or fallback: render `TerminalRawView` (dynamic import OK)
- Never mount both surfaces at once
- Preserve Ghostty `data-terminal-engine="ghostty"` when Ghostty is active

### TaskDetail.svelte
Replace direct `TerminalRawView` import with `TerminalSurfaceSelector`:

```svelte
{#await import("./TerminalSurfaceSelector.svelte") then { default: TerminalSurfaceSelector }}
  <TerminalSurfaceSelector handle={detail.qualified_handle} />
{/await}
```

### TERMINAL.md
- Add ownership row for experimental wterm surface + selector + setting module
- Add concise iPhone Safari bake-off checklist (10 items from user request)
- Note: packages pinned `@wterm/dom@0.3.0` / `@wterm/ghostty@0.3.0` from npm

### vite.config.mts
Include `/node_modules/@wterm/` in the `terminal` manual chunk condition.

## 7. Test-first instructions

Create/extend tests in this order; first focused red command must fail before
production edits for the setting module, then green, then continue.

### A. `terminalSurfaceSetting.test.ts`
- defaults to off
- persists true/false across get/set
- subscribe notifies on change

Red/green command:
```bash
npm run web:test -- --run src/terminalSurfaceSetting.test.ts
```

### B. `SettingsView.test.ts`
- renders Experimental / Terminal Surface V2
- toggle calls setter / reflects storage

### C. `TerminalSurfaceSelector.test.ts` (mock both child surfaces + setting)
- default → Ghostty only
- enabled → wterm only
- switching unmounts previous (spy dispose/unmount counters)
- only one `data-terminal-engine` panel at a time
- wterm `onInitFailure` → Ghostty + fallback error banner

### D. `WtermTerminalView.test.ts` (mock `@wterm/dom`, `@wterm/ghostty`, `terminalConnection`)
- exposes `data-terminal-engine="wterm"`
- PTY output from connection callback reaches `term.write`
- `onData` reaches `connection.sendInput`
- resize reaches `connection.sendResize` with `{type:"resize",…}` protocol via mock
- unmount calls `connection.dispose` and `term.destroy`
- init throw invokes `onInitFailure` and does not leave a live connection

### E. Existing `TerminalRawView.test.ts` still passes unchanged
### F. `TaskDetail.test.ts` expects selector (or still finds `task-terminal-panel` after await)

Intended first failing assertion (setting):
`expect(isTerminalSurfaceV2Enabled()).toBe(false)` against missing module.

## 8. Edit instructions

1. Write failing tests for `terminalSurfaceSetting` → implement module → green
2. Extend SettingsView tests → add Experimental toggle → green
3. `npm install --save-exact @wterm/dom@0.3.0 @wterm/ghostty@0.3.0`
4. Write failing WtermTerminalView tests with mocks → implement slim component → green
5. Write failing TerminalSurfaceSelector tests → implement → green
6. Wire TaskDetail; update TaskDetail test if it asserts TerminalRawView import
7. Update vite chunk + TERMINAL.md (checklist + ownership + pin note)
8. Run verification commands
9. Check off plan checklist items in `.planning/agent-plans/wterm-terminal-surface-v2-spike.md`

Keep WtermTerminalView intentionally smaller than TerminalRawView. Document
upstream blockers instead of parity hacks.

## 9. Verification commands

```bash
npm run web:check
npm run web:test -- --run src/terminalSurfaceSetting.test.ts src/components/SettingsView.test.ts src/components/WtermTerminalView.test.ts src/components/TerminalSurfaceSelector.test.ts src/components/TerminalRawView.test.ts src/components/TaskDetail.test.ts
npm run web:build
```

## 10. Acceptance criteria

- V2 setting defaults off and persists
- Selector mounts Ghostty or wterm exclusively; switch disposes prior surface/WS
- Failed wterm init falls back to Ghostty with visible error
- wterm uses Ajax `terminalConnection` for output/input/resize
- `data-terminal-engine` is `wterm` or `ghostty` correctly
- Existing Ghostty tests still pass
- web:check and web:build succeed
- Device checklist documented; no recommendation to replace Ghostty

## 11. Stop conditions

- Patch would exceed ~400 changed lines of production logic (tests+lockfile excluded from the soft cap; if production balloons, stop and report)
- Need to edit `TerminalRawView.svelte` beyond zero changes
- Need Rust / protocol changes
- `@wterm` packages fail to install or lack `GhosttyCore.load` / `WTerm`
- Existing TerminalRawView tests fail for unrelated reasons you cannot fix within allowed files
