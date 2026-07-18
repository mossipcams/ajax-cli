# Ajax Web: Svelte → React + TypeScript + shadcn/ui Migration Blueprint

Status: S4 merged. S5 verify green + on-device §9 passed — PR open.
Grounded against: branch `ajax/react-migration`, forked from `main` @ `c547918` (0.50.0), 2026-07-17.
Orchestrator: GPT-5.6 Sol (see §13). Implementation: delegated per slice via `model-router` → `tdd-implementation-packet` per `AGENTS.md`.

Anything that could not be verified from the repository is marked **[UNVERIFIED]**.

---

## 1. Current-state findings

### 1.1 It is not SvelteKit

The frontend is a **plain Svelte 5 (runes) + Vite 6 SPA** at `crates/ajax-web/web/`. There is no SvelteKit, no SSR, no file-based routing, no server load functions. Entry chain:

- `crates/ajax-web/web/app.html` → `src/main.ts` → `mount(App, { target: #app })`
- Vite emits **deterministic, unhashed** `dist/index.html`, `dist/app.js`, `dist/app.css` (see `vite.config.mts`: `entryFileNames: "app.js"`, `cssCodeSplit: false`, rename plugin `app.html → index.html`).
- The Rust crate `ajax-web` **embeds those three files at compile time** via `include_bytes!` in `crates/ajax-web/src/adapters/assets.rs`, fingerprints them into `/api/version` (FNV-1a), and serves them at `/`, `/app.js`, `/app.css` (`crates/ajax-web/src/runtime.rs:381-400`).

**Consequence (the single most important migration fact):** the Rust side is completely framework-agnostic. As long as the build keeps emitting `dist/index.html` + `dist/app.js` + `dist/app.css` with the `__AJAX_APP_VERSION__` placeholder and `id="app"` mount node, swapping Svelte for React is invisible to every Rust crate, the server, TLS, sessions, and the deploy pipeline. No Next.js, no SSR — repository evidence is decisively against both.

### 1.2 Component inventory and dependency graph

15 Svelte components, ~3,400 lines total:

```
App.svelte (396)                     — shell orchestrator: hash routing, cockpit/version
 ├─ AppViewport (44)                 —   polling, connection state, result panel, sheet
 ├─ AppShell (40)                    — visualViewport band host ($effect → initViewport)
 ├─ RouteScroll (13)                 — chrome/main/nav slots
 ├─ ConnectionStatus (33)            — single scroll owner
 ├─ Skeleton (49)
 ├─ ResultPanel (52)                 — toast/undo/commit surface
 ├─ TaskList (480) ── ActionBar (152), swipeRevealAction
 ├─ TaskDetail (445) ── ActionBar, TaskTerminal, TestInDevPanel (215)
 ├─ SettingsView (240)
 └─ NewTaskSheet (329) ── FullscreenLayer (29), sheetDragAction
TaskTerminal.svelte (1592)           — xterm 6.0 + fit addon; imperative core
```

`ActionBar` is shared by `TaskList` and `TaskDetail` — the only shared leaf that forces a documented temporary duplicate (see slice S2/S5).

### 1.3 Already framework-neutral TypeScript (~2,000 lines — keep byte-identical)

These modules contain **zero Svelte imports** and must not be rewritten during migration:

| Module | Role |
|---|---|
| `src/api.ts` | Same-origin transport, session renewal, typed errors, `openTaskTerminalSocket` (`/api/tasks/{handle}/terminal` WS) |
| `src/contracts.ts` | Hand-written response guards (`IncompatibleResponseError`) |
| `src/types.ts` | All API/browser types |
| `src/state.ts` | Pure presentation helpers (status meta, sort, relative time) — *not* a store |
| `src/polling.ts`, `src/cockpitPoll.ts` | Adaptive intervals; apply-gate + in-flight guard |
| `src/routes.ts` | Hash-route parse/format (`#/`, `#/settings`, `#/t/…`, `#/p/…`) |
| `src/taskActions.ts`, `src/taskSlug.ts`, `src/diagnostics.ts` | Small pure helpers |
| `src/terminalConnection.ts` | WS lifecycle, backoff, foreground reconnect |
| `src/terminalGeometry.ts`, `src/terminalRefit.ts` | Cols/font geometry, refit controller |
| `src/viewport.ts` | iOS keyboard band (`--app-top`/`--app-height`, `keyboard-open` class, 250ms close-settle) |
| `src/gestures/{pullToRefresh,sheetDrag,swipeReveal}.ts` | Pure gesture math |

Svelte-coupled adapters (thin, to be replaced by React hooks per slice): `gestures/*Action.ts` (Svelte `use:` actions) and the `$state`/`$derived`/`$effect` runes inside components. There are **no `svelte/store` writables anywhere** — App.svelte holds all shell state in local runes, explicitly as a "shallow, replaceable projection of server truth." This maps 1:1 onto `useState` in a React root component.

### 1.4 Tests

- **Vitest (jsdom, globals)** — `src/**/*.test.ts`, run via `npm run web:test -- --run`. Mix of pure-TS unit tests (survive migration untouched) and `@testing-library/svelte` component tests (ported per slice to `@testing-library/react`).
- **Playwright e2e** — `web/e2e/{smoke,actions,layout-scroll,terminal-behavior,visual}.test.ts`, projects `desktop-chromium` + `mobile-webkit` (iPhone 15 Pro), served by Vite dev server on :5173 with `/api` proxied to the Rust dev server on :8788. **These are testid/behavior-driven and framework-agnostic — they are the characterization suite for the whole migration and must pass unmodified for every slice.** Coverage: routing, actions, two-tap destructive confirm, single-scroll-owner invariants, keyboard band, terminal socket lifecycle/cardinality/paste/fullscreen (23 terminal tests), and stylesheet application.
- **Rust guard tests** (must stay green — they are contract, not implementation):
  - `crates/ajax-web/src/slices/install.rs`: shell has one script + one stylesheet, version placeholder replaced, `id="app"`, no legacy DOM, **no manifest/apple-touch-icon/`/sw.js`**, CSS carries the cockpit palette hexes, safe-area insets, `keyboard-open`, ≥16px inputs, **no `100vh`**; bundle contains `/api/cockpit`, `/api/operations`, `#/settings`, `request_id`, `no-store` and **must not contain the string `serviceWorker`**, `pushManager.subscribe`, `/answer`, `/input`.
  - `src/design-colors.test.ts`: `:root` custom props in `styles.css` must match the `colors:` map in `DESIGN.md` (colors are locked).
  - `src/legacyTerminalRemoval.test.ts`: greps specific files (including `App.svelte`, `TaskDetail.svelte`, `SettingsView.svelte`) for banned legacy symbols. Slices that delete these files must repoint the guard at the `.tsx` successor **without weakening any assertion**.
  - `scripts/web-build-check.mjs` (`npm run web:build:check`): exactly one `dist/*.js`, no stale `terminal.js`/`ghostty-vt.wasm`, placeholder intact.

### 1.5 PWA / service worker reality — prompt discrepancy, labeled

The task prompt says "iOS Safari PWA." Repository evidence: the Home-Screen install surface was **deliberately retired** — no `manifest.webmanifest`, no icons, no service worker, and `install.rs` + `AGENTS.md` ("no Home Screen PWA dependency; no service worker/offline mutation model") **enforce their absence**. What remains is safe standalone metadata (`apple-mobile-web-app-*`, `theme-color`, `viewport-fit=cover`). Therefore: this plan validates the **normal iOS Safari browser experience** (plus safe-area/standalone metadata), and treats any request to reintroduce manifest/SW as an explicitly rejected scope change. Do not add a service worker in any slice — a guard test will fail the build if a React dependency even embeds the string `serviceWorker`.

### 1.6 iOS-coupled behavior Playwright WebKit cannot adequately validate

- Real `visualViewport` keyboard geometry (Playwright simulates it in `layout-scroll`/`terminal-behavior` via injected bands; real iOS timing — address-bar drift, the 250ms close-settle in `viewport.ts`, autocorrect popover transients — needs a device).
- Native paste invocation (the terminal Paste key is the native iOS paste trigger; `readText` requires HTTPS; LAN-HTTP fallback path must stay).
- iOS momentum scrolling / pinch inside the terminal, focus-without-zoom (16px inputs), backgrounding → WS drop → foreground reconnect.
- Fallback tooling when no device is at hand: iOS Simulator via `xcrun simctl` + idb (established repro workflow), but **actual-iPhone validation is mandatory per slice** for anything touching keyboard, terminal, gestures, or layout.

### 1.7 Dev workflow (verified from `scripts/dev-web-restart.sh` and `slices/dev_deploy.rs`)

- Two profiles: **stable :8787** (never touched by migration) and **dev :8788** — the single shared validation environment, reachable at `https://ajaxdev.mossyhome.net:8788` (constant `OPEN_URL` in `TestInDevPanel.svelte`).
- `scripts/dev-web-restart.sh` (no args): force-sync local `main` worktree to `origin/main`, `cargo install` from it, restart tmux session `ajax-web-dev` on :8788. **This is the baseline restore command.**
- `scripts/dev-web-restart.sh --worktree PATH`: build *that worktree as-is* (runs `npm run web:build` there, `cargo install` into `.ajax-dev-web/bin` slot), restart dev only. Refuses `--profile stable`. Auto-rollback to previous slot binary if start or `/api/health` fails. **This is how a slice reaches the shared dev URL.** It is also triggerable from the phone via the Test in Dev panel (`POST /api/dev-deploy`).
- Staleness check before believing any "bug persists" report: compare dev server start time against slot-binary mtime.
- Worktrees live at `ajax-cli__worktrees/<slug>`; fresh worktrees need `npm ci` before anything (pre-commit runs `npm run verify`).

### 1.8 CI and pre-commit

- CI web job: `npm ci` → Playwright webkit install → `npm run web:smoke -- --project=mobile-webkit`. Rust jobs: fmt, check, clippy `-D warnings`, nextest (all features, single-threaded), docs, audit. PR title must be conventional (`fix`/`feat` cut releases; `refactor`/`chore` ship unreleased — dev deploys build from source, so migration slices do not need releases).
- `.husky/pre-commit`: `npm run verify` (fmt + check + clippy + nextest + doc tests + `web:check` + `web:test`) **plus** release build + `cargo install`. Slow but the authoritative local gate.
- `npm run web:check` = `tsc -p tsconfig.check.json` (TypeScript 7) + `svelte-check` (shimmed onto TypeScript 5 via `scripts/svelte-check-legacy-ts.cjs`). The shim and `typescript-5` dev-dep are deleted with the last Svelte file.

---

## 2. Target architecture and architectural decisions

Every decision below is driven by a verified repository constraint, not preference.

**D1 — React 19 SPA, same bundle contract, no meta-framework.**
`react` + `react-dom` + `@vitejs/plugin-react` added to the existing Vite config alongside the Svelte plugin during coexistence. Build output stays `dist/index.html` + `app.js` + `app.css`, unhashed, single script/stylesheet — the `include_bytes!` embed contract (§1.1) makes any other shape a Rust compile-time or guard-test failure. No Next.js, no SSR, no code splitting (`web-build-check.mjs` forbids a second JS file). **[UNVERIFIED: exact React/plugin versions available at implementation time — pin latest stable then.]**

**D2 — Routing: keep `routes.ts` + hash routing; no react-router.**
Three routes, 53 lines of pure parse/format code, already extracted. React gets a ~15-line `useHashRoute()` hook (hashchange listener + `parseRoute`). A router library would violate the repo dependency policy ("must remove meaningful custom code" — there is none to remove).

**D3 — State: React local state in the shell component; no state library.**
The current architecture is explicit: server-projected data only, no authored store, no optimistic mutation (`state.ts` header, `AGENTS.md` "browser code must not become an alternate registry"). `App.tsx` mirrors `App.svelte`'s runes with `useState`/`useEffect`; `cockpitPoll.ts` apply-gate and in-flight guard are reused as-is. Redux/Zustand/TanStack Query would create the "independent source of truth" the constraints prohibit.

**D4 — Data fetching/forms: existing `api.ts` + `contracts.ts`; controlled form for NewTaskSheet.** No fetch or form library.

**D5 — Coexistence: React islands inside the Svelte shell, one direction only, until the final slice inverts the root.**
One temporary abstraction — `ReactIsland.svelte` + `src/react/mountIsland.tsx` (~40 lines total): Svelte renders a host `<div>`, mounts a React root, forwards props on change (`$effect` → `root.render`), unmounts on destroy. Callbacks flow through props. Rules:
  - React-inside-Svelte only. Never Svelte-inside-React (no reverse adapter — this forces the migration order to be leaf-upward, terminal before its parent).
  - Each migrated component's Svelte source is **deleted in the same slice** — no dual implementations except the one documented `ActionBar` exception (S2→S5).
  - The island adapter is deleted in S6 (shell inversion). Its existence is tracked in this file's status table.

**D6 — Terminal boundary: untouched engine, thin React wrapper.**
xterm 6.0 + fit addon stay (constraint: no renderer replacement). `terminalConnection.ts`, `terminalGeometry.ts`, `terminalRefit.ts`, `viewport.ts` remain the controllers; `TaskTerminal.tsx` is a wrapper holding refs and wiring the same imperative calls the Svelte component makes today. Terminal lifecycle, reconnect, keyboard, focus, and resize logic must not move into React effects beyond what `TaskTerminal.svelte` already does in `onMount`/`$effect`.

**D7 — Styling: `styles.css` stays authoritative; Tailwind v4 added preflight-less; shadcn/ui vendored and token-mapped.**
Constraints: single `app.css`, DESIGN.md-locked palette enforced by `design-colors.test.ts`, CSS-content guards in `install.rs`, and "no visual redesign." Therefore:
  - Keep `src/styles.css` (global cockpit classes, tokens) byte-compatible; all guard-asserted selectors/values remain.
  - Add Tailwind v4 via `@tailwindcss/vite`, **utilities + theme only — no preflight** (preflight would restyle the live Svelte components mid-migration and break `visual.test.ts`). Theme maps to the existing custom properties (`--paper`, `--ink`, `--accent`, …), never duplicates hex values.
  - shadcn/ui components are vendored (CLI) into `src/components/ui/`, themed exclusively through the existing tokens. Use them where the primitive buys behavior (Radix focus trap, aria): Sheet (NewTaskSheet), AlertDialog (restart confirm), Button, Badge, Skeleton, Sonner-style toast is **not** used — `ResultPanel` semantics (undo/commit) stay bespoke. Terminal surfaces, ActionBar's two-tap confirm, and gesture-driven UI stay bespoke per constraint. New deps limited to the Radix packages of components actually generated, `clsx`, `tailwind-merge`, `class-variance-authority`. **[UNVERIFIED: shadcn CLI output shape for Tailwind v4 at implementation time; adjust paths in `components.json` accordingly.]**

**D8 — Error boundaries and loading.** One React `ErrorBoundary` at the island root (and later app root) rendering the existing "incompatible server response" language; loading states keep the current `Skeleton` component semantics. No suspense/data-fetching architecture — polling is imperative by design.

**D9 — No feature flags.** The route/component seam *is* the switch: a slice merges only after dev validation, each PR is one revert away from restoration, and dev/stable separation means users only meet a slice after merge. A runtime flag would be an independent source of truth and is not needed. Rollback = `git revert` + baseline redeploy (§11).

**D10 — Test boundaries.** Playwright e2e = frozen characterization layer (edited only to *add* coverage, never edited to accommodate a slice). Vitest pure-TS suites untouched. Component tests ported per slice, assertion-for-assertion, to `@testing-library/react`. Rust guard tests repointed (never weakened) when a grepped file is renamed `.svelte` → `.tsx`.

**D11 — PWA/service-worker ownership.** Nobody's. Absence is the contract (§1.5); guards enforce it.

---

## 3. Framework-neutral boundaries

Code that must be framework-neutral **already is** (§1.3) — no pre-migration extraction refactor is needed or allowed (it would be unrelated churn). The per-slice rules:

1. Imports from `.tsx` files may target only: the §1.3 modules, other `.tsx` files, `src/components/ui/*`, React itself, and xterm (terminal slice only).
2. New Svelte-adapter equivalents are React hooks colocated in `src/react/`: `useHashRoute`, `usePullToRefresh`, `useSwipeReveal`, `useSheetDrag`, `useViewportBand` (wraps `initViewport`). Each hook delegates to the existing pure module; hooks contain wiring only, no gesture math.
3. The Svelte `*Action.ts` adapters are deleted when their last Svelte consumer is deleted (tracked per slice).
4. Nothing moves task truth, status derivation, or lifecycle policy into components — `AGENTS.md` architecture guardrails apply verbatim to React code.

---

## 4. Migration safety rules

1. One active slice at a time. Next slice starts only after the previous PR is **merged** and the dev baseline restored.
2. Every commit on a slice branch leaves `npm run verify` green — the app builds and runs at every commit (embed contract means a broken `dist/` is a Rust compile failure, which is the enforcement mechanism).
3. e2e suites run unmodified for every slice; a slice that "needs" an e2e edit (other than repointing an import path or adding new tests) is misdesigned — stop and escalate.
4. Behavior changes require explicit approval and a written note in §14's status table. Default is pixel/behavior parity; `visual.test.ts` + iPhone comparison against stable :8787 is the parity check.
5. Svelte source deleted in the same PR that ships its replacement (single source of truth), except `ActionBar.svelte` which survives S2→S5 with a `// ponytail: duplicate of ActionBar.tsx until TaskDetail migrates (S5)` header.
6. No unrelated refactors, no dependency bumps, no cleanup inside slices.
7. Bundle-string guard: after every slice's `web:build`, `grep -c serviceWorker dist/app.js` must be 0 (nextest enforces; check early to fail fast).
8. Per `AGENTS.md`, every slice gets `.planning/agent-plans/react-slice-<id>.md` as its execution ledger.

---

## 5. Shared dev workflow

One dev environment, one command in each direction:

```bash
# Point dev :8788 at the active slice worktree (build worktree as-is, slot install, restart):
scripts/dev-web-restart.sh --worktree /Users/matt/Desktop/Projects/ajax-cli__worktrees/<slice-worktree>

# Restore baseline (sync main → install → restart dev):
scripts/dev-web-restart.sh
```

- The script serializes deploys by construction (kills the previous tmux session `ajax-web-dev`, refuses unmanaged listeners, health-checks `/api/health`, auto-restores the previous slot binary on failure). There is no second dev server; never start ad-hoc `ajax-cli web` processes on other ports for validation.
- Phone-side redeploys mid-validation: Test in Dev panel on any task detail (`POST /api/dev-deploy` targets the task's worktree).
- Before trusting any behavior observation, confirm freshness: the deploy log line `AJAX_DEV_DEPLOY_PHASE=restarting` and `/api/version` change (asset-fingerprinted) prove the new bundle is live.
- Stable :8787 is never touched by migration work.

---

## 6. Ordered slice dependency map

Leaf-upward so React-inside-Svelte is the only coexistence direction ever needed; risk rises monotonically (static chrome → forms/gestures → terminal → shell inversion). Terminal (S5) must precede its parent TaskDetail (S6 depends on S5's ordering; see D5).

```
S1 foundation (mount seam + tokens + ConnectionStatus + Skeleton)
 └─ S2 dashboard (TaskList + ActionBar.tsx)            [needs island + tokens]
 └─ S3 settings (SettingsView + ResultPanel)           [needs island; parallel-safe after S1 but run after S2 — one slice at a time]
 └─ S4 new-task sheet (NewTaskSheet + FullscreenLayer) [needs island; first shadcn Sheet + keyboard band]
 └─ S5 terminal (TaskTerminal)                         [needs island; hardest; before its parent]
     └─ S6 task detail (TaskDetail + TestInDevPanel; deletes ActionBar.svelte)
         └─ S7 shell inversion (App/AppShell/AppViewport/RouteScroll → React root; delete Svelte toolchain)
```

Why this order minimizes risk: S1 proves build/bundle/guard/test integration on the two smallest visible components before anything behavioral moves. S2–S4 exercise progressively richer patterns (list + swipe gesture + mutations → route view + server restart flow → modal form + drag gesture + keyboard band + first shadcn primitives) while the battle-tested Svelte shell still owns routing/polling. S5 isolates the highest-risk surface (1,592 lines, iOS keyboard/fullscreen/paste) into a slice whose *only* job is the terminal, still inside the known-good Svelte TaskDetail. S6 then migrates a parent whose children are all already React. S7 flips main.ts last, when the shell is the only Svelte left, and deletes the framework in the same PR — the island adapter never has to host anything again.

---

## 7. Implementation packets

Common to every packet (stated once, binding for all):

- **Worktree/branch**: `git worktree add ../ajax-cli__worktrees/react-<id> -b ajax/react-<id> origin/main` (from the main checkout), then `npm ci` in the worktree.
- **Local validation commands** (in order; all must pass before dev deploy):
  ```bash
  npm run web:build          # must precede cargo (include_bytes! needs dist/)
  grep -c serviceWorker crates/ajax-web/web/dist/app.js   # expect 0
  npm run web:check
  npm run web:test -- --run
  npm run web:build:check
  npm run web:smoke          # both projects; use -- --project=mobile-webkit for the fast loop
  cargo nextest run -p ajax-web
  npm run verify             # full gate, matches pre-commit
  ```
- **Dev routing**: §5 command with this worktree's path; confirm `/api/version` changed.
- **Dev validation checklist** (baseline for every slice; slice packets add specifics): all e2e green locally; side-by-side parity vs stable `:8787` for the migrated surface; iPhone pass per §9 rows marked for the slice; results recorded in `.planning/agent-plans/react-slice-<id>.md` and summarized in the PR body.
- **PR**: title `refactor(web): <slice name> (react S<id>)` (no release cut; dev builds from source). One slice, its tests, its deletions, its recorded validation, its manual checklist. Never the first functional test of the behavior.
- **Rollback**: pre-merge — redeploy baseline (§5); post-merge — `git revert <merge-commit>` then baseline redeploy. Every slice is a single revertible commit-set with no cross-slice file overlap except as documented.
- **Escalate to Sol/Fable instead of guessing** (all slices): any e2e test that seems to need weakening; any Rust guard failure not obviously caused by a renamed file; any visual difference vs :8787; any new dependency beyond the packet's list; anything touching `architecture.md` boundaries; `serviceWorker` string appearing in the bundle.

---

### S1 — Foundation: React mount seam, tokens, first two components

- **Behavior migrated**: connection status banner (states checking/connected/disconnected/backend-unreachable/stale-session, Retry/Reload/Copy-diagnostics buttons) and loading skeletons — identical rendering and callbacks, now React islands.
- **Coherent vertical boundary**: smallest user-visible surfaces that still exercise every integration layer (build, bundle contract, CSS, vitest, e2e, Rust guards) end to end.
- **Current Svelte files**: `ConnectionStatus.svelte` (+`.test.ts`), `Skeleton.svelte`; consumer `App.svelte` (import swap only).
- **Target files**: `src/react/mountIsland.tsx`, `src/react/ReactIsland.svelte`, `src/react/ErrorBoundary.tsx`, `src/components/ConnectionStatus.tsx` (+`.test.tsx`), `src/components/Skeleton.tsx`; Tailwind v4 entry (`@import "tailwindcss/utilities"` + `@theme` token mapping appended to the CSS pipeline, preflight off); `components.json` for shadcn (no components generated yet); config edits: `vite.config.mts` (add `@vitejs/plugin-react`, widen vitest `include` to `*.test.{ts,tsx}`), `tsconfig.json`/`tsconfig.check.json` (`"jsx": "react-jsx"`, include `.tsx`), `package.json` deps.
- **Shared contracts**: island prop-forwarding contract (plain serializable props + function callbacks; no children across the boundary — components that need slots migrate whole, which the slice plan already guarantees).
- **Depends on**: nothing.
- **Characterization tests already covering it**: e2e `actions.test.ts` "connection Copy Diagnostics jumps to the settings route", "connection Reload calls location.reload"; `smoke.test.ts` skeleton testids. **Gap to close first**: none — coverage adequate.
- **Automated tests during implementation**: port `ConnectionStatus.test.ts` to RTL assertion-for-assertion; new `mountIsland.test.tsx` (mount, prop update propagates, unmount cleans up React root, error boundary catches a throwing child).
- **Svelte removed**: `ConnectionStatus.svelte`, `ConnectionStatus.test.ts`, `Skeleton.svelte`.
- **Temporary compatibility code**: `ReactIsland.svelte`/`mountIsland.tsx` — removal condition: S7 merges.
- **iPhone steps**: load dev URL; kill dev server (`tmux kill-session -t ajax-web-dev`) → banner shows backend-unreachable with Retry; restart via `scripts/dev-web-restart.sh --worktree …` → Retry reconnects; visual parity of banner and skeletons vs :8787.
- **Acceptance criteria**: all common validation green; bundle guards green; no `serviceWorker` string; banner/skeleton pixel-parity; island unmount leak-free (navigate dashboard↔settings 20×, no console errors).
- **Risks/failure modes**: Tailwind emitting preflight (visual diff everywhere — caught by `visual.test.ts`); a second JS chunk from React (`web-build-check` fails — fix with `manualChunks: undefined`/single-entry config); vitest picking up `.tsx` with the Svelte testing plugin (scope plugins correctly).
- **Recommended implementer**: mid-tier code model (Sonnet-class / Codex). Escalate: any guard-test edit beyond adding `.tsx` to includes.

### S2 — Dashboard: TaskList + ActionBar

- **Behavior migrated**: the dashboard/project journey — task cards (status dot, badges, relative times via `state.ts`), project filter chips (`#/p/…`), open-task navigation, swipe-to-reveal row actions, ActionBar with two-tap destructive confirm and expiry, mutations via `postOperation` with cockpit re-projection, per-row error surfacing.
- **Coherent boundary**: one complete journey (glance → filter → act on a task) with UI, gesture, state, API, error handling.
- **Current Svelte files**: `TaskList.svelte` (+test), `ActionBar.svelte` (+test), `gestures/swipeRevealAction.ts` (+test); consumer `App.svelte` (island swap for the dashboard outlet content).
- **Target files**: `src/components/TaskList.tsx` (+`.test.tsx`), `src/components/ActionBar.tsx` (+`.test.tsx`), `src/react/useSwipeReveal.ts` (+test). shadcn `Badge`/`Button` may be introduced here **only if** they render class-compatible output; otherwise keep bespoke markup (parity trumps shadcn adoption — decide by `visual.test.ts` result).
- **Shared contracts**: `ActionBar.tsx` props identical to the Svelte props (it becomes the shared implementation once S6 lands).
- **Depends on**: S1.
- **Characterization coverage**: e2e `smoke.test.ts` (5 tests incl. two-tap destructive), `actions.test.ts` nav tests, `layout-scroll.test.ts` row-height + scroll-owner invariants. **Gap to close before implementing**: no e2e for swipe-reveal — add `e2e/` mobile-webkit test (touch-drag a row, reveal width `SWIPE_REVEAL_WIDTH`, action tap dispatches) against the *Svelte* implementation first, commit it, then migrate.
- **Automated tests during**: RTL ports of `TaskList.test.ts`, `ActionBar.test.ts` (confirm-expiry timing via fake timers), `useSwipeReveal` unit test reusing `swipeReveal.ts` fixtures.
- **Svelte removed**: `TaskList.svelte`(+test). **Kept**: `ActionBar.svelte` + `swipeRevealAction.ts` remain for `TaskDetail.svelte` (ActionBar) — mark both with removal-condition comments (S6 deletes ActionBar.svelte; swipeRevealAction dies here if TaskList was its only consumer — verify with grep, expected sole consumer, then delete).
- **iPhone steps**: filter chips; swipe-reveal feel (reveal, snap-back, fling); two-tap destructive confirm + timeout; pull-to-refresh still works (owned by Svelte App — regression check, not migration); rotation; long task list scroll with momentum.
- **Acceptance**: e2e green incl. new swipe test; dashboard visually identical; sort stability preserved (`sortCards` previous-order behavior — open dashboard during status churn).
- **Risks**: touch-event wiring differences (React synthetic vs native listeners — attach native listeners in the hook exactly as the Svelte action does, passive flags identical); ActionBar duplication drift (freeze `ActionBar.svelte` — any bugfix during S2–S5 must be applied to both, noted in the ledger).
- **Implementer**: mid-tier. Escalate: any need to change `layout-scroll` invariants.

### S3 — Settings: SettingsView + ResultPanel

- **Behavior migrated**: settings route (server restart with confirm + poll-until-back via `RESTART_POLL_MS`/`RESTART_TIMEOUT_MS`, diagnostics run/copy, back nav) and the global result panel (message/output, error tone, Undo/Commit callbacks, dismiss).
- **Boundary**: the complete operator-maintenance journey plus its result surface.
- **Current files**: `SettingsView.svelte` (+test), `ResultPanel.svelte` (+test); `App.svelte` island swaps.
- **Target files**: `SettingsView.tsx`, `ResultPanel.tsx` (+tests). shadcn `AlertDialog` for the restart confirm if class-parity holds; else bespoke.
- **Depends on**: S1.
- **Characterization coverage**: e2e `actions.test.ts` (4 settings tests + dismiss), `visual.test.ts` settings styling. Gap: none material.
- **Automated tests**: RTL ports; restart flow with mocked `api.ts` (fake timers through poll/timeout).
- **Svelte removed**: both components + tests. **Guard update**: `legacyTerminalRemoval.test.ts` greps `SettingsView.svelte` — repoint to `SettingsView.tsx`, same symbols.
- **iPhone steps**: Restart from the phone (confirm → server restarts → app recovers when `/api/health` returns); diagnostics copy (HTTPS clipboard); result panel Undo path on a start-task result (cross-check with S2 dashboard actions).
- **Acceptance**: restart journey survives a real dev-server restart initiated from the migrated UI; e2e green.
- **Risks**: restart polling races (keep the existing `api.ts` polling helper — do not reimplement); ResultPanel z-order vs keyboard band.
- **Implementer**: mid-tier. Escalate: any change to session-renewal behavior in `api.ts` (should be zero).

### S4 — New-task sheet: NewTaskSheet + FullscreenLayer

- **Behavior migrated**: "New" journey — fullscreen sheet over the viewport band, repo select, prompt entry (≥16px font, no iOS zoom), sheet drag-to-dismiss, submit via `postStartTask` → open created task, cancel, error display, keyboard-band containment.
- **Current files**: `NewTaskSheet.svelte` (+test), `FullscreenLayer.svelte`, `gestures/sheetDragAction.ts` (+test).
- **Target files**: `NewTaskSheet.tsx` (+test), `FullscreenLayer.tsx`, `src/react/useSheetDrag.ts` (+test). First real shadcn usage: `Sheet`/`Drawer` primitive **only if** it can render inside `FullscreenLayer`'s band-pinned geometry without a portal that escapes `--app-height` (Radix portals to `body` — must portal into the viewport-band element or stay bespoke; decide in-slice, record decision).
- **Depends on**: S1.
- **Characterization coverage**: e2e `actions.test.ts` (cancel, start), `layout-scroll.test.ts` "new task sheet stays inside the simulated keyboard viewport band", `keyboardBandPin.test.ts` unit. Gap: none.
- **Automated tests**: RTL port incl. submit/disabled/error states; `useSheetDrag` unit.
- **Svelte removed**: all three files + tests (`FullscreenLayer` sole consumer verified: NewTaskSheet only; `sheetDragAction` sole consumer likewise).
- **iPhone steps**: open sheet → keyboard opens → sheet stays inside visual band (no jump on the 250ms close-settle edge — type, pause, autocorrect); drag-dismiss; focus does not zoom; submit → lands on task route.
- **Acceptance**: keyboard-band e2e green on mobile-webkit; on-device typing shows zero band jumps.
- **Risks**: Radix portal breaking band pinning (fallback: bespoke sheet, keep shadcn Button/Label only); focus management differences (Radix autofocus may fight iOS keyboard heuristics — replicate current focus order exactly).
- **Implementer**: strong mid-tier. Escalate: any `viewport.ts` change (frozen module).

### S5 — Terminal: TaskTerminal

- **Behavior migrated**: the full in-browser terminal — xterm mount, WS connect/reconnect (background/foreground), status strip (Connecting/Reconnecting/Unavailable + manual reconnect), key/ctrl toolbar with focus preservation, held-key repeat, bracketed paste + native-paste trigger + HTTPS/`readText` fallback UI + disconnected-paste retention, font-size persistence, geometry/refit (80-col floor, aspect-driven below-80 grid), fullscreen expand/exit (zero-lag overlay, chrome inertness, synchronous `beginExpandFlush` resize path), unseen-output indicator, band pinning under keyboard.
- **Boundary**: one journey (operate a task's terminal) — but it is the whole terminal contract, documented in `web/TERMINAL.md` and `TERMINAL_BEHAVIOR_CONTRACT.md` (read both before implementing).
- **Current files**: `TaskTerminal.svelte` (1592) + `TaskTerminal.test.ts`; consumers: `TaskDetail.svelte` (island swap).
- **Target files**: `TaskTerminal.tsx` (+`.test.tsx`). **Mechanical port, not a redesign**: same imperative calls into `terminalConnection`/`terminalGeometry`/`terminalRefit`/`viewport`, refs instead of `bind:this`, effects mirroring the existing `onMount`/`$effect` structure one-for-one. No shadcn anywhere in this slice. xterm CSS import stays.
- **Depends on**: S1 (and sequencing after S4 per §6).
- **Characterization coverage**: e2e `terminal-behavior.test.ts` — 23 tests covering socket cardinality, reconnect, PTY input ordering/cardinality, paste variants, focus preservation, fullscreen/keyboard band pinning. This suite passing **unmodified** is the slice's definition of done. Gap: none — this is the best-covered surface in the repo.
- **Automated tests**: RTL port of `TaskTerminal.test.ts`; keep every unit suite (`terminalConnection.test.ts` etc.) untouched.
- **Svelte removed**: `TaskTerminal.svelte` + test. Guard: `legacyTerminalRemoval.test.ts` references stay valid (it greps other files for terminal symbols; verify and repoint only paths that changed).
- **iPhone steps** (full §9 matrix): typing burst + autocorrect popovers (no band jump); background app 30s → foreground (auto-reconnect); fullscreen expand/exit repeatedly with keyboard open and closed (Bugs A/B/C regressions: no giant/dup text, no chrome peek-through, no left-shift/empty column); native paste via toolbar Paste key over HTTPS; multiline Unicode paste; font-size pinch/setting persists across reload; pan/scroll inside scrollback; kill tmux pane server-side → Unavailable → manual reconnect.
- **Acceptance**: 23/23 terminal e2e on both projects, on-device matrix clean twice (two separate sessions), no PTY input duplication under held keys (count frames server-side if in doubt via the e2e cardinality tests).
- **Risks**: React 18+ StrictMode double-effect mounting opening two sockets (do **not** enable StrictMode; e2e asserts one-socket cardinality); effect-ordering differences vs Svelte `$effect` batching around the synchronous expand path (`beginExpandFlush` must stay synchronous — use `flushSync` or plain imperative calls outside React state where the Svelte code is imperative); island prop-update timing for `handle` changes (navigation must tear down the socket — e2e covers it).
- **Implementer**: **top-tier code model** (Fable/Sol-supervised Codex at minimum). Escalate: any test in `terminal-behavior` failing after two focused fix attempts; any temptation to alter `viewport.ts`, `terminalRefit.ts`, or scroll behavior; any perceived need for `smoothScroll`/renderer changes (out of scope by constraint).

### S6 — Task detail: TaskDetail + TestInDevPanel (+ ActionBar dedup)

- **Behavior migrated**: task-detail journey — server-status panels, branch/worktree copy buttons, action bar (resume/open/…, two-tap destructive), result wiring, back nav (project-aware), dismiss, resume-on-open interplay (owned by App, verified not regressed), Test in Dev panel (deploy status poll, start deploy, open dev URL).
- **Current files**: `TaskDetail.svelte` (+test), `TestInDevPanel.svelte` (+test), `ActionBar.svelte` (deleted — S2's `ActionBar.tsx` becomes sole implementation), `gestures/swipeRevealAction.ts` if still present.
- **Target files**: `TaskDetail.tsx` (+test), `TestInDevPanel.tsx` (+test). Children `TaskTerminal.tsx`, `ActionBar.tsx`, `TestInDevPanel.tsx` compose natively — the island now hosts one bigger React subtree instead of three small ones.
- **Depends on**: S2 (ActionBar.tsx), S5 (TaskTerminal.tsx).
- **Characterization coverage**: e2e `smoke.test.ts` (detail render, one-tap/two-tap actions), `actions.test.ts` (back, copy), terminal suite (mounted-in-detail cardinality). Gap: TestInDevPanel has no e2e (needs a real deployable worktree — impractical); its vitest suite + on-device use during this very slice's validation is the test.
- **Svelte removed**: `TaskDetail.svelte`, `TestInDevPanel.svelte`, `ActionBar.svelte` + tests. Guard: repoint `legacyTerminalRemoval.test.ts` greps from `TaskDetail.svelte`/`App.svelte` symbols as needed (TaskDetail only here).
- **iPhone steps**: full open-task journey from dashboard; copy buttons; destructive action confirm; **use the migrated Test in Dev panel to redeploy this very slice** (self-hosting proof); terminal within detail unchanged.
- **Acceptance**: detail journey parity; Test in Dev round-trip works from the phone; only `App` + structural shell remain Svelte.
- **Risks**: prop drilling divergence from `App.svelte` callbacks (`onCockpit`, `onResult`, `onMutated` — keep signatures identical); losing the stale-response guard on detail loads (stays in App this slice).
- **Implementer**: mid-tier. Escalate: anything touching resume-on-open semantics.

### S7 — Shell inversion and Svelte removal

- **Behavior migrated**: the shell itself — hash routing, cockpit/version polling with adaptive intervals, visibility/focus/pageshow resume hooks, connection state, document titles, update banner, pull-to-refresh on dashboard outlet, bottom nav, sheet-open state, result-panel hosting, viewport band init.
- **Current files**: `App.svelte`, `AppShell.svelte`, `AppViewport.svelte`, `RouteScroll.svelte`, `main.ts`, `gestures/pullToRefreshAction.ts` (+tests), island adapter, `svelte.config.mjs`, `scripts/svelte-check-legacy-ts.cjs`.
- **Target files**: `App.tsx` (+test), `AppShell.tsx`, `AppViewport.tsx` (hosts `useViewportBand`), `RouteScroll.tsx`, `src/react/useHashRoute.ts`, `src/react/usePullToRefresh.ts`, `main.tsx` (`createRoot`), root `ErrorBoundary`.
- **Also in this slice (removal, not refactor)**: delete `svelte`, `@sveltejs/vite-plugin-svelte`, `svelte-check`, `@testing-library/svelte`, `typescript-5` from `package.json`; `web:check` becomes tsc-only; remove svelte plugin + `svelteTesting` from `vite.config.mts`; delete `ReactIsland.svelte`/island mount; `app.html` script src → `/src/main.tsx`; update `install.rs`/`architecture.md`/`TERMINAL.md` prose that says "Svelte" (wording only — assertions unchanged); repoint remaining `legacyTerminalRemoval.test.ts` paths.
- **Depends on**: S2, S3, S4, S5, S6 all merged.
- **Characterization coverage**: the entire e2e corpus is the shell's test (routing, polling-driven rendering, scroll owner, band pinning). Gap to close before implementing: add one e2e for the **update banner** (mock `/api/version` change → banner appears, tap reloads) and one for **pull-to-refresh** (touch-drag distance ≥ `PULL_THRESHOLD` triggers cockpit reload) against Svelte first.
- **Automated tests**: RTL `App.test.tsx` port; `useHashRoute` unit; polling-interval rescheduling test (fake timers, visibility flips).
- **iPhone steps**: full regression day — every §9 row; cold load performance sanity vs :8787; backgrounding/resume polling; Safari back/forward with hash routes; add-to-home-screen launch still renders (metadata-only standalone).
- **Acceptance**: `grep -ri svelte package.json crates/ajax-web/web/src` → nothing; `npm run verify` green; every e2e green; §12 criteria all met.
- **Risks**: adaptive-polling effect dependencies (interval must reschedule on route/visibility exactly — port the two `$effect` blocks as two `useEffect`s with identical dep sets); double initial cockpit fetch (mount effect + interval — match current once-then-interval semantics); `untrack` semantics in the detail-load effect (React has no `untrack`; structure deps so `detail` reset doesn't loop).
- **Implementer**: **top-tier**. Escalate: any lifecycle behavior that can't be reproduced 1:1 (e.g. effect timing) — that's an approved-behavior-change decision, not an implementer judgment call.

---

## 8. Local testing and dev-validation workflow (per slice, chronological)

1. `git worktree add … && npm ci` (fresh worktree has no `node_modules`).
2. Confirm current Svelte behavior by running the slice's e2e subset against the untouched worktree (`npm run web:smoke -- --grep "<area>"`).
3. Close any characterization gap listed in the packet **first**, committed separately, green against Svelte.
4. Implement (delegated); TDD loop with focused vitest + the slice's e2e subset.
5. Full local gate: the §7 common command list, ending with `npm run verify`.
6. Deploy to dev: `scripts/dev-web-restart.sh --worktree <path>`; verify `/api/version` changed.
7. Dev validation: e2e subset against dev is not possible directly (e2e runs against :5173 dev server) — dev validation is manual + on-device per packet checklist, plus a full local e2e run at the same commit. Record results in the slice ledger (commands, output summaries, device observations, screenshots where visual).
8. Failures → fix → redeploy (step 6) → revalidate. No PR until clean.
9. Record validation + any approved behavior deltas in the ledger and §14 table; open PR; CI + review; merge.
10. Restore baseline: `scripts/dev-web-restart.sh` (now serves merged main — a lightweight smoke on the phone: open dashboard, open a task, type one command).

## 9. iOS Safari verification matrix (real iPhone, dev URL)

| Check | Slices |
|---|---|
| Visual parity vs stable :8787 (same screens side by side) | all |
| Keyboard open/close band pinning, no jump on 250ms settle edge | S4, S5, S7 |
| Typing burst + autocorrect popover transients in terminal | S5, S7 |
| Fullscreen expand/exit ×5, keyboard open and closed (Bug A/B/C regressions) | S5 |
| Native paste (toolbar Paste key), multiline Unicode, HTTPS `readText`, fallback UI | S5 |
| Background 30s → foreground: WS reconnect, cockpit repoll, version check | S5, S7 |
| Pull-to-refresh, swipe-reveal, sheet drag feel (momentum, snap) | S2, S4, S7 |
| No focus zoom on any input (16px floor) | S3, S4, S7 |
| Two-tap destructive confirm + expiry | S2, S6 |
| Server restart from Settings recovers cleanly | S3 |
| Test in Dev deploy round-trip from the panel | S6 |
| Safari back/forward across hash routes; reload mid-route | S7 |
| Add-to-Home-Screen launch renders (metadata-only; no install surface expected) | S7 |
| Rotation portrait↔landscape on dashboard + terminal | S2, S5, S7 |

Simulator (`xcrun simctl` + idb) may be used for iteration; every slice's final validation is on-device.

## 10. PR, CI, review, and merge workflow

- Branch `ajax/react-<id>`; PR title `refactor(web): … (react S<n>)`; conventional-commit check applies. No `Co-Authored-By`/AI lines; no "Claude" in titles.
- PR body: slice summary, dev-validation record (from ledger), manual test checklist, list of deleted Svelte files, temporary-code inventory with removal conditions, revert instructions ("revert this merge commit; run `scripts/dev-web-restart.sh`").
- CI must be fully green (`ci` gate requires web + all Rust jobs). The web CI job runs mobile-webkit smoke — it re-checks, it never first-checks.
- Review fixes go through the same delegate loop; re-run dev validation if the fix touches behavior (not for comment-only changes).
- Merge (squash per repo default **[UNVERIFIED: merge method — check repo settings; recent history shows squash-style single commits]**), then baseline restore + phone smoke (§8.10).

## 11. Rollback strategy

- **Pre-merge**: nothing shipped; `scripts/dev-web-restart.sh` restores baseline instantly. Slot-binary auto-restore already covers failed deploys.
- **Post-merge**: `git revert <merge-commit>` on a branch → PR (CI green by construction, since each slice is self-contained and later slices haven't started) → merge → baseline redeploy. The one-active-slice rule guarantees no forward slice depends on unreverted code.
- **ActionBar exception**: reverting S6 resurrects `ActionBar.svelte` automatically (it was deleted there); reverting S2 while S3–S5 are merged is the only compound case — it also reverts `ActionBar.tsx`, so S6 must not have started (enforced by ordering).
- **Deploy-time failure**: dev-web-restart's health check + previous-slot restore is the automatic layer; a bad *merged* main caught at baseline restore = revert PR immediately (stable :8787 is unaffected throughout).

## 12. Final Svelte removal criteria (all must hold at S7 merge)

1. No `.svelte` files; no `svelte*`/`@sveltejs/*`/`@testing-library/svelte`/`typescript-5` in `package.json`; `svelte.config.mjs` and `scripts/svelte-check-legacy-ts.cjs` deleted.
2. `web:check` = `tsc --noEmit` only; `npm run verify` green end to end.
3. Island adapter deleted; grep for `mountIsland`/`ReactIsland` empty.
4. All Rust guard tests and `web-build-check` pass unmodified in intent (paths repointed, assertions never weakened).
5. Full e2e corpus green on both projects; §9 full matrix passed on-device.
6. `architecture.md` and `web/TERMINAL.md` prose updated (same PR as S7, per AGENTS.md architecture rule).
7. Frozen modules (`api.ts`, `viewport.ts`, `terminalConnection.ts`, `terminalGeometry.ts`, `terminalRefit.ts`, gestures core, `contracts.ts`, `polling.ts`) byte-identical to pre-migration except imports — verify with `git diff main-at-start -- <files>`.
8. §14 status table shows every slice merged with recorded validation and zero unapproved behavior changes.

## 13. Instructions for GPT-5.6 Sol (implementation orchestrator)

You orchestrate; you do not implement. Per `AGENTS.md`: model-router chooses the delegate; `tdd-implementation-packet` is the delegation artifact; you review diffs and run validation personally.

1. **Select the next slice**: the lowest-numbered slice in §14 whose dependencies (§6) are all `merged` and whose own status is `not-started`. Exactly one slice may be non-merged at any time — if one is in flight, resume it, never start another.
2. **Revalidate the packet**: before creating the worktree, re-check the packet's file list against current `main` (`git log --oneline -20`, `ls`, grep the named files). If files moved, tests changed, or new guard tests appeared, update the packet in this document first (commit the doc change to the slice branch) — the packet, not memory, is the source of truth.
3. **Convert to an assignment**: create `.planning/agent-plans/react-slice-<id>.md` from the packet: scope, non-goals, file-by-file task checklist (test → implement → verify per task), the exact validation commands, and the "escalate" list verbatim. Record `Delegation decision: delegated via model-router`.
4. **Delegate**: run `model-router` with the packet; produce a `tdd-implementation-packet`; dispatch. One bounded task per round — split big slices (S5, S7) into sequential `implement` → `resume` rounds along the task checklist. Never delegate from a vague prompt; never let the delegate commit, push, or change branches.
5. **Inspect the diff, not the summary**: `git diff` against the branch base. Check: only packet-listed files touched; Svelte deletions present; frozen modules untouched; tests ported assertion-for-assertion (diff the old and new test files side by side); no new deps beyond the packet.
6. **Reject**: unrelated changes, weakened assertions, missing deletions, partial behavior → focused `resume` order naming the exact violation. An empty diff plus a success claim is a failure.
7. **Run the checks yourself**: the §7 common command list, in order, in the slice worktree. Do not trust delegate-reported results.
8. **Switch dev**: `scripts/dev-web-restart.sh --worktree <slice worktree>`; confirm `AJAX_DEV_DEPLOY_PHASE=restarting` in output and a changed `/api/version`.
9. **Dev validation**: execute the packet's dev checklist + §9 rows. On-device steps require Matt — post the checklist and wait; do not mark rows passed on simulator evidence alone. Record every result (pass/fail + observation) in the slice ledger.
10. **Fix loop**: failures become focused `resume` orders with repro steps; redeploy and revalidate after every fix. Repeat until the checklist is fully green.
11. **PR only after validation**: §10 format, ledger contents included. Never open the PR to "see what CI says."
12. **Monitor CI**: on failure, pull logs, diagnose, delegate the fix, re-run local gates, push. Coordinate review comments the same way; re-run dev validation when a review fix changes behavior.
13. **Merge** per repo settings once CI + review pass.
14. **Restore baseline**: `scripts/dev-web-restart.sh`; phone smoke (dashboard → task → one terminal keystroke) — report result.
15. **Update this document**: flip the slice's row in §14 to `merged` with date, PR number, validation-ledger link, and any approved behavior deltas; commit to `main` via the slice PR itself (preferred: include the table update in the slice PR).
16. **Advance**: only then return to step 1. If any step forces a decision reserved for Matt (behavior change, guard-test intent, new dependency class, architecture boundary), stop and ask instead of proceeding.

## 14. Migration status

| Slice | Name | Status | PR | Validated | Approved behavior deltas |
|---|---|---|---|---|---|
| S1 | Foundation + ConnectionStatus/Skeleton | merged | #571 | 2026-07-17 | ledger: `.planning/agent-plans/react-slice-s1.md` |
| S2 | Dashboard (TaskList + ActionBar) | merged | #573 | 2026-07-17 | ledger: `.planning/agent-plans/react-slice-s2.md` |
| S3 | Settings + ResultPanel | merged | #575 | 2026-07-17 | ledger: `.planning/agent-plans/react-slice-s3.md` |
| S4 | New-task sheet + FullscreenLayer | merged | #577 | 2026-07-17 | ledger: `.planning/agent-plans/react-slice-s4.md` |
| S5 | Terminal (TaskTerminal) | merged | #578 | verify+on-device 2026-07-17 | ledger: `.planning/agent-plans/react-slice-s5.md` |
| S6 | Task detail + TestInDevPanel | in-progress | — | — | ledger: `.planning/agent-plans/react-slice-s6.md` |
| S7 | Shell inversion + Svelte removal | not-started | — | — | — |

Temporary migration code inventory (all rows must be empty before S7 closes):

| Item | Introduced | Removal condition |
|---|---|---|
| `ReactIsland.svelte` + `mountIsland.tsx` | S1 | S7 merge |
| `ActionBar.svelte` duplicate (frozen) | S2 | S6 merge |
| Svelte `*Action.ts` gesture adapters | pre-existing | deleted with last Svelte consumer (S2/S4/S7) |
| `svelte-check` + TS5 shim in `web:check` | pre-existing | S7 merge |
