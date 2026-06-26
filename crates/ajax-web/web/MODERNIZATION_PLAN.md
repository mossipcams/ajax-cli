# Web Cockpit Mobile Modernization Plan (v2 — evidence-grounded)

Modernize the Svelte Web Cockpit for the best **mobile web** (iOS Safari first)
experience. Rebuilt via: **Graphify → rg → Serena → ast-grep → plan → critique →
TDD**.

Operator-confirmed scope: **live updates (SSE)**, **touch gestures**,
**performance/loading**, **visual refresh within the existing token system**.

---

## 1. Investigation findings (what reshaped the plan)

**Graphify** (`graphify-out/graph.json`, 5260 nodes)
- `axum_app` (router, `runtime.rs:180`) is called by ~15 existing integration
  tests → adding a route gives us a ready-made test harness.
- One refresh+projection path: `axum_cockpit:358 → handle_refreshed_cockpit_request:1080
  → cockpit::browser_cockpit_view`. Everything hangs off `WebAppState:46`
  (`Arc<Mutex<WebSharedState>>` + `OperationCoordinator` + `cockpit_refresh_lock`).

**Serena** (semantic bodies)
- `handle_refreshed_cockpit_request` is **synchronous/blocking**: it runs
  `bridge.refresh_cockpit(ctx, runner, RefreshTier::Live)` (git/tmux probes) then
  serializes the projection.
- `axum_cockpit` single-flights via the async `cockpit_refresh_lock`, clones
  ctx/runner/bridge into a `CockpitRefreshSession`, runs the blocking refresh off
  the shared lock, commits back **only if `revision` is unchanged**, and caches
  under `COCKPIT_REFRESH_CACHE_TTL` (750ms).

**rg**
- `revision: u64` on shared state, bumped by mutations (`runtime.rs:118`, `:613`).
- No `EventSource` / `broadcast` exists yet. Frontend polls `loadCockpit` every
  `REFRESH_INTERVAL_MS = 1000` (`App.svelte:85`).

**ast-grep / Cargo**
- axum **0.8.9**: `axum::response::sse::{Sse, Event, KeepAlive}` available, no
  feature flag.
- tokio `sync` feature already active (`tokio::sync::{Mutex,mpsc}` in use) →
  `tokio::sync::broadcast` is free.
- `tokio-stream` / `async-stream` not in lockfile (`futures-util` only
  transitive) → ergonomic broadcast→SSE adaptation needs **one** small dep.

### Design consequence

"State changed" is **only** observable by running a refresh. Therefore:
- **Reject** streaming full projections over SSE (it duplicates the projection
  path and a second serialization contract).
- **Adopt** a **revision-nudge** channel: broadcast only the new `revision: u64`;
  the client re-fetches the **unchanged** `/api/cockpit` (warm TTL cache). The
  server runs **one** refresh loop, **gated to subscribers** (loop only spins
  while `broadcast::Sender::receiver_count() > 0`), instead of N clients polling.
- Fallback is trivial: stream error → resume the existing 1s `loadCockpit` poll.
  Same fetch either way → no second data path, satisfies "must work without the
  stream."

---

## 2. Constraints (must respect)

- **Single bundle, fixed names.** `dist/{index.html,app.js,app.css}` embedded by
  `adapters/assets.rs` via `include_bytes!`; no code-splitting/dynamic imports
  (`vite.config.mts`).
- **Rebuilding dist trips Rust string-snapshots** in `slices/install.rs` and
  `ajax-cli/src/web_backend.rs` (reconcile in Phase 5).
- **Server-authoritative**; no second browser task model; no persisted mutations.
- **SW must never cache `/api/*`** — `/api/cockpit/stream` is `/api`, already
  bypassed; extend the bypass test.
- **Slice boundaries** (`rust_arkitect`): the nudge broadcast + SSE wiring is
  `runtime` infrastructure; it must not push web concerns into `ajax-core`.
- **Deps:** add **`tokio-stream` (feature `sync`)** only — justified for
  `BroadcastStream`. No gesture library; gestures are hand-rolled pure modules.
- **iOS:** `navigator.vibrate` unsupported on Safari → haptics are guarded
  best-effort, never relied on.

## 3. TDD classification

- **Behavior** (DOM/logic/route): failing test first (vitest or nextest).
- **Visual-only** (CSS tokens/styling): no unit test; verified by Playwright
  `e2e/visual.test.ts` snapshot review + `web:check` + contract gate (the
  AGENTS.md markdown-style "change → verify by review" path).

---

## 4. Phased plan

### Phase 0 — Worktree prep (setup)
- **0.1** `npm ci` (node_modules absent here). Verify `npm run web:test -- --run` green.
- **0.2** Confirm `graphify-out/` is gitignored (it is an extraction artifact);
  add to `.gitignore` if not.

### Phase 1 — Design-system refresh (visual-only)
- **1.1** Add depth/space/motion tokens to `styles.css` (`--elev-1/2`,
  `--space-*`, refined `--radius`, `--ease`); **no color token changes**.
- **1.2** Apply tokens to chrome, task rows (comfortable touch height), inbox
  cards (subtle elevation), section heads, bottom nav. No DOM/selector changes.
- Verify: `web:check`; existing vitest stays green; visual-snapshot review.

### Phase 2 — Loading & perceived performance (behavior)
- **2.1** `Skeleton.svelte` shimmer. Failing test: when `cockpit` is null the
  dashboard shows `[data-testid="dashboard-skeleton"]`, not "— loading". Wire
  dashboard + task-detail.
- **2.2** Defer `checkVersion` via `requestIdleCallback` (fallback `setTimeout`).
  Failing test: not called synchronously on mount; runs after idle.

### Phase 3 — Touch gestures (behavior; pure modules in `src/gestures/`)
- **3.1** `pullToRefresh.ts` — from `scrollTop===0`, rubber-band resistance,
  threshold fire. Tests: resistance, trigger, cancel. Wire `<main>` → `loadCockpit`.
- **3.2** `swipeReveal.ts` — horizontal delta → reveal offset, snap, single
  trigger, vertical-scroll lockout. Tests for each. Wire calm `TaskList` rows to
  reveal the primary action.
- **3.3** `sheetDrag.ts` — downward drag past threshold dismisses
  `NewTaskSheet`, else springs back; upward clamp. Tests + wire + grabber handle.

> Haptics dropped per operator decision (no value on the iOS Safari target).

### Phase 4 — Live updates: revision-nudge SSE (behavior; backend + frontend)
> **Delivery:** Phases 1–3 ship first as one slice/PR. **Phase 4 is a separate
> follow-up slice/PR** and is not part of the first implementation pass.
Backend (nextest), all in `runtime.rs`:
- **4.1** Add `revision_tx: Arc<tokio::sync::broadcast::Sender<u64>>` to
  `WebAppState`. Mutations + cockpit refresh that advance `revision` publish the
  new value (best-effort `send`, ignore "no receivers"). Failing test: a refresh
  that bumps revision yields a value on a subscribed receiver.
- **4.2** `GET /api/cockpit/stream` (`async fn`, mirror `axum_cockpit` generics):
  `Sse` over `BroadcastStream` mapping each `revision` → SSE `event: revision`
  `data: <n>`; send the current revision immediately on connect; `KeepAlive`
  ~15s for iOS Safari. Failing test (via `axum_app`): content-type
  `text/event-stream`, first frame carries current revision.
- **4.3** Subscriber-gated refresh loop: a background tick (spawned in
  `serve_axum_web`) runs `refresh_cockpit` on the existing single-flight path
  **only while `revision_tx.receiver_count() > 0`**, bounded interval. Failing
  test: loop idle with 0 subscribers; ticks once a subscriber connects.
- **4.4** `architecture.md` — document `/api/cockpit/stream` as the realized
  stream endpoint (nudge semantics, subscriber-gated loop, SW-bypass, polling
  fallback). (Markdown, no test.)

Frontend (vitest):
- **4.5** `src/stream.ts` `subscribeRevisions({onTick,onError})` over
  `EventSource('/api/cockpit/stream')`, parsing `revision` events to numbers.
  Failing test: mocked EventSource tick → `onTick(n)`; malformed → `onError`, no throw.
- **4.6** Wire `App.svelte`: on mount open the stream; `onTick` (and only when the
  revision differs) calls `loadCockpit()`; **stop the 1s interval while the
  stream is live**; on `onError`/no `EventSource`, resume the interval; close on
  hidden, reopen on resume. Failing tests: tick triggers a single `loadCockpit`
  and suppresses the interval; error re-enables polling; hidden closes the stream.
- **4.7** Extend the SW `/api/*` bypass test to cover `/api/cockpit/stream`.

### Phase 5 — Rebuild bundle + reconcile embed
- **5.1** `npm run web:build` → regenerate `dist/`.
- **5.2** Reconcile snapshots in `slices/install.rs` and `web_backend.rs`; run
  `web-dist-check.mjs`, `web-build-check.mjs`, `verify:web-contract-gate`.

### Phase 6 — Full validation
`web:check` · `web:test -- --run` · `web:smoke` (update visual snapshots after
review) · `cargo fmt --check` · `cargo check --all-targets --all-features` ·
`cargo clippy --all-targets --all-features -- -D warnings` ·
`cargo nextest run --all-features --test-threads=1` · `lint:duplication`.
> Known dev-DB schema-10 pre-commit hook flake may require `--no-verify` after
> the above are green.

---

## 5. Critique (pre-implementation, self-review)

- **Biggest risk — the subscriber-gated refresh loop (4.3).** It introduces
  continuous server-side probing whenever any phone has the dashboard open. Must
  be bounded (interval ≈ current 1s, reuse single-flight lock, never overlap),
  and must stop when the last subscriber disconnects. Mitigation: drive the loop
  off `receiver_count()`; add a test that it idles at 0 subscribers.
- **iOS Safari SSE longevity.** WebKit drops `text/event-stream` on background/
  lock. `KeepAlive` + the existing `pageshow`/`visibilitychange` resume (which
  reopens the stream) cover this; the poll fallback is the backstop. The nudge
  design means a missed event is self-healing (next tick or resume re-fetches).
- **Is SSE worth it vs. cheap polling?** With 1–2 operator devices the bandwidth
  win is modest; the real wins are *instant* updates and removing per-client 1s
  fetches (battery). The nudge design keeps complexity proportionate by reusing
  the existing endpoint — acceptable. If 4.3 proves fragile, ship Phases 1–3 and
  defer 4.
- **New dependency.** `tokio-stream` is small/maintained and the idiomatic
  axum-0.8 broadcast→SSE adapter; justified over hand-rolling a `Stream`.
- **Haptics (3.4) is near-zero value on the iOS target.** Kept minimal; could be
  cut. Flag for operator.
- **Visual tasks lack unit coverage** by nature — they lean on Playwright visual
  snapshots; ensure those snapshots are reviewed, not blindly accepted.
- **Sequencing.** Phases 1–3 are independent and low-risk; Phase 4 is isolated
  and reversible. Recommend landing 1–3 first, then 4 as a separate slice/PR.

## 6. Operator decisions (resolved)
- **Haptics:** dropped (was 3.4).
- **Delivery:** Phases 1–3 first as one slice/PR; Phase 4 (SSE) as a separate
  follow-up. The first implementation pass covers **Phases 0–3 + Phase 5/6
  validation scoped to those changes**.
