# Plan: Full drop + orphan GC (no leftover names)

## Scope

Make Ajax drop always tear down a task’s recorded substrate, keep the registry
row until that substrate is gone, and add a bounded orphan sweeper for leftovers
already detached from SQLite.

## Non-goals

- Deleting arbitrary non-Ajax worktrees by default (foreign `fix/*` / `feat/*`
  checkouts stay opt-in under an explicit orphan mode).
- Changing start placement, slugify, or ship/merge semantics.
- Auto-deleting remote `origin/ajax/*` branches (local only unless a later task
  explicitly opts into remote prune).
- Broad refactors of drop into a new framework.

## Approval

- Status: **approved for implementation** (user: “delegate until finished”)
- Architecture impact: yes (drop observation matching, remove eligibility,
  ghost prune guard, tidy orphan GC). Update `architecture.md` drop/tidy
  paragraphs in the same change set that lands those behaviors.

## Delegation decision

`Delegation decision: delegated via model-router`

One bounded packet per task (or tightly paired 1+2 / 6+7 when anchors fit).
Parent reviews every diff and runs verification personally.

---

## Problem → fix map

| # | Failure mode | Fix |
|---|---|---|
| A | Drop treats worktree Absent unless path **and** branch match | Observe worktree Present if path is registered in `git worktree list` (path-only for drop) |
| B | Checkout mismatch blocks `Remove`, so drifted tasks can’t drop | Allow `TaskOperation::Remove` despite checkout mismatch; keep Merge/Clean blocked |
| C | Soft `git branch -d` on clean path leaves unmerged `ajax/*` branches | Operator Drop / force remove always uses `-D` + force/fast worktree remove; keep soft `-d` only for post-merge tidy `Clean` |
| D | Registry row can vanish while path/branch remain (ghost prune / false Absent) | Never prune/delete task while live observation shows path or `ajax/<handle>` branch present; final drop re-observe is authoritative |
| E | `TeardownIncomplete` can be easy to ignore | Keep visible; Drop retries remaining ops; Cockpit/CLI copy states remaining resources |
| F | Leftovers already off-registry (`ajax/*` branches, `ajax-*` dirs) | New orphan GC under `tidy` (or `tidy --orphans`) |
| G | Foreign sibling worktrees (`fix/*`, manual `main`, etc.) | Same sweeper, **opt-in** flag; never default-delete |

---

## Task checklist

### Task 1 — Drop observation: path-present worktree (fix A)

- [x] **Test:** `observe_drop_resources_marks_worktree_present_when_path_matches_even_if_branch_differs`
- [x] **Test:** branch observation still Present when local branch exists
- [x] **Impl:** In `commands/teardown.rs` drop observation, treat worktree Present
  if any `git worktree list` entry path equals `task.worktree_path` (do not
  require `worktree_matches_task_intent` for **drop** presence). Keep the
  stricter matcher for repair/checkout-mismatch UX elsewhere.
- [x] **Verify:** `cargo test -p ajax-core observe_drop` / focused teardown tests
- **Files:** `crates/ajax-core/src/commands/teardown.rs`, tests in same module /
  `commands.rs`
- **Must not change:** repair occupied-path behavior, start collision checks

### Task 2 — Remove allowed on checkout mismatch (fix B)

- [x] **Test:** `remove_eligibility_allows_checkout_mismatch` (via
  `branch_sensitive_checkout_mismatch_operations_are_blocked_with_details`)
- [x] **Test:** `clean_and_merge_still_blocked_on_checkout_mismatch`
- [x] **Impl:** In `operation.rs`, stop applying checkout-mismatch block to
  `TaskOperation::Remove` (only Merge + Clean).
- [x] **Verify:** `cargo test -p ajax-core branch_sensitive_checkout_mismatch`
- **Files:** `crates/ajax-core/src/operation.rs`
- **Must not change:** Merge/Clean safety policy

### Task 3 — Operator Drop always force-tears substrate (fix C)

- [x] **Test:** `execute_drop_always_force_deletes_branch_with_D`
- [x] **Test:** `clean_task_plan_on_merged_still_uses_soft_branch_d` (covered by existing
  `clean_plan_uses_policy_and_native_cleanup` in `commands.rs`)
- [x] **Impl:** Drop execution always uses `force = true`; removed `drop_needs_force`.
- [x] **Verify:** focused drop_task tests in `task_operations.rs`
- **Files:** `drop_task.rs`, possibly `teardown.rs` native command builders
- **Must not change:** ship/merge; don’t force-delete unrelated branches

### Task 4 — No registry delete / ghost prune while substrate remains (fix D)

- [x] **Test:** `removed_task_with_existing_branch_is_not_a_registry_ghost`
- [x] **Test:** `removed_task_with_existing_worktree_is_not_a_registry_ghost`
- [x] **Test:** `removed_and_stale_tasks_are_registry_ghosts` (no substrate → prune)
- [x] **Impl:** Persist `Removed` when `git_status` reports worktree or branch exists
- [ ] **Deferred:** orphan-recovery `delete_task` without drop teardown of stale branch
- [x] **Verify:** `cargo test -p ajax-core ghost_task`
- **Files:** `ghost_task.rs`
- **Must not change:** true no-substrate ghost visibility

### Task 5 — TeardownIncomplete always actionable (fix E)

- [x] **Characterization:** existing tests + `format_drop_teardown_incomplete_message`
  retry hint; CLI/web `resuming_incomplete`; Drop always force (Task 3)
- [x] **Impl:** none required
- [x] **Verify:** `cargo test -p ajax-core teardown_incomplete`
- **Files:** none
- **Must not change:** success path for fully Removed

### Task 6 — Orphan GC for Ajax-shaped leftovers (fix F)


- [x] **Test:** planner lists orphan `ajax/*` branches not claimed by any task
- [x] **Test:** planner lists orphan worktree paths under legacy sibling /
  worktree root matching Ajax naming not in registry
- [x] **Test:** execute deletes branch with `-D` and removes worktree (force),
  requires `--yes` / confirmation
- [x] **Test:** never touches paths/branches owned by live registry tasks
- [x] **Impl:** Extend `tidy` with orphan discovery (default on for **ajax-shaped
  only**), or `ajax tidy --orphans`. Reuse trash sweep. Plan-first, execute with
  confirmation.
- [x] **Verify:** `cargo test -p ajax-core` tidy/sweep + `cargo test -p ajax-cli`
  tidy smoke if present
- **Files:** `commands/teardown.rs` or new `commands/orphan_gc.rs`,
  `task_operations/sweep_cleanup.rs`, CLI `tidy` flags, short `architecture.md`
  + README/CONTRIBUTING note
- **Must not change:** default deletion of non-`ajax/` branches

### Task 7 — Opt-in foreign orphan mode (fix G)

- [x] **Test:** `classify_orphans_skips_foreign_sibling_worktree` (ajax mode)
- [x] **Test:** `classify_orphans_all_includes_foreign_sibling_worktree`
- [x] **Test:** `classify_orphans_all_skips_main_worktree`
- [x] **Impl:** `OrphanGcMode::All` + CLI `--orphans=all`
- [x] **Verify:** focused classify tests + `cargo check -p ajax-cli`
- **Files:** orphan_gc.rs, cli, dispatch, architecture.md
- **Must not change:** default `--orphans` / `--orphans=ajax` behavior

### Task 8 — Docs + one-shot local cleanup (ops, not product)

- [x] Update `architecture.md` drop/tidy orphan paragraph
- [x] Record validation commands/results in this plan
- [x] Dry-run: `./target/debug/ajax-cli --profile stable tidy --orphans`
  (lists ajax-shaped GC commands; requires confirmation)
- [ ] **Operator execute when ready:**
  `ajax-cli --profile stable tidy --orphans --execute --yes`
  (add `=all` only for foreign sibling worktrees). FS-only dirs like
  `ajax-xterm-implementation` (no git worktree entry) still need manual `rm`.


---

## Suggested implementation order

1. Task 1 + 2 (observation + eligibility) — unblocks correct drop planning  
2. Task 3 + 4 (force teardown + no premature registry delete) — prevents new leftovers  
3. Task 5 (incomplete UX) — small, can ship with 3/4  
4. Task 6 + 7 (orphan GC) — cleans history and future detachments  
5. Task 8 (docs + local sweep)

Each of 1–4 and 6 is one delegation packet after approval.

## Risks

- Force `-D` destroys unmerged local work — acceptable for explicit Drop; must
  not leak into tidy Clean of merged tasks incorrectly.
- Path-only worktree Present might remove a dir another tool is using at the
  same path — path is Ajax-owned by contract for registered tasks.
- Orphan GC false positives if someone manually creates `ajax/foo` for non-Ajax
  work — mitigate by requiring confirmation and listing plan first.
- Orphan recovery + Task 4 interaction: must not strand two tasks on one path.

## Validation strategy

Per task: failing test → impl → focused green.

Broader before PR:

```bash
cargo fmt --check
cargo check -p ajax-core -p ajax-cli -p ajax-web --all-features
cargo nextest run -p ajax-core
cargo nextest run -p ajax-cli --test smoke_user_flows
```

Full `npm run verify` / husky gate before any PR.

## Deviations

- Task 1: GLM (`opencode-go/glm-5.2`) hit weekly usage limit → escalated to
  Cursor `composer-2.5`. Delegate report schema invalid but diff correct.
- Task 1 gate: parent fixed
  `confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline` final
  observation fixture to use `absent_drop_observation_outputs()` (path-only
  leftover porcelain was incorrectly treated as Absent before Task 1).
- Task 2: `Delegation decision: not delegated because R-LOCAL-TINY` (one-file
  eligibility flip + existing test update).
- Task 4: orphan-recovery `delete_task` without drop teardown deferred.
- Task 5: no code change (already covered).
- Task 6–7: Cursor delegate + parent Task 7 All-mode. Cursor report YAML often
  fails schema (fenced block) but diffs verified by parent.
- Task 7: `Delegation decision: not delegated because R-LOCAL-TINY` extension of
  existing orphan_gc module.
- Validation (parent): `cargo test -p ajax-core --lib` → 825 passed;
  `cargo check -p ajax-cli` ok; dry-run
  `ajax-cli --profile stable tidy --orphans` lists GC commands (confirmation
  required; not executed).

## Current machine leftovers (for Task 8 execute)

- Live tasks (do not GC): see `ajax-cli --profile stable tasks`
- Dry-run shows many `ajax/*` branch deletes + some ajax-* worktree removes
  (and autosnooze orphans). FS-only dirs without git worktree entries are
  not covered — remove manually if needed.
- Foreign siblings: only with `--orphans=all`
