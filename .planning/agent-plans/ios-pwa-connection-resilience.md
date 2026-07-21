# iOS PWA ↔ server connection resilience

Status: complete — all implementation, review, and validation checklist items
finished on 2026-07-20.

## Objective

Harden the complete iOS Home Screen app-to-server contract: cockpit reads must
remain responsive while server control work is already in flight, and the PWA
must recover automatically when its own first request fails or stalls while
WebKit still reports the document as hidden.

## Evidence

- Stable and dev listeners, TLS, and authenticated cockpit responses are fast
  locally (roughly 50–196 ms), so the current evidence does not implicate the
  server control lane as the primary failure.
- `App.tsx` performs one mount load while hidden, but its repeating poll skips
  every request while `document.hidden` remains true.
- The existing hidden-launch test advances two minutes and intentionally
  observes only one cockpit request. If that request fails, automatic recovery
  depends on an iOS focus/pageshow/visibility event.
- Resume events currently call the non-trailing in-flight guard path, so an
  event received during a stalled request is discarded.
- `GET /api/cockpit` currently waits on the same process-local control lane as
  notify refreshes, actions, and task starts. A browser request arriving behind
  slow lane work can consume its full client deadline even though the server's
  current in-memory Cockpit projection is safe to render.
- Core already owns the projection and `ajax-web` already renders it from the
  shared `CommandContext`; a busy-lane fallback can reuse that server-owned
  snapshot without creating browser task truth or weakening mutation safety.

## Scope

- Make `GET /api/cockpit` return the current server-owned projection promptly
  when another refresh/notify/mutation already owns the control lane, instead
  of queuing the read behind that work.
- Keep normal cache-miss refreshes serialized when the lane is available; the
  control lane remains the state-safety boundary.
- Retry PWA startup while no cockpit projection has loaded, including while
  the iOS Home Screen document reports hidden.
- Queue one trailing retry when focus, pageshow, visible, or online recovery
  signals overlap an in-flight cockpit request.
- Preserve quiet hidden polling after the first cockpit projection loads.

## Non-goals

- No notify configuration changes and no removal or weakening of the server
  `control_lane` for refreshes or mutations.
- No polling framework, retry library, service worker, manifest, or offline
  task model.
- No WebSocket/terminal, session-cookie, TLS, or mutation-request changes.
- No browser-owned snapshot, stale merge policy, or optimistic task state.
- No global command-runner timeout policy or cross-crate runtime rewrite.
- No files under a `tests/` directory and no weakening of existing assertions.

## Delegation decision

Delegation decision: delegated via model-router. After revised approval, create
one READY TDD implementation packet per task and dispatch sequentially. The
server/architecture task routes to a backend-capable lane; the bounded frontend
task routes separately. Packet facts and tool availability remain
authoritative.

```yaml
ROUTING_DECISION:
  ACTION: LOCAL
  LANE: local
  MODE: NONE
  MODEL: NONE
  PACKET_STATUS: NOT_REQUIRED
  PACKET_REBUILD_COUNT: NONE
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE: [.planning/agent-plans/ios-pwa-connection-resilience.md]
  REASON: The user materially expanded scope to the PWA-server contract, requiring a revised architecture-aware plan before implementation.
  ESCALATE_IF: [The user does not approve the revised plan]
```

## Approval status

The original PWA-only plan was approved on 2026-07-20 with instruction to
delegate until finished. Implementation had not started when the user expanded
scope to the server contract. The revised plan was then approved with another
instruction to delegate until finished, which is advance approval to continue
through all tasks without per-task pauses. Each task still uses a separate
failing-test-first round and parent review gate.

## Task checklist

### Task 1 — Keep cockpit reads responsive behind server control work (10–15 min) — complete

- [x] **Failing test:** In `crates/ajax-web/src/runtime.rs`, prime a valid
      server projection, hold the `control_lane` with a simulated slow refresh
      or mutation, then issue authenticated `GET /api/cockpit`. Prove the
      current handler waits behind the lane. Require the revised handler to
      return `200` promptly with the current server-owned projection and no
      second refresh. After releasing the lane, prove a later cache miss still
      performs the normal serialized refresh.
- [x] **Minimal implementation:** In `crates/ajax-web/src/runtime.rs`, reuse
      `cockpit::browser_cockpit_view` to render the current shared
      `CommandContext` only when a cache miss finds the control lane already
      busy. When the lane is available, retain the existing refresh, cache,
      revision, and notification behavior. Do not move task truth, bypass
      mutation serialization, or invent a client-side merge.
- [x] **Architecture contract:** Update `architecture.md` in the same task to
      state that a Cockpit read may return the current server-owned projection
      rather than wait behind already-running control work; subsequent polls
      reconcile through the normal refresh path.
- [x] **Verification:** Run the focused RED/GREEN runtime test, existing cache
      and slow-refresh/control-lane tests, `cargo nextest run -p ajax-web`,
      `cargo fmt --check`, and focused Clippy for `ajax-web`.
- [x] **Execution gate:** Parent accepted the two-file diff after independent
      verification: focused concurrency 5/5, `ajax-web` 174/174, fmt and
      focused Clippy passed. Cursor's first report was invalid; a report-only
      revision returned the required schema without source changes.

### Task 2 — Recover a failed hidden PWA launch (5–15 min) — complete

- [x] **Failing test:** In
      `crates/ajax-web/web/src/app/App.test.tsx`, model an iOS launch with
      `document.hidden === true`; make the first cockpit request fail and the
      next succeed. Advance timers and prove the current app never issues the
      recovery request, while preserving the existing assertion that a
      successful hidden launch does not keep background-polling.
- [x] **Minimal implementation:** In
      `crates/ajax-web/web/src/app/App.tsx`, keep a short retry cadence only
      until the first cockpit projection is loaded, even if the document is
      hidden. After success, retain the existing hidden-document polling
      suppression and 60-second hidden cadence.
- [x] **Verification:** Re-run the focused new test, the existing hidden-launch
      test, and the App test file. Confirm RED for the intended missing retry,
      then GREEN without changing unrelated polling behavior.
- [x] **Execution gate:** Parent rejected Cursor's eager first revision, then
      accepted the cadence-only revision after independently passing the
      focused retry test, both hidden-launch tests, all 44 App tests, and
      `npm run web:check`. Both Cursor runs produced valid evidence in their
      raw logs but failed the runner's structured-report parser; no source
      scope violation occurred beyond expected runner artifacts.

### Task 3 — Preserve overlapping iOS recovery signals (5–15 min) — complete

- [x] **Failing test:** In
      `crates/ajax-web/web/src/app/App.test.tsx`, hold the first cockpit request
      in flight, dispatch an iOS recovery signal (`pageshow`, focus,
      visible-state transition, or `online`), finish the first request as a
      failure, and prove no trailing recovery request currently runs. Pin one
      trailing request rather than one request per event.
- [x] **Minimal implementation:** In
      `crates/ajax-web/web/src/app/App.tsx`, route resume/visibility/network
      recovery signals through the existing `loadCockpit({ trailing: true })`
      path and register the native `online` event beside the existing shell
      listeners. Add no retry abstraction or dependency.
- [x] **Verification:** Re-run the focused RED/GREEN test, App tests, cockpit
      resource/in-flight-guard tests, then `npm run web:check`,
      `npm run web:lint`, and `npm run web:build:check`.
- [x] **Execution gate:** Parent accepted the two-file Task 3 diff after
      independently passing the focused recovery-signal test, all 45 App tests,
      all 22 Cockpit resource/guard tests, type checking, and lint. Cursor's
      raw report proves RED/GREEN, but its otherwise-valid report again failed
      the runner's structured-report parser.

## Validation commands and results

```bash
# Baseline diagnosis — PASS: 1 test, 42 skipped
npm run web:test -- --run crates/ajax-web/web/src/app/App.test.tsx \
  -t 'loads the cockpit on mount while hidden, but skips the background poll'

# Task 1 server contract — PASS
cargo nextest run -p ajax-web -E \
  'test(axum_cockpit_returns_current_projection_while_control_lane_is_busy)' # 1/1
cargo nextest run -p ajax-web -E \
  'test(/axum_cockpit_serves_cached_projection_within_refresh_ttl|refresh_cockpit_and_cache_refreshes_once_and_caches|axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state|axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state/)' # 4/4
cargo nextest run -p ajax-web # 174/174
cargo fmt --check # PASS
cargo clippy -p ajax-web --all-targets --all-features -- -D warnings # PASS

# Task 2 PWA hidden-launch retry — PASS
npm run web:test -- --run src/app/App.test.tsx \
  -t 'retries a failed hidden PWA launch until the first cockpit projection loads'
npm run web:test -- --run src/app/App.test.tsx \
  -t 'loads the cockpit on mount while hidden, but skips the background poll|retries a failed hidden PWA launch until the first cockpit projection loads'
npm run web:test -- --run src/app/App.test.tsx
npm run web:check

# Task 3 PWA recovery — PASS
npm run web:test -- --run src/app/App.test.tsx
npm run web:test -- --run \
  src/shared/hooks/useCockpitResource.test.tsx \
  src/shared/lib/cockpitPoll.test.ts
npm run web:check
npm run web:lint
npm run web:build:check

# Final integration — PASS
npm run web:test -- --run # 46 files, 417 tests
npm run web:build:check # production embedded assets rebuilt and checked
cargo nextest run -p ajax-web # 174 tests
cargo fmt --check
cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
cargo check --all-targets --all-features
git diff --check
```

## Deviations

- The originally referenced
  `.planning/agent-plans/notify-off-control-lane.md` is absent. This plan uses
  the validated iOS hidden-launch recovery gap and deliberately excludes the
  unsupported notify/control-lane hypothesis.
- The user's later `delegate until finished` instruction supersedes the
  per-task continuation pauses in this plan. Parent review and validation gates
  remain required between delegated rounds.
- Before source or test implementation began, the user expanded scope from a
  PWA-only fix to the PWA-server contract. The plan now preserves the control
  lane but prevents Cockpit reads from waiting behind already-running lane
  work by serving the current server-owned projection.
- Task 2's first delegated implementation retried immediately when the first
  hidden request failed. Parent review rejected that burstier behavior; the
  revision pins and implements the approved one-second retry cadence.
- The successful full App test emits jsdom's existing xterm canvas
  `HTMLCanvasElement.prototype.getContext` warning while exiting zero.
- Delegates were forbidden from touching generated assets. Final
  `web:build:check` was required and updated the tracked `dist/app.js`; that
  asset is embedded by `crates/ajax-web/src/adapters/assets.rs`, so the parent
  reviewed and retained the generated update because it reflects only the
  accepted source changes.
- `web:build:check` updated only tracked `dist/app.js` (17 added and 17 removed
  minified lines); review confirmed the compiled shell contains the accepted
  one-second startup retry and trailing recovery listeners.
- Expected TDD RED commands failed before each implementation as required.
  Cursor's structured-report wrapper also exited 65 for both Task 2 rounds and
  Task 3 even though each raw log contained a complete report; parent review
  used those preserved logs and independent validation instead. Two initial
  `delegate-delta` calls used the wrong option form and printed usage; the
  corrected `inspect ... --allowed` checks passed with only runner artifacts
  outside the source allowlist.
- The full frontend suite passed 417/417 while printing two non-fatal existing
  jsdom/xterm canvas warnings. No product validation command failed.
