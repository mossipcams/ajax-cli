PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Fix Bugbot medium: durable CAS-loss recovery must not return HTTP success with a cockpit from stale shared state when `reload_registry_from_disk` is a no-op (e.g. `CliRuntimeBridge` with `paths == None`, or in-memory bridges). If reload did not replace context from durable storage, install the operate clone into shared (same as a CAS win), then attach fresh cockpit.

## Allowed files

- `crates/ajax-web/src/runtime/state.rs`
- `crates/ajax-web/src/runtime/bridge.rs`
- `crates/ajax-web/src/runtime/tests/mod.rs`
- `crates/ajax-web/src/runtime/tests/suite_5.rs`
- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-cli/src/web_backend/tests/suite_2.rs`
- `docs/architecture/web-cockpit.md`

## Forbidden changes

- Changing when `durable` is set from `state_changed` (call sites can stay)
- Removing OperationCoordinator / SQLite merge guards
- R0/R2 behavior
- Frontend, commits, branch changes
- Files outside Allowed

## Acceptance

1. `RuntimeBridge::reload_registry_from_disk` returns `Result<bool, WebError>`: `true` iff `context` was replaced from durable storage; default trait body `Ok(false)`.
2. `CliRuntimeBridge`: `Ok(false)` when `paths` is `None`; after successful `load_tracked_context`, `Ok(true)`.
3. `TestBridge`: return `Ok(true)` only when a disk snapshot was applied; otherwise `Ok(false)` (still increment `reload_calls`).
4. `run_optimistic` CAS loss + durable:
   - Call reload into **shared** context/bridge.
   - On reload `Err` → existing error response path.
   - On reload `Ok(false)` → install operate `context`/`runner`/`bridge` clone into shared (CAS-win style).
   - On reload `Ok(true)` → do **not** reinstall the pre-reload clone (disk wins).
   - Always bump revision, clear cockpit cache, `response_with_fresh_cockpit` from **shared** context.
5. Existing durable recovery + ephemeral conflict tests still pass.
6. New test: durable operate + CAS loss where reload does not apply a snapshot → HTTP not generic conflict 409, and shared state reflects the operate clone (operate must mutate the clone so the assertion is meaningful — e.g. clear registry on `state_changed` when a test flag is set, and do not record disk).
7. One-line arch doc tweak: recovery uses SQLite reload when present, otherwise installs the durable operate clone.

## Constraints

Smallest diff. Keep single-mutation gate.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web -- run_optimistic recover cas durable
      expected: pass (existing + new no-disk/install-clone case)
    - type: test
      command: cargo nextest run -p ajax-cli -- web_backend reload_registry
      expected: pass
    - type: build
      command: cargo check -p ajax-web -p ajax-cli --all-targets
      expected: compiles
  reason: Locks the Bugbot false-success path without reopening full web_backend.
```

## Stop if

- Diff needs ajax-core redesign
- Unclear whether to install clone after successful disk reload — **do not**; disk is authority
