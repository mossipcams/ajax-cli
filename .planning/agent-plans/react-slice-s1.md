# React migration S1 — foundation

Source of truth: `docs/react-migration-plan.md`, especially S1 and §§4, 7–10, 13–14.

Status: awaiting explicit implementation approval.

Delegation decision: delegated via model-router.

## Scope

- Add the React 19/Vite/Tailwind-v4 coexistence toolchain without preflight.
- Add the one-way Svelte-to-React island seam and root error boundary.
- Port `ConnectionStatus` and `Skeleton` to React with behavior and visual parity.
- Keep the Rust embed contract and all framework-neutral TypeScript unchanged.

## Non-goals

- No shell, routing, polling, terminal, task-list, settings, form, or gesture migration.
- No visual redesign, service worker/PWA install surface, code splitting, or new app state.
- No shadcn component generation yet; only the S1 configuration file required by the blueprint.
- No edits to frozen modules, e2e characterization tests, Rust guard assertions, or files under `tests/`.

## Guardrails

- Preserve `dist/index.html`, `dist/app.js`, and `dist/app.css` exactly as the Rust embed contract expects.
- Keep `styles.css` authoritative; Tailwind utilities/theme only, with no preflight.
- Do not enable React StrictMode.
- Delete each migrated Svelte component in the same slice.
- Delegates may not commit, push, merge, rebase, create/switch branches, or edit outside a packet's allowed paths.
- Each implementation round requires model-router routing, a READY TDD packet, a pre-dispatch snapshot, parent diff review, and parent-run verification.

## Task checklist

- [x] **Task 1 — establish the coexistence toolchain and island lifecycle (10–15 min).**
  - Test first: add `src/react/mountIsland.test.tsx` covering mount, prop update, unmount cleanup, and a throwing child caught by the boundary; run it and record the expected failure before the missing seam exists.
  - Implement: add only the React/Vite/Tailwind/RTL dependencies and TS/Vitest/Vite coexistence settings needed by S1; add `components.json`, `src/react/mountIsland.tsx`, `src/react/ReactIsland.svelte`, and `src/react/ErrorBoundary.tsx`.
  - Verify: rerun the focused test, then `npm run web:check` and `npm run web:build:check` after a fresh web build.

- [x] **Task 2 — port ConnectionStatus through the island (10–15 min).**
  - Test first: port `ConnectionStatus.test.ts` assertion-for-assertion to RTL as `ConnectionStatus.test.tsx`; run it while `ConnectionStatus.tsx` is absent and record the expected failure.
  - Implement: add the minimal `ConnectionStatus.tsx`, swap only this consumer in `App.svelte` to `ReactIsland`, and delete the Svelte component and old Svelte test.
  - Verify: run the focused RTL test plus the existing App shell tests and the two connection-action Playwright cases without changing their assertions.

- [x] **Task 3 — port Skeleton through the island (5–10 min).**
  - Test first: add `Skeleton.test.tsx` for row count, `data-testid`, and decorative `aria-hidden`; run it while `Skeleton.tsx` is absent and record the expected failure.
  - Implement: add the minimal `Skeleton.tsx`, move its existing component CSS unchanged into the authoritative stylesheet, swap the two App consumers to `ReactIsland`, and delete `Skeleton.svelte`.
  - Verify: run the focused RTL test and the existing App skeleton tests; confirm both skeleton test IDs remain unchanged.

- [x] **Task 4 — lock the Tailwind token/no-preflight contract (5–10 min).**
  - Test first: extend the existing source-level design contract to require Tailwind utilities/theme mapping to existing CSS variables and forbid preflight/base imports; run it and record the expected failure.
  - Implement: append only the required Tailwind-v4 utilities/theme declarations to the existing CSS pipeline, reusing the locked Ajax tokens and adding no duplicated color literals.
  - Verify: run the focused design-color test, build, confirm `grep -c serviceWorker crates/ajax-web/web/dist/app.js` returns `0`, and run the Rust install guards.

- [x] **Task 5 — full automated validation (no implementation unless a failure identifies an in-scope defect).**
  - Test: no new test; this task executes the complete frozen characterization and repository gates.
  - Implement: none when green. Any fix requires a new focused failing-test task and another approval checkpoint.
  - Verify in order: `npm run web:build`; service-worker grep; `npm run web:check`; `npm run web:test -- --run`; `npm run web:build:check`; `npm run web:smoke`; `cargo nextest run -p ajax-web`; `npm run verify`; and, if hooks did not already do so, `cargo build --release -p ajax-cli` plus `cargo install --path crates/ajax-cli --locked --force`.

- [x] **Task 6 — shared-dev and real-iPhone validation (manual gate).**
  - Test: deploy this worktree with `scripts/dev-web-restart.sh --worktree /Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-react-migration`, confirm the restart marker and changed `/api/version`, then execute the S1 phone checklist.
  - Implement: none; failures route back to a focused test-first delegated task.
  - Verify: backend-unreachable banner/Retry recovery, banner and skeleton parity against stable `:8787`, and 20 dashboard/settings navigations without console errors.

- [ ] **Task 7 — PR/CI/merge/baseline restore (external-state gate).**
  - Test: confirm all local and on-device rows are recorded green before opening a PR; monitor required CI to completion.
  - Implement: only review-requested fixes through the same delegated TDD loop.
  - Verify: merge only after CI/review pass, restore baseline with `scripts/dev-web-restart.sh`, complete the phone smoke, and update the S1 row in `docs/react-migration-plan.md` with the PR/date/ledger and any explicitly approved behavior delta.

## Escalate instead of guessing

- Any e2e test that appears to require weakening.
- Any Rust guard edit beyond a path/include repoint.
- Any visual difference from stable `:8787`.
- Any dependency beyond the S1 list in the blueprint.
- Any architecture-boundary change or `serviceWorker` string in the bundle.
- Any need to change a frozen framework-neutral module.

## Deviations

- Task 1 delegate round 1 produced valid RED/GREEN evidence but modified tracked
  `dist/app.js` outside the packet and returned a nonconforming report. Review
  gate verdict: `REVISE`.
- Task 1 delegate round 2 restored the generated artifact and corrected every
  code-review finding, but its `FILES_CHANGED` report field still failed the
  router's mechanical inline-list contract. It also reported using prohibited
  `git checkout HEAD -- crates/ajax-web/web/dist/app.js` for the restoration.
- Per model-router, two failed delegate rounds require `STOP`. Task 1 remains
  procedurally stopped even though the current code delta passed parent
  verification. Matt explicitly accepted that verified patch by directing work
  to continue to Task 2 on 2026-07-17.
- Task 2's first Cursor round was terminated by Cursor's loop detector after a
  coherent, scope-clean component port but before returning a report. Parent
  verification found the existing App synchronous-DOM assertion failing
  because `createRoot().render()` had not committed. The Task 2 packet was
  amended before revision to permit one root-cause `flushSync` change in
  `mountIsland.tsx`; consumer waits and assertion changes remain forbidden.

## Follow-ups (out of S1 scope)

- Fullscreen ⛶ exit button lands under the iOS notch. Reported during S1 as a
  migration regression but **proven pre-existing on main**: dev :8788 and stable
  :8787 serve byte-identical fullscreen CSS (same Svelte scope hashes); S1 left
  the terminal/viewport modules untouched and its Tailwind additions are inert
  `@layer utilities`. Cause: expanded panel is `position:fixed; top:var(--app-top,…0px)`
  with no `env(safe-area-inset-top)`, so `.terminal-corner-actions{top:6px}` sits
  under the status bar. Likely fallout from the PR #448 revert. Matt chose to
  finish S1 first and fix this in a separate follow-up task (frozen terminal
  module → own plan/approval). Recorded in memory `fullscreen_notch_button_pre_existing`.
  **UPDATE 2026-07-17:** Matt re-raised ("off the screen") and chose to apply the
  fix now in this worktree. Added one scoped rule to `TaskTerminal.svelte`
  (`.terminal-panel.is-expanded .terminal-corner-actions { top: calc(6px + env(safe-area-inset-top)); }`).
  Rebuilt + redeployed dev :8788 (serves the rule; stable :8787 is the clean
  control). Guards green (159 ajax-web, 20 terminal vitest, web:check clean).
  **This is a non-S1 terminal fix now sitting uncommitted in the S1 worktree —
  it MUST be committed separately from the S1 migration PR to preserve D9
  one-revert.** Not auto-verifiable (safe-area is 0 in test envs);
  **CONFIRMED working on device by Matt 2026-07-17.** **LANDED off `main` as
  PR #569** (`fix/web-fullscreen-notch-button`), separate from S1. Committing it
  surfaced a second, unrelated bug: the husky gate was broken repo-wide because
  the Test-in-Dev `dev_deploy` git probes inherit `GIT_DIR` from the hook env
  (`resolve_rejects_path_outside_ajax_worktrees` fails only in-hook). Fixed as
  **PR #568** (`fix/web-dev-deploy-git-env`, `git_probe()` clears git env);
  #569 is stacked on #568 and should retarget to `main` after #568 merges. See
  memory `husky_gate_git_dir_devdeploy`. The react-migration worktree still
  carries an uncommitted copy of the notch rule (harmless; real fix is #569) —
  drop or ignore it; it must NOT ride into the S1 PR.

## Validation results

- PASS — `npm exec -- vitest run crates/ajax-web/web/src/react/mountIsland.test.tsx --environment jsdom` (2 tests).
- PASS — `npm run web:test -- --run crates/ajax-web/web/src/react/mountIsland.test.tsx` (2 tests).
- PASS — `npm run web:check` (TypeScript and Svelte check, 0 diagnostics).
- PASS — `npm run web:build`.
- PASS — `npm run web:build:check`.
- EXPECTED NONZERO — `grep -c serviceWorker crates/ajax-web/web/dist/app.js` exited 1 and reported 0 matches.
- PASS — generated `dist/app.js`, `dist/app.css`, and `dist/index.html` restored to their pre-verification SHA-256 hashes; no generated file remains in the delta.
- STOP — Task 1 has not passed the model-router procedural gate; full S1 validation has not run.
- PASS — Task 2 baseline Svelte `ConnectionStatus` suite: 5/5.
- RED — Task 2 React suite initially failed because `ConnectionStatus.tsx` did not exist.
- PASS — Task 2 React `ConnectionStatus` suite: 5/5.
- RED — unchanged App suite initially failed because the React root had not committed `.connection-status` synchronously.
- PASS — unchanged App suite after the shared `flushSync` correction: 34/34. The command emits the existing jsdom canvas-not-implemented warning but exits 0.
- PASS — `npm run web:check` after Task 2.
- PASS — focused Playwright connection actions on desktop Chromium and mobile WebKit: 4/4.
- PASS — `git diff --check`; no remaining `ConnectionStatus.svelte` references.
- RED — Task 3 `Skeleton.test.tsx` failed with the intended missing-module error before `Skeleton.tsx` existed (delegate evidence, exit 1).
- PASS — Task 3 React `Skeleton` suite: 3/3 (parent-run).
- PASS — unchanged App suite after both skeleton consumers moved to `ReactIsland`: 34/34 (parent-run).
- PASS — `design-colors` source contract: 2/2; the skeleton CSS moved verbatim into `styles.css` adds no color literals (parent-run).
- PASS — `npm run web:check` after Task 3: 173 files, 0 diagnostics (parent-run).
- PASS — snapshot delta: only the five packet-allowed paths changed; `Skeleton.svelte` deleted; no `Skeleton.svelte` references remain.
- NOTE — Task 3 Cursor round 1 returned a fully conforming DELEGATE_REPORT (STATUS COMPLETE, inline FILES_CHANGED, RED/GREEN evidence); review gate verdict ACCEPT.
- NOTE — router helper scripts (`scripts/check-packet`, `delegate-snapshot`, `router-log`) do not exist in this repo; packet was validated by checklist and the snapshot/delta was done with manual SHA-256 hashing, as in prior rounds.
- RED — Task 4 Tailwind-contract assertions failed as intended (3 new failing, 2 original passing) before the CSS edit (delegate evidence, exit 1).
- PASS — Task 4 `design-colors` suite: 5/5; App suite 34/34; `npm run web:check` clean (parent-run).
- PASS — `npm run web:build`; `grep -c serviceWorker dist/app.js` reported 0 (exit 1); `npm run web:build:check` pass; `cargo nextest run -p ajax-web` 159/159 — all parent-run against the built bundle containing the new Tailwind declarations.
- PASS — `dist/app.js`, `dist/app.css`, `dist/index.html` restored to pre-dispatch SHA-256 parity after the verification build; Task 4 leaves no generated file in its delta.
- NOTE — Task 4 MiniMax (opencode-go/minimax-m3) round 1 returned a fully conforming DELEGATE_REPORT; snapshot delta touched only the two allowed files; review gate verdict ACCEPT. Health probe before dispatch answered in seconds.
- NOTE — the worktree carries a pre-existing 1-line `dist/app.css` drift vs HEAD, hash-verified to predate the Task 4 dispatch (present in the Task 4 pre-snapshot, absent in both Task 3 snapshots; origin not identified). It only affects the minified bundle line; Task 5's `npm run web:build` produces the canonical artifacts.
- PASS — **Task 5 full gate (all parent-run 2026-07-17), exit 0 each:**
  - `npm run web:build` (build_exit=0); `grep -c serviceWorker dist/app.js` → 0 (grep exit 1); `npm run web:check` 173 files/0 diagnostics; `npm run web:build:check` deterministic-shell pass.
  - `npm run web:test -- --run` full suite: **319 passed (35 files)**.
  - `npm run web:smoke` Playwright: **114 passed** (26.0s).
  - `cargo nextest run -p ajax-web`: **159 passed**.
  - `npm run verify`: fmt/check/clippy clean, **1628 Rust tests passed**, doctests pass, web:check clean, web:test **319 passed** (jsdom xterm `getContext` warnings are the known non-fatal WebGL probe).
  - `cargo build --release -p ajax-cli` (exit 0); `cargo install --path crates/ajax-cli --locked --force` (exit 0; replaced global binary; `num-bigint` yanked-in-lockfile warning is informational, install succeeded under `--locked`).
- NOTE — Task 5's build leaves `dist/app.js` + `dist/app.css` genuinely modified: the shippable bundle now contains the React island seam (`createRoot`/`flushSync`, 3 hits) and Tailwind `@layer utilities` (1), still 0 `serviceWorker`. Left as-built for the Task 7 PR (Rust embed contract requires the rebuilt bundle); no restore this time.
- PASS — **Task 6 on-device gate (Matt, 2026-07-17):** dev worktree deployed via `scripts/dev-web-restart.sh --worktree …`; restart marker and changed `/api/version` confirmed; backend-unreachable banner + Retry recovery green; banner and skeleton visual parity vs stable `:8787`; 20× dashboard↔settings navigation with no console errors.
- IN-PROGRESS — Task 7: PR opened; CI/review/merge/baseline-restore pending.
