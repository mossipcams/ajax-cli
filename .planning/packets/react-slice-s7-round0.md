PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Close the S7 characterization gap: add two Playwright e2e tests — **update banner** and **pull-to-refresh** — that pass **green against the current Svelte `App`**. These lock the behavior before the S7 shell inversion ports `App.svelte` → `App.tsx`. New self-contained test file; no production or fixtures edits.

Worktree: `/Users/matt/Desktop/Projects/ajax-cli__worktrees/react-s7`, branch `ajax/react-s7`.

## Allowed files

- `crates/ajax-web/web/e2e/shell-characterization.test.ts` (create)

## Forbidden changes

- No production edits (`App.svelte` etc.). No edits to `fixtures.ts` or any existing e2e suite (self-contained mocking in the new file).
- No new dependencies. Do not weaken or duplicate existing tests.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- **Update banner**: `App.svelte:262-269` — `<button class="update-banner" hidden={!updateAvailable} onclick={() => location.reload()}>Update ready — tap to reload</button>`. Set by `checkVersion()` (`App.svelte:130-135`): fetches `/api/version`, and `if (version !== bootVersion) updateAvailable = true`. `bootVersion` is captured on first version read; polling interval `versionPollIntervalMs`.
- **Version mock**: `fixtures.ts:mockFetch` overrides `globalThis.fetch` via `addInitScript` with a static route map; `/api/version` → `VERSION_A = {version:"0.20.5"}`; `VERSION_B = {version:"0.21.0-new"}` also exported (`fixtures.ts:68-69`). No existing time-varying `/api/version` — the new test must supply its own call-count-aware `/api/version` handler (A on first read = bootVersion, B on later polls).
- **Reload assertion precedent**: `actions.test.ts:186-194` — `await Promise.all([page.waitForEvent("load"), reload.click()])`.
- **Pull-to-refresh**: dashboard outlet `App.svelte:316` `use:pullToRefresh={{ onRefresh: () => loadCockpit(), onDistance: … }}`; `armed={pullDistance >= PULL_THRESHOLD}` (`App.svelte:320`). `PULL_THRESHOLD` from `../gestures/pullToRefresh`. Trigger = drag ≥ threshold → `loadCockpit()` → another `/api/cockpit` fetch.
- **Touch-drag precedent**: `swipe-reveal.test.ts:60-70` — dispatch `touchstart`/`touchmove`/`touchend` via an in-page `make(type,x,y)` helper on the target element (runs on mobile-webkit, `pointer: coarse`).
- **Boot**: tests `page.goto("/app.html")` then assert visible text (`smoke.test.ts`). Dashboard handles: `web/fix-login`, `api/add-auth` (COCKPIT_FIXTURE).

## Code anchors

- Reuse `mockFetch(page)` from `./fixtures` for the baseline routes, then **override `/api/version`** for the update-banner test with a stateful `addInitScript` added AFTER `mockFetch` (last fetch override wins) OR pass a bespoke handler — keep it in the test file.
- Selectors: `button.update-banner` (assert `hidden` attr flips / becomes visible); dashboard outlet `[data-testid="outlet-dashboard"]` or `[data-outlet="dashboard"]`.

## Test-first instructions

NOT_APPLICABLE: tests-only characterization. Both tests must pass against the **current Svelte** implementation (they describe existing behavior, not a change). If either cannot be made green against Svelte, STOP and report — do not modify production to force green.

## Edit instructions

Create `e2e/shell-characterization.test.ts` with two tests:

1. **"update banner appears on version change and reloads on tap"**
   - `addInitScript` a stateful `/api/version` (or wrap `mockFetch` + a second init script): first call returns `{version:"0.20.5"}` (bootVersion), subsequent calls `{version:"0.21.0-new"}`. Keep other routes from `mockFetch`.
   - `page.goto("/app.html")`; wait for dashboard (`web/fix-login` visible).
   - Wait for `button.update-banner` to become visible (poll interval elapses; use a generous `toBeVisible({ timeout })`).
   - Click it inside `Promise.all([page.waitForEvent("load"), banner.click()])`; assert the reload fired.
   - Prefer running on both projects (no touch needed).

2. **"pull-to-refresh past threshold reloads the cockpit"** (mobile-webkit; guard with `test.skip(project !== mobile-webkit)` if the drag needs coarse pointer)
   - `mockFetch(page)` but make `/api/cockpit` increment an in-page counter (e.g. `window.__cockpitCalls`) so the test can read call count; or count via `page.on("request", …)` for `/api/cockpit`.
   - `page.goto("/app.html")`; wait for dashboard.
   - Record cockpit call count; dispatch a touch-drag on the dashboard outlet from a start Y to an end Y with `Δ ≥ PULL_THRESHOLD` using the `swipe-reveal.test.ts` `make()`/dispatch pattern.
   - Assert the cockpit call count increased (i.e. `loadCockpit()` fired). Use `expect.poll` for the async refetch.

Keep both tests hermetic (no live server); match the file header style of `smoke.test.ts`.

## Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/shell-characterization.test.ts
npm run web:smoke -- --project=desktop-chromium crates/ajax-web/web/e2e/shell-characterization.test.ts
```
(If `web:smoke` does not accept a path filter, use `--grep "update banner|pull-to-refresh"`.)

## Acceptance criteria

- Both tests green against the current Svelte `App`, on their targeted project(s).
- No production/fixtures/existing-suite edits; only the new file added.
- No new dependencies; no flakiness (use `expect.poll`/`toBeVisible` timeouts, not fixed sleeps).

## Stop conditions

- A test cannot pass against Svelte without editing production — stop, report (behavior may differ from the packet's model of it).
- The touch-drag can't trigger `pullToRefresh` in mobile-webkit after two attempts — stop and report the harness limitation (do not fake the assertion).
- `web:smoke` requires infra not available in the worktree — report instead of guessing.
