# Ctrl-Q State Conflict Fix Plan

## Diagnosis

Native Cockpit reloads SQLite when the state-file mtime advances, but
`reload_cockpit_context_if_stale` updates only the in-memory registry and mtime.
It leaves `ContextSaveState.loaded_registry` and `loaded_revision` pointing at
the pre-reload snapshot. A later live refresh changes the reloaded task, and the
Ctrl-Q return/exit save compares that task against the obsolete baseline,
producing:

`state conflict for <task>: disk and in-memory task facts diverged`

The intended optimistic-concurrency behavior remains valid for genuinely
independent same-task edits. The fix should only synchronize save tracking when
Cockpit has deliberately reloaded the latest disk state.

## Task 1: Keep Cockpit Save Tracking Aligned With Disk Reloads

Estimated time: 10–15 minutes.

- Failing behavior test to write:
  - Add a unit regression test in
    `crates/ajax-cli/src/cockpit_backend.rs`.
  - Load a tracked Cockpit context, persist a concurrent same-task disk change,
    trigger the mtime-based Cockpit reload, apply a subsequent in-memory
    refresh/mutation, and assert that `save_cockpit_state_to_sqlite` succeeds
    while preserving both the reloaded disk fact and the later in-memory fact.
  - Run the focused test first and show that it fails with the current
    `disk and in-memory task facts diverged` error.
- Code to implement:
  - Thread Cockpit's `ContextSaveState` through the reload path.
  - When a stale context is replaced from SQLite, reset the loaded registry
    baseline and loaded revision to that freshly loaded disk state.
  - Preserve the existing conflict behavior for writers that did not reload
    and genuinely edit the same task concurrently.
- Verification:
  - Run the focused regression test with `cargo nextest run`.
  - Run the existing concurrent same-task conflict tests to confirm real
    conflicts are still rejected.

## Task 2: Validate the Completed Fix

Estimated time: 10–15 minutes.

- Verification only:
  - `rtk cargo fmt --check`
  - `rtk cargo check --all-targets --all-features`
  - `rtk cargo clippy --all-targets --all-features -- -D warnings`
  - `rtk cargo nextest run --all-features`
- Documentation check:
  - Re-read the Native and Web persistence section of `architecture.md`.
  - No architecture edit is expected because the fix restores the documented
    reload/save contract rather than changing it.

