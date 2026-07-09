# Plan: Recover missing worktree (Op-3)

## Scope

Make `ajax repair` / `task_window_repair_plan` recreate a missing git worktree
when the task branch still exists, using core only.

## Non-goals

- Recreate missing branches (REC-05)
- CI/conflict status probes
- Web/TUI-only recovery UI
- Changing `GitAdapter::add_worktree` `-b` start semantics

## Approval

- Status: planning complete; implementation authorized by user
  (“plan both … using tdd imp packet”)
- Packet: `.planning/packets/op-3-01-repair-missing-worktree.md`

## Delegation decision

`Delegation decision: delegated via model-router` (not implemented in this
planning turn).

## Task checklist

### Task 1: Op-3.01 — repair recreates missing worktree when branch exists

- [x] Test to write: `task_window_repair_plan_recreates_missing_worktree_when_branch_exists`
      (+ branch-missing negative) in `crates/ajax-core/src/commands.rs`
- [x] Code to implement: `GitAdapter::add_worktree_existing_branch`; fix
      early-return in `task_window.rs`; update property test
- [x] Verify: commands in packet §8
- [x] Packet path: `.planning/packets/op-3-01-repair-missing-worktree.md`

## Validation ledger

- Planning: inspected `task_window.rs` early-return, `GitAdapter::add_worktree`
  (`-b`), property test expecting empty commands, `context_with_tasks` fixture
- Implementation: complete (Cursor CLI worker)
  - [x] Pre-impl positive test FAIL observed
  - [x] Production edit
  - [x] Post-impl verification
    - `cargo test -p ajax-core task_window_repair_plan_recreates_missing_worktree` PASS
    - `cargo test -p ajax-core task_window_plan_repairs_generated_tmux_and_task_states` PASS
    - `cargo test -p ajax-core task_window_repair_plan_` PASS (7)
    - `cargo nextest run -p ajax-core` PASS (756)

## Deviations

- None
