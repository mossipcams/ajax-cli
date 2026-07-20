# TDD Implementation Packet — occupied worktree repair

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

When Git reports that a task's expected worktree path is occupied by another
branch, retain the occupying branch in refreshed evidence and block Repair with
a precise conflict instead of running `git worktree add` against an existing
path.

## Allowed files

- `crates/ajax-core/src/commands.rs`
- `crates/ajax-core/src/commands/task_window.rs`

## Forbidden changes

- Do not switch, remove, rename, or overwrite any worktree or branch.
- Do not change task intent, runtime-health variants, lifecycle semantics,
  public APIs, or unrelated repair behavior.
- Do not delete or weaken existing test assertions. The existing wrong-branch
  test may replace its stale `current_branch == None` assertion with the more
  precise observed occupying branch required by this behavior change.
- Do not add dependencies or abstractions.
- Do not edit files outside Allowed files.
- Do not commit, push, merge, rebase, create branches, or change branches.

## Context evidence

- **Desired behavior:** A wrong branch at the exact expected worktree path is a
  conflict, not a repairable absence; Repair must not issue a command known to
  fail or mutate the occupying worktree.
- **Observed failure:** SQLite task `ajax-cli/agent-launch-and-terminal-scrollbar`
  expects branch `ajax/agent-launch-and-terminal-scrollbar` at
  `/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-status-refactor`, while
  `git worktree list --porcelain` reports that path on branch
  `fix/pane-stuck-status-notifications`. Cached evidence says
  `git_worktree_exists=0`, `git_branch_exists=1`, and Repair would add at the
  occupied path.
- **Existing implementation:** `refresh_git_substrate_evidence` in
  `crates/ajax-core/src/commands.rs` finds worktrees only through
  `worktree_matches_task_intent`, so it discards the branch occupying the exact
  path and stores `current_branch = None`.
- **Existing repair path:** `task_window_repair_plan_with_open_mode` in
  `crates/ajax-core/src/commands/task_window.rs` sees missing worktree plus
  existing expected branch and unconditionally appends
  `GitAdapter::add_worktree_existing_branch`.
- **Existing test pattern:** tests near
  `refresh_git_substrate_evidence_rejects_other_branch_at_expected_path` in
  `crates/ajax-core/src/commands.rs` use `QueuedRunner`, porcelain worktree
  output, and `task_window_repair_plan`.
- **Architecture boundary:** Core owns substrate interpretation and repair
  planning; Git remains authoritative. Keep the planner pure and non-destructive.

## Code anchors

- `crates/ajax-core/src/commands.rs`:
  `refresh_git_substrate_evidence`, specifically `observed_worktree`,
  `worktree_exists`, and `current_branch`; test module beside
  `refresh_git_substrate_evidence_rejects_other_branch_at_expected_path`.
- `crates/ajax-core/src/commands/task_window.rs`:
  `task_window_repair_plan_with_open_mode`, before
  `add_worktree_existing_branch` is appended.

## Test-first instructions

Add
`repair_plan_blocks_when_expected_worktree_path_is_occupied_by_another_branch`
to the inline tests in `crates/ajax-core/src/commands.rs`.

Arrange a task expecting `ajax/fix-login` at
`/tmp/worktrees/web-fix-login`; have `QueuedRunner` return a porcelain worktree
entry at that exact path on `dependabot/pip/minor` plus branches containing
`ajax/fix-login`. Refresh Git substrate evidence, then build the repair plan.
Assert that refreshed `current_branch` is `dependabot/pip/minor`, the plan has
no commands, and its sole blocker identifies the occupied path and branch.

RED command:

```bash
rtk cargo test -p ajax-core repair_plan_blocks_when_expected_worktree_path_is_occupied_by_another_branch -- --nocapture
```

The first run must fail because current code drops the occupying branch and
emits `git worktree add`.

## Edit instructions

1. In `refresh_git_substrate_evidence`, separately find any parsed worktree at
   the exact expected path. Keep `worktree_exists` tied to the existing
   path-and-expected-branch match, but populate `current_branch` from the exact
   path occupant so a mismatch remains observable.
2. In `task_window_repair_plan_with_open_mode`, when `worktree_exists` is false
   and `current_branch` names a branch other than `task.branch`, return a
   blocked plan describing that the expected path is occupied by that branch.
3. Otherwise preserve the existing missing-branch and worktree recreation paths.
4. Update only the incompatible `current_branch == None` assertion in
   `refresh_git_substrate_evidence_rejects_other_branch_at_expected_path` to
   assert the observed occupying branch; keep all its other assertions.
5. Use concrete local logic only; no new helper or type unless compilation
   makes one unavoidable.

## Verification commands

```bash
rtk cargo test -p ajax-core repair_plan_blocks_when_expected_worktree_path_is_occupied_by_another_branch -- --nocapture
rtk cargo test -p ajax-core
rtk cargo fmt --check
rtk cargo clippy -p ajax-core --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- The focused test has captured RED before production edits and passes GREEN
  afterward.
- Wrong-branch occupancy preserves the occupying branch in `current_branch`.
- Repair emits no external commands for an occupied expected path and returns
  one precise blocker containing the path and occupying branch.
- Truly absent worktrees with an intact expected branch are still recreated.
- Changes stay within Allowed files and introduce no dependency or abstraction.

## Stop conditions

- The fix requires switching/removing a worktree, changing task intent, adding
  a runtime-health variant, or editing persistence schemas.
- An existing test exposes incompatible intended behavior beyond the named
  conflict.
- Any edit outside Allowed files is needed.
- The patch grows materially beyond the focused regression test and two small
  production guards.
- The focused RED does not fail for the expected missing-evidence/repair-plan
  reason.
