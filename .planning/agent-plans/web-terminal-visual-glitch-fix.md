# Web Terminal Visual Glitch Fix Plan

## Scope

Fix visual glitches inside the Web Cockpit task terminal caused by transient UI
changing or colliding with the terminal viewport:

- "New output" should not shrink or jump the Ghostty canvas when it appears.
- Status, paste fallback, and copy fallback should be owned by one bottom chrome
  stack instead of mixing normal-flow and absolute bottom overlays.
- Desktop terminal min-height must not exceed its max-height on short viewports.
- Expanded top overlays should respect the same safe-area treatment.

## Non-goals

- No backend, task truth, registry, lifecycle, tmux, or WebSocket protocol
  changes.
- No terminal model replacement, composer restoration, snapshot/live terminal,
  service worker, or offline state.
- No new dependencies or screenshot infrastructure.
- Do not modify the repository root `tests/` directory.

## Deeper Diagnosis

The current terminal panel mixes three layout models in one flex column:

- `terminal-host` is the flexible Ghostty viewport.
- `terminal-new-output` and `terminal-status` are conditional flex siblings, so
  appearing/disappearing can change the host's measured height.
- paste/copy fallback trays are absolutely positioned at the panel bottom with a
  higher `z-index`, so they can overlap the key bar/status instead of occupying a
  predictable bottom layer.

MDN confirms the CSS mechanics behind the suspected regressions:

- `max-height` is overridden by `min-height`, so the current desktop
  `min-height: 280px` can defeat `max-height: min(58vh, 560px)` on short
  viewports.
- Absolutely positioned elements leave normal flow, and higher `z-index` layers
  cover lower layers, matching the fallback/key-bar collision risk.

## Delegation

Delegation decision for implementation: delegate via `model-router` after user
approval. This is a bounded web behavior change with clear tests and file
targets.

## Task Checklist

### Task 1: Prove transient "New output" does not jump the terminal viewport

- Test to write: extend `crates/ajax-web/web/e2e/terminal-scroll.test.ts`.
  Capture `.terminal-host` and `[data-testid="terminal-bottom-controls"]`
  bounding boxes before emitting output while scrolled up, then assert the host
  height and bottom controls position remain stable when "New output" appears.
- Expected initial failure: the "New output" button is a flex sibling and changes
  panel layout when it appears.
- Code to implement: move/render the "New output" affordance inside
  `.terminal-host` and position it as an absolute overlay anchored to the host's
  bottom center.
- Verification: run
  `rtk npx playwright test e2e/terminal-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`.

### Task 2: Put status and fallback trays under one bottom chrome owner

- Test to write: add a focused component/source contract in
  `crates/ajax-web/web/src/components/TerminalRawView.test.ts` that requires
  paste fallback, copy fallback, key bar, and status to live inside
  `.terminal-bottom-controls`, and that `.terminal-paste-fallback` is no longer
  absolutely positioned.
- Expected initial failure: fallback trays and status are siblings outside
  `.terminal-bottom-controls`, and fallback CSS is `position: absolute`.
- Code to implement: move paste fallback, copy fallback, and terminal status
  markup inside `.terminal-bottom-controls`; keep the status row always present
  as a reserved compact slot, hiding empty connected state visually instead of
  removing the row; make fallback trays normal-flow rows in the bottom chrome
  stack.
- Verification: run
  `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`.

### Task 3: Prevent desktop min-height from exceeding max-height

- Test to write: add a short-viewport assertion to
  `crates/ajax-web/web/e2e/layout-scroll.test.ts` at a desktop-ish viewport, then
  assert computed terminal `min-height <= max-height` and the panel remains
  within the visible route band.
- Expected initial failure: desktop `min-height: 280px` can exceed
  `max-height: min(58vh, 560px)`.
- Code to implement: change the desktop task terminal min-height to a bounded
  CSS `min(...)` value that cannot exceed the same `58vh` cap.
- Verification: run
  `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`.

### Task 4: Align expanded top overlays with safe-area rules

- Test to write: add a small component/source contract in
  `crates/ajax-web/web/src/components/TerminalRawView.test.ts` requiring expanded
  `.terminal-copy-overlay` to use `env(safe-area-inset-top/right)` like the
  expand corner.
- Expected initial failure: only `.terminal-expand-corner.is-armed` has safe-area
  offsets.
- Code to implement: add a scoped expanded rule for `.terminal-copy-overlay`
  safe-area top/right offsets.
- Verification: run
  `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`.

## Final Validation

- `rtk npm run web:check`
- `rtk npm run web:test -- --run`
- `rtk npx playwright test e2e/terminal-scroll.test.ts e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
- `rtk git diff --check`

## Approval Status

Approved by user: "implement until finished".

## Execution Log

- Created implementation packet:
  `.planning/packets/web-terminal-visual-stability.md`.
- Delegation decision: delegated via model-router. Routed to Cursor/Grok 4.5
  High because this is complex Svelte/PWA terminal layout behavior.
- Implementation worker: TDD complete.
  - [x] Task 1: New-output host/bottom-controls stability (e2e + overlay CSS)
  - [x] Task 2: Bottom-controls ownership for status/fallbacks (unit + markup)
  - [x] Task 3: Desktop short-viewport min-height cap (styles.css + e2e)
  - [x] Task 4: Expanded copy-overlay safe-area offsets (CSS + source contract)
- Focused validation: all three packet commands PASS.
- Broader validation: `web:check` PASS, full `web:test` PASS (379), `git diff --check` PASS.
- Parent validation rerun:
  - PASS `rtk npx playwright test e2e/terminal-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
  - PASS `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`
  - PASS `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
  - PASS `rtk npm run web:check`
  - PASS `rtk npm run web:test -- --run`
  - PASS `rtk npx playwright test e2e/terminal-scroll.test.ts e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
  - PASS `rtk git diff --check`

## Deviations

- Layout e2e scrolls the panel into the route-scroll band before asserting
  `panelBottom <= routeBottom`, and skips `mobile-webkit` (desktop-only
  contract). Min/max-height assertion remains the load-bearing check.
