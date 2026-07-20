PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Record a registered Git worktree path as physically present regardless of its
checked-out branch, including detached HEAD, while keeping expected-branch
availability and the observed current branch as separate facts.

## Allowed files

- `crates/ajax-core/src/adapters/git.rs`
- `crates/ajax-core/src/commands.rs`

## Forbidden changes

- Do not edit any other file.
- Do not add a new type, field, database column, dependency, or abstraction.
- Do not switch, create, rename, adopt, or delete a Git branch or worktree.
- Do not change task identity, path, tmux naming, lifecycle, or runtime-health
  semantics in this task.
- Do not delete or weaken existing safety assertions. Assertions whose old
  path-plus-branch meaning is superseded must be replaced with equally specific
  assertions for physical presence, expected-branch availability, observed
  checkout, and absence of `git worktree add`.
- Do not perform unrelated cleanup or formatting churn.

## Context evidence

- Desired behavior: the approved plan defines `GitStatus.worktree_exists` as
  exact registered-path presence, `branch_exists` as independent expected-branch
  availability, and `current_branch` as the checkout at that path. Anchor:
  `.planning/agent-plans/separate-worktree-checkout-state.md`, Locked behavior.
- Parser defect: `parse_worktree_entry` currently returns `path.zip(branch)`, so
  a porcelain entry containing `detached` has a path but is discarded. Anchor:
  `crates/ajax-core/src/adapters/git.rs`, `parse_worktree_entry`.
- Refresh defect: `refresh_git_substrate_evidence` finds both
  `observed_worktree` by path+expected branch and `path_worktree` by exact path,
  but assigns `worktree_exists = observed_worktree.is_some()`. Anchor:
  `crates/ajax-core/src/commands.rs`, the per-task loop in
  `refresh_git_substrate_evidence`.
- Existing pattern: `Task::apply_git_status` clears `SideFlag::WorktreeMissing`
  whenever `status.worktree_exists` is true. Anchor:
  `crates/ajax-core/src/models.rs`, `Task::apply_git_status` (read-only context;
  not an allowed file).
- Existing regression anchors: the adapter fixture already contains a detached
  worktree but expects only two entries; the refresh fixture already observes
  `dependabot/pip/minor` at the exact task path but calls it missing; the repair
  fixture checks that no `git worktree add` command is emitted. Anchors:
  `crates/ajax-core/src/adapters/git.rs::tests::parse_worktrees_keeps_paths_and_branches`
  and the adjacent refresh/repair tests in `crates/ajax-core/src/commands.rs`.

## Code anchors

- `crates/ajax-core/src/adapters/git.rs`: `parse_worktree_entry` and
  `tests::parse_worktrees_keeps_paths_and_branches`.
- `crates/ajax-core/src/commands.rs`: imports from
  `analysis::git_evidence`, the per-task evidence reduction inside
  `refresh_git_substrate_evidence`,
  `refresh_git_substrate_evidence_rejects_other_branch_at_expected_path`, and
  `repair_plan_blocks_when_expected_worktree_path_is_occupied_by_another_branch`.

## Test-first instructions

Before editing production logic:

1. Update `parse_worktrees_keeps_paths_and_branches` to expect all three fixture
   entries and assert the third path is `/repos/ajax-cli__worktrees/manual` with
   `branch == None`. Run:
   `cargo test -p ajax-core parse_worktrees_keeps_paths_and_branches -- --nocapture`
   It must fail because the detached entry is currently discarded.
2. Rename the other-branch refresh test to
   `refresh_git_substrate_evidence_treats_other_branch_at_expected_path_as_present`.
   Preserve its branch/current-branch assertions, but assert
   `worktree_exists == true` and no `SideFlag::WorktreeMissing`. Run:
   `cargo test -p ajax-core refresh_git_substrate_evidence_treats_other_branch_at_expected_path_as_present -- --nocapture`
   It must fail on the old false presence result.
3. Rename the adjacent repair test to
   `repair_plan_does_not_add_worktree_when_expected_path_is_on_another_branch`.
   Preserve the observed-current-branch assertion; assert the plan has no
   worktree-occupancy blocker and still assert that no `git worktree add`
   command is present. Run:
   `cargo test -p ajax-core repair_plan_does_not_add_worktree_when_expected_path_is_on_another_branch -- --nocapture`
   It must fail against the old path-plus-branch presence semantics.

Capture the nonzero exit and intended assertion failure for each focused red
command before any production edit.

## Edit instructions

1. In `parse_worktree_entry`, retain every entry with a `worktree` path. Store
   its named local branch when present and `None` for detached entries. The
   minimal implementation should replace the path+branch zip; do not add a
   detached flag or another parser pass.
2. In `refresh_git_substrate_evidence`, derive `worktree_exists` solely from
   `path_worktree.is_some()`, derive `current_branch` from that same exact-path
   entry, and derive `branch_exists` solely from whether the expected task
   branch appears in the separately parsed branch set.
3. Remove the now-unused `worktree_matches_task_intent` import from
   `commands.rs` if the compiler reports it unused. Do not change or delete the
   helper in `analysis::git_evidence`; teardown still uses it.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core parse_worktrees_keeps_paths_and_branches -- --nocapture`
2. `cargo test -p ajax-core refresh_git_substrate_evidence_treats_other_branch_at_expected_path_as_present -- --nocapture`
3. `cargo test -p ajax-core repair_plan_does_not_add_worktree_when_expected_path_is_on_another_branch -- --nocapture`
4. `cargo test -p ajax-core refresh_git_substrate_evidence -- --nocapture`
5. `cargo fmt --check`

## Acceptance criteria

- Porcelain worktree parsing retains named-branch and detached entries by path.
- A different named branch at the exact task worktree path yields
  `worktree_exists == true`, `branch_exists == true` when the expected branch is
  listed, and `current_branch` equal to the observed other branch.
- That observation clears `SideFlag::WorktreeMissing` and does not claim the
  observed branch equals task intent.
- The repair/open plan does not emit `git worktree add` for an occupied path and
  does not retain the superseded “expected path is occupied” blocker.
- All focused and grouped verification commands pass with no out-of-scope diff.

## Stop conditions

- Stop if either named production anchor has materially changed.
- Stop if satisfying the behavior requires editing a third file, changing a
  public model, or adding a database/runtime-health concept.
- Stop on an unrelated baseline failure; report it without changing unrelated
  code or tests.
- Stop if the patch would delete/weaken safety coverage rather than replacing
  stale semantics with the explicit assertions above.
