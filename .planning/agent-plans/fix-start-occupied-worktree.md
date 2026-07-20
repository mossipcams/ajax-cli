# Fix start collisions for occupied worktree path / branch

## Scope

When Start plans `git worktree add -b …`, block with a precise plan error
before git runs if the target worktree path already exists, the target branch
already exists, or another non-removed registry task already claims that path
or branch.

Non-goals: Repair behavior (already fixed in #597), deleting/moving occupying
worktrees, auto-adopting orphan worktrees, tmux session collisions, changing
task naming/placement.

Delegation decision: delegated via model-router

Approval status: authorized by user request to patch this and similar edge cases
(2026-07-20).

## Task checklist

- [x] Task 1 — Block Start on create-time substrate collisions.
  - Test: add focused behavior tests in `crates/ajax-core/src/commands/new_task.rs`
    for (a) existing worktree path, (b) observed existing branch, (c) registry
    path/branch claim by another task. Prove RED then GREEN.
  - Implementation: extend `StartPlanObservation` + `new_task_plan_with_observation`
    preflight; update CLI/web observation builders (and cockpit start) so live
    branch existence is observed. Keep planners non-destructive.
  - Verification: focused tests, then `cargo test -p ajax-core`,
    `cargo fmt --check`, `cargo clippy -p ajax-core --all-targets --all-features -- -D warnings`.
- [x] Task 2 — Parent review gate + broader validation (no PR unless asked).

## Risks

- Must not delete or overwrite occupying paths/branches.
- Default `new_task_plan` unit tests use fake paths that do not exist; live
  filesystem/branch probes must keep those green.
- Call sites that construct `StartPlanObservation` must be updated.
- Readonly start plan preview in `snapshot_dispatch` still calls bare
  `new_task_plan` (path check yes; live branch probe no). Executing start
  paths (CLI/web/cockpit) all probe branch.

## Deviations

- Pi/GLM completed the source edits but returned an invalid
  `DELEGATE_REPORT` schema. Parent reviewed the filesystem delta, ran
  validation independently, and accepted the diff.
- Parent added a missing blank line before the new path-exists test.
- Pre-commit `live_cli` failed because fake git rejected the new
  `show-ref` branch probe. Parent taught live_cli and smoke fake-git to
  treat absent ajax branches as exit 1.
- `smoke_new_plan_has_no_side_effects` still forbade any git during plan;
  updated it to allow read-only `show-ref` while forbidding mutations.

## Validation results

- Focused: `rtk cargo test -p ajax-core new_task_plan_blocks_when` → 3 passed.
- `rtk cargo test -p ajax-core --lib` → 838 passed.
- `rtk cargo fmt --check` → passed.
- `rtk cargo clippy -p ajax-core --all-targets --all-features -- -D warnings` → no issues.
- `rtk cargo check -p ajax-cli -p ajax-web` → passed.
- `rtk cargo test -p ajax-cli start_plan_observation` → 1 passed.
- `rtk cargo test -p ajax-web start_task` → 12 passed.
