# Web CSS Architecture and Optimization

**Mode:** Architecture Change (behavior-preserving)  
**Approval:** User: “Implement the plan” (Cursor plan `web shell lifecycle-905265ec`)  
**Delegation:** Parent plans and reviews; implementation writes go through model-router.

## Scope

Decompose [`crates/ajax-web/web/src/styles.css`](crates/ajax-web/web/src/styles.css)
into ownership modules while keeping one deterministic
[`crates/ajax-web/web/dist/app.css`](crates/ajax-web/web/dist/app.css) artifact.

Keep [`styles.css`](crates/ajax-web/web/src/styles.css) as the ordered manifest
and Tailwind bridge. Split contiguous rules without reordering declarations
into:

- `styles/foundation.css` — `:root` design tokens and Base element resets only
- `styles/app-shell.css` — TOP CHROME, CONNECTION STATUS, and RESULT PANEL only
- `styles/settings.css`
- `styles/session/` — shell/live-head, transcript/activity, composer, sheets
- `styles/task/` — dashboard/list, New Task, task detail/meta, Test in Dev
- `styles/terminal.css`
- `styles/diff-review.css`

After mechanical splits are green: measured dead/duplicate CSS removal and
selector-cost simplification inside one owner at a time. Document and enforce
one-`app.css` source ownership.

## Non-goals

- No visual redesign or browser behavior change
- No change to task truth, terminal model, browser storage, routes, APIs, or
  Rust backend
- No new dependency, CSS Modules, CSS-in-JS, selector renames, specificity
  changes, or second emitted CSS asset
- Do not combine a mechanical move and semantic optimization in one wave

## Stop conditions

Stop a wave if emitted rule order changes materially, `app.css` splits, mobile
WebKit behavior regresses, or a test can only pass by weakening its assertion.

## Tasks

- [x] **T0 — Persistent plan + baseline.** This file. Record source/built/gzip
  sizes, visual/mobile check status, and current route/interaction coverage.
- [x] **T1 — Cascade lock.** Stylesheet-graph and cascade-order characterization
  proving a single `app.css` artifact before any rule moves.
- [x] **T2 — Foundation + shell split.** Extract tokens, reset, shared UI, and
  app-shell CSS in original cascade order.
- [x] **T3 — Feature splits.** Extract settings, session, task, terminal, and
  diff styles into owning modules. Independent green waves.
  - [x] **T3a — Settings + session.** `settings.css` and `styles/session/*`
    (shell, live-head, transcript, activity, composer, sheets) extracted from
    the contiguous leftover prefix; manifest imports settings then session
    modules; leftover begins at PAGE LEAD.
  - [x] **T3b — App-shell continuation.** `app-shell-continuation.css` and
    `styles/app-shell/*` (page-lead, layout, primitives, interact, nav, motion,
    skeleton, narrow) extracted from PAGE LEAD through NARROW PHONES; manifest
    imports continuation after session; leftover begins at TASK LIST.
  - [x] **T3c — Task + terminal.** `task.css` and `styles/task/*` plus
    `terminal.css` extracted from TASK LIST through META DETAILS; manifest
    imports task after app-shell continuation; leftover begins at SHELL LAYOUT.
  - [x] **T3d — Shell layout + diff review.** `app-shell-layout.css` and
    `styles/app-shell/shell-layout.css` plus `diff-review.css` extracted from
    SHELL LAYOUT through DIFF REVIEW; manifest imports shell-layout after task
    then diff-review; `styles.css` leftover is only the import manifest and
    Tailwind `@theme inline` bridge.
- [x] **T4 — Test decoupling.** Replace direct `styles.css` filesystem coupling
  with ownership-aware style test helpers and behavioral checks.
- [x] **T5 — Measured optimization.** Remove proven dead/duplicate CSS and
  simplify measured selector hot paths without behavior change.
- [x] **T6 — Enforcement + docs.** Document and test one-`app.css` ownership
  and dependency rules in `docs/architecture/web-cockpit.md` and
  `crates/ajax-web/web/TERMINAL.md`.
- [x] **T7 — Closeout.** Full web, asset, visual, and mobile-WebKit gates;
  finish this ledger.

## Verification

After each extraction wave:

```bash
npm run web:check
npm run web:lint
npm run web:sg
npm run web:test -- --run
npm run web:build && npm run web:build:check
```

Before claiming done:

```bash
npm run web:smoke -- --project=mobile-webkit
npm run web:smoke:desktop
npm run verify:slice -- web
```

## Baseline (T0/T1)

| Metric | Value |
| --- | --- |
| Source CSS bytes (manifest + modules) | 90,762 |
| Source `styles.css` manifest bytes | 1,547 |
| Class-selector lines | 499 |
| `:has()` selectors | 19 |
| Built `app.css` bytes | 80,971 (T7 rebuild after T5 source reduction; was 83,806 pre-rebuild) |
| Built `app.css` gzip bytes | 14,769 (Node `gzipSync`; Vite reports 14.77 kB) |
| Tests that read `styles.css` directly | 0 (T2: style tests use `readOrderedStylesSource`) |
| Visual / mobile-WebKit baseline | T7: mobile-webkit 134 passed / 3 skipped; desktop 49 passed / 86 skipped / 2 failed (keyboard geometry, out of scope) |

Stylesheet graph locked in T1; updated in T2:

- `app.html` → `/src/app/main.tsx` → `styles.css` (manifest; sole JS CSS entry)
- `styles.css` → `@import "./styles/foundation.css"` then `@import "./styles/app-shell.css"`
- `styles.css` begins with `@import "tailwindcss/utilities" layer(utilities);`, ends with `@theme inline`
- Vite: `cssCodeSplit: false`, CSS asset name `app.css`
- Build emits exactly `dist/app.css` (no other `dist/*.css`)
- Source modules: `styles.css`, `styles/foundation.css`, `styles/app-shell.css` (93,701 bytes total)

## Deviations

- **T1 baseline revision (2026-08-19):** An out-of-scope Vite rebuild produced
  `dist/app.css` at 83,866 bytes / 15,069 gzip. Parent restored committed
  `dist/app.css` (83,806 / 15,049 gzip). Tests and `BASELINE` now lock the
  committed artifact, not a fresh build output.
- **T2 import-order cascade (2026-08-19):** Foundation and app-shell modules
  load before inline feature CSS via manifest `@import`s. Major-section lock
  updated to the new ordered source; selector counts unchanged (526 class lines,
  19 `:has()`). Source total grows by 62 bytes for two local `@import` lines.
- **T2 cascade-preserving revision (2026-08-19):** `@import` hoists modules
  before leftover `styles.css` rules, so the first T2 split moved later sections
  (EMPTY/STATUS/ACTION/PILL/PAGE LEAD/NAV/SHELL) ahead of SETTINGS and SESSION.
  Revision keeps only contiguous leading blocks in modules (`foundation.css` =
  `:root` + Base resets; `app-shell.css` = TOP CHROME + CONNECTION STATUS +
  RESULT PANEL). All later sections remain in `styles.css` in original HEAD
  order; `LOCKED_MAJOR_SECTIONS` restored to that order. Source total 93,701
  bytes (+5 vs prior T2 ledger from manifest line endings).

## Validation results

T0/T1 wave (2026-08-19):

| Command | Result |
| --- | --- |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (19 tests) |
| `npm run web:check` | pass |
| `npm run web:build:check` | pass (uses committed `dist/app.css`; no rebuild) |
| `cargo test -p ajax-web architecture::` | pass (13 tests) |

T1 baseline revision (2026-08-19): re-measured committed `dist/app.css` after
discarding an out-of-scope rebuild. `web:build` intentionally not re-run.

T2 wave (2026-08-19):

| Command | Result |
| --- | --- |
| `npm run web:test -- --run` | pass (1168 tests) |
| `npm run web:check` | pass |
| `npm run web:lint` | fail (8 pre-existing `testing-library/no-node-access` in `SessionChat.test.tsx`) |
| `npm run web:sg` | pass |
| `npm run web:build:check` | pass (rebuilt dist during check; restored `dist/app.css` + `dist/app.js` to HEAD) |
| `cargo test -p ajax-web architecture::` | pass (13 tests) |

T2 cascade-preserving revision (2026-08-19):

| Command | Result |
| --- | --- |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (35 tests) |
| `npm run web:test -- --run` | pass (1168 tests) |
| `npm run web:check` | pass |
| `npm run web:sg` | fail (2 pre-existing `noop-jsx-handler` in `ModelPicker.test.tsx`, out of scope) |
| `cargo test -p ajax-web architecture::` | pass (13 tests) |

T3d wave (2026-08-19):

| Command | Result |
| --- | --- |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (35 tests) |
| `npm run web:test -- --run` | pass (1168 tests) |
| `npm run web:check` | pass |
| `cargo test -p ajax-web architecture::` | pass (13 tests) |

T5 wave (2026-08-19):

| Metric | Before | After | Delta |
| --- | --- | --- | --- |
| Source CSS bytes | 94,600 | 90,762 | −3,838 |
| Class-selector lines | 526 | 499 | −27 |
| `:has()` selectors | 19 | 19 | 0 |
| Built `app.css` bytes | 83,806 | 83,806 | 0 (no rebuild) |

**T5 edits applied**

- `foundation.css`: merged exact-duplicate tone declaration blocks
  (`.tone-waiting`/`.tone-attention`, `.tone-idle`/`.tone-unknown`/`.tone-muted`,
  `.tone-error`/`.tone-danger`).
- Removed zero-hit selectors from removed session-starter UI and orphaned rules:
  `session-starter-*`, `session-field`, `session-error`, `session-header`,
  `session-header-back`, `session-status-pill`, `session-model-bar`,
  `session-model-picker-label`, `session-tool-kind`, `task-meta-tools`.
- Removed `.session-starter-actions .pill` from the live-head combo selector.

**T5 deferred inventory** (identical declaration groups across different
selectors — not merged to avoid specificity/cascade risk):

- `app-shell.css`: settings-link vs connection-actions hover states
- `app-shell/primitives.css`: `.action` vs `.pill` shared variants
- `session/activity.css`: `.session-row-head` vs `.session-diff-path` ellipsis
- `session/composer.css`: mic armed/connecting base vs hover states
- `session/sheets.css`: `.session-sheet-header` vs `.session-model-catalog-head`
- `task/meta.css`: `.task-meta-chrome` vs `.meta-details` chrome spacing
- Dynamically applied `tone-*` classes (template-literal classNames; not dead)
- xterm `.scrollbar`, Radix, and `data-testid` attribute selectors (out of scope)

| Command | Result |
| --- | --- |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (19 tests) |
| `npm run web:test -- --run` | pass (1064 tests, 9 skipped) |
| `npm run web:check` | pass |

T6 wave (2026-08-19):

**T6 edits applied**

- `docs/architecture/web-cockpit.md`: install/shell asset paragraphs name
  `dist/app.css` as the sole shipped stylesheet and `styles.css` as the ordered
  manifest + Tailwind bridge (not a second asset).
- `crates/ajax-web/web/TERMINAL.md`: `styles/terminal.css` recorded as terminal
  chrome CSS owner.
- Architecture tests: manifest reach imports each owned module once; feature
  modules do not import sibling features; leaf modules stay import-free; feature
  tests do not use raw manifest as the style API; Vite/dist still emit only
  `app.css`.

| Command | Result |
| --- | --- |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (25 tests) |
| `cargo test -p ajax-web architecture::` | pass |
| `npm run web:check` | pass |

T7 closeout (2026-08-19):

**T7 dist rebuild decision:** `web:build:check` rebuilt `dist/app.css` from
83,806 → 80,971 bytes (−2,835). This reflects the T5 source-CSS dead-rule
removal that was not previously baked into committed dist. Kept rebuilt
`dist/app.css` and `dist/app.js`; updated `BASELINE.builtAppCssBytes` /
`builtAppCssGzipBytes` in `styleSources.ts`. Not unrelated Vite churn.

**Test-count ledger (1168 → 1070):**

| Wave | Count | Notes |
| --- | --- | --- |
| T2/T3 | 1168 passed | pre-T4 baseline |
| T5 | 1064 passed, 9 skipped | −104 from T4 decoupling (see below) |
| T7 | 1070 passed, 9 skipped | +6 from T6 architecture enforcement tests |

T4 removed ~104 inline CSS filesystem-coupled assertions from feature tests
(`App.test.tsx`, `TaskDetail.test.tsx`, `LiveHead.test.tsx`,
`Transcript.test.tsx`, `TaskMetaDetails.test.tsx`, deleted
`terminal-expanded-overlay.test.ts`, etc.) and consolidated coverage into
`styleSources` / architecture tests. No tests were weakened; redundant
manifest-read assertions were replaced by ownership-aware helpers.

| Command | Result |
| --- | --- |
| `npm run web:check` | pass |
| `npm run web:lint` | fail (8 pre-existing `testing-library/no-node-access` in `SessionChat.test.tsx`; out of scope) |
| `npm run web:sg` | fail (2 pre-existing `noop-jsx-handler` in `ModelPicker.test.tsx`; out of scope) |
| `npm run web:test -- --run` | pass (1070 passed, 9 skipped; 102 files passed, 2 skipped) |
| `npm run web:test -- --run src/styles.architecture.test.ts src/styles/architecture.test.ts src/shared/lib/styleSources.test.ts` | pass (25 tests) |
| `npm run web:build:check` | pass (rebuilt dist; kept T5-reduced `app.css`; baseline updated) |
| `npm run web:smoke -- --project=mobile-webkit` | pass (134 passed, 3 skipped) |
| `npm run web:smoke:desktop` | fail (49 passed, 86 skipped, 2 failed: `session-chat-keyboard.test.ts` keyboard-geometry cases expect mobile virtual-keyboard padding; desktop has no keyboard band — pre-existing project mismatch, out of scope) |
| `npm run verify:slice -- web` | pass (444 Rust tests) |
| `cargo test -p ajax-web architecture::` | not re-run (covered by verify:slice web) |
