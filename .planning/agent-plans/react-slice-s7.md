# React Slice S7 — Shell inversion + Svelte removal (FINAL)

Worktree: `ajax-cli__worktrees/react-s7`, branch `ajax/react-s7`, based on `ajax/react-s6` tip `2d9da64` (S6 + main 0.50.6).
Blueprint: `docs/react-migration-plan.md` §7 S7 + §12 removal criteria. Deps: S2–S6 (S6 = PR #583, still open — **S7 stacks on #583 and cannot merge until it lands**).

## Scope

Invert the shell to a React root and delete the Svelte toolchain entirely:
- Port `App.svelte` (416), `AppShell.svelte` (40), `AppViewport.svelte` (44), `RouteScroll.svelte` (13) → `.tsx`.
- New hooks in `src/react/`: `useHashRoute`, `usePullToRefresh`, `useViewportBand` (wraps `initViewport`).
- `main.ts` → `main.tsx` (`createRoot`); `app.html` script → `/src/main.tsx`; root `ErrorBoundary`.
- Delete island adapter (`ReactIsland.svelte` + `mountIsland.tsx`), `pullToRefreshAction.ts`(+test), `svelte.config.mjs`, `scripts/svelte-check-legacy-ts.cjs`.
- `package.json`: drop `svelte`, `@sveltejs/vite-plugin-svelte`, `svelte-check`, `@testing-library/svelte`, `typescript-5`; `web:check` → `tsc --noEmit` only; remove svelte plugin + `svelteTesting` from `vite.config.mts`.
- Repoint `legacyTerminalRemoval.test.ts` App.svelte greps → `App.tsx`; update `install.rs`/`architecture.md`/`TERMINAL.md` prose that says "Svelte" (wording only, assertions unchanged).

## Non-goals

- No behavior change; e2e frozen (edited only to ADD the two new characterization tests). Frozen TS modules byte-identical except imports.

## Delegation decision

`Delegation decision: delegated via model-router`

## Decomposition

- [x] **Round 0 — characterization gap (MANDATED FIRST, against Svelte)** — delegated to Cursor/composer-2.5, ACCEPTED. New `e2e/shell-characterization.test.ts`; independently reverified: mobile-webkit 2/2 (update-banner + pull-to-refresh), desktop-chromium update-banner pass + pull-to-refresh skipped (coarse-pointer only).
  - Add e2e for the **update banner** (mock `/api/version` change → banner appears, tap reloads) and **pull-to-refresh** (touch-drag ≥ `PULL_THRESHOLD` → cockpit reload), green against the current Svelte `App`. Commit before any port.
- [~] **Round 1 — shell port + framework removal** (split into sub-rounds)
  - [x] **1a-i — hooks** (`useHashRoute`, `usePullToRefresh`, `useViewportBand` + tests). Cursor/composer-2.5, ACCEPTED (nonconforming envelope, content reverified). Additive; hook tests 7/7, full web:test 327/327, web:check clean. No existing files touched.
  - [x] **1a-ii — root inversion**: App.tsx (4 effects, exact deps, no StrictMode) + AppShell/AppViewport/RouteScroll.tsx + main.tsx createRoot + app.html + hooks wired + shell styles→styles.css + delete all .svelte + island adapter + pullToRefreshAction; ported App.test.ts→.test.tsx (34 tests). Cursor/composer-2.5, ACCEPTED (nonconforming envelope, fully reverified). Gate: 0 .svelte, vitest 321/321, web:check clean, build+build:check, sw=0, mobile-webkit smoke 92, desktop 26, nextest 159/159. Necessary fallout (verified not weakened): repointed TaskDetail.test.tsx + keyboardBandPin.test.ts raw imports, deleted orphan mountIsland.test.tsx. Callback stability confirmed (resume-once preserved).
  - [ ] **1b — toolchain/dep/config removal**
  - App/AppShell/AppViewport/RouteScroll → `.tsx`; hooks; `main.tsx` + `createRoot`; root ErrorBoundary; `App.test.ts` → `.test.tsx`; delete island adapter + `pullToRefreshAction`; remove svelte plugin/deps/config; `app.html` + `web:check` + guard repoints + prose.
  - May split into implement→resume sub-rounds along: (a) App.tsx + hooks + shell components, (b) main.tsx flip + toolchain/dep/config removal + guard/prose. Each sub-round must leave `npm run verify` green.

## Risks (from §7 S7)

- React StrictMode double-effect → double sockets/double cockpit fetch. Do NOT enable StrictMode (e2e asserts one-socket cardinality).
- Adaptive-polling effect deps must reschedule on route/visibility exactly (two `$effect` → two `useEffect` with identical dep sets).
- `untrack` in the detail-load effect has no React equivalent — structure deps so `detail` reset doesn't loop.
- Once App is the React root, the island adapter + svelte plugin are removed in the same change.

## Validation commands (per round)

```bash
npm run web:build && grep -c serviceWorker crates/ajax-web/web/dist/app.js
npm run web:check
npm run web:test -- --run
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
cargo nextest run -p ajax-web
npm run verify
# §12: grep -ri svelte package.json crates/ajax-web/web/src  → nothing (after Round 1)
```

## On-device gate (Matt — full §9 regression day)

Every §9 row: keyboard band, terminal, gestures, routing, resume, update banner, pull-to-refresh, rotation, add-to-home-screen launch.

## Policy

Commit each round the moment it passes the gate (worktrees can be reaped — see S6). Background the slow verify hook.

## Deviations / Validation results

- (pending Round 0)
