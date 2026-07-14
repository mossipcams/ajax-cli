# Plan: Repair missing worktree from CLI and Web Cockpit

## Scope

Make a successful `repair` immediately record a recreated task worktree as
present. The shared core task-operation reducer serves both the CLI and Web
Cockpit; neither surface will own separate recovery logic.

## Non-goals

- Recreating a missing branch.
- Changing worktree command planning (the existing plan already uses
  `git worktree add <path> <existing-branch>` without `-b`).
- UI changes, new dependencies, or test-directory edits.

## Approval

- Status: approved; delegation requested by user.

## Delegation decision

Delegation decision: delegated via model-router. Selected lane: OpenCode GLM
5.2, because this is a bounded Rust core reducer change with two surface-level
regressions.

## Task checklist

### Task 1: Execute and persist missing-worktree repair (10–15 min)

- [x] Test to write: added inline regression coverage in
  `crates/ajax-cli/src/lib/tests.rs` and
  `crates/ajax-web/src/slices/operate.rs` that begins with a missing worktree
  and existing branch, runs `repair`, asserts a `git worktree add` command was
  issued (via `commands().iter().any(...)` so CLI/Web refresh probes are
  tolerated), and asserts the task no longer reports its worktree missing.
- [x] Expected initial failure: the Web regression failed with
  `PlanBlocked(["task worktree is missing"])`. `repair_task_plan` appends a
  check plan generated from pre-repair evidence, so execution never reaches
  the already-planned `git worktree add`.
- [x] Implementation: added crate-visible
  `check_task_plan_after_worktree_recreate` in `commands/check.rs`, which
  retains every `TaskOperation::Check` blocker except the missing-worktree
  reason and otherwise mirrors `check_task_plan`. `repair_task_plan` uses it
  only when the task-window plan contains a `git worktree add` command
  (detected via the existing `is_git_worktree_add_command`). On success,
  `mark_task_window_repaired` flips an explicitly missing-worktree
  `GitStatus` to `worktree_exists = true` through
  `Registry::update_git_status`, which clears `SideFlag::WorktreeMissing`.
- [x] Verify: focused tests green; `cargo fmt --check`,
  `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  and full `-p ajax-core -p ajax-cli -p ajax-web` suites all pass.

## Validation ledger

- Pre-delegation: `ast-grep` located
  `commands::task_window::mark_task_window_repaired` at lines 101–131.
- Baseline worktree: only this plan is untracked.
- Parent validation (all passed):
  - `cargo test -p ajax-cli repair_execute_recreated_worktree_is_marked_present -- --nocapture`
  - `cargo test -p ajax-web operate_slice_repair_recreated_worktree_is_marked_present -- --nocapture`
  - `cargo test -p ajax-core repair -- --nocapture`
  - `cargo fmt --check`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run -p ajax-core -p ajax-cli -p ajax-web` (1,244 passed)

## Deviations

- Root cause expanded from stale post-repair state to a pre-execution blocker
  in `task_operations/task_command.rs` / `commands/check.rs`. This remains a
  shared core fix; no surface-local behavior is needed.
- Packet allowed-files list omitted `crates/ajax-core/src/commands.rs`. A
  one-line `pub(crate) use check::check_task_plan_after_worktree_recreate;`
  re-export was added so the entry point is reachable from
  `task_operations/task_command.rs`, since `mod check` is private. This is the
  mechanical exposure required by packet edit instruction #1 ("crate-visible
  ... entry point") and changes no behavior.
