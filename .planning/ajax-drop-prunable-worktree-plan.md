# Ajax Drop Prunable Worktree Plan

## Goal

Make forced task drop remove Git's stale worktree registration before deleting
the task branch, and make a retry recover when the worktree directory was
already moved to trash by the previous attempt.

## Task 1: Prune fast-removed worktrees before branch deletion

### Failing behavior test

- Update the focused behavior test in
  `crates/ajax-core/src/task_operations.rs`.
- Assert the forced-drop fast-remove command:
  - moves the expected worktree to Ajax trash when the source path exists;
  - tolerates the source path already being absent on retry;
  - runs `git worktree prune` before the branch-delete step;
  - keeps trash deletion asynchronous.
- Run the focused test and confirm it fails against the current fast-remove
  command, which moves the directory but never prunes Git's registration.

### Code to implement

- Update `fast_remove_worktree` in
  `crates/ajax-core/src/task_operations.rs` with the smallest shell-command
  change that:
  - conditionally moves an existing worktree path;
  - always prunes stale worktree metadata after the move or on retry;
  - preserves background trash deletion.
- Do not change public APIs, operation ordering, or unrelated drop behavior.

### Verification

```sh
rtk cargo nextest run -p ajax-core confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline
rtk cargo nextest run -p ajax-core task_operations
```

## Final Validation

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```
