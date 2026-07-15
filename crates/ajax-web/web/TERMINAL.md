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
| Experimental xterm mount + chrome UI | XtermTerminalView.svelte |
| Surface V2 selector + setting | TerminalSurfaceSelector.svelte, terminalSurfaceSetting.ts |
| Zero-lag input echo + overlay paint | terminalZeroLag.ts |
| Route scroll / chrome hide | styles.css + App layout |

## Experimental Terminal Surface V2

- Settings toggle: `ajax.terminal.surfaceV2` (default off)
- `TerminalSurfaceSelector.svelte` chooses the active surface
- Ghostty remains default when the experiment is off
- While Surface V2 is enabled, Ghostty is not mounted or preloaded; the selector
  mounts `XtermTerminalView.svelte` instead
- Packages pinned: `@xterm/xterm@6.0.0`, `@xterm/addon-fit@0.11.0`
- On xterm init failure: show error + Retry; do not auto-fallback to Ghostty
  while the flag stays on

### iPhone bake-off checklist (Surface V2)

- [ ] Open task terminal with Surface V2 on
- [ ] Backspace hold
- [ ] Keyboard open / close
- [ ] Paste
- [ ] Select + copy
- [ ] Scroll during output
- [ ] Rotate device
- [ ] Codex / Claude in tmux
- [ ] Alt-screen apps (e.g. less, vim)
- [ ] Toggle back to Ghostty (Surface V2 off)

## Anti-patterns
- Do not add new one-shot `*FlushPending` (or equivalent) booleans in
  TerminalRawView.svelte — put fit/resize permission in terminalLayoutPolicy.ts
  and refit scheduling in terminalRefit.ts
- Do not fix iOS bugs only in CSS/component without a failing Vitest or
  mobile-webkit Playwright case first
- Do not scatter Ghostty private API casts; isolate in one adapter when extracting

## Review rule
Terminal behavior PRs: failing test first; policy change in the owning module.
