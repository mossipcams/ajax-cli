# React migration S2 — Dashboard (TaskList + ActionBar)

Source of truth: `docs/react-migration-plan.md` §S2 and §§4, 7–10, 13–14.

Status: in progress on branch `ajax/react-s2` (reset to `origin/main` after S1 #571 merged).

Delegation decision: delegated via model-router.

## Scope

- Add mobile-webkit swipe-reveal e2e against the **current Svelte** TaskList first (characterization gap).
- Port `ActionBar` to React (keep `ActionBar.svelte` for TaskDetail until S6; freeze comment).
- Port swipe reveal to `useSwipeReveal` React hook; port `TaskList` to React; island-swap in `App.svelte`.
- Delete `TaskList.svelte` (+ test) in the same slice.

## Non-goals

- No TaskDetail / TestInDev / terminal / settings / shell migration.
- No visual redesign; parity with stable dashboard.
- No shadcn unless class-compatible (default: bespoke markup).
- No frozen-module edits (`api.ts`, `state.ts` logic changes, etc.).
- Compact Test-in-Dev UX is **out of this slice** (stashed separately as `stash@{0}`).

## Task checklist

- [x] **Task 0 — swipe-reveal e2e against Svelte (gap close).**
  - Test first: add mobile-webkit Playwright test — touch-drag a dashboard row, assert reveal width `SWIPE_REVEAL_WIDTH` (88), revealed action tap dispatches.
  - Implement: e2e only; no production migration yet.
  - Verify: focused e2e on mobile-webkit project green against Svelte.
  - Delivered: `crates/ajax-web/web/e2e/swipe-reveal.test.ts` — single test `left swipe opens the row to SWIPE_REVEAL_WIDTH and the revealed action dispatches the operation`. Asserts `.task-row[data-handle="web/fix-login"]` gains `is-revealed` and inline `transform: translateX(-88px)`, the wrap exposes `[data-action="review"]`, and the tap POSTs to `/api/operations` (fetch spy wrapped outside `mockFetch` so the mock still serves the response). Skips non-`mobile-webkit` projects.

- [x] **Task 1 — port ActionBar to React (keep Svelte).**
  - Test first: port `ActionBar.test.ts` → RTL `.test.tsx` assertion-for-assertion (incl. confirm expiry fake timers).
  - Implement: `ActionBar.tsx`; leave `ActionBar.svelte` + freeze/removal-condition comment for S6; no App consumer swap yet for ActionBar in TaskDetail.
  - Verify: focused RTL + existing ActionBar Svelte tests still pass.
  - Validation: `npm run web:test -- --run ActionBar.test.tsx ActionBar.test.ts` → 16 passed, exit 0. `npm run web:check` → exit 0.

- [x] **Task 2 — useSwipeReveal + TaskList React port + island swap.**
  - Test first: `useSwipeReveal` unit tests reusing swipeReveal fixtures; port `TaskList.test.ts` → RTL.
  - Implement: `useSwipeReveal.ts`, `TaskList.tsx`, swap App dashboard outlet to `ReactIsland`, delete `TaskList.svelte` (+ old test). Verify swipeRevealAction consumer list — delete only if TaskList was sole consumer.
  - Verify: focused RTL + Task 0 e2e + smoke/actions/layout-scroll e2e subsets.
  - Validation: `npm run web:test -- --run useSwipeReveal.test.tsx TaskList.test.tsx App.test.ts` → 51 passed, exit 0. `npm run web:check` → exit 0. `npm run web:smoke -- --project=mobile-webkit swipe-reveal.test.ts smoke.test.ts` → 6 passed, exit 0.

- [x] **Task 3 — full automated validation.**
  - Verify: web build/check/test/smoke, ajax-web nextest, `npm run verify` as in S1 Task 5.
  - PASS — parent 2026-07-17: `web:build`; `serviceWorker`=0; `web:check`; `web:build:check`; `web:test` **327 passed (36 files)**; `web:smoke` **115 passed**; `cargo nextest -p ajax-web` **159 passed**; `npm run verify` exit 0.

- [x] **Task 4 — on-device + PR gate.**
  - Matt: filter chips, swipe feel, two-tap destructive, PTR regression, rotation — **validated 2026-07-17**.
  - PR `refactor(web): … (react S2)`; CI; merge; baseline restore — in progress.

## Escalate instead of guessing

- Any e2e that seems to need weakening.
- Any `layout-scroll` invariant change.
- Touch wiring that diverges from passive flags in `swipeRevealAction.ts`.
- New dependencies beyond S1 toolchain.

## Deviations

- Delegates repeatedly returned nonconforming report envelopes (`MISSING_STRUCTURED_REPORT`); parent accepted each round after reviewing diffs and re-running validation.
- `useSwipeReveal` test file landed as `.test.tsx` (packet said `.test.ts`); equivalent coverage.

## Validation results

- Task 0 — swipe e2e mobile-webkit: 1 passed.
- Task 1 — ActionBar svelte+react: 16 passed; web:check clean.
- Task 2 — useSwipeReveal + TaskList + App focused: 59 passed; swipe+smoke mobile-webkit: 6 passed.
- Task 3 — full gate: web:test 327; web:smoke 115; ajax-web 159; `npm run verify` exit 0; serviceWorker 0.
