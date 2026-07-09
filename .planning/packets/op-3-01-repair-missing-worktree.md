# TDD Implementation Packet — Op-3.01: Repair recreates missing worktree

## 1. Goal

When a registered task’s worktree is missing but its branch still exists,
`task_window_repair_plan` must plan a `git worktree add` that attaches the
**existing** branch at the registered path (then continue with normal tmux
repair). It must no longer return an empty command list with only a blocked
reason.

One behavior change only: missing-worktree + existing-branch → recreate
worktree in the repair plan.

## 2. Allowed files

Test files:

- `crates/ajax-core/src/commands.rs` (tests module only — add/update tests)

Production files:

- `crates/ajax-core/src/commands/task_window.rs`
- `crates/ajax-core/src/adapters/git.rs` (add attach-existing-branch helper only)
- `crates/ajax-core/src/adapters.rs` only if an existing GitAdapter property
  test must mention the new method (keep minimal)

Planning ledger (check off as you go):

- `.planning/agent-plans/op-recover-missing-worktree.md`
- `.planning/packets/op-3-01-repair-missing-worktree.md` (this file)

## 3. Forbidden changes

- Do not implement CI/conflict observation (vOp-1 Op-1/Op-2).
- Do not change drop/tidy/start/new_task flows except reusing
  `GitAdapter` / `is_git_worktree_add_command`.
- Do not use `GitAdapter::add_worktree` (that always passes `-b` and would
  fail when the branch already exists).
- Do not recreate missing branches (REC-05) — if `branch_exists == false`,
  keep a typed blocked reason and empty recreate commands.
- Do not change Web/TUI UI; core plan only.
- Do not weaken unrelated assertions.
- Do not add dependencies.
- Do not commit, push, or change branches.

## 4. Architecture context

- Task truth and repair planning live in `ajax-core` (`commands/task_window.rs`).
- Git substrate commands are built via `adapters::GitAdapter`.
- CLI/TUI/Web must consume the same repair plan — no surface-local recreate.
- `architecture.md`: substrate evidence and task operations stay in core;
  presentation adapters do not invent recovery.

## 5. Code anchors

Broken early-return (production):

```text
crates/ajax-core/src/commands/task_window.rs
  fn task_window_repair_plan_with_open_mode
  if task.git_status.is_some_and(|status| !status.worktree_exists) {
      plan.blocked_reasons.push(format!("task worktree is missing: {}", …));
      return Ok(plan);  // commands empty — REMOVE this empty return for
                        // the branch_exists == true case
  }
```

Existing create-new-branch helper (do **not** reuse for repair):

```text
crates/ajax-core/src/adapters/git.rs
  pub fn add_worktree(..., branch, start_point) -> CommandSpec
  // args include: worktree, add, -b, branch, path, start_point
```

Detector already accepts any `worktree add`:

```text
crates/ajax-core/src/commands/new_task.rs
  pub fn is_git_worktree_add_command(command: &CommandSpec) -> bool
```

Fixture:

```text
crates/ajax-core/src/commands.rs tests
  fn context_with_tasks()
  // repo path: /Users/matt/projects/web
  // worktree: /tmp/worktrees/web-fix-login
  // branch: ajax/fix-login
  // handle: web/fix-login
```

Property test that **currently encodes the bug** (must be updated after
implementation, not deleted):

```text
crates/ajax-core/src/commands.rs
  fn task_window_plan_repairs_generated_tmux_and_task_states
  if !worktree_exists {
      prop_assert!(plan.commands.is_empty());  // WRONG once fixed
      prop_assert_eq!(blocked_reasons, vec!["task worktree is missing: …"]);
  }
```

Nearby good pattern (tmux recreate still works when worktree exists):

```text
fn task_window_repair_plan_recreates_missing_tmux_session_with_task
```

ast-grep / search anchors:

```bash
rg -n "task worktree is missing" crates/ajax-core/src/commands/task_window.rs
rg -n "fn add_worktree" crates/ajax-core/src/adapters/git.rs
rg -n "task_window_plan_repairs_generated_tmux" crates/ajax-core/src/commands.rs
```

## 6. Test-first instructions

### 6a. Add focused unit test (must fail first)

In `crates/ajax-core/src/commands.rs` tests module, near other
`task_window_repair_plan_*` tests, add:

**Name:** `task_window_repair_plan_recreates_missing_worktree_when_branch_exists`

**Setup:**

1. `let mut context = context_with_tasks();`
2. Mutate task `task-1`:
   - `git_status = Some(GitStatus { worktree_exists: false, branch_exists: true, current_branch: Some("ajax/fix-login".into()), …other fields false/0/None as in sibling tests })`
   - leave tmux missing or present — either OK; assert worktree command first
3. `let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();`

**Assert:**

1. `plan.blocked_reasons` does **not** contain a reason that causes empty
   commands for this case (prefer `assert!(plan.blocked_reasons.is_empty())`
   if tmux path is also repairable).
2. `plan.commands.iter().any(|c| is_git_worktree_add_command(c))`
3. The worktree-add command args include:
   - `"-C"`, `"/Users/matt/projects/web"` (managed repo path from fixture)
   - `"worktree"`, `"add"`
   - worktree path `"/tmp/worktrees/web-fix-login"`
   - branch `"ajax/fix-login"`
4. Args must **not** include `"-b"` (existing branch attach).
5. After the worktree command, plan still includes tmux session/window repair
   and attach (same shape as
   `task_window_repair_plan_recreates_missing_tmux_session_with_task` when
   tmux is missing).

### 6b. Add negative unit test (may pass already; keep as lock)

**Name:** `task_window_repair_plan_blocks_missing_worktree_when_branch_missing`

Setup: `worktree_exists: false`, `branch_exists: false`.  
Assert: no `is_git_worktree_add_command`, and a blocked reason mentioning
worktree and/or branch missing (exact string: prefer
`"task worktree is missing: /tmp/worktrees/web-fix-login"` if branch
missing is folded into the same message, **or** a new clear
`"task branch is missing: ajax/fix-login"` — pick one string in
implementation and assert it exactly).

### 6c. Pre-impl verification (must fail)

```bash
cargo test -p ajax-core task_window_repair_plan_recreates_missing_worktree_when_branch_exists -- --nocapture
```

Expected: FAIL — either test missing, or plan has empty commands / only
blocked reason.

Do **not** edit production code until this failure is observed.

## 7. Production edit instructions

1. **`GitAdapter`** in `adapters/git.rs`: add a method, e.g.
   `add_worktree_existing_branch(repo_path, worktree_path, branch) -> CommandSpec`
   with args exactly:
   `["-C", repo_path, "worktree", "add", worktree_path, branch]`
   (no `-b`, no start_point). Add a small unit test next to existing
   `add_worktree` tests if present in `git.rs` / `adapters.rs`.

2. **`task_window_repair_plan_with_open_mode`** in `task_window.rs`:
   - Resolve managed repo path from `context.config.repos` by `task.repo`
     (same pattern as other commands that need repo root). If repo missing,
     keep/err as existing lookup patterns do.
   - Replace the blank early-return:
     - If `!worktree_exists && branch_exists` (treat missing `git_status` as
       unknown — only enter recreate when status explicitly says worktree
       missing **or** follow existing `is_some_and(!worktree_exists)` and
       require `branch_exists == true`):
       - `plan.commands.push(git.add_worktree_existing_branch(...))`
       - **do not return**; fall through to existing tmux repair logic
     - If `!worktree_exists && !branch_exists`: push blocked reason, return
       with no commands (preserve safe failure).
   - Do not run check_task_plan here (that stays in `repair_task_plan`).

3. **Update property test**
   `task_window_plan_repairs_generated_tmux_and_task_states`:
   when `!worktree_exists`, if the generated status keeps `branch_exists: true`
   (it already does), assert a worktree-add command is present and
   `blocked_reasons` is empty (or only non-fatal reasons). Remove the
   `prop_assert!(plan.commands.is_empty())` branch for that case.

## 8. Verification commands

```bash
# Pre-impl (expect FAIL)
cargo test -p ajax-core task_window_repair_plan_recreates_missing_worktree_when_branch_exists -- --nocapture

# Post-impl focused
cargo test -p ajax-core task_window_repair_plan_recreates_missing_worktree -- --nocapture
cargo test -p ajax-core task_window_plan_repairs_generated_tmux_and_task_states -- --nocapture
cargo test -p ajax-core task_window_repair_plan_ -- --nocapture

# Broader
cargo nextest run -p ajax-core
```

If nextest unavailable: `cargo test -p ajax-core`.

## 9. Acceptance criteria

- [x] New positive test failed before production edit
- [x] Positive test passes after edit
- [x] Negative branch-missing test passes
- [x] Property test updated and green
- [x] Repair plan for missing worktree + existing branch includes
      `git worktree add` **without** `-b`, then tmux repair commands
- [x] No unrelated test regressions in focused `task_window_repair_plan_*` set
- [x] Diff limited to allowed files

## 10. Stop conditions

Stop and ask the parent if:

- Repo path cannot be resolved from `context.config` for the fixture task
- You believe `add_worktree` with `-b` is required (it is not for this case)
- Property test failures outside the intended branch appear
- You need to edit `new_task.rs` start planning or drop/tidy
- Pre-impl test unexpectedly passes
- Required edits fall outside allowed files

## Delegation

`Delegation decision: delegated via model-router` after packet approval.

Suggested lane: **OpenCode GLM 5.2** or **Cursor Composer 2.5** (bounded Rust
core behavior). Parent reviews diff before accept.
