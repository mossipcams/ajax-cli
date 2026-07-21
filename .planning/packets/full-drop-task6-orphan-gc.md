# TDD Implementation Packet — ajax-shaped orphan GC (tidy --orphans)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Add orphan garbage-collection for **Ajax-shaped** leftovers not claimed by any
registry task:

1. Local branches named `ajax/*` not in any task’s `branch`
2. Git worktrees whose path is under a managed repo’s legacy sibling
   `*__worktrees/` directory and basename starts with `ajax-`, or under
   `WorktreePlacement::Root`, and path is not any task’s `worktree_path`

Wire as `ajax tidy --orphans` (requires `--execute --yes` to run). Default tidy
without `--orphans` unchanged. Foreign non-`ajax-` sibling dirs are **excluded**
(Task 7 can add `--orphans=all` later — leave a clear extension point / enum).

## Allowed files

- `crates/ajax-core/src/commands/orphan_gc.rs` (new)
- `crates/ajax-core/src/commands.rs` (mod export + re-exports if needed)
- `crates/ajax-core/src/commands/teardown.rs` (only if needed to share helpers;
  prefer keeping orphan logic in `orphan_gc.rs`)
- `crates/ajax-core/src/task_operations/sweep_cleanup.rs`
- `crates/ajax-cli/src/cli.rs`
- `crates/ajax-cli/src/execution_dispatch.rs`
- `crates/ajax-cli/src/snapshot_dispatch.rs` (readonly plan preview if tidy plan
  is shown)
- `architecture.md` (short drop/tidy orphan paragraph)
- `.planning/agent-plans/full-drop-and-orphan-gc.md` (checklist)

## Forbidden changes

- Do not delete non-`ajax/` branches or non-`ajax-` worktree basenames in
  default `--orphans` mode.
- Do not change Drop force policy or ghost prune (Tasks 1–4 done).
- Do not auto-run orphan GC without `--orphans` and confirmation/`--yes`.
- Do not add dependencies.
- Do not commit, push, merge, rebase, or change branches.
- Do not edit files outside Allowed files.

## Context evidence

- **Need:** ~35 `ajax/*` branch-only leftovers + ajax-shaped orphan worktrees
  with no registry row; Drop cannot target them.
- **Tidy today:** `sweep_cleanup_plan` / `execute_sweep_cleanup_operation` only
  clean **registered** Safe tasks + trash sweep. CLI:
  `cli.rs` tidy subcommand; `execution_dispatch.rs` tidy arm.
- **Git helpers:** `GitAdapter::list_worktrees`, `list_branches`,
  `force_remove_worktree`, `force_delete_branch`; parse helpers exist.
- **Placement:** `WorktreePlacement` / `legacy_worktrees_root` patterns in
  config / new_task — reuse existing path helpers if present; otherwise
  `repo.path.parent()/"{repo_name}__worktrees"` for legacy sibling.
- **Plan:** Tasks 6 (+ extension point for 7) in
  `.planning/agent-plans/full-drop-and-orphan-gc.md`.

## Code anchors

- New `commands/orphan_gc.rs`:
  - `OrphanGcMode { AjaxShaped }` (add `All` variant stub or `#[non_exhaustive]`
    for Task 7 without implementing foreign yet)
  - Pure `classify_orphans(claimed_paths, claimed_branches, worktrees,
    branches, mode, repo_path) -> Vec<OrphanGcTarget>`
  - `orphan_gc_commands(repo_path, targets) -> Vec<CommandSpec>` using force
    worktree remove then `-D` branch (order: worktree first if both)
  - `plan_orphan_gc_for_repo` / `collect_orphan_gc_commands` using runner to
    list worktrees/branches per configured repo
- `sweep_cleanup.rs`: when orphans flag true, after candidate drops (or before),
  collect orphan commands; require `confirmed`; run them; fail on nonzero
- `cli.rs`: `.arg(Arg::new("orphans").long("orphans").action(ArgAction::SetTrue)
  .help("Also force-remove unregistered ajax/* branches and ajax-* worktrees"))`
- `execution_dispatch.rs`: pass orphans flag into execute/plan

## Test-first instructions

In `orphan_gc.rs` tests (or commands.rs if module tests live there):

1. `classify_orphans_lists_ajax_branch_not_claimed_by_registry`
   - claimed empty; branches include `ajax/hotbar` and `main`
   - mode AjaxShaped → target for `ajax/hotbar` only

2. `classify_orphans_lists_ajax_worktree_path_not_claimed`
   - worktree path `/repo/web__worktrees/ajax-xterm-implementation` with any
     branch; claimed empty → worktree target
   - claimed contains that path → no target

3. `classify_orphans_skips_foreign_sibling_worktree`
   - path `/repo/web__worktrees/fix-web-cf-shell-cache` → **not** classified
     under AjaxShaped

4. `orphan_gc_commands_force_remove_worktree_then_delete_branch_D`
   - Assert command args contain `worktree remove --force` (or force remove
     helper) and `branch -D`

5. Optional integration: `execute_sweep_cleanup_runs_orphan_commands_when_flag_set`
   with QueuedRunner — only if wiring is small; otherwise unit-level is enough
   for this packet and CLI smoke can wait.

RED:

```bash
cargo test -p ajax-core classify_orphans_lists_ajax_branch_not_claimed_by_registry -- --nocapture
```

## Edit instructions

Implement classify + commands + tidy `--orphans` wiring as above. Mark Task 6
checklist done; note Task 7 still open (`All` mode). Update `architecture.md`
one short paragraph under tidy/drop.

## Verification commands

```bash
cargo test -p ajax-core classify_orphans -- --nocapture
cargo test -p ajax-core orphan_gc -- --nocapture
cargo test -p ajax-core --lib
cargo test -p ajax-cli tidy -- --nocapture
```

## Acceptance criteria

- Ajax-shaped orphans classified; foreign sibling dirs skipped.
- Force remove + `-D` commands generated; never touch claimed registry paths/branches.
- `tidy` without `--orphans` unchanged.
- `tidy --orphans` requires confirmation/`--yes` to execute.
- RED/GREEN + lib green.
- No commits.

## Stop conditions

- Implementing full foreign `All` mode in this packet (defer to Task 7).
- Diff >> ~250 lines without clear structure.
- Need new crate or dependency.
