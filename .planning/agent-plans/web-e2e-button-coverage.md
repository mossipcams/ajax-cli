# Web E2E: fullscreen refit regression + full button/action coverage

## Context

Last release, the fullscreen (⛶ expand) button broke the PWA by leaving the
terminal zoomed after the visual viewport settled. The geometry *fuzzer*
(`terminalGeometry.fuzz.test.ts`) could not catch it: it only exercises pure
scalar functions, and the bug was a DOM/timing orchestration miss (a missing
post-settle refit), not a math error. #375 fixed it and added jsdom unit
regressions (`TerminalRawView.test.ts` "re-fits again after the expand viewport
settles"). The remaining gap is **real-browser (webkit) proof** that expand
keeps the terminal fitted, plus general e2e coverage of every button/action.

An *external* fuzzer would not help — same category (pure-function property
testing). The right tool for this bug class is Playwright, which is already
wired (`mobile-webkit` = iPhone 12).

## Scope

- Extract the shared mock harness out of `e2e/smoke.test.ts` into
  `e2e/fixtures.ts` (behavior-preserving refactor; smoke imports it).
- `e2e/fullscreen-refit.test.ts` — real-webkit regression for the expand bug.
- `e2e/actions.test.ts` — one test per interactive control not already covered
  by smoke/layout-scroll/visual.

## Non-goals

- No source/behavior changes to the app. Tests only.
- No new deps, no external fuzzer.
- Do not re-cover controls smoke.test.ts already exercises (project filter,
  review/drop actions, terminal toolbar keys, paste/resize/reconnect, new-task
  keyboard band, connection Retry, settings render, update banner).

## Coverage gap inventory (buttons NOT yet e2e-covered)

- Terminal: **Expand → fullscreen** (regression), **Exit fullscreen**, **Hide keyboard**
- TaskDetail: **Back**, **Copy branch**, **Copy worktree**
- SettingsView: **Back**, **Restart** (two-tap confirm → POST /api/server/restart), **Run diagnostics**, **Copy Diagnostics**
- ResultPanel: **Dismiss**
- NewTaskSheet: **Cancel**, **Start** (submit → create)
- App: **Settings link**, bottom-nav **Dashboard**, **update-banner reload**
- ConnectionStatus: **Reload**, **Copy Diagnostics**

## Delegation decision

Delegated to cursor-delegate (Composer 2.5) per AGENTS.md lane 2 (web frontend).
Harness extraction (T1), fullscreen regression (T2), and 9/13 actions tests were
authored + verified green inline before the correction; the remaining 4 failing
actions tests are delegated with the diagnosis below. Cursor must not touch app
source or the passing tests, and must land both playwright projects green.

### Work order for cursor (fix these 4 failing tests in e2e/actions.test.ts)

1. "task detail Copy buttons…": `.meta-copy` buttons live inside a collapsed
   `<details class="meta-details">`. Expand it first — click the summary
   ("Task details") — then click `.meta-copy` nth(0)/nth(1).
2. "terminal Hide keyboard…": ghostty input is `term.textarea`; select
   `[data-testid='task-terminal-panel'] textarea` (not `.xterm-helper-textarea`).
3. "connection Reload…": the `location.reload` stub does not take in webkit.
   Replace with a reliable signal (e.g. race `page.waitForEvent('load')` on
   click) or, if not reliable, assert the Reload button is present+enabled in
   `.connection-actions` and drop the call-count assertion. No app changes.
4. "new task sheet Start…": diagnose by running; the sheet did not close.
   Confirm the `/api/tasks` POST mock returns a success mutation and the submit
   fires. Keep assertions on sheet close + "Task started" result.

Validation: `npm run web:smoke` (both projects) must be fully green. Do not
weaken assertions to pass. Do not edit app source.

## Task checklist

- [x] T1 Extract `e2e/fixtures.ts`; slim `smoke.test.ts` to import it. Verified: 22/22 smoke green (webkit + chromium).
- [x] T2 `fullscreen-refit.test.ts`: expand → asserts expanded markers + post-expand resize frame (finite grid) + visualViewport.scale===1 (not zoomed); exit restores inline. Verified green both projects.
- [x] T3 `actions.test.ts`: 13 gap-fill tests. 9 authored+green inline; 4 delegated to cursor and fixed. Verified green both projects.
- [x] T4 Full e2e (both projects): 69 passed, 1 pre-existing skip. Web unit: 356 passed.

## Deviations

- cols≥80 floor only holds in "wide" geometry mode; default fits real width (43 cols on 390px). Regression asserts finite positive grid instead.
- Copy-buttons test forces a desktop viewport: `.meta-details` (branch/worktree Copy) is `display:none` on the mobile layout.
- Reload test performs a real page reload (race `waitForEvent('load')`); the `location.reload` stub was not observable in webkit.

## Validation commands + results

- `playwright test --config crates/ajax-web/web/playwright.config.mts` → 69 passed, 1 skipped (pre-existing visual.test.ts mobile skip). EXIT 0.
- `vitest --config crates/ajax-web/web/vite.config.mts --run` → 30 files, 356 tests passed. EXIT 0.
