# Goal

Stabilize Web Cockpit task terminal visuals so transient UI does not shrink,
jump, or collide with the Ghostty viewport. Specifically: the "New output" pill
must overlay the terminal host instead of joining the panel flex flow; status,
paste fallback, and copy fallback must be owned by the bottom chrome stack; the
desktop panel min-height must not defeat its max-height on short viewports; and
expanded copy overlay placement must honor safe-area insets.

# Allowed files

Test files:

- `crates/ajax-web/web/e2e/terminal-scroll.test.ts`
- `crates/ajax-web/web/e2e/layout-scroll.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

Production files:

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/styles.css`

Planning ledger:

- `.planning/agent-plans/web-terminal-visual-glitch-fix.md`

# Forbidden changes

- Do not edit the repository root `tests/` directory.
- Do not edit backend Rust, terminal WebSocket protocol, task lifecycle,
  registry, tmux attach planning, or architecture docs.
- Do not introduce Live/snapshot/composer terminal paths.
- Do not add dependencies, screenshot infrastructure, service workers, browser
  persistence, or new task state.
- Do not rewrite unrelated terminal behavior, gesture logic, connection logic,
  viewport logic, or existing assertions.
- Do not commit, push, merge, rebase, or change branches.

# Architecture context

`ajax-web` is the browser presentation adapter. The browser task terminal must
remain raw Ghostty/tmux-first and must not own task truth, registry semantics, or
runtime reconciliation. This change is limited to browser shell markup/CSS and
visual regression tests under `crates/ajax-web/web`.

# Code anchors

- `TerminalRawView.svelte` markup around `data-testid="task-terminal-panel"`,
  `.terminal-host`, `{#if hasUnseenOutput}`, `.terminal-copy-overlay`,
  `{#if copyFallbackOpen}`, `{#if pasteFallbackOpen}`,
  `.terminal-bottom-controls`, and `{#if status !== "connected" || statusDetail
  || pasteNotice}`.
- `TerminalRawView.svelte` CSS anchors:
  `.terminal-panel`, `.terminal-expand-corner`, `.terminal-copy-overlay`,
  `.terminal-expand-corner.is-armed`, `.terminal-host`,
  `.terminal-paste-fallback`, `.terminal-new-output`,
  `.terminal-bottom-controls`, `.terminal-status`.
- `styles.css` anchors:
  `.task-detail .terminal-panel, .task-detail [data-testid="task-terminal-panel"]`
  with `min-height: 280px`, and desktop media query with
  `max-height: min(58vh, 560px)`.
- Existing e2e helper to reuse:
  `crates/ajax-web/web/e2e/terminal-scroll.test.ts` has `openTaskTerminal`,
  `emitTerminalOutput`, `swipeIntoScrollback`, and `newOutputButton`.
- Existing Playwright fixtures to reuse:
  `mockFetch`, `mockTerminalWebSocket`, `terminalPanel`, `waitForTerminalSocket`
  from `crates/ajax-web/web/e2e/fixtures.ts`.

# Test-first instructions

1. In `terminal-scroll.test.ts`, add a failing visual stability assertion to the
   existing scrollback/new-output path or a nearby test:
   - Capture `.terminal-host` and `[data-testid="terminal-bottom-controls"]`
     bounding boxes after scrollback is open and before unseen output appears.
   - Emit output while scrolled up so "New output ↓" appears.
   - Assert the host height changes by at most 1px and the bottom-controls top
     changes by at most 1px.
   - Focused command that must fail before implementation:
     `rtk npx playwright test e2e/terminal-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`

2. In `TerminalRawView.test.ts`, add source/DOM contract coverage:
   - Rendering exposes status, paste fallback, copy fallback, and key bar under
     `[data-testid="terminal-bottom-controls"]`.
   - Source asserts `.terminal-paste-fallback` does not contain
     `position: absolute`.
   - Source asserts `.terminal-new-output` contains `position: absolute`.
   - Source asserts `.terminal-copy-overlay.is-armed` or an equivalent expanded
     selector includes `env(safe-area-inset-top)` and
     `env(safe-area-inset-right)`.
   - Focused command that must fail before implementation:
     `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

3. In `layout-scroll.test.ts`, add a desktop short-viewport regression:
   - Set a desktop-ish viewport with a short height, e.g. width 900, height 420.
   - Enable placeholder terminal, mock fetch, open `/app.html#/t/web%2Ffix-login`.
   - Assert computed terminal `minHeight <= maxHeight` when `maxHeight` is not
     `none`.
   - Assert the panel bottom is within the route-scroll visible bottom.
   - Focused command that must fail before implementation:
     `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`

# Production edit instructions

- Move the "New output ↓" button inside `.terminal-host`, after placeholder /
  zero-lag content, and make `.terminal-new-output` `position: absolute` at the
  bottom center of the host. Do not change its click behavior.
- Move copy fallback, paste fallback, and terminal status markup inside
  `.terminal-bottom-controls`, above the `.terminal-keys` toolbar. Keep the
  existing buttons and handlers.
- Keep a stable status row in bottom controls. Prefer always rendering
  `data-testid="terminal-status"` with a hidden/empty class or similar when
  status is connected and there is no detail/notice, so the reserved slot keeps
  terminal height stable. Preserve visible text for reconnecting, unavailable,
  statusDetail, and pasteNotice.
- Change `.terminal-paste-fallback` from absolute overlay to normal-flow flex
  row inside bottom controls. It should not cover key buttons or status.
- Add safe-area top/right offsets for the expanded copy overlay, mirroring the
  expand corner's expanded treatment without changing normal inline placement.
- Change desktop task terminal min-height so it cannot exceed
  `min(58vh, 560px)` on short viewports. Keep mobile rules intact.
- Keep comments minimal and only where they explain non-obvious layout contract.

# Verification commands

Run focused commands first:

- `rtk npx playwright test e2e/terminal-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
- `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`

Then run broader validation:

- `rtk npm run web:check`
- `rtk npm run web:test -- --run`
- `rtk git diff --check`

# Acceptance criteria

- The new tests fail before production edits for the expected reasons.
- After implementation, the focused tests pass.
- Existing terminal paste/copy/status behavior tests still pass.
- "New output" appearance does not move terminal host height or bottom controls.
- Paste/copy fallback trays do not overlay the key bar.
- Desktop short viewport does not let min-height exceed max-height.
- No changed files outside the allowed list except this packet/plan ledger.

# Stop conditions

- Stop if a required edit would touch backend Rust, protocol, registry, task
  lifecycle, or root `tests/`.
- Stop if a focused test already passes before production edits; report the
  stale assumption.
- Stop if the layout cannot be stabilized without changing the terminal model.
- Stop on unrelated failing tests and report the command/output tail.
