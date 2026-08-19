# Dropped and stale Web Session Cleanup

## Scope

Ensure Web Cockpit orchestration sessions cannot outlive their owning Ajax
task. A task is an owner while it exists in the registry and is not in the
`Removed` lifecycle. Sessions with no such owner are stale and must be
removed, including their persisted JSONL transcript and any live ACP slot.
Dropping a task must perform the same cleanup before a later task can reuse the
same qualified handle.

## Non-goals

- Do not delete transcripts for existing, recoverable tasks.
- Do not change core task lifecycle or registry authority.
- Do not infer ownership from browser routes, localStorage, or ACP state.
- Do not remove Git, tmux, worktree, branch, or pull-request data beyond the
  existing Drop operation.

## Checklist

- [x] Add store-level deletion and stale-session classification from registry
      task handles.
- [x] Prune unowned persisted sessions during Web Cockpit initialization.
- [x] Make successful task Drop shut down the live session and delete its
      persisted transcript before handle reuse.
- [x] Add regression coverage for stale startup cleanup and Drop/recreate
      isolation.
- [x] Update the Web Session architecture contract with the ownership/cleanup
      invariant.
- [x] Run focused `ajax-web` tests and relevant Rust checks.

## Approval

Implementation was explicitly requested by the operator on 2026-08-19.

## Deviations

None.

## Validation

- `cargo test -p ajax-web` (or `cargo nextest run -p ajax-web` when available):
  pass — includes `issue_977_*` regression tests in
  `session_cleanup_tests.rs` and store deletion/list tests.
