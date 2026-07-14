# TDD Implementation Packet: Persist recreated worktree state

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

After a successful `repair` has recreated a known-missing worktree on its
existing branch, persist the Git evidence as present. The shared core reducer
must clear the missing-worktree state so both CLI and Web Cockpit immediately
show a recovered task.

## 3. Allowed files

- `crates/ajax-core/src/commands/task_window.rs`
- `crates/ajax-core/src/commands/check.rs`
- `crates/ajax-core/src/commands.rs` (crate-visible re-export only)
- `crates/ajax-core/src/task_operations/task_command.rs`
- `crates/ajax-cli/src/lib/tests.rs`
- `crates/ajax-web/src/slices/operate.rs`
- `.planning/agent-plans/repair-missing-worktree-surfaces.md`

## 4. Forbidden changes

- Do not change worktree planning, branch recreation, UI code, public command
  syntax, dependencies, generated assets, or files under `tests/`.
- Do not add a surface-specific recovery path; both surfaces use the shared
  core task-operation reducer.
- Do not commit, push, merge, rebase, create worktrees, or change branches.
- No refactors beyond the smallest reducer update and focused inline tests.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — this one-function reducer change stays within the
  documented core task-operation boundary; `architecture.md` lines 133–135 and
  396–403 establish that CLI/Web are adapters over core command operations.
- Serena: `NOT_REQUIRED` — no Serena service is available, and direct source
  tracing covers the single symbol and both callers: CLI
  `dispatch::render_task_command` and Web `slices::operate::execute_task_command`.
- ast-grep: ran
  `ast-grep --pattern 'pub fn mark_task_window_repaired<$T>($$$ARGS) -> $RET { $$$BODY }' --lang rust crates/ajax-core/src/commands/task_window.rs`;
  it found `mark_task_window_repaired` at lines 101–131.
- Red-test evidence: the Web test failed with
  `Command(PlanBlocked(["task worktree is missing"]), true)`. Repair planning
  correctly calls `GitAdapter::add_worktree_existing_branch`, but then appends
  `check_task_plan`'s pre-repair missing-worktree blocker, so external-plan
  execution never begins.

## 6. Code anchors

- Production: `crates/ajax-core/src/commands/task_window.rs`,
  `mark_task_window_repaired`, immediately after the cloned task and before
  tmux/task-window status updates.
- Planning: `crates/ajax-core/src/task_operations/task_command.rs`,
  `repair_task_plan` lines 186–200, which appends `check_task_plan`.
- Check eligibility: `crates/ajax-core/src/commands/check.rs`,
  `check_task_plan` lines 10–39; it returns with the missing-worktree reason
  before adding the configured test command.
- CLI regression area: `crates/ajax-cli/src/lib/tests.rs`, beside
  `repair_execute_clears_missing_tmux_and_task_flags`.
- Web regression area: `crates/ajax-web/src/slices/operate.rs` test module,
  beside `operate_slice_delegates_resume_to_core_operation_without_attach`.
- Reuse: `Task::apply_git_status` via `Registry::update_git_status`; it clears
  `SideFlag::WorktreeMissing` when `worktree_exists` is true.

## 7. Test-first instructions

1. Add one CLI inline test and one Web inline test, each beginning with a task
   whose `git_status` has `worktree_exists: false`, `branch_exists: true`, and
   `current_branch: Some("ajax/fix-login")`.
2. Invoke the respective repair entry point with `RecordingCommandRunner`:
   CLI `ajax repair web/fix-login --execute`; Web `operate` with
   `OperateRequest { action: "repair" }`.
3. Assert the recorded commands include `git worktree add` for the registered
   worktree path and existing branch (no `-b`), then assert
   `task.git_status.worktree_exists` and the absence of
   `SideFlag::WorktreeMissing`.
4. Run red commands before production edits:

```bash
cargo test -p ajax-cli repair_execute_recreated_worktree_is_marked_present -- --nocapture
cargo test -p ajax-web operate_slice_repair_recreated_worktree_is_marked_present -- --nocapture
```

Observed red reason: repair is blocked by `task worktree is missing` after
planning the worktree-add command. Keep the status assertions: they verify the
success reducer once the operation can run.

## 8. Edit instructions

1. Give `check_task_plan` a crate-visible, narrowly named entry point for a
   worktree that this same repair plan will recreate. It must retain every
   eligibility blocker except exactly `"task worktree is missing"`, then add
   the configured check command as the normal check planner does. The existing
   public `check_task_plan` must retain its current blocking behavior.
2. In `repair_task_plan`, use that entry point only when the existing
   task-window repair plan contains a `git worktree add` command. Otherwise
   retain the existing `check_task_plan` call and behavior.
3. In `mark_task_window_repaired`, if the cloned task has a `GitStatus` with
   `worktree_exists == false`, change only that status to `worktree_exists = true`
   and send it through `context.registry.update_git_status`. Do not manufacture
   status when Git evidence is absent and do not change branch/Git metadata.
   Preserve existing tmux, task-window, and live-status behavior.
4. Fix the two added tests to assert that the command list *contains* the
   worktree-add spec rather than asserting it is the first command: CLI/Web
   refresh evidence may run Git probes first.

## 9. Verification commands

```bash
cargo test -p ajax-cli repair_execute_recreated_worktree_is_marked_present -- --nocapture
cargo test -p ajax-web operate_slice_repair_recreated_worktree_is_marked_present -- --nocapture
cargo fmt --check
cargo check --all-targets --all-features
```

## 10. Acceptance criteria

- Both named tests fail before production code and pass afterward.
- Both entry points execute the shared repair plan containing existing-branch
  `git worktree add`.
- After successful repair, the registry has `worktree_exists == true` and no
  `WorktreeMissing` side flag.
- No branch recreation or UI-local repair logic is added.
- Diff is limited to the allowed files.

## 11. Stop conditions

- The red tests already pass before production edits.
- A repair without explicit missing Git evidence would require fabricated
  status.
- The implementation needs changes outside allowed files.
- Any focused test fails for an unrelated pre-existing reason.
