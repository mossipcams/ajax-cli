PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Fix persist-then-lose-CAS: when `run_optimistic` persists durably on a clone then loses the process-local `revision` CAS (to terminal ack / Diff `run_read`), do **not** return a false `409`. Reload shared state from SQLite (authority) and return a success/error response with a fresh cockpit. Never store a lost-race false `409` for `request_id` replay when the operate was durable.

## Allowed files

- `crates/ajax-web/src/runtime/state.rs`
- `crates/ajax-web/src/runtime/bridge.rs`
- `crates/ajax-web/src/runtime/task_routes/live.rs`
- `crates/ajax-web/src/runtime/tests/mod.rs`
- `crates/ajax-web/src/runtime/tests/suite_2.rs`
- `crates/ajax-web/src/runtime/tests/suite_3.rs`
- `crates/ajax-web/src/runtime/tests/suite_4.rs`
- `crates/ajax-web/src/runtime/tests/suite_5.rs`
- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-cli/src/web_backend/tests/suite_1.rs`
- `crates/ajax-cli/src/web_backend/tests/suite_2.rs`
- `docs/architecture/web-cockpit.md`

## Forbidden changes

- Per-task mutation concurrency / removing OperationCoordinator
- R0/R2 behavior regressions
- Frontend
- Commits / branch changes
- Weakening SQLite merge / empty-registry guards

## Acceptance

1. Change `run_optimistic` so the operate closure returns durability (e.g. `(Response, bool)` where `true` means disk was / should have been persisted — `state_changed` from operate success or `ActionFailure.state_changed`).
2. On CAS win: unchanged (commit clone, bump revision, clear cache, return response).
3. On CAS loss + **not** durable: keep today’s conflict `409` with `conflict_message`.
4. On CAS loss + **durable**:
   - Force-reload registry context from disk via a new `RuntimeBridge` method (e.g. `reload_registry_from_disk`) into **shared** state (not reinstall the pre-ack clone).
   - Default trait method may no-op for in-memory tests; `CliRuntimeBridge` must load tracked context from paths (always reload on this path, not only when mtime/revision look stale relative to the clone’s save_state).
   - Bump `shared.revision`, clear cockpit cache.
   - Return a response that preserves operate `ok` / `output` / error fields from the durable result but attaches `browser_cockpit_view` from the reloaded shared context (HTTP 200 for successful operate, existing operate-error status for durable failures — **not** the generic conflict 409).
5. `OperationCoordinator.finish` / replay must record the **final** returned response (recovered success), never a discarded false conflict 409 for a durable operate.
6. Characterization test: while an optimistic mutate runs, bump `shared.revision` (e.g. via `operator_input_sink` or direct revision bump + optional persist); when operate reports durable, HTTP is not the generic conflict 409 and shared state reflects reloaded/disk truth. True double-mutate via OperationCoordinator still 409.
7. Update `docs/architecture/web-cockpit.md` optimistic-commit paragraph to state: after durable persist, lost process-local CAS recovers via SQLite reload.

## Constraints

Smallest diff that meets acceptance. Keep single-mutation gate. Do not parse durability solely from JSON if the closure can return a bool.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web -- run_optimistic cas recover revision operator_input axum_action axum_task_start control_lane
      expected: pass (include new recovery test; adjust filter to match names)
    - type: test
      command: cargo nextest run -p ajax-cli -- web_backend
      expected: pass
    - type: build
      command: cargo check -p ajax-web -p ajax-cli --all-targets
      expected: compiles
  reason: Race characterization plus CLI bridge reload tests lock the High correctness bug.
```

## Stop if

- Cannot force-reload without breaking ContextSaveState invariants
- Diff exceeds ~300 production LOC or needs ajax-core registry redesign
- Unclear whether durable operate-error (state_changed true) should recover — **yes, reload and keep the operate error response shape with fresh cockpit, not generic conflict 409**
