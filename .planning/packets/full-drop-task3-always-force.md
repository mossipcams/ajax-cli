# TDD Implementation Packet — drop execution always force-tears

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Operator Drop execution (`execute_drop_task_operation`) always force-tears
substrate: fast/force worktree remove and `git branch -D`. Keep soft
`git branch -d` / non-force worktree remove only on the Clean/tidy planning
path (`clean_task_plan` → `native_cleanup_commands`).

## Allowed files

- `crates/ajax-core/src/task_operations/drop_task.rs`
- `crates/ajax-core/src/task_operations.rs` (tests only — update expectations
  that assumed unforced Drop execution)
- `crates/ajax-core/src/commands.rs` (tests only — characterize soft clean plan
  still uses `-d` when merged)
- `.planning/agent-plans/full-drop-and-orphan-gc.md` (checklist only)

## Forbidden changes

- Do not change `native_cleanup_commands` soft-delete semantics for tidy/Clean.
- Do not change ship/merge.
- Do not force-delete branches other than `task.branch`.
- Do not add dependencies or broad refactors.
- Do not weaken unrelated tests.
- Do not commit, push, merge, rebase, or change branches.
- Do not edit files outside Allowed files.

## Context evidence

- **Desired:** `ajax drop` must not leave unmerged `ajax/*` branches because
  soft `-d` failed or was skipped.
- **Execute path:** `execute_drop_task_operation` computes
  `force = drop_needs_force(...)` then `drop_op_execution_decision(..., force)`.
  Force → fast worktree remove + `branch -D`; unforced → plain remove + `-d`.
- **Clean path (keep soft):** `clean_task_plan` → `native_cleanup_commands` /
  `native_teardown_commands` with force=false for merged cleanable.
- **Existing tests:**
  - `drop_op_execution_decision` unit checks in `task_operations.rs` (~1400)
    still valid for the decision helper with explicit force bool.
  - `unforced_dirty_drop_keeps_plain_git_worktree_remove` expects plain remove
    under Drop execute — must flip to force/fast under new rule.
  - `confirmed_drop_renames_worktree_to_trash_*` already expects fast remove
    (Dirty → force).
- **Plan:** Task 3 in `.planning/agent-plans/full-drop-and-orphan-gc.md`.

## Code anchors

- `drop_task.rs`: `execute_drop_task_operation` — replace `drop_needs_force(...)`
  with always-true force for this operator path (or make `drop_needs_force`
  always `Ok(true)` and delete dead branches only if unused elsewhere).
  Prefer smallest: `let force = true;` at the call site and keep
  `drop_needs_force` if still used in tests; if unused, remove it.
- `commands/teardown.rs`: `native_cleanup_commands` — do not edit (soft path).

## Test-first instructions

1. Add `execute_drop_always_force_deletes_branch_with_D` in
   `task_operations.rs` tests:
   - Cleanable merged task, **not** Dirty (today’s unforced case).
   - `present_drop_observation_outputs` + three successful command outputs +
     `absent_drop_observation_outputs`.
   - `execute_drop_task_operation(..., confirmed=true, ...)`.
   - Assert a git command with `branch` and `-D` (not only `-d`) for
     `ajax/fix-login`.
   - Assert fast-remove `sh`/`ajax-fast-worktree-remove` **or**
     `worktree remove --force` (either force form is OK; prefer whatever
     `drop_op_execution_decision(..., true)` already emits).
   - Assert completion `Removed`.

2. Update `unforced_dirty_drop_keeps_plain_git_worktree_remove` to match always-
   force Drop (rename to reflect force; expect fast/`--force` + `-D`), **or**
   delete it if fully superseded by the new test (prefer update/rename over
   delete if it still adds coverage).

3. Add or extend a clean-plan characterization in `commands.rs` tests:
   `clean_task_plan_on_merged_still_uses_soft_branch_d` — merged cleanable,
   `clean_task_plan` contains `branch` + `-d` and does **not** require `-D`.
   (Skip if an existing property/test already asserts this — cite it in the
   report instead of duplicating.)

RED:

```bash
cargo test -p ajax-core execute_drop_always_force_deletes_branch_with_D -- --nocapture
```

## Edit instructions

In `execute_drop_task_operation`, always use force teardown for Drop ops.
Do not change Clean/tidy command builders. Update tests as above. Mark Task 3
checklist complete in the plan file.

## Verification commands

```bash
cargo test -p ajax-core execute_drop_always_force_deletes_branch_with_D -- --nocapture
cargo test -p ajax-core unforced_dirty_drop -- --nocapture
cargo test -p ajax-core clean_task_plan -- --nocapture
cargo test -p ajax-core --lib
```

## Acceptance criteria

- Drop execute always force-tears worktree + branch.
- Clean/tidy soft `-d` preserved for merged safe clean.
- RED then GREEN with command evidence.
- `ajax-core --lib` green.
- No commits/branch changes.

## Stop conditions

- Soft clean path would need changing to pass tests.
- Diff > ~120 lines or files outside Allowed.
- Unrelated failures not explained by force-drop expectation updates.
