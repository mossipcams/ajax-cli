PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Make authenticated `GET /api/cockpit` return the current server-owned Cockpit
projection promptly when another refresh, notify tick, action, or task start
already owns the process-local control lane. Preserve normal serialized refresh
behavior when the lane is available.

## Allowed files

- `crates/ajax-web/src/runtime.rs`
- `architecture.md`

## Forbidden changes

- Do not edit frontend files, generated assets, dependencies, configuration,
  session/authentication code, notification behavior, terminal/WebSocket code,
  operation/start admission, registry/core projection logic, or any file under
  a `tests/` directory.
- Do not remove the `control_lane`, let mutations bypass it, cache the fallback
  snapshot, add browser-owned state, or change API DTO fields/status codes.
- Do not weaken, delete, or rewrite existing assertions.

## Context evidence

- Desired behavior: `crates/ajax-web/src/runtime.rs:835-845` marks browser
  presence, uses a fresh cache when available, then unconditionally calls the
  lane-waiting refresh path.
- Server bottleneck: `crates/ajax-web/src/runtime.rs:853-899`
  `refresh_cockpit_and_cache` awaits `control_lane.lock()` before cloning,
  refreshing, committing, and caching the projection.
- Existing projection owner: `crates/ajax-web/src/runtime.rs:1264-1277`
  `handle_refreshed_cockpit_request` renders
  `cockpit::browser_cockpit_view(context)` from server/core state.
- Test seam: `crates/ajax-web/src/runtime.rs:1390-1518` `TestBridge` exposes
  `refresh_entered`, `refresh_release`, `refresh_calls`, and `release_gate` for
  deterministic slow-refresh concurrency tests.
- Preserved state-safety contract:
  `crates/ajax-web/src/runtime.rs:3510-3629` proves operations and task starts
  wait for a slow refresh and do not discard refresh state.
- Architecture boundary: `architecture.md:830-855` requires short shared-state
  locks, a single async control lane for refresh/notify/mutations, and keeps
  lightweight routes outside the lane.

## Code anchors

- Test module anchor: place the new multi-thread Tokio test beside
  `axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state`
  in `crates/ajax-web/src/runtime.rs`.
- Handler anchor: `axum_cockpit` in `crates/ajax-web/src/runtime.rs`.
- Refresh anchor: `refresh_cockpit_and_cache` in the same file. Split only the
  already-locked synchronous body if needed so `axum_cockpit` can atomically
  `try_lock` and either refresh under that guard or render the current shared
  projection when the lane is busy.
- Documentation anchor: the `Runtime coordination contract (implemented)` and
  `control_lane` paragraphs in `architecture.md:838-855`.

## Test-first instructions

Add
`axum_cockpit_returns_current_projection_while_control_lane_is_busy` as a
`#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`.

1. Build state/app/cookie with `context_with_task()` and a `TestBridge` whose
   first refresh blocks through `refresh_entered`/`refresh_release`.
2. Spawn the first authenticated Cockpit GET and wait until the bridge reports
   that refresh entered (therefore the control lane is held).
3. Issue a second authenticated Cockpit GET under a short timeout. Always call
   `release_gate` before asserting the timeout result so the RED test cannot
   strand a Condvar-blocked worker.
4. Require the second response to complete within the short timeout, return
   `200`, contain the existing `web/fix-login` card from the shared server
   context, and leave `refresh_calls == 1` while the first refresh is blocked.
5. Release and await the original refresh. After the cache TTL expires, issue
   another GET and require normal refresh count to advance, proving the
   fallback did not become a second cache/freshness policy.

Run RED before production edits:

```bash
cargo nextest run -p ajax-web -E 'test(axum_cockpit_returns_current_projection_while_control_lane_is_busy)'
```

The expected failure is that the second GET times out waiting for the control
lane.

## Edit instructions

- Keep the fresh-cache fast path in `axum_cockpit` unchanged.
- On a cache miss, attempt to acquire the existing `control_lane` without
  waiting. If already busy, take only the short shared-state lock and serialize
  `cockpit::browser_cockpit_view(&guard.context)` into the same `200` JSON
  response shape. Do not cache or commit this fallback response.
- If the lane is available, perform the exact existing refresh/commit/cache
  body under the acquired guard. Refactor the current function only enough to
  avoid dropping and reacquiring the guard; background notify callers must
  continue using the awaiting wrapper.
- Update `architecture.md` with the narrow rule: Cockpit reads may serve the
  current server-owned projection while control work is already in flight;
  later polls use the normal refresh path. Keep the control lane and core
  authority statements intact.

## Verification commands

```bash
cargo nextest run -p ajax-web -E 'test(axum_cockpit_returns_current_projection_while_control_lane_is_busy)'
cargo nextest run -p ajax-web -E 'test(/axum_cockpit_serves_cached_projection_within_refresh_ttl|refresh_cockpit_and_cache_refreshes_once_and_caches|axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state|axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state/)'
cargo nextest run -p ajax-web
cargo fmt --check
cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- The new focused test is observed RED for a lane-wait timeout before source
  edits and GREEN afterward.
- A second Cockpit GET completes with the current server-owned projection while
  a first refresh holds the lane, without starting another refresh.
- Lane-available cache misses still refresh, commit by revision, and cache as
  before.
- Operations/task starts still wait for refreshes and retain their existing
  concurrency semantics.
- Architecture text accurately documents the implemented read fallback.
- All verification commands pass with no unrelated file edits.

## Stop conditions

- Stop if the behavior requires changing browser DTOs, core projection logic,
  authentication, mutation/start serialization, or notification delivery.
- Stop if the fallback cannot be built solely from the current shared
  `CommandContext`, or if it would fabricate/merge task state.
- Stop on unrelated baseline failures, changed anchors, edits outside allowed
  files, or a patch approaching 400 changed lines.
