# API Speed Plan Execution Plan

This plan follows `api-speed-plan.md` and keeps the architecture boundaries in `architecture.md` intact.

## Task 1: Add external command timing logs

- Test to write:
  - `crates/ajax-core/src/adapters/process.rs`
  - `timing_log_line_renders_program_args_and_elapsed_ms`
  - `timing_log_line_truncates_long_argument_lists`
- Code to implement:
  - Add a pure `timing_log_line(&CommandSpec, Duration) -> String` helper.
  - Emit the line from `ProcessCommandRunner::run` only when `AJAX_TIMING` is enabled.
- Verification:
  - Run the focused `ajax-core` tests for timing log line behavior.
  - Manually confirm one timing line per external command with `AJAX_TIMING=1`.

## Task 2: Bound `git fetch` during task start

- Test to write:
  - `crates/ajax-core/src/adapters/git.rs`
  - `fetch_origin_branch_carries_bounded_timeout`
- Code to implement:
  - Add `GIT_FETCH_TIMEOUT`.
  - Apply the timeout to `fetch_origin_branch`.
- Verification:
  - Run the focused `ajax-core` test for fetch timeout behavior.
  - Re-run the `new_task_plan` tests to confirm existing expectations still hold.

## Task 3: Skip fetch when origin evidence is fresh

- Test to write:
  - `crates/ajax-core/src/commands/new_task.rs`
  - `new_task_plan_skips_fetch_when_origin_fetch_is_fresh`
  - `new_task_plan_fetches_when_origin_fetch_is_stale`
  - `new_task_plan_fetches_when_origin_fetch_age_is_unknown`
  - `crates/ajax-core/src/adapters/environment.rs`
  - `origin_fetch_age_reads_fetch_head_mtime`
  - `origin_fetch_age_is_none_without_fetch_head`
- Code to implement:
  - Add origin-fetch age probing in the environment adapter.
  - Thread the observation into start planning.
  - Skip `git fetch` when the evidence is fresh.
- Verification:
  - Run the focused `origin_fetch` and `new_task_plan` tests.
  - Run the `ajax-web` start-task path check.

## Task 4: Move husky install and bootstrap into the task session

- Test to write:
  - `crates/ajax-core/src/commands/new_task.rs`
  - `new_task_plan_has_no_standalone_husky_command`
  - `new_task_plan_chains_setup_before_agent_in_task_session`
  - `new_task_plan_chains_bootstrap_between_husky_and_agent`
  - Update the existing command-indexing assertions that depend on the shorter plan.
- Code to implement:
  - Remove standalone husky and bootstrap commands from the plan.
  - Inline them into the agent session `send-keys` shell line.
- Verification:
  - Run the focused `new_task` and `start_operation` tests.
  - Run the `ajax-web` start-task path check.

## Task 5: Background graphify update

- Test to write:
  - `crates/ajax-core/src/commands/new_task.rs`
  - `new_task_plan_runs_graphify_update_detached`
  - Update the existing graphify expectation.
- Code to implement:
  - Wrap `graphify_update` in a detached subshell so it returns immediately.
- Verification:
  - Run the focused `graphify` test.

## Task 6: Make confirmed drop faster

- Test to write:
  - `crates/ajax-core/src/task_operations.rs`
  - `confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline`
  - `unforced_dirty_drop_keeps_plain_git_worktree_remove`
  - `fast_drop_mv_failure_marks_teardown_incomplete`
  - Classifier coverage for the fast worktree remove composite and missing-source handling.
- Code to implement:
  - Rename worktrees into sibling `.ajax-trash` entries for fast confirmed drops.
  - Prune and delete in the background.
  - Preserve the plain `git worktree remove` backstop when required.
- Verification:
  - Run the focused `drop` and `teardown` tests.
  - Run the `ajax-web` drop path check.

## Task 7: Sweep stale trash entries

- Test to write:
  - `crates/ajax-core/src/task_operations.rs`
  - `sweep_cleanup_removes_stale_trash_entries`
- Code to implement:
  - Add a guarded trash sweep step to `tidy` planning.
- Verification:
  - Run the focused `sweep` test.

## Task 8: Coalesce Web Cockpit refreshes

- Test to write:
  - `crates/ajax-web/src/runtime.rs`
  - `axum_cockpit_serves_cached_projection_within_refresh_ttl`
  - `axum_cockpit_refreshes_again_after_ttl_expires`
  - `axum_operation_invalidates_cockpit_refresh_cache`
  - `concurrent_cockpit_polls_share_one_refresh`
- Code to implement:
  - Add refresh TTL and single-flight state to the web runtime.
  - Serve cached cockpit projections within the TTL.
  - Invalidate the cache on mutations.
- Verification:
  - Run the focused `cockpit` tests.
  - Run the full `ajax-web` suite.

## Task 9: Update architecture documentation

- Documentation to update:
  - `architecture.md`
  - Reflect the start-path evidence changes.
  - Reflect the fast drop and trash sweep behavior.
  - Reflect the Web Cockpit refresh cache behavior.
- Verification:
  - Read through the updated sections against the implementation.

## Final Verification

- Run:
  - `rtk cargo fmt --check`
  - `rtk cargo check --all-targets --all-features`
  - `rtk cargo clippy --all-targets --all-features -- -D warnings`
  - `rtk cargo nextest run --all-features`
