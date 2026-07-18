PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Invert the shell to a React root (the capstone of the migration). Port `App.svelte` + `AppShell`/`AppViewport`/`RouteScroll` to `.tsx`, flip `main.ts`→`main.tsx` (`createRoot`, NO StrictMode), point `app.html` at `main.tsx`, wire the S7 hooks, render all children directly (no more `ReactIsland`), move shell styles into `styles.css`, and port the 694-line `App.test.ts` to RTL. Delete every `.svelte` file + the island adapter + `pullToRefreshAction`. This is an APPROVED large atomic port (like the S5 TaskTerminal round) — a half-migrated tree does not build, so it lands in one commit. Behavior parity is mandatory. `package.json`/`vite.config` Svelte-toolchain removal is NOT in this round (that is 1b) — the svelte plugin may stay loaded with zero `.svelte` files.

Worktree: `/Users/matt/Desktop/Projects/ajax-cli__worktrees/react-s7`, branch `ajax/react-s7` (Round 0 + hooks committed; hooks exist at `src/react/useHashRoute.ts`, `usePullToRefresh.ts`, `useViewportBand.ts`).

## Allowed files

- create `crates/ajax-web/web/src/components/App.tsx` (+ `App.test.tsx`)
- create `crates/ajax-web/web/src/components/AppShell.tsx`, `AppViewport.tsx`, `RouteScroll.tsx`
- create `crates/ajax-web/web/src/main.tsx`
- edit `crates/ajax-web/web/app.html` (script src only)
- edit `crates/ajax-web/web/src/styles.css` (append shell styles, de-scoped)
- edit `crates/ajax-web/web/src/legacyTerminalRemoval.test.ts` (repoint `App.svelte` grep → `App.tsx`)
- delete `App.svelte`, `App.test.ts`, `AppShell.svelte`, `AppViewport.svelte`, `RouteScroll.svelte`, `src/main.ts`, `src/react/ReactIsland.svelte`, `src/react/mountIsland.tsx`, `src/gestures/pullToRefreshAction.ts`, `src/gestures/pullToRefreshAction.test.ts`
- regenerate `crates/ajax-web/web/dist/*` via `npm run web:build`

## Forbidden changes

- Do NOT enable React StrictMode anywhere (e2e asserts one-socket terminal cardinality; StrictMode double-mounts effects → double sockets).
- No edits to frozen neutral modules (`api.ts`, `routes.ts`, `polling.ts`, `cockpitPoll.ts`, `viewport.ts`, `state.ts`, `contracts.ts`, gestures pure `pullToRefresh.ts`, terminal modules) beyond imports. No edits to child `.tsx` components (ConnectionStatus, ResultPanel, TaskList, TaskDetail, SettingsView, NewTaskSheet, Skeleton, TaskTerminal, ActionBar) or the three hooks.
- Do NOT remove `svelte`/`@sveltejs/*`/`svelte-check`/`typescript-5` from `package.json`, and do NOT touch `vite.config.mts` or `svelte.config.mjs` (that is round 1b). `web:check` stays as-is this round.
- No e2e edits except none (do not touch the frozen suites). No behavior/testid/DOM-hook/CSS-value changes. Do not weaken any App.test assertion — adapt `?raw` greps to their post-migration source of truth.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence — App.svelte structure (anchor: full file `App.svelte:1-416`)

**State → useState** (`App.svelte:27-44`): `route` (use the `useHashRoute()` hook instead of local state + hashchange), `cockpit`, `detail`, `connection`, `connectionDetail`, `updateAvailable`, `sheetOpen`, `result`, `pullDistance`, `documentVisibility`.
**Non-state refs**: `bootVersion` (`:48`, plain mutable → `useRef<string|null>(null)`); `cockpitApplyGate = createCockpitApplyGate()` and `cockpitPollGuard = createInFlightGuard()` (`:50-51`, created once → `useRef(() => createX()).current` or module-once).
**Derived** (`:46-47,216-220`): `selectedProject`, `taskOpenHandle`, `statusText` — compute in render.
**Functions** (`:53-214`): `showResult`, `applyCockpit`, `applyConnectionError`, `loadCockpit`, `resumeOnOpen`, `loadDetail`, `checkVersion`, `whenIdle`/`cancelIdle`, `go`. Port verbatim; wrap in `useCallback`/refs as needed so effect deps stay correct. Keep `loadCockpit`'s `if (document.hidden) return` guard and the `cockpitPollGuard.run`/`cockpitApplyGate.applyIfChanged` gating EXACTLY.

**FOUR effects — mirror as four useEffect with EXACT dep sets:**
1. Shell listeners (`:154-180`) — `useEffect(…, [])` mount-once: `void loadCockpit()`, `whenIdle(checkVersion)`, add `hashchange`(if not owned by useHashRoute)/`focus`/`pageshow`/`visibilitychange` listeners, return the cleanup. (Route/hashchange is owned by `useHashRoute`; keep `focus`/`pageshow`/`visibilitychange` here. `onVisibilityChange` sets `documentVisibility` + polls when visible.)
2. Adaptive intervals (`:183-194`) — `useEffect(…, [documentVisibility, route.kind])`: `setInterval(loadCockpit, cockpitRefreshIntervalMs({visibilityState: documentVisibility, routeKind: route.kind}))` + `setInterval(checkVersion, versionPollIntervalMs(...))`; cleanup clears both. Deps MUST be exactly `[documentVisibility, route.kind]`.
3. Detail load (`:197-210`) — `useEffect(…, [taskOpenHandle])`: if no handle → `setDetail(null)`; else `setDetail(null)`, `void loadDetail(handle)`, `void resumeOnOpen(handle).then(m => m && loadDetail(handle))`. The Svelte `untrack(...)` wrapper maps to simply keeping the dep list `[taskOpenHandle]` (do NOT add `detail` to deps — that would loop).
4. Document title (`:222-233`) — `useEffect(…, [route])`.

**Markup** (`:236-390`): `<AppViewport>` > `<AppShell chrome={…} nav={…}>{children}</AppShell>`. All `<ReactIsland component={X} props={{…}} />` become direct `<X … />`:
- ConnectionStatus (`:251`), SettingsView (`:280`), TaskDetail (`:296`), Skeleton (`:305,308,341`), TaskList (`:327`), ResultPanel (`:364`), NewTaskSheet (`:378`). Keep every prop/callback identical. Keep the outlet `<section data-outlet=… data-testid=…>` wrappers and all conditionals (`route.kind` branches, `{detail ? … : <Skeleton/>}`, `{cockpit ? … : <Skeleton/>}`, `{result && …}`, `{sheetOpen && …}`).
- Dashboard outlet uses `usePullToRefresh({ onRefresh: () => loadCockpit(), onDistance: setPullDistance })` as a ref callback on the `<section>`; keep `.pull-indicator`/`armed`/`style={{height: pullDistance+"px"}}` and `PULL_THRESHOLD`.
- `class:is-live={connection === "connected"}` (`:245-249`) → `className={\`live-dot\${connection === "connected" ? " is-live" : ""}\`}`.
- update-banner (`:264-271`): `<button className="update-banner" hidden={!updateAvailable} onClick={() => location.reload()}>Update ready — tap to reload</button>`.
- bottom nav (`:349-358`): keep `data-bottom-route`, `data-bottom-action`, `aria-current` logic; New button → `setSheetOpen(true)`.

**Shell components:**
- `AppShell.tsx` (`AppShell.svelte`): props `{ chrome: ReactNode; children: ReactNode; nav: ReactNode }`; render `<div data-testid="app-shell" className="app-shell">{chrome}<main data-testid="app-main" className="app-main">{children}</main>{nav}</div>`. Styles → styles.css.
- `AppViewport.tsx` (`AppViewport.svelte`): call `useViewportBand()`; render `<div data-testid="app-viewport" className="app-viewport">{children}</div>`. Styles (incl. `:global(html.keyboard-open) .app-viewport` → plain `html.keyboard-open .app-viewport`) → styles.css.
- `RouteScroll.tsx` (`RouteScroll.svelte`): `<div data-testid="route-scroll" className="route-scroll">{children}</div>`. No styles.

**main.tsx** (`main.ts:1-9`): `import { createRoot } from "react-dom/client"; import App from "./components/App"; import "./styles.css";` then `const el = document.getElementById("app"); if (el) createRoot(el).render(<App />); else console.error(...)`. Wrap `<App/>` in the root `ErrorBoundary` from `src/react/ErrorBoundary`. NO StrictMode.
**app.html** (`app.html:14`): `<script type="module" src="/src/main.ts">` → `src="/src/main.tsx"`.

## Context evidence — App.test.ts port (anchor: `App.test.ts:1-694`)

- Framework: `@testing-library/svelte`→`@testing-library/react`; `render(App)`→`render(<App/>)`; import `App from "./App"`. Svelte `tick()` (`:5`) has no React analog — replace with `await waitFor(...)`/`await act(async()=>{})`/a microtask flush as each assertion needs. Keep `StubWebSocket` global (`:35-41`), `setHash` (`:43-46`, dispatches `HashChangeEvent` — works with `useHashRoute`), `jsonResponse`, `loadStylesSource`/`taskTerminalStylesSection`/`taskTerminalMobileBlock` (`:12-30`), and the fetch stub.
- `?raw` repointing (do NOT weaken):
  - `appSource` (`App.svelte?raw`) → `App.tsx?raw`. Adapt Svelte-syntax asserts to TSX: `:92` `/class:is-live=\{connection === "connected"\}/` → assert the TSX conditional that adds `is-live` when connected (e.g. `/is-live[\s\S]*connection === "connected"|connection === "connected"[\s\S]*is-live/`). `:125` `not.toMatch(/initViewport/)`, `:127` `/ajax-dashboard-open/`, `:130` `/--app-height|--app-top/` stay as absence asserts on `App.tsx?raw`.
  - `appViewportSource` (`AppViewport.svelte?raw`) → `AppViewport.tsx?raw` for the `:126` `toMatch(/initViewport/)` presence assert. The CSS asserts on it (`:128,129,135-147`: `--app-band-top`, `--app-band-height`, the `keyboard-open` rule) move to `stylesSource` (styles.css) since AppViewport styles relocate there — repoint those to `stylesSource`, dropping the `:global(...)` wrapper in the regex (match `html.keyboard-open .app-viewport`).
  - `stylesSource` (already `loadStylesSource()` reading styles.css) asserts stay valid — shell CSS now lives in styles.css.
- Behavioral tests (`:56-694`, ~30): render `<App/>`, same assertions (title per route, live-dot, skeletons, outlets, resume-once + re-resume, stale detail ignored, version-idle defer, update banner, connection error kinds, detail failure/recovery). Port assertion-for-assertion.

## Test-first instructions

1. Create `App.test.tsx` first (ported), run the focused red command (App.tsx absent → import failure):
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/App.test.tsx
   ```
   Record RED, then implement `App.tsx` + shell components + hooks wiring + main.tsx to GREEN.
2. The port is large — if you hit an output limit, prioritize App.tsx + shell + main flip building green, then complete the full App.test.tsx port; you MAY be resumed to finish, but do not leave the tree non-building.

## Edit instructions

Follow the Context evidence above file-by-file. Order: (1) App.tsx + AppShell/AppViewport/RouteScroll.tsx; (2) move shell styles into styles.css; (3) main.tsx + app.html; (4) delete the `.svelte` files + `main.ts` + `ReactIsland.svelte` + `mountIsland.tsx` + `pullToRefreshAction.ts(+test)`; (5) repoint `legacyTerminalRemoval.test.ts` `App.svelte`→`App.tsx`; (6) port App.test.ts→App.test.tsx; (7) `npm run web:build`.

## Verification commands (ALL must pass — includes the Playwright smoke that CI runs)

```bash
npm run web:test -- --run
npm run web:check
npm run web:build
grep -c serviceWorker crates/ajax-web/web/dist/app.js   # expect 0
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
npm run web:smoke -- --project=desktop-chromium
cargo nextest run -p ajax-web
```

## Acceptance criteria

- No `.svelte` files remain under `src/`; `main.tsx` is the entry; `app.html` points at it; island adapter + `pullToRefreshAction` gone.
- Full `web:test` green (App.test.tsx ported, no weakened asserts); `web:check` clean; `web:build`+`build:check` pass; `serviceWorker`=0; `cargo nextest -p ajax-web` green.
- `web:smoke` green on BOTH projects — especially `terminal-behavior.test.ts` (one-socket cardinality, no "element is not stable"), `layout-scroll`, `smoke`, `actions`, `visual`, `shell-characterization`.
- No StrictMode; behavior/testids/CSS values unchanged; diff limited to Allowed files.

## Stop conditions

- Any terminal e2e regresses (socket cardinality or "element is not stable") — stop, report (effect/StrictMode/timing issue).
- A `?raw` grep cannot be repointed without weakening intent — stop, report.
- An effect's timing can't reproduce the Svelte `$effect` behavior (double cockpit fetch, polling not rescheduling, detail-load loop) — stop, report (this is an approved-behavior-change decision, not the worker's call).
- Any need to edit a frozen module, a child `.tsx`, `vite.config`, or `package.json` — stop, report.
- Patch sprawls beyond the Allowed files.
