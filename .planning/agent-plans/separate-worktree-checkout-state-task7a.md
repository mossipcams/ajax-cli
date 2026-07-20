ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/commands.rs, crates/ajax-core/src/commands/task_window.rs]
  REASON: This is a two-file deletion plus focused regression discovered during the approved closeout scan; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, behavior requires a new state field, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Remove the last planner path that can call a task worktree "occupied" from an
old `current_branch` while the registered Git worktree path is absent. Physical
path presence and observed checkout are separate: if `worktree_exists` is
false, task-window Repair may use expected-branch existence to recreate the
worktree, but it must not interpret stale checkout evidence.

## Allowed files

- `crates/ajax-core/src/commands.rs`
- `crates/ajax-core/src/commands/task_window.rs`

## Forbidden changes

- Do not edit another file or any file under a `tests/` directory. The inline
  command test module is allowed.
- Do not change refresh, mismatch derivation, adoption, registry, lifecycle,
  task identity, tmux planning, or missing-branch behavior.
- Do not clear or rewrite `current_branch`; simply stop consulting it in the
  missing-path branch of task-window planning.
- Do not auto-adopt or auto-switch branches, add a fallback command, or inspect
  the filesystem directly.
- Do not add a dependency, helper, abstraction, compatibility shim, or unrelated
  cleanup. Do not delete or weaken assertions.

## Context evidence

- Correct Git refresh now sets `worktree_exists` from exact registered-path
  presence and `current_branch` only from that path. A different or detached
  checkout at a present path is mismatch, never missing.
- `repair_task_plan` handles present mismatch before composing
  `task_window_repair_plan_with_open_mode`.
- `task_window_repair_plan_with_open_mode` still contains an older nested branch:
  when `worktree_exists` is false and `current_branch` differs from task intent,
  it emits `expected worktree path ... is occupied by branch ...`. That combines
  mutually inconsistent/stale fields and recreates the original defect for
  direct/public callers or cached legacy evidence.
- The same function already has the correct missing-path behavior immediately
  afterward: block if the expected branch does not exist; otherwise plan
  `git worktree add <path> <expected-branch>`.

## Code anchors

- `crates/ajax-core/src/commands/task_window.rs` lines 29-51: the
  `if !git_status.worktree_exists` branch.
- `crates/ajax-core/src/commands.rs` tests around
  `task_window_repair_plan_recreates_missing_worktree_when_branch_exists` and
  `task_window_repair_plan_blocks_missing_worktree_when_branch_missing`.

## Test-first instructions

1. Before production edits add
   `task_window_repair_plan_ignores_stale_current_branch_when_worktree_is_missing`
   beside the existing missing-worktree tests. Set `worktree_exists: false`,
   `branch_exists: true`, and stale `current_branch: Some("fix/pane-stuck")`
   while task intent remains `ajax/fix-login`.
2. Assert blocked reasons are empty, no reason contains `occupied`, and the plan
   contains the exact existing-branch worktree-add command for
   `/tmp/worktrees/web-fix-login` and `ajax/fix-login`. Do not weaken this to
   merely checking that some command exists.
3. Run
   `cargo test -p ajax-core task_window_repair_plan_ignores_stale_current_branch_when_worktree_is_missing -- --nocapture`
   before production edits. It must exit 101 because the current plan contains
   the superseded occupied-path blocker and no worktree-add command.

## Edit instructions

1. Delete only the nested `current_branch`/different-branch occupied-path block
   from the `!git_status.worktree_exists` branch.
2. Leave the subsequent `!git_status.branch_exists` blocker and existing
   expected-branch worktree-add plan unchanged.
3. Do not add replacement logic: `current_branch` has no authority when the
   registered path is absent.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core task_window_repair_plan_ignores_stale_current_branch_when_worktree_is_missing -- --nocapture`
2. `cargo test -p ajax-core task_window_repair_plan_recreates_missing_worktree_when_branch_exists -- --nocapture`
3. `cargo test -p ajax-core task_window_repair_plan_blocks_missing_worktree_when_branch_missing -- --nocapture`
4. `cargo test -p ajax-core task_window_repair_plan -- --nocapture`
5. `cargo check -p ajax-core --all-targets`
6. `cargo fmt --check`
7. `git diff --check`
8. `cargo clippy -p ajax-core --all-targets -- -D warnings`

## Acceptance criteria

- Missing path plus an existing expected branch plans exact worktree recreation,
  regardless of stale `current_branch`.
- Missing path plus missing expected branch keeps the existing blocker.
- Present mismatch continues through the already-accepted typed adoption path;
  no mismatch or adoption code changes.
- The exact `occupied by branch` text is absent from production source.
- Only the two allowed files change and all eight commands pass.

## Stop conditions

- Stop if the focused RED unexpectedly passes, if another source file must
  change, or if deletion changes missing-branch/tmux behavior.
- Stop on unrelated baseline failures without changing unrelated code/tests.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or prose before or
  after it. Every command needs its own evidence item.

---DELEGATE_REPORT_START---
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [<allowed source paths>]
  TEST_FIRST: PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED
      COMMAND: <exact focused command>
      EXIT_CODE: <nonzero>
      OUTPUT_EXCERPT: <intended failure>
    - PHASE: GREEN
      COMMAND: <same focused command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
    - PHASE: VERIFY
      COMMAND: <remaining command; add one item per command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---

## Parent gate result

- Round 1 source accepted on 2026-07-20 after deterministic review showed only
  the two allowed files changed: one exact regression and deletion of the stale
  ten-line occupied-path branch. The adapter emitted
  `MISSING_STRUCTURED_REPORT`, so the parent used the complete raw evidence and
  its own validation.
- Parent commands 1-7 exited 0. Command 8,
  `cargo clippy -p ajax-core --all-targets -- -D warnings`, exited 101 on
  pre-existing cumulative Task 5 test code at `task_operations.rs:1098`
  (`clippy::type_complexity`), outside this packet. That warning is assigned to
  follow-up Task 7C and remains a blocking PR gate until fixed.
