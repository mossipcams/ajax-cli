# Web viewport / terminal design cleanup

## Scope

Clean up the current Web Cockpit viewport, task-page scrolling, terminal
handling, and paste/copy plumbing without changing the raw Ghostty/tmux-first
terminal architecture.

Primary target: code quality that removes weird behavior: fewer competing
owners, clearer failure paths, less duplicated browser hacking, and smaller
terminal/viewport state surfaces.

## Non-goals

- No terminal engine swap.
- No Live/snapshot/composer terminal mode.
- No browser-owned task state, task truth, or tmux target selection.
- No broad visual redesign.
- No new dependencies.
- No changes under root `tests/`.
- No weakening existing assertions.
- No behavior cleanup that silently drops user-visible error feedback.

## Current findings

- `TerminalRawView.svelte` owns too much: terminal lifecycle, socket glue,
  scroll-follow policy, viewport snapping, keyboard compensation, paste/copy
  fallback UI, selection plumbing, font fitting, and control-key behavior.
- `viewport.ts`, `AppViewport.svelte`, `styles.css`, `TaskDetail.svelte`, and
  `TerminalRawView.svelte` all participate in viewport/keyboard/scroll policy.
- The good seams already exist: `viewport.ts`, `terminalGestures.ts`,
  `terminalGeometry.ts`, `terminalRefit.ts`, `RouteScroll.svelte`, and the
  existing Vitest/Playwright suites.
- The lazy path is not a rewrite. Keep the good pure helpers, move duplicated
  policy to the existing seam that already owns it, and delete component-local
  hacks as each behavior is protected.

## Approval

- Status: approved by user with "approved delegate until finished".

## Delegation decision

`Delegation decision: delegated via model-router`.

Each bounded implementation task should be routed separately through
`model-router`. Svelte/viewport/terminal tasks are likely Cursor/Grok 4.5 High
candidates once a complete TDD packet exists.

## Task checklist

### Task 1: Make viewport document-scroll cleanup a single owner

- [x] Test to write: add a focused `viewport.test.ts` case for an exported document
  scroll reset helper that clears `window`, `documentElement`, `body`, and
  `document.scrollingElement` safely when available.
- [x] Code to implement: export the smallest helper from `viewport.ts`; replace the
  duplicated reset logic in `initViewport` and `TerminalRawView.svelte`.
- [x] Verify: run `rtk npm run web:test -- --run viewport.test.ts TerminalRawView.test.ts`.

### Task 2: Move task-route keyboard chrome policy out of TaskDetail

- [x] Test to write: add/update a colocated component/source or e2e assertion that
  task-route keyboard-open chrome hiding is owned by app/global route layout,
  while `TaskDetail.svelte` only describes task content layout.
- [x] Code to implement: remove global `html.keyboard-open` / `html.terminal-expanded`
  ownership from `TaskDetail.svelte`; keep equivalent behavior in `styles.css`
  or existing app layout only if still needed.
- [x] Verify: run `rtk npm run web:test -- --run TaskDetail.test.ts` and
  `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit`.

### Task 3: Split paste fallback state from terminal transport

- [x] Test to write: source-contract `names paste fallback state transitions`
  asserting `openPasteFallback`, `closePasteFallback`, `sendPasteFallbackText`,
  `requestPaste`, and `data-testid="terminal-paste-fallback"`.
- [x] Code to implement: keep `term.paste()` via `pasteToTerm` in
  `TerminalRawView.svelte`; name fallback open/close/send as component-local
  helpers and use them from clipboard miss/reject, native onpaste, Send, Cancel.
- [x] Verify: run `rtk npm run web:test -- --run TerminalRawView.test.ts` and
  `rtk npm run web:check`.

### Task 4: Extract output-follow and resize error policy from TerminalRawView

- [x] Test to write: add a tiny unit test for the scroll-follow rule: when pinned,
  new output follows bottom; when not pinned, scrollback growth preserves the
  reader's position and marks unseen output. Add one assertion that invalid
  geometry/resize inputs fail closed instead of throwing or sending nonsense.
- [x] Code to implement: move only the pure decision/error math into a small helper
  near terminal modules; leave Ghostty/socket calls in `TerminalRawView.svelte`.
- [x] Verify: run `rtk npm run web:test -- --run TerminalRawView.test.ts terminal*.test.ts`.

### Task 5: Delete any newly dead CSS/state

- [x] Test to write: no new test unless deletion exposes uncovered behavior; rely
  on the layout and terminal tests from Tasks 1-4.
- [x] Code to implement: remove dead comments, state variables, or CSS selectors
  made obsolete by the previous tasks. Audit found no newly dead production
  CSS/state; remaining `ajax-task-open` references are negative regression tests.
- [x] Verify: run `rtk npm run web:check`, `rtk npm run web:test -- --run`, and the
  focused Playwright layout tests used above.

## Validation ledger

- Planning commands run:
  - `rtk sed -n '451,646p' architecture.md`
  - `rtk sed -n '1,240p' crates/ajax-web/web/src/viewport.ts`
  - `rtk sed -n '1,620p' crates/ajax-web/web/src/terminalGestures.ts`
  - `rtk sed -n '1,260p' crates/ajax-web/web/src/terminalGeometry.ts`
  - `rtk sed -n '1,260p' crates/ajax-web/web/src/terminalRefit.ts`
  - `rtk sed -n '1,1520p' crates/ajax-web/web/src/components/TerminalRawView.svelte`
  - `rtk sed -n '1,320p' crates/ajax-web/web/src/components/TaskDetail.svelte`
  - `rtk sed -n '330,720p' crates/ajax-web/web/src/styles.css`
  - `rtk sed -n '1,220p' crates/ajax-web/web/src/components/AppViewport.svelte`
  - `rtk sed -n '1,380p' crates/ajax-web/web/src/components/App.svelte`
  - `rtk cat package.json`
- Task 1 pre-impl (expected fail): `rtk npm run web:test -- --run viewport.test.ts`
  → FAIL: `TypeError: resetDocumentScroll is not a function`
- Task 1 post-impl: `rtk npm run web:test -- --run viewport.test.ts TerminalRawView.test.ts`
  → PASS (150 tests)
- Task 1 post-impl: `rtk npm run web:check` → PASS (0 errors/warnings)
- Task 2 pre-impl (expected fail): `rtk npm run web:test -- --run TaskDetail.test.ts`
  → FAIL: `expected ... not to match /:global\(html\.keyboard-open/`
- Task 2 post-impl: `rtk npm run web:test -- --run TaskDetail.test.ts` → PASS (12 tests)
- Task 2 post-impl: `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit`
  → PASS (6 tests)
- Task 2 post-impl: `rtk npm run web:check` → PASS (0 errors/warnings)
- Task 3 pre-impl (expected fail): `rtk npm run web:test -- --run TerminalRawView.test.ts`
  → FAIL: `expected ... to contain 'openPasteFallback'`
- Task 3 post-impl: `rtk npm run web:test -- --run TerminalRawView.test.ts`
  → PASS (131 tests)
- Task 3 post-impl: `rtk npm run web:check` → PASS (0 errors/warnings)
- Task 3 resume (clipboard-success via pasteToTerm) pre-impl:
  `rtk npm run web:test -- --run TerminalRawView.test.ts`
  → FAIL: `expected ... not to contain 'if (text) term?.paste(text)'`
- Task 3 resume post-impl: same test → PASS (131); `web:check` → PASS
- Task 4 pre-impl (expected fail): `rtk npm run web:test -- --run terminalOutputPolicy.test.ts`
  → FAIL: `Failed to resolve import "./terminalOutputPolicy"`
- Task 4 post-impl: `rtk npm run web:test -- --run terminalOutputPolicy.test.ts TerminalRawView.test.ts`
  → PASS (134 tests)
- Task 4 post-impl: `rtk npm run web:check` → PASS (0 errors/warnings)
- Task 4 parent validation: `rtk npm run web:test -- --run terminalOutputPolicy.test.ts TerminalRawView.test.ts terminalConnection.test.ts terminalGeometry.test.ts terminalRefit.test.ts terminalSelection.test.ts terminalTouchScroll.test.ts`
  → PASS
- Task 5 cleanup audit:
  - `rtk rg -n "resetDocumentScroll|scrollbackGrowthCompensation|outputFollowEffects|validTerminalSize|openPasteFallback|closePasteFallback|sendPasteFallbackText|:global\\(html\\.keyboard-open|:global\\(html\\.terminal-expanded|ajax-task-open|if \\(text\\) term\\?\\.paste|scrollback grows|growth = scrollbackLines|documentElement\\.scrollTop|document\\.body\\.scrollTop" crates/ajax-web/web/src`
  - `rtk rg -n "terminalOutputPolicy|Task 5|Deviations|Task 4" .planning/agent-plans/web-viewport-terminal-design-cleanup.md crates/ajax-web/web/src`
  - Result: no newly dead production CSS/state found; no source cleanup needed.
- Task 5 whitespace check: `rtk git diff --check` → PASS
- Final validation: `rtk npm run web:check` → PASS
- Final validation: `rtk npm run web:test -- --run` → PASS
- Final validation: `rtk npx playwright test e2e/layout-scroll.test.ts --config crates/ajax-web/web/playwright.config.mts`
  → PASS

## Deviations

- None yet.
