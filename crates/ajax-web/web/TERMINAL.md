# Web Cockpit terminal ownership

## Product contract
- Raw Ghostty/tmux-first on mobile and desktop
- Do not reintroduce Live/snapshot/composer as default
- Browser modules do not own task truth or tmux target selection

## Ownership table
| Concern | Owner |
| --- | --- |
| Keyboard / visualViewport / --app-* | viewport.ts |
| Fit / font / pan / scale math | terminalGeometry.ts |
| Refit scheduling | terminalRefit.ts (when to fit/send; not permission) |
| Layout fit/resize permission | terminalLayoutPolicy.ts |
| Paste/copy UI state (fallback/overlay/notice) | terminalClipboard.ts |
| Gestures / selection geometry | terminalGestures.ts |
| Scroll-follow state + resize validity | terminalOutputPolicy.ts |
| WS connect / backoff / status | terminalConnection.ts |
| Ghostty mount + chrome UI | TerminalRawView.svelte |
| Experimental wterm surface + selector + setting | WtermTerminalView.svelte, TerminalSurfaceSelector.svelte, terminalSurfaceSetting.ts |
| Zero-lag input echo + overlay paint | terminalZeroLag.ts |
| Route scroll / chrome hide | styles.css + App layout |

## Experimental Terminal Surface V2

- Packages pinned from npm: `@wterm/core@0.3.0`, `@wterm/dom@0.3.0`, `@wterm/ghostty@0.3.0`
- Settings toggle: `ajax.terminal.surfaceV2` (default off)
- Uses Ajax `connectTaskTerminal`; does **not** use wterm `WebSocketTransport`
- Ghostty remains default when the experiment is off
- While Surface V2 is enabled, Ghostty is not mounted or preloaded (failure shows
  an error + Retry; it does not fall back to Ghostty until the flag is turned off)
- Terminal core: Surface V2 imports built-in `WasmBridge` from `@wterm/core` and
  passes it explicitly to `WTerm`; it does **not** load or preload `@wterm/ghostty`
- Logical grid is fixed at **80x24** with `autoResize: false` so CSS viewport,
  expand, or keyboard flex does not rebuild the VT grid or corrupt scrollback
- WASM: normal ghostty-web stays at `/ghostty-vt.wasm` for the default path;
  `@wterm/ghostty` and `/wterm-ghostty-vt.wasm` remain installed and served
  (same upstream filename, incompatible exports) but are inactive in Surface V2
- Intentionally smaller than Ghostty: no zero-lag overlay, no selection-manager casts, native wterm scroll/selection
- Known gaps (document-only, not blocking the spike): readonly copy fallback when
  clipboard write fails; physical iPhone bake-off still required (see checklist below)

## iPhone Safari bake-off checklist (wterm vs Ghostty)

Run on a physical iPhone (Safari / PWA) with Terminal Surface V2 on. Mark
Pass/Fail. Do not replace Ghostty based on this spike alone.

1. Open terminal
2. Type and hold backspace
3. Open and close keyboard repeatedly
4. Paste text
5. Select and copy output
6. Scroll upward during active output
7. Rotate the phone
8. Run Codex or Claude inside tmux
9. Enter and exit an alternate-screen program
10. Toggle back to Ghostty

## Anti-patterns
- Do not add new one-shot `*FlushPending` (or equivalent) booleans in
  TerminalRawView.svelte — put fit/resize permission in terminalLayoutPolicy.ts
  and refit scheduling in terminalRefit.ts
- Do not fix iOS bugs only in CSS/component without a failing Vitest or
  mobile-webkit Playwright case first
- Do not scatter Ghostty private API casts; isolate in one adapter when extracting

## Review rule
Terminal behavior PRs: failing test first; policy change in the owning module.
