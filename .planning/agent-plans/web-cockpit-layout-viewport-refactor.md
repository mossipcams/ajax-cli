# Web Cockpit Layout Viewport Refactor

## Scope

Refactor Ajax Web Cockpit layout, scroll ownership, and viewport authority so
mobile/PWA-style usage has one normal route scroll owner, bounded terminal
panels, keyboard-safe overlays, and regression coverage across desktop
Chromium and mobile WebKit.

## Non-Goals

- Do not migrate to wterm.
- Do not treat the blank-space bug as a Ghostty issue unless implementation
  evidence proves it.
- Do not move task truth, registry state, lifecycle decisions, or terminal
  substrate authority into browser layout code.
- Do not weaken existing tests or assertions.

## Approval

- Status: approved by user with "delegate to cursor".
- Cursor chat: `ca67598a-d381-4373-b7ac-420856d526e8`

## Discovery Map

- [x] Read `architecture.md` Web Cockpit/task/terminal ownership context.
- [x] Used Serena first by reading initial instructions and activating this
  project.
- [x] Used source-scoped search/ast-grep to map touchpoints.

Mapped files touching requested patterns:

- `crates/ajax-web/web/src/viewport.ts`
- `crates/ajax-web/web/src/viewport.test.ts`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/components/App.test.ts`
- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- `crates/ajax-web/web/src/components/TaskList.svelte`
- `crates/ajax-web/web/src/components/TaskList.test.ts`
- `crates/ajax-web/web/src/components/NewTaskSheet.svelte`
- `crates/ajax-web/web/src/components/NewTaskSheet.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/e2e/smoke.test.ts`
- `crates/ajax-web/web/playwright.config.mts`
- `crates/ajax-web/src/slices/install.rs`
- Supporting type/API imports found in `api.ts`, `types.ts`, `contracts.ts`,
  `terminalGeometry.ts`, `terminalGestures.ts`, `terminalSelection.ts`, and
  `main.ts`.

Current ownership conflict:

- `App.svelte` initializes viewport state and gives mobile `main` route
  scrolling.
- `TaskDetail.svelte` also applies fixed mobile visual-viewport ownership and
  document scroll locks.
- `styles.css` has additional global task-detail and terminal-expanded fixed
  viewport rules.
- `TerminalRawView.svelte` owns terminal geometry listeners and terminal
  expanded state, but normal/expanded layout is partly controlled by ancestor
  selectors.
- `NewTaskSheet.svelte` consumes `--app-height`/`--app-top` directly.

## Tasks

- [x] Task 1: Add failing e2e layout-scroll tests and Playwright project matrix.
  - Test: add `crates/ajax-web/web/e2e/layout-scroll.test.ts` using the
    existing `smoke.test.ts` fetch mock style, with desktop Chromium and
    mobile WebKit coverage in `playwright.config.mts`.
  - Code: only test/config changes for this task; add assertions for scroll
    owner count, html/body/#app overflow, row heights, keyboard-band sheet
    bounds, terminal placeholder visibility, and expanded-terminal close
    preserving normal route scroll.
  - Verify: run `cd crates/ajax-web/web && npx playwright test e2e/layout-scroll.test.ts`
    and confirm failures point to current layout/placeholder gaps.

- [x] Task 2: Add layout primitives with data-testids.
  - Test: add/update component tests that assert `app-viewport`, `app-shell`,
    `app-main`, and `route-scroll` render and establish the intended DOM
    hierarchy.
  - Code: add `AppViewport.svelte`, `AppShell.svelte`, `RouteScroll.svelte`,
    and `FullscreenLayer.svelte` only if the expanded terminal/sheet overlay
    needs a shared fixed-layer primitive.
  - Verify: run focused Vitest for the new/updated layout component tests.

- [x] Task 3: Move app viewport authority into `AppViewport`.
  - Test: update the existing viewport/App tests so only `AppViewport` consumes
    `initViewport` and app CSS variables.
  - Code: remove `initViewport` from `App.svelte`; mount the application inside
    `AppViewport`; set html/body/#app non-scrolling base behavior in global CSS.
  - Verify: run focused Vitest for `viewport.test.ts` and `App.test.ts`.

- [x] Task 4: Make `RouteScroll` the only normal route scroll owner.
  - Test: add/update tests that fail if mobile dashboard/task detail scrolling
    is owned by `main`, task rows, task cards, html/body, or component-specific
    fixed route wrappers.
  - Code: wrap dashboard/project/settings/task route content in `RouteScroll`;
    remove route-scroll rules from `App.svelte`, `TaskDetail.svelte`, and
    broad global selectors that make normal routes fixed or independently
    scrollable.
  - Verify: run focused Vitest plus the failing e2e scroll-owner test until it
    passes.

- [x] Task 5: Bound normal terminal panel without changing page height.
  - Test: add/update terminal/detail tests and e2e computed-layout assertions
    for `task-terminal-panel` min height, max viewport-relative height,
    `overflow: hidden`, and non-mutating normal route scroll.
  - Code: keep terminal geometry math in terminal modules; change layout CSS so
    normal `TerminalRawView` is an inline bounded panel with sane min/max height
    and no `flex: 1` viewport takeover in normal mode.
  - Verify: run focused `TerminalRawView`/`TaskDetail` Vitest and e2e terminal
    placeholder panel checks.

- [x] Task 6: Implement expanded terminal as a separate fixed layer.
  - Test: add/update tests that open expanded terminal, scroll the normal route,
    close expanded terminal, and assert the route remains scrollable and
    html/body/#app stay locked.
  - Code: move expanded terminal presentation into a fixed overlay layer rather
    than mutating the normal route layout; preserve existing terminal controls
    and refit/snap behavior.
  - Verify: run focused terminal tests and the e2e expanded-terminal regression.

- [x] Task 7: Make `NewTaskSheet` keyboard-safe inside the app band.
  - Test: add/update tests for `data-testid="new-task-sheet"` and simulated
    `keyboard-open` with `--app-height`/`--app-top`, asserting the sheet and its
    focused input stay within the visible app band and the sheet has internal
    scrolling when content exceeds the band.
  - Code: route the sheet through the layout/fixed-layer system or make its
    fixed band ownership explicit while keeping internal `.sheet-card` scroll.
  - Verify: run focused `NewTaskSheet` tests and e2e keyboard-band assertion.

- [x] Task 8: Add dev/test-only terminal placeholder path.
  - Test: add e2e assertion that setting
    `localStorage["ajax.debug.terminalPlaceholder"]="true"` renders
    `data-testid="terminal-placeholder"` and proves layout works without
    Ghostty.
  - Code: gate a placeholder branch in `TerminalRawView.svelte` before Ghostty
    initialization; keep it dev/test-only and avoid touching production terminal
    substrate contracts.
  - Verify: run focused terminal placeholder tests and confirm no WebSocket or
    Ghostty canvas is required in placeholder mode.

- [x] Task 9: Clean up obsolete viewport and scroll rules.
  - Test: update source-string tests that currently expect legacy
    `ajax-dashboard-open`, fixed `.task-detail`, or global
    `terminal-expanded` route mutations.
  - Code: remove duplicate `--app-height` consumers outside `AppViewport` and
    overlay components, remove normal-route `position: fixed` takeovers, and
    keep terminal geometry listeners separate from app viewport CSS variables.
  - Verify: run focused Vitest for affected source-string/component tests.

- [x] Task 10: Run required validation and update this ledger.
  - Test: no new test; validation task.
  - Code: update this plan with final deviations and checked-off tasks.
  - Verify: run `npm run web:check`, `npm run web:test -- --run`, and
    `cd crates/ajax-web/web && npx playwright test`; record any failures
    exactly.

## Deviations

- After PR creation, GitHub CI showed the Web job only installed Chromium even
  though the Playwright matrix now includes mobile WebKit. Updated the CI
  browser install step to install `chromium webkit`.

## Validation

- Planning/discovery commands run:
  - `rtk sed -n '1,220p' architecture.md`
  - `rtk rg -l ... crates/ajax-web/web/src crates/ajax-web/web/e2e ...`
  - `rtk ast-grep --pattern 'window.visualViewport' --lang ts crates/ajax-web/web/src`
  - `rtk ast-grep --pattern 'localStorage.$METHOD($$$ARGS)' --lang ts crates/ajax-web/web/src crates/ajax-web/web/e2e`
  - targeted `rtk sed` reads for `App.svelte`, `TaskDetail.svelte`,
    `NewTaskSheet.svelte`, `TaskList.svelte`, `TerminalRawView.svelte`,
    `styles.css`, `viewport.ts`, `smoke.test.ts`, and `playwright.config.mts`
- Task 1:
  - Cursor test-only delegation completed in chat
    `ca67598a-d381-4373-b7ac-420856d526e8`.
  - `rtk npx playwright test e2e/layout-scroll.test.ts` from
    `crates/ajax-web/web` failed as expected: 15 failed, 3 passed.
  - Expected failures include missing `route-scroll`, html overflow-y `auto`,
    missing `new-task-sheet`, missing `terminal-placeholder`, and expanded
    terminal scroll recovery blocked by missing placeholder/route scroll.
- Task 1 adjustment:
  - Removed mobile Chromium from the Playwright project matrix after user
    clarified the target browsers are local desktop terminal usage and mobile
    iOS Safari/WebKit.
  - Re-ran `rtk npx playwright test e2e/layout-scroll.test.ts` from
    `crates/ajax-web/web`; it failed as expected with 10 failed and 2 passed
    across desktop Chromium and mobile WebKit.
- Tasks 2-4:
  - Cursor implementation delegation completed in chat
    `ca67598a-d381-4373-b7ac-420856d526e8`.
  - `rtk npm run web:test -- --run App.test.ts viewport.test.ts` passed.
  - `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium`
    failed as expected with 3 passed and 3 failed. Passing assertions cover
    `route-scroll`, locked root shells, and task row heights; remaining
    failures cover `new-task-sheet`, `terminal-placeholder`, and expanded
    terminal scroll recovery.
- Tasks 5-8:
  - Cursor implementation delegation completed in chat
    `ca67598a-d381-4373-b7ac-420856d526e8`.
  - `rtk npm run web:test -- --run NewTaskSheet.test.ts TerminalRawView.test.ts TaskDetail.test.ts`
    passed.
  - `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium`
    passed.
  - `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit`
    passed.
- Task 9:
  - Cursor cleanup delegation completed in chat
    `ca67598a-d381-4373-b7ac-420856d526e8`.
  - Removed obsolete `ajax-task-open` production ownership and stale fixed task
    shell comments/tests.
  - Added `FullscreenLayer` and centralized raw `--app-height` / `--app-top`
    consumption in `AppViewport`; overlays consume semantic app-band sizing.
  - `rtk rg -n -e "--app-height" -e "--app-top" crates/ajax-web/web/src/components crates/ajax-web/web/src/styles.css --glob '!**/*.test.ts'`
    now reports only `AppViewport.svelte`.
- Task 10:
  - `rtk npm run web:check` passed.
  - `rtk npm run web:test -- --run` passed: 28 files, 337 tests.
  - `cd crates/ajax-web/web && rtk proxy npx playwright test` passed: 39
    passed, 1 skipped.
  - The earlier subdirectory Playwright failure was fixed by making
    `playwright.config.mts` anchor its web server cwd to the repo root.
  - Added viewport orientation rebasing so mobile WebKit rotation does not
    masquerade as keyboard-open and suppress terminal resize frames.
  - PR CI repair: Web job failed because WebKit was not installed on the runner.
    Updated `.github/workflows/ci.yml` to run
    `npx playwright install --with-deps chromium webkit`.
