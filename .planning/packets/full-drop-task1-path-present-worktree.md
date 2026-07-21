# TDD Implementation Packet — drop observes worktree by path

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Make drop substrate observation mark the task worktree `Present` when
`git worktree list` contains the task’s `worktree_path`, even if that checkout
is on a different branch (or detached). Branch presence stays based on local
branch list / path entry branch name. Do not change repair’s stricter
path+branch intent matcher.

## Allowed files

- `crates/ajax-core/src/commands/teardown.rs`
- `crates/ajax-core/src/commands.rs` (tests module only — add/adjust observe_drop tests)
- `.planning/agent-plans/full-drop-and-orphan-gc.md` (checklist only)

## Forbidden changes

- Do not change `worktree_matches_task_intent` semantics in
  `analysis/git_evidence.rs` (repair/checkout-mismatch still path+branch).
- Do not change start collision, ship/merge, or force-drop policy (Tasks 2–7).
- Do not add dependencies or new abstraction layers.
- Do not weaken or delete existing tests (update expectations only if this
  behavior change requires it, and say why).
- Do not edit files outside Allowed files.
- Do not commit, push, merge, rebase, create branches, or change branches.

## Context evidence

- **Desired behavior:** Drifted checkout at the registered path must still be
  torn down on drop. Today observation uses
  `worktree_matches_task_intent(path AND branch)`, so a wrong branch →
  `worktree: Absent` → drop skips `EnsureWorktreeAbsent` → leftover dir.
- **Anchor (bug):** `observe_drop_resources_with_cache` in
  `commands/teardown.rs` ~621–625 finds worktree via intent matcher, then
  `state_from_bool(observed_worktree.is_some())`.
- **Fixture path:** `context_with_tasks` / cleanable task uses
  `worktree_path = /tmp/worktrees/web-fix-login`, branch `ajax/fix-login`,
  repo `/Users/matt/projects/web`.
- **Existing test pattern:**
  `observe_drop_resources_prefers_live_tmux_and_git_state_over_registry_cache`
  in `commands.rs` ~3591 — QueuedRunner with tmux list, worktree porcelain,
  branch list; asserts ResourceState.
- **Plan:** `.planning/agent-plans/full-drop-and-orphan-gc.md` Task 1.

## Code anchors

- `crates/ajax-core/src/commands/teardown.rs`:
  `observe_drop_resources_with_cache` — replace intent-based worktree presence
  with path equality against `task.worktree_path`. Reuse the path-matched entry
  for `branch_seen_in_worktree` (branch name on that entry equals `task.branch`).
  Remove unused `worktree_matches_task_intent` import if nothing else in the
  file needs it.
- `crates/ajax-core/src/commands.rs` tests: near
  `observe_drop_resources_prefers_live_tmux_and_git_state_over_registry_cache`.

## Test-first instructions

Add in `crates/ajax-core/src/commands.rs` tests module:

1. `observe_drop_resources_marks_worktree_present_when_path_matches_even_if_branch_differs`
   - `context_with_cleanable_task()`
   - QueuedRunner outputs:
     - tmux: `"ajax-web-fix-login\n"` (or empty if irrelevant — prefer present)
     - worktree porcelain including:
       `worktree /tmp/worktrees/web-fix-login\nHEAD 1111111\nbranch refs/heads/docs/other\n\n`
       (path matches task; branch differs)
     - branches: `"main\najax/fix-login\n"`
   - Assert `observation.worktree == ResourceState::Present`
   - Assert `observation.branch == ResourceState::Present` (branch still listed)
   - Assert updated `git_status.worktree_exists == true`

2. Keep/confirm existing
   `observe_drop_resources_prefers_live_tmux_and_git_state_over_registry_cache`
   still expects `worktree: Absent` when porcelain has only a different path
   (main repo path), not the task path.

RED command:

```bash
cargo test -p ajax-core observe_drop_resources_marks_worktree_present_when_path_matches_even_if_branch_differs -- --nocapture
```

Expect compile failure or assertion failure before production edit.

## Edit instructions

In `observe_drop_resources_with_cache`, after `parsed_worktrees`:

- Find entry where `Path::new(&worktree.path) == task.worktree_path.as_path()`
  (or equivalent exact path compare already used in the crate).
- Set `worktree` Present/Absent from that path match when list output is known.
- Set `branch_seen_in_worktree` from the path-matched entry’s branch vs
  `task.branch` (not from intent matcher).
- Do not alter tmux observation or branch-list logic beyond that.

Smallest diff only.

## Verification commands

```bash
cargo test -p ajax-core observe_drop_resources_marks_worktree_present_when_path_matches_even_if_branch_differs -- --nocapture
cargo test -p ajax-core observe_drop_resources_ -- --nocapture
cargo test -p ajax-core --lib
```

Mark Task 1 checklist items done in
`.planning/agent-plans/full-drop-and-orphan-gc.md` when green.

## Acceptance criteria

- Path-only match → worktree Present for drop observation.
- Different path → still Absent.
- Branch Present when listed locally even if worktree branch differs.
- `worktree_matches_task_intent` unchanged for repair.
- RED then GREEN proven in DELEGATE_REPORT.
- No commits/branch changes.

## Stop conditions

- Need to change git_evidence intent matcher or repair plans.
- Diff grows beyond ~80 lines or touches non-allowed files.
- Unrelated test failures that are not simple expectation updates from this
  observation change.
