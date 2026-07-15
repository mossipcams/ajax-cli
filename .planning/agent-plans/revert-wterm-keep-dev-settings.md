# Revert wterm; keep Dev settings; plan xterm spike

## Scope

Remove the entire wterm Terminal Surface V2 experiment (including first spike
#461 and follow-ups through #495). Keep Dev settings (#491) and Surface V2 flag
plumbing. Revert wterm-only #497 CRLF history tweak. Product default stays
Ghostty. Document xterm.js spike behind the same flag (implement later only
with explicit approval).

## Non-goals

- Implementing xterm / adding `@xterm/*` packages in this change
- Changing Ghostty / `TerminalRawView` behavior
- Redesigning Dev settings beyond copy for the missing engine
- Deleting historical `.planning` wterm packets
- Committing / pushing / releasing

## Delegation decision

`Delegation decision: delegated via model-router` — BUILD_PACKET then
`cursor-delegate` / `composer-2.5` (multi-file frontend teardown exceeding
MiniMax bounds; PTY/#497 revert included as mechanical companion). Parent
Review Gate re-ran validation.

## Task checklist

- [x] Persistent plan + READY packet
- [x] Delete wterm frontend/deps/wasm; strip vite + Rust asset/route/tests
- [x] Revert `captured_history_frame_bytes` in `terminal_pty.rs`
- [x] Selector V2-on → unavailable banner; keep flag + Dev settings
- [x] Update preload, Settings copy, TERMINAL.md, architecture.md
- [x] Parent validation (vitest focus, web:check/build, cargo ajax-web)
- [x] Record Phase B xterm spike design (no code)

## Phase B — xterm spike (design only)

Gate: existing `ajax.terminal.surfaceV2` / Dev settings toggle.

| Piece | Behavior |
| --- | --- |
| Deps | Exact `@xterm/xterm` + `@xterm/addon-fit`; CSS from `@xterm/xterm/css/xterm.css` |
| View | `XtermTerminalView.svelte`: Terminal + FitAddon; `connectTaskTerminal`; `data-terminal-engine="xterm"` |
| Selector | Dynamic-import xterm view when V2 on; error+Retry on init failure; no auto Ghostty fallback while flag on |
| Preload | V2 on → xterm chunk only |
| Docs | TERMINAL.md bake-off (10 items); architecture one-liner |

Non-goals: Ghostty cutover, zero-lag port, extra addons, CRLF unless proven needed.

**Do not implement Phase B until explicit user approval.**

## Deviations

(none)

## Validation commands

```bash
cd crates/ajax-web/web && npx vitest run \
  src/terminalSurfaceSetting.test.ts \
  src/components/SettingsView.test.ts \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts \
  src/diagnostics.test.ts \
  src/components/TaskDetail.test.ts \
  src/components/TerminalRawView.test.ts
npm run web:check && npm run web:build
cargo test -p ajax-web -- assets runtime terminal_pty
cargo fmt --check && cargo check -p ajax-web --all-targets
rg -n '@wterm|wterm-ghostty|WtermTerminal' crates package.json package-lock.json
```

## Results

Delegate claimed RED/GREEN then VERIFY. Parent Review Gate re-ran:

- Focused vitest (7 files): 202/202 passed
- `npm run web:check`: 0 errors/warnings
- `npm run web:build`: success; `dist/wterm-ghostty-vt.wasm` absent
- `cargo test -p ajax-web -- assets runtime terminal_pty`: 80/80 passed
- `cargo fmt --check` + `cargo check -p ajax-web --all-targets`: passed
- `rg @wterm|wterm-ghostty|WtermTerminal` on crates/package.json/package-lock.json: clean

Phase B xterm design remains in this file only — not implemented.
