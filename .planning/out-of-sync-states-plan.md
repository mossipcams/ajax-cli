# Out-of-sync states — plan

## Symptom

Web Cockpit shows tasks that the native Cockpit (TUI) does not. Reproduced
against the stable profile, with `ajax` auto-launching `ajax web` as a child.

## Root cause

Native and Web Cockpit run in two processes that share one SQLite state file,
but they apply asymmetric persistence contracts:

| Surface | Loads SQLite | Reloads on mtime advance | Saves on state-changing action |
|---|---|---|---|
| `ajax-cli web` companion | startup | yes — `CliRuntimeBridge::reload_context_if_stale` (crates/ajax-cli/src/web_backend.rs:206) | yes — `persist_operate` and after each cockpit refresh state change |
| Native Cockpit interactive loop | startup via `load_tracked_context` (crates/ajax-cli/src/lib.rs:176) | **no** — neither `refresh_live_context` nor `refresh_cockpit_snapshot` consults SQLite (crates/ajax-cli/src/cockpit_backend.rs:342, 371) | **no** — `state_changed` only flips a local flag; `save_tracked_context` is reached only after the loop exits in `run_with_args_to_writer` (crates/ajax-cli/src/lib.rs:206) |

Consequences:

1. Web Cockpit (or any external writer) advances SQLite → Native Cockpit never
   reloads → Native Cockpit shows fewer / stale tasks. (Reported symptom.)
2. Native Cockpit-initiated actions remain in-memory until exit → Web Cockpit
   lags until the operator quits the TUI.

Architecture intent (`architecture.md` lines 271–273, 606–622) describes Native
Cockpit as "in-memory context, saves on change" and Web Cockpit as
"reloads SQLite only when the state file mtime advances or after a mutating
operation persisted to disk." The Native Cockpit interactive loop violates the
"saves on change" half and lacks any reload mechanism. This plan brings Native
Cockpit's interactive loop in line with the same reload-on-mtime + save-on-
state-change contract Web Cockpit already implements.

## Out of scope

- Refactoring `CliRuntimeBridge` to also back the Native Cockpit. Reusing the
  bridge wholesale would re-introduce indirection that the Native Cockpit does
  not need; instead this plan extracts the small reload/save helpers used by
  both surfaces.
- Saving on every substrate-only refresh. Only operator-initiated state changes
  (drop, ship, start, ack, etc.) and any refresh that materially changes the
  registry should hit SQLite.
- Any change to `ajax-tui` rendering, layout, or input.
- Modifying `crates/ajax-cli/tests/smoke_user_flows.rs` (prohibited by
  AGENTS.md).

## Tasks

Each code task includes a failing behavior test, the production change, and a
verification command.

### Task 1 — Native Cockpit refresh reloads SQLite on mtime advance

- **Failing test** (new module `cockpit_persistence_tests` in
  `crates/ajax-cli/src/cockpit_backend.rs`):
  - Set up a `CliContextPaths` pointing at a tempdir SQLite database.
  - Save a registry with one task `web/a` via `SqliteRegistryStore::save`.
  - Build a cockpit `TrackedContext` from those paths and assert the snapshot
    shows one card.
  - From a separate `SqliteRegistryStore` handle, save a registry with two
    tasks (`web/a`, `web/b`), forcing the SQLite revision to advance and the
    file's mtime to move (sleep ≥ 1s on macOS HFS+/APFS resolution if needed,
    or assert via `current_revision`).
  - Call the new `refresh_cockpit_snapshot_with_paths(..., paths,
    last_loaded_mtime)` with a no-op runner and assert that the returned
    snapshot now contains two cards and `last_loaded_mtime` has advanced.
  - Test currently fails because `refresh_cockpit_snapshot` never reloads
    SQLite.
- **Implementation** in `crates/ajax-cli/src/cockpit_backend.rs`:
  - Add a small helper
    `reload_cockpit_context_if_stale(context, paths, last_loaded_mtime)` that
    mirrors `CliRuntimeBridge::reload_context_if_stale` (state_file_mtime,
    `load_context`, swap `context.registry`). Place this helper next to
    `refresh_live_context` so it stays operator-visible in code review.
  - Introduce `refresh_cockpit_snapshot_with_paths(...)` that calls the
    reload helper before `refresh_live_context`, then keeps the existing
    snapshot-cache logic.
  - Have `refresh_cockpit_snapshot` delegate by passing `None` for the new
    paths argument so existing call sites and tests stay green.
- **Verify**:
  - `cargo nextest run -p ajax-cli cockpit_backend::tests` and the new
    `cockpit_persistence_tests` module.

### Task 2 — Native Cockpit interactive loop reloads via tracked paths

- **Failing test** (`cockpit_persistence_tests`):
  - Build `TrackedContext` with paths and an initial 1-task SQLite.
  - Run one iteration of `refresh_cockpit_snapshot_with_paths` (Task 1) and
    cache its snapshot.
  - Mutate SQLite externally to add a task. Re-run refresh and assert the
    cached snapshot is rebuilt with the new card (this asserts both the SQLite
    reload AND the cache-invalidation interaction).
- **Implementation** in `crates/ajax-cli/src/cockpit_backend.rs`:
  - Plumb `Option<(&CliContextPaths, &mut Option<SystemTime>)>` into
    `render_interactive_cockpit_command`, `InteractiveCockpitHandler`, and
    `refresh_cockpit_snapshot_with_paths`. Maintain `last_loaded_mtime` across
    iterations so the reload check is O(1) when SQLite is unchanged.
  - Have `InteractiveCockpitHandler::on_refresh` call the new
    paths-aware refresh.
  - Update `render_cockpit_entry_command` /
    `render_matches_mut_with_paths` to forward `paths` to the interactive
    handler (it already has `paths: Option<&CliContextPaths>` in scope).
- **Verify**:
  - `cargo nextest run -p ajax-cli cockpit_backend`.

### Task 3 — Native Cockpit persists state-changing actions inside the loop

- **Failing test** (`cockpit_persistence_tests`):
  - Build `TrackedContext` with paths and a 1-task SQLite registry.
  - Construct a minimal `PendingAction` (e.g. `Drop` against the seeded task)
    and call a refactored `handle_pending_cockpit_action(...)` that takes the
    tracked context + paths.
  - After the call returns (with mocked `CommandRunner` that drives the drop
    to terminal lifecycle), read SQLite directly via `SqliteRegistryStore::load`
    and assert the task lifecycle is `Removed` / matching the in-memory state.
  - Currently fails because nothing persists between actions.
- **Implementation** in `crates/ajax-cli/src/cockpit_backend.rs`:
  - Extract a `handle_pending_cockpit_action(...)` helper that takes
    `&mut TrackedContext`, `Option<&CliContextPaths>`, the pending action, the
    runner, and the task session. After
    `execute_pending_cockpit_action_with_task_session` returns and bumped
    `state_changed`, call `save_tracked_context` and refresh `last_loaded_mtime`.
  - In `render_interactive_cockpit_command`, replace the inline call with
    `handle_pending_cockpit_action`. Keep `tracked.save_state` as the source of
    truth for revisions so the existing exit-time save in
    `run_with_args_to_writer` still works (idempotent on no further changes).
- **Verify**:
  - `cargo nextest run -p ajax-cli cockpit_backend`.

### Task 4 — Architecture documentation

- **Doc change** in `architecture.md` (Native and Web persistence section,
  lines ~606–622):
  - Replace "Native Cockpit uses in-memory context and saves on change; web
    reloads SQLite only when the state file mtime advances or after a mutating
    operation persisted to disk." with a description that makes both surfaces
    use the same reload-on-mtime + save-on-operator-action contract during
    their interactive loops, and note that the exit-time `save_tracked_context`
    in `run_with_args_to_writer` is now a defensive backstop rather than the
    primary persistence path for Native Cockpit.
- **Verify**:
  - Re-read the updated section.
  - `grep -n "Native Cockpit" architecture.md` to confirm wording stays
    consistent with the rest of the document.

## Validation after Task 3

Per AGENTS.md "Required Validation":

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Plan ready. Approve to proceed.
