# TDD Implementation Packet: Revert wterm; keep Dev settings

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Remove the entire wterm Terminal Surface V2 engine (deps, views, wasm serve/embed,
tests, #497 CRLF history helper). Keep Dev settings (#491) and Surface V2 flag
API (`terminalSurfaceSetting`, Settings toggle, diagnostics fields,
`TerminalSurfaceSelector` + TaskDetail wiring). When Surface V2 is on, show the
existing error banner that the experimental surface has no engine yet (no Ghostty
auto-fallback while flag on). Ghostty remains default when flag off. Do not add
xterm. Update TERMINAL.md + one architecture.md sentence.

## 3. Allowed files

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/vite.config.mts`
- `crates/ajax-web/web/TERMINAL.md`
- `architecture.md`
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.svelte`
- `crates/ajax-web/web/src/components/TerminalSurfaceSelector.test.ts`
- `crates/ajax-web/web/src/components/SettingsView.svelte`
- `crates/ajax-web/web/src/components/SettingsView.test.ts` (only if copy assertions break)
- `crates/ajax-web/web/src/terminalPreload.ts`
- `crates/ajax-web/web/src/terminalPreload.test.ts`
- `crates/ajax-web/src/adapters/assets.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/dist/*` (rebuild via `npm run web:build`; delete `wterm-ghostty-vt.wasm`)
- Delete only:
  - `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
  - `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
  - `crates/ajax-web/web/src/terminalWtermWasm.ts`
  - `crates/ajax-web/web/src/terminalWtermWasm.test.ts`
  - `crates/ajax-web/web/src/terminalWtermGhosttyCore.ts`
  - `crates/ajax-web/web/src/terminalWtermGhosttyCore.test.ts`
  - `crates/ajax-web/web/src/terminalWtermGhosttyCore.integration.test.ts`
  - `crates/ajax-web/web/src/terminalWtermCore.integration.test.ts`
  - `crates/ajax-web/web/e2e/terminal-surface-v2.test.ts`
  - `crates/ajax-web/web/dist/wterm-ghostty-vt.wasm`
- `.planning/agent-plans/revert-wterm-keep-dev-settings.md` (checklist only)
- `.planning/packets/revert-wterm-keep-dev-settings.md` (this file)

## 4. Forbidden changes

- Do not add `@xterm/*` or implement `XtermTerminalView`
- Do not change Ghostty / `TerminalRawView.svelte` behavior
- Do not remove Dev settings layout, Surface V2 toggle, `terminalSurfaceSetting.ts`, or diagnostics surface fields
- Do not delete historical `.planning` wterm packets
- Do not commit, push, merge, rebase, or change branches
- Do not touch unrelated crates or drive-by formatting

## 5. Context evidence

### Graphify
`NOT_REQUIRED`: teardown confined to Web Cockpit terminal experiment + asset
embed; architecture already documents Ghostty as product terminal.

### Serena
`NOT_REQUIRED`: anchors collected via direct source reads.

### ast-grep / code anchors
- Selector currently `{#await import("./WtermTerminalView.svelte")}` when `v2Enabled`
- `terminalPreload.ts` `preloadWtermTerminalView` + `warmTerminalAssets` V2 branch
- `vite.config.mts`: `wtermGhosttyWasm`, `/wterm-ghostty-vt.wasm` middleware/copy, `@wterm` / `WtermTerminalView` in `manualChunks`
- `assets.rs`: `wterm_ghostty_wasm` arg to `shell_version_from_assets`, `include_bytes!(...wterm-ghostty-vt.wasm)`, route match, tests
- `runtime.rs`: `.route("/wterm-ghostty-vt.wasm", ...)`, `axum_wterm_ghostty_wasm`, public-route test asserting distinct binaries
- `terminal_pty.rs`: `captured_history_frame_bytes` + call site + unit test — revert call to `output_frame_bytes(output.stdout)`
- Settings note currently mentions DOM-rendered terminal; soften to no-engine-until-xterm-spike
- package.json: `"@wterm/core"|"@wterm/dom"|"@wterm/ghostty": "0.3.0"`

## 6. Code anchors

### Selector (post-change behavior)
When `v2Enabled`, do not dynamic-import wterm. On mount / when enabling V2, set
`initError` to a stable message containing `no engine` (or equivalent) so
`data-testid="terminal-surface-v2-error"` renders with Retry. Retry clears error
and remounts; immediately set the same message again (engine still absent).
When `!v2Enabled`, keep Ghostty dynamic import unchanged. Keep `display: contents`.

### Preload
Remove `preloadWtermTerminalView`. `warmTerminalAssets`: if V2 on → `Promise.all([])`
or resolve empty array (do not preload Ghostty); if off → Ghostty + TerminalRawView
as today.

### assets.rs fingerprint
`shell_version_from_assets` drops `wterm_ghostty_wasm` parameter; all call sites
and tests use five assets only. Remove wterm serve arm and
`assets_adapter_serves_wterm_ghostty_wasm_asset` /
`app_version_changes_when_wterm_wasm_asset_changes`.

### runtime.rs
Remove route, handler, and assertions that fetch `/wterm-ghostty-vt.wasm`.

## 7. Test-first instructions

1. Update `TerminalSurfaceSelector.test.ts` first:
   - Remove `@wterm/*` mocks and wterm engine expectations.
   - Keep Ghostty mock.
   - `"defaults to Ghostty only"`: ghostty present; no wterm engine attr.
   - Replace `"renders wterm only when enabled"` with: V2 on →
     `getByTestId("terminal-surface-v2-error")` text matches /no engine/i;
     no `[data-terminal-engine="ghostty"]`; `ghosttyWebLoad` not called.
   - Adjust switch/unmount test: enabling V2 disposes Ghostty connection and
     shows error (no wterm engine).
   - `"never mounts both"` → V2 on mounts zero `[data-terminal-engine]` (error only).
   - Replace init-fail test with the unavailable-banner assertion (or merge).
2. Update `terminalPreload.test.ts`: V2 on does not call wterm preload; does not
   preload Ghostty.
3. Run focused selector + preload tests → RED (production still imports wterm).
4. Then apply production edits until GREEN.

## 8. Edit instructions

1. Apply test updates (section 7).
2. Edit `TerminalSurfaceSelector.svelte` per anchors.
3. Edit `terminalPreload.ts` + tests.
4. Soften Settings note; keep Dev settings + toggle.
5. Strip vite wterm wasm + chunks; uninstall `@wterm/core` `@wterm/dom` `@wterm/ghostty`.
6. Delete listed wterm source/e2e/wasm files.
7. Patch `assets.rs`, `runtime.rs`, `terminal_pty.rs` per anchors.
8. Rewrite `TERMINAL.md` experimental section: flag + selector + Ghostty default +
   V2 unavailable pending xterm spike; remove wterm packages/bake-off.
9. `architecture.md` ~L690-694: TaskDetail mounts `TerminalSurfaceSelector`;
   default Ghostty; Surface V2 gate exists with no alternate engine after wterm
   removal.
10. `npm run web:build` so dist drops wterm wasm and embeds stay consistent.
11. Check off items in `.planning/agent-plans/revert-wterm-keep-dev-settings.md`.

## 9. Verification commands

```bash
cd /Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-revert
# RED then GREEN focused:
cd crates/ajax-web/web && npx vitest run \
  src/components/TerminalSurfaceSelector.test.ts \
  src/terminalPreload.test.ts
cd ../../..
npm uninstall @wterm/core @wterm/dom @wterm/ghostty   # if not already removed
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
rg -n '@wterm|wterm-ghostty|WtermTerminal' crates package.json package-lock.json || true
```

Expect rg to find no production references (planning dirs may still mention wterm).

## 10. Acceptance criteria

- No `@wterm` deps; no `WtermTerminalView`; no `/wterm-ghostty-vt.wasm` serve/embed
- Dev settings + Surface V2 toggle + setting module + diagnostics fields remain
- V2 off → Ghostty; V2 on → error banner only, no Ghostty mount
- #497 CRLF helper gone; history uses `output_frame_bytes`
- Focused tests + web:check/build + ajax-web cargo tests listed above pass
- No xterm implementation

## 11. Stop conditions

- Any requirement to implement xterm in this packet
- Need to change Ghostty behavior to make tests pass
- Asset embed breaks because `web:build` cannot run — stop and report
