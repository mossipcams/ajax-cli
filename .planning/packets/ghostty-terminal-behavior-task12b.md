# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: destructive-cleanup
TEST_FIRST: SATISFIED_BY_TASK_12A
REPLACEMENT_SURFACE: FORBIDDEN
BLOCKERS: []
```

# Goal

Unconditionally remove Ajax's old Ghostty browser surface and the committed
experimental xterm Surface V2 implementation. Do not add a replacement. The
task detail must compile without a terminal mount, while the permanent
black-box behavior suite remains present and intentionally red for the later
ground-up rebuild.

# Preserve

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts` and its engine-neutral
  fixture helpers/test IDs as the acceptance contract (do not skip/edit/weaken).
- `terminalConnection.ts` / `.test.ts`, terminal WebSocket API helpers, the Rust
  `/api/tasks/{handle}/terminal` route, PTY adapter/slice, and their tests.
- `TERMINAL_BEHAVIOR_CONTRACT.md`, `TERMINAL_REBUILD_ACCEPTANCE.md`, and
  `TERMINAL_LEGACY_SURFACE_TESTS.md` as delivery records; update status wording
  but do not delete their required inventory/checklist/matrix content.
- General `viewport.ts` behavior used by the rest of the iOS application.

# Delete old implementation files

Use patch-based deletion for authored files.

## Components and renderer modules/tests

- `src/components/TerminalRawView.svelte` and `.test.ts`
- `src/components/TerminalSurfaceSelector.svelte` and `.test.ts`
- `src/components/XtermTerminalView.svelte` and `.test.ts`
- `src/terminalClipboard.ts` and `.test.ts`
- `src/terminalGeometry.ts`, `.test.ts`, `.fuzz.test.ts`
- `src/terminalGestures.ts`
- `src/terminalLayoutPolicy.ts` and `.test.ts`
- `src/terminalOutputPolicy.ts` and `.test.ts`
- `src/terminalPreload.ts` and `.test.ts`
- `src/terminalRefit.ts` and `.test.ts`
- `src/terminalSelection.test.ts`
- `src/terminalSurfaceSetting.ts` and `.test.ts`
- `src/terminalTouchScroll.test.ts`
- `src/terminalZeroLag.ts` and `.test.ts`
- old `src/terminalOwnership.test.ts` (the new hygiene test replaces it)

## Renderer-specific browser proxy files

- `e2e/terminal-scroll.test.ts`
- `e2e/terminal-scroll-garble.test.ts`
- `e2e/terminal-zero-lag.test.ts`
- `e2e/fullscreen-refit.test.ts`
- `scripts/ios-terminal-smoke.mjs`

# Edit live application/tests

- `TaskDetail.svelte`: remove selector import/mount and terminal-first class;
  keep task status/actions/details usable. Update `TaskDetail.test.ts` by
  removing only old renderer mocks/raw imports/selector assertions.
- `App.svelte`: remove terminal preload/warm effect. Update `App.test.ts`
  accordingly.
- `SettingsView.svelte`: remove Surface V2 imports/state/toggle/error/debug
  rows. Update only the matching old assertions in `SettingsView.test.ts`.
- `diagnostics.ts`: remove Surface V2/error report fields; update its tests.
- `styles.css` and TaskDetail scoped CSS: delete terminal-panel, terminal-first,
  keyboard-open/fullscreen-terminal styles and restore ordinary route scrolling
  for task details. Do not redesign unrelated UI.
- `e2e/actions.test.ts`, `layout-scroll.test.ts`, `smoke.test.ts`: remove only
  renderer/terminal-specific cases and imports; keep non-terminal coverage.
- `e2e/fixtures.ts`: remove the old engine-specific `terminalPanel` locator only
  after all retained callers are gone. Keep all permanent behavior helpers.

# Remove dependencies and build/static wiring

- Remove `ghostty-web`, `@xterm/xterm`, and `@xterm/addon-fit` from
  `package.json` and `package-lock.json` using npm's package-manager operation;
  do not add dependencies.
- Simplify `vite.config.mts`: no Ghostty WASM copy/dev middleware, no old
  terminal manual chunk/component rules. Build deterministic `index.html`,
  `app.js`, `app.css` only.
- Update `scripts/web-build-check.mjs` to require those three assets and reject
  stale `terminal.js` / `ghostty-vt.wasm`; remove old engine-content checks.
- Rebuild `crates/ajax-web/web/dist`; generated `terminal.js` and
  `ghostty-vt.wasm` must disappear.
- Update `crates/ajax-web/src/adapters/assets.rs`, `runtime.rs`, and
  `crates/ajax-cli/src/web_backend.rs` static-asset wiring/tests so only the
  shell HTML/app JS/CSS are embedded/served. Do not change the terminal
  WebSocket route or PTY behavior.

# Documentation

- `architecture.md`: replace the old frontend selector/engine/WASM description
  with the explicit current state: the old surfaces are removed, no browser
  terminal is mounted, the backend/connection behavior contract is retained
  for a later ground-up controller/adapter rebuild.
- Rewrite `crates/ajax-web/web/TERMINAL.md` as a short ownership/status note for
  the absent frontend, retained connection/backend boundaries, permanent test
  suite, and acceptance docs. State no shared old/new adapter exists.
- Mark inventory/legacy/acceptance docs as pre-removal evidence and Task 12
  complete; preserve the full inventory, matrix, physical checklist, and known
  bug list.

# Hygiene GREEN

Run first after cleanup:

```bash
npm run web:test -- --run src/legacyTerminalRemoval.test.ts
```

It must pass without weakening its violations. Also verify the removed package
entries and stale generated files with repository searches; the hygiene test
intentionally does not ban future generic xterm dependencies/architecture.

# Validation

```bash
npm run web:check
npm run web:test -- --run
npm run web:build
npm run web:build:check
cargo fmt --check
cargo nextest run -p ajax-web
cargo nextest run -p ajax-cli
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

Run the full permanent behavior file and record its expected failures caused
by the intentionally absent surface:

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

Do not make it green, skip it, delete it, or add a placeholder surface.

# Acceptance criteria

- Hygiene RED from Task 12A becomes GREEN.
- No old component/module/test/dependency/WASM/preload/static route/build chunk
  remains.
- Web app and retained tests/build compile and pass without a terminal mount.
- Rust PTY/WebSocket backend remains green and unchanged in behavior.
- Permanent iOS-WebKit behavior suite remains intact and is red only because
  there is no current terminal surface.

# Stop conditions

- Cleanup would require removing the backend PTY/WebSocket contract or
  permanent behavior tests.
- A replacement/placeholder/shared adapter would be needed to compile.
- Unrelated behavior or tests would need changes.
