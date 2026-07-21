# TDD Implementation Packet — ghost prune keeps substrate-bearing rows

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Do not ghost-prune a task from the registry when git evidence (or missing-
substrate flags that still imply a recoverable target) shows the worktree or
branch still exists. Specifically: `LifecycleStatus::Removed` must **Persist**
when `git_status.worktree_exists` or `git_status.branch_exists` is true, so a
row cannot vanish while local substrate remains. True ghosts (Removed/Stale/
abandoned with no recoverable git substrate) still prune.

Out of scope for this packet: changing orphan-recovery `delete_task` (note in
plan deviations as follow-up). Drop `complete_drop` already gates `delete_task`
on final observation all-Absent — do not rewrite it unless a test proves a gap.

## Allowed files

- `crates/ajax-core/src/ghost_task.rs`
- `crates/ajax-core/src/registry/sqlite.rs` (tests only, if an existing prune
  test must flip; prefer adding coverage in `ghost_task.rs` tests)
- `.planning/agent-plans/full-drop-and-orphan-gc.md` (checklist / deviations)

## Forbidden changes

- Do not change Cockpit visibility for true no-substrate ghosts.
- Do not edit `runtime_refresh.rs` orphan recovery in this packet.
- Do not change drop execution force policy (Task 3 done).
- Do not add dependencies.
- Do not commit, push, merge, rebase, or change branches.
- Do not edit files outside Allowed files.

## Context evidence

- **Bug class:** 35 leftover `ajax/*` branches with no registry row — prune or
  delete can detach the task while git remains.
- **Anchor:** `registry_persistence_disposition` in `ghost_task.rs`:
  `Removed` → always `Prune`. Stale already persists when
  `worktree_exists || branch_exists` or WorktreeMissing/BranchMissing flags.
- **Existing tests:** `removed_and_stale_tasks_are_registry_ghosts` expects
  Removed → prune even with TmuxMissing only (no git present flags) — keep.
- **Plan:** Task 4 (narrowed) in `full-drop-and-orphan-gc.md`.

## Code anchors

- `ghost_task.rs`: `registry_persistence_disposition` — before pruning Removed,
  if `git_status` says `worktree_exists || branch_exists`, return `Persist`.
  Optionally also Persist when only BranchMissing/WorktreeMissing flags are set
  **without** positive exists=false evidence — prefer the clear rule:
  **Persist Removed iff git_status reports worktree_exists OR branch_exists.**

## Test-first instructions

In `ghost_task.rs` tests:

1. `removed_task_with_existing_branch_is_not_a_registry_ghost`
   - lifecycle Removed
   - `git_status` with `branch_exists: true`, `worktree_exists: false`
   - Assert disposition Persist, `is_cockpit_visible_task` true,
     `is_registry_ghost_task` false

2. `removed_task_with_existing_worktree_is_not_a_registry_ghost`
   - Removed + `worktree_exists: true`, `branch_exists: false`
   - Same Persist/visible assertions

3. Keep `removed_and_stale_tasks_are_registry_ghosts` for Removed **without**
   git_status present bits (or with both exists false) still pruning.

RED:

```bash
cargo test -p ajax-core removed_task_with_existing_branch_is_not_a_registry_ghost -- --nocapture
```

## Edit instructions

Update `registry_persistence_disposition` Removed arm as above. Add tests.
Mark Task 4 checklist items that this packet covers; note orphan-recovery
follow-up under Deviations.

## Verification commands

```bash
cargo test -p ajax-core removed_task_with_existing_branch_is_not_a_registry_ghost -- --nocapture
cargo test -p ajax-core removed_task_with_existing_worktree_is_not_a_registry_ghost -- --nocapture
cargo test -p ajax-core ghost_task -- --nocapture
cargo test -p ajax-core --lib
```

## Acceptance criteria

- Removed + branch or worktree exists → Persist / visible.
- Removed with no present substrate → still Prune.
- Stale rules unchanged unless a test forces a tiny clarification.
- RED/GREEN proven; lib green.
- No commits.

## Stop conditions

- Need runtime_refresh orphan-recovery changes to pass.
- Diff > ~80 lines outside ghost_task tests.
- Visibility regressions for true ghosts.
