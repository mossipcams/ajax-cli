PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Stop soft-wedging the Web Cockpit Tokio runtime during cockpit refresh, and stop the background push tick from running `RefreshTier::Full` while a browser is connected.

1. Move cockpit refresh substrate work onto `tokio::task::spawn_blocking` using `control_lane.blocking_lock()` / `try_lock` inside the blocking task (same pattern as start/action/Diff). Do not hold a non-Send async MutexGuard across `spawn_blocking`.
2. Preserve: 750ms cache hit, stale current-projection fallback when the lane is busy, revision-checked commit, push delivery side effect when requested.
3. Push tick (`spawn_push_tick`): if `browser_connected()` → skip the tick body entirely; else if subscriptions exist → Full + deliver; else skip.

## Allowed files

- `crates/ajax-web/src/runtime/task_routes/cockpit.rs`
- `crates/ajax-web/src/runtime/mod.rs`
- `crates/ajax-web/src/runtime/tests/suite_2.rs`
- `crates/ajax-web/src/runtime/tests/suite_3.rs`
- `crates/ajax-web/src/runtime/tests/suite_4.rs`
- `docs/architecture/web-cockpit.md`

## Forbidden changes

- Changing projection JSON shapes or RefreshTier meanings in ajax-core
- R0 restart policy, R4 CAS recovery, R1 health schema
- Frontend
- Commits / branch changes
- Broad runtime refactors unrelated to refresh/tick

## Acceptance

1. `GET /api/cockpit` cache hit path unchanged (no lane, no spawn).
2. When refresh runs, work executes inside `spawn_blocking` with lane acquired via `blocking_lock` or `try_lock` inside that task — not sync substrate work on the async worker after `lock().await`.
3. When lane is busy, cockpit still returns the current in-memory projection promptly (existing busy-lane behavior).
4. `axum_health_stays_responsive_during_slow_cockpit_refresh` (and similar health-isolation tests) still pass.
5. Push tick does not call Full refresh while `browser_connected()` is true (add/adjust test).
6. Without browser + with subscriptions, tick still Full + deliver.
7. Update the push-tick paragraph in `docs/architecture/web-cockpit.md` to match.

## Constraints

Keep single-flight serialization via `control_lane`. Smallest diff. Prefer one shared helper for “refresh under blocking lane” used by HTTP and tick.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web -- refresh_cockpit axum_health axum_cockpit browser_connected push_tick control_lane
      expected: pass (adjust filter if test names differ; must cover refresh + health isolation + tick)
    - type: build
      command: cargo check -p ajax-web --all-targets
      expected: compiles
  reason: Existing characterization tests plus tick-tier assertion validate soft-wedge fix.
```

## Stop if

- Cannot move refresh off async worker without changing snapshot semantics
- Diff sprawls beyond Allowed or exceeds ~250 lines of production code
- Need ajax-core RefreshTier API changes
