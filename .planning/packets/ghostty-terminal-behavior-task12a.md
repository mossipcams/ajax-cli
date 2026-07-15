# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: test-only-red
TEST_FIRST: REQUIRED_RED_ONLY
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Add the permanent repository-hygiene test that names the exact old Ghostty and
experimental Surface V2 implementation paths/symbols, and capture its expected
RED result before cleanup. It must not ban a future ground-up xterm dependency,
controller, adapter, chunk name, or architecture.

# Allowed file

- create `crates/ajax-web/web/src/legacyTerminalRemoval.test.ts`

# Forbidden changes

- Everything else; no git commands and no cleanup yet.
- Do not assert generic absence of `xterm`, `@xterm/xterm`, terminal WebSocket /
  PTY backend, `terminalConnection`, or generic future file names.

# Test requirements

Use Node `existsSync`/`readFileSync` relative to the repo root. One focused test
must report all remaining violations together:

1. Exact old paths that must be absent:
   - components `TerminalRawView.svelte`, `TerminalSurfaceSelector.svelte`,
     `XtermTerminalView.svelte` and their tests;
   - `terminalPreload`, `terminalSurfaceSetting`, and all old renderer-only
     policy/gesture/geometry/refit/output/zero-lag/clipboard modules/tests;
   - legacy renderer-specific Playwright files `terminal-scroll.test.ts`,
     `terminal-scroll-garble.test.ts`, `terminal-zero-lag.test.ts`,
     `fullscreen-refit.test.ts`;
   - `scripts/ios-terminal-smoke.mjs`;
   - built `dist/ghostty-vt.wasm`.
2. Exact old symbols/wiring absent from the live files that remain:
   - package dependency `ghostty-web` (do not ban `@xterm`);
   - `TerminalSurfaceSelector` in `TaskDetail.svelte`;
   - `terminalPreload` in `App.svelte`;
   - `surfaceV2` / `Terminal Surface V2` in Settings/diagnostics;
   - `ghostty-vt.wasm`, `copyGhosttyWasm`, old component names in Vite;
   - Ghostty WASM static routes/assets in Rust runtime/assets;
   - old component/Surface V2 descriptions in `architecture.md` and
     `crates/ajax-web/web/TERMINAL.md`.

Ignore historical `CHANGELOG.md`, behavior inventory/acceptance/deletion docs,
planning files, AGENTS, and the permanent black-box test. Keep the test simple:
collect violation strings and `expect(violations).toEqual([])`.

# RED command

```bash
npm run web:test -- --run src/legacyTerminalRemoval.test.ts
```

# Acceptance criteria

- Command exits 1 because the old implementation still exists.
- Failure lists multiple exact old paths/symbols.
- No production or cleanup file changes.
