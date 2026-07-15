# Xterm terminal rebuild

## Scope

Implement one xterm.js Web Cockpit terminal surface on task routes and make the
27 permanent `mobile-webkit` behavior cases introduced by PR 510 pass. Reuse
the existing `terminalConnection.ts` WebSocket boundary and keep terminal state
inside the browser presentation layer.

## Non-goals

- Do not modify files under `tests/` or alter PR 510's behavior cases.
- Do not restore Ghostty, the Surface V2 flag, legacy terminal helpers, or a
  renderer abstraction.
- Do not change the Rust PTY/WebSocket route, task lifecycle, registry truth,
  authentication, or public network behavior.
- Do not commit, push, merge, rebase, switch branches, or open/update a PR.

## Workflow notes

- Approval status: **approved 2026-07-15**. The user instructed delegation to
  continue until finished, superseding the per-task pause gate.
- Delegation decision: **delegated via model-router**. The selected OpenCode
  discovery tool is unavailable; local read-only discovery established exact
  anchors. Approved implementation slices route to Cursor with a complete TDD
  packet, followed by parent diff review and independent validation.
- The strict workflow requires a failing test first, while the repo contract
  forbids modifying test files unless explicitly requested. PR 510 already
  supplies the failing tests, so each task uses the named existing case as its
  RED test and leaves all test sources unchanged.
- Ponytail constraint: one concrete xterm component plus existing connection
  code; no speculative helpers or new abstraction layer.

## Task checklist

- [x] **Task 1 — Mount the xterm surface and own its lifecycle (5–15 min)**
  - Test to write/use: use the existing cases `task route mounts one terminal
    surface and opens one socket`, `delayed socket open shows Connecting then
    connects`, `socket close reconnects...`, `navigation away closes...`,
    `pty output corpus keeps surface connected...`, and `reopening the task
    route...`; first run the focused group and retain its current missing-panel
    failure as RED. No test file edits.
  - Implementation: add the pinned xterm/fit dependencies, create the smallest
    `XtermTerminalView.svelte` that mounts one terminal, connects through
    `connectTaskTerminal`, renders status/reconnect UI, disposes everything on
    unmount, and mount it once from `TaskDetail.svelte`.
  - Verify: rerun the six focused cases; run `npm run web:check`.

- [x] **Task 2 — Preserve terminal input and toolbar behavior (5–15 min)**
  - Test to write/use: use the existing printable/control/navigation,
    cardinality, Unicode paste, Hide keyboard, reconnect input, and sticky Ctrl
    cases as RED before implementation. No test file edits.
  - Implementation: wire xterm `onData` directly to the existing connection;
    add only the required Esc/Tab/Ctrl-C/arrows/sticky-Ctrl/Paste/Hide keyboard
    controls with exact one-frame behavior and focus preservation.
  - Verify: rerun those focused cases; rerun Task 1's six cases.

- [x] **Task 3 — Fit and deduplicate PTY resize outcomes (5–15 min)**
  - Test to write/use: use the existing initial-size, orientation,
    same-dimension burst, keyboard burst, fullscreen resize, and reopen-resize
    cases as RED. No test file edits.
  - Implementation: fit on open and the existing viewport event sources,
    debounce/coalesce resize work, deduplicate adjacent `{cols, rows}`, suppress
    keyboard-open storms, and reset resize reporting on a new socket without
    changing the backend contract.
  - Verify: rerun the six resize cases; run `npm run web:check`.

- [x] **Task 4 — Add stable interaction, fullscreen, scrollback, and pinch (5–15 min)**
  - Test to write/use: use the existing stable interaction locator, New output,
    long-press, synthetic scroll containment, fullscreen input, and persisted
    pinch cases as RED. No test file edits.
  - Implementation: expose one renderer-neutral interaction surface; contain
    terminal gestures/document scroll; show and clear New output while the user
    reads scrollback; toggle one fullscreen surface without reconnecting; and
    persist the minimum bounded font-size adjustment needed for pinch refit.
  - Verify: rerun those six focused cases; rerun Tasks 1–3 focused groups.

- [x] **Task 5 — Close cross-behavior gaps and update terminal docs (5–15 min)**
  - Test to write/use: use the existing delayed-output, output-during-viewport,
    Paste-after-scroll/fullscreen cases plus the complete 27-case suite as RED
    for any remaining integration failures. No test file edits.
  - Implementation: make only the smallest component fixes required by those
    failures; update `TERMINAL.md` (and `architecture.md` only if a durable
    boundary statement is stale) to describe the implemented xterm surface.
  - Verify: run all 27 mobile-WebKit cases, `npm run web:check`,
    `npm run web:test -- --run`, and `npm run web:build:check`.

- [x] **Task 6 — Skip xterm initialization without required browser APIs (5–15 min)**
  - Test to write/use: use the existing full Vitest suite as RED; it currently
    fails 16 TaskDetail tests because jsdom lacks `window.matchMedia` and canvas
    rendering. No test file edits.
  - Implementation: add the smallest capability guard in
    `TaskTerminal.svelte` before xterm is constructed/opened. Do not introduce
    a test-only flag, mock, dependency, or alternate renderer.
  - Verify: run the full Vitest suite, all 27 mobile-WebKit cases,
    `npm run web:check`, and `npm run web:build:check`.

- [x] **Final repository validation**
  - Run `npm run verify`.
  - Run `cargo build --release -p ajax-cli` and
    `cargo install --path crates/ajax-cli --locked --force` if preparing the PR
    for review requires the full local gate.
  - Record every command and exit status below; do not claim completion if the
    full gate is skipped or fails.

## Validation ledger

- `npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit` — exit 1; expected RED baseline: 0 passed, 27 failed because `task-terminal-panel` is absent (80.837s).
- Task 1 delegate RED focused lifecycle/output group — exit 1; missing
  `task-terminal-panel` (0 passed, 7 failed).
- Task 1 parent verification: focused lifecycle/output group — exit 0 (7
  passed); `npm run web:check` — exit 0; legacy-removal Vitest — exit 0.
- Task 2 delegate RED focused input/toolbar group — exit 1 (0 passed, 6
  failed); parent verification after implementation: input group — exit 0 (6
  passed), Task 1 group — exit 0 (7 passed), `npm run web:check` — exit 0.
- Task 3 delegate RED resize/fullscreen group — exit 1 (2 passed, 4 failed);
  parent verification after implementation: resize group — exit 0 (6 passed),
  prior lifecycle/input group — exit 0 (13 passed), `npm run web:check` — exit
  0.
- Task 4 delegate RED interaction group — exit 1 (0 passed, 7 failed). First
  green patch passed tests but failed review because wrapper scroll did not move
  the real xterm buffer; one focused revision added `scrollToLine` synchronization
  and write-completion updates. Parent verification: interaction group — exit 0
  (7 passed), prior group — exit 0 (19 passed), `npm run web:check` — exit 0.
- Task 5 full mobile-WebKit suite — exit 0 (27 passed). Documentation hygiene
  and `npm run web:check` — exit 0. Full Vitest — exit 1: 16 TaskDetail tests
  failed because xterm initialization reached missing jsdom `matchMedia` and
  canvas APIs; `web:build:check` was not reached by the chained command.
- Task 6 parent verification: full Vitest — exit 0 (245 passed), mobile-WebKit
  — exit 0 (27 passed), `npm run web:check` — exit 0,
  `npm run web:build:check` — exit 0. jsdom prints two dependency canvas
  capability diagnostics during module import, but no test or unhandled-error
  failure remains.
- `npm run verify` — exit 0: formatting, all-target/all-feature check, clippy
  with warnings denied, 1,579 nextest tests, doc tests, Svelte/TypeScript
  checks, and 245 Vitest tests passed.
- `npm ls @xterm/xterm @xterm/addon-fit --depth=0` — exit 0; exact versions
  6.0.0 and 0.11.0 installed.
- `git diff --check` — exit 0.
- `cargo build --release -p ajax-cli` and global `cargo install` — skipped;
  no commit, push, PR creation/update, or local binary installation was
  requested. The full non-installing repository verify gate passed.

## Deviations

- Task 4 required one router Review Gate revision: the initially passing
  synthetic spacer tracked follow state without displaying actual xterm
  scrollback. The accepted revision synchronizes wrapper and xterm positions.
- Broad validation added Task 6 after jsdom exposed a missing browser-capability
  guard in the new terminal component.
- `npm run web:build:check` regenerated tracked `dist/app.js` and `dist/app.css`;
  these are required embedded browser-shell outputs for the new terminal.
