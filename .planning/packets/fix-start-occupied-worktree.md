# TDD Implementation Packet — start occupied worktree / branch

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Block Start planning when the target worktree path or branch would make
`git worktree add -b` fail, or when another non-removed registry task already
claims that path or branch. Emit precise `PlanBlocked` errors; never delete,
move, or overwrite the occupant.

## Allowed files

- `crates/ajax-core/src/commands/new_task.rs`
- `crates/ajax-core/src/adapters/environment.rs`
- `crates/ajax-core/src/task_operations/start.rs`
- `crates/ajax-cli/src/execution_dispatch.rs`
- `crates/ajax-cli/src/cockpit_actions.rs`
- `crates/ajax-web/src/slices/operate.rs`
- `.planning/agent-plans/fix-start-occupied-worktree.md`

## Forbidden changes

- Do not delete, remove, rename, switch, or overwrite any worktree or branch.
- Do not change Repair / `task_window.rs` occupied-path behavior (#597).
- Do not change worktree placement, slugify, or public start CLI flags.
- Do not add dependencies or new abstraction layers.
- Do not weaken or delete existing tests.
- Do not edit files outside Allowed files.
- Do not commit, push, merge, rebase, create branches, or change branches.

## Context evidence

- **Desired behavior:** `ajax start` for title `owasp` must not run
  `git worktree add -b ajax/owasp <path>` when `<path>` already exists (user
  failure: status 128 “already exists”). Same class: leftover `ajax/<handle>`
  branch, or another registry task already using that path/branch.
- **Prior fix gap:** #597 only blocks Repair via
  `task_window_repair_plan_with_open_mode` when refreshed evidence shows the
  expected path occupied by another branch. Start still always appends
  `GitAdapter::add_worktree` in `new_task_plan_with_observation`.
- **Create planner:** `crates/ajax-core/src/commands/new_task.rs`
  `new_task_plan_with_observation` only rejects duplicate qualified handle;
  then always plans `git worktree add -b`.
- **Observation pattern:** `StartPlanObservation { origin_fetch_age }` is built
  in `ajax-cli` `execution_dispatch::start_plan_observation` and
  `ajax-web` `operate::start_plan_observation`. Cockpit start still calls
  `plan_start_task_operation` without live substrate preflight.
- **Filesystem probe pattern:** `DoctorEnvironment::path_exists` /
  `origin_fetch_age` in `adapters/environment.rs` for injectable/live probes.
- **Error style:** existing start collision uses
  `Err(CommandError::PlanBlocked(vec![format!("task already exists: …")]))`.

## Code anchors

- `crates/ajax-core/src/commands/new_task.rs`:
  `StartPlanObservation`, `new_task_plan`, `new_task_plan_with_observation`
  (after handle collision check, before `plan.commands.push(git.add_worktree…)`),
  private `ajax_worktree_path` / `slugify_title`.
- `crates/ajax-core/src/adapters/environment.rs`: beside `origin_fetch_age`.
- `crates/ajax-core/src/task_operations/start.rs`:
  `plan_start_task_operation` default observation construction.
- `crates/ajax-cli/src/execution_dispatch.rs`: `start_plan_observation`.
- `crates/ajax-cli/src/cockpit_actions.rs`: Start `OperatorAction` arm using
  `plan_start_task_operation`.
- `crates/ajax-web/src/slices/operate.rs`: `start_plan_observation`.

## Test-first instructions

Add these tests in `crates/ajax-core/src/commands/new_task.rs` tests module
(near existing `new_task_plan_*` tests):

1. `new_task_plan_blocks_when_worktree_path_already_exists`
   - Create a temp repo dir and the legacy sibling worktree path
     `{repo_parent}/{repo_name}__worktrees/ajax-fix-login` as an existing
     directory (use `std::env::temp_dir` + pid/nanos like environment tests;
     do not add tempfile dependency).
   - Context repo path points at that temp repo.
   - Call `new_task_plan` for title `Fix login`.
   - Assert `Err(CommandError::PlanBlocked(_))` whose message contains the
     worktree path and indicates it already exists.
   - Assert no successful plan / no worktree-add command is returned.

2. `new_task_plan_blocks_when_target_branch_already_exists`
   - Empty registry, path does not exist.
   - Call `new_task_plan_with_observation` with
     `StartPlanObservation { origin_fetch_age: None, target_branch_exists: true, … }`
     (exact fields per implementation).
   - Assert PlanBlocked mentioning branch `ajax/fix-login`.

3. `new_task_plan_blocks_when_registry_claims_worktree_path_or_branch`
   - Insert a non-removed task with a different handle whose `worktree_path`
     or `branch` matches what the new request would use (cover at least one
     path claim and one branch claim, either as one parameterized-style pair
     of asserts in one test or two tiny tests).
   - Assert PlanBlocked naming the conflict; no `git worktree add`.

RED command (run before production edits):

```bash
rtk cargo test -p ajax-core new_task_plan_blocks_when_worktree_path_already_exists -- --nocapture
```

Expect nonzero exit because the test (and siblings) do not exist yet or the
behavior is missing.

## Edit instructions

1. Extend `StartPlanObservation` with `target_branch_exists: bool` (default
   `false` at all existing struct literals). Keep `origin_fetch_age`.
2. Optionally add `local_branch_exists(repo_path, branch) -> bool` in
   `adapters/environment.rs` using `git show-ref --verify --quiet
   refs/heads/<branch>` (success ⇒ true; spawn/failure ⇒ false). Reuse from
   CLI/web observation builders.
3. In `new_task_plan_with_observation`, after computing `branch` /
   `worktree_path` and the existing handle check, before building commands:
   - If `worktree_path.exists()` → `Err(PlanBlocked)` with
     `worktree path already exists: {path}`.
   - If `observation.target_branch_exists` → `Err(PlanBlocked)` with
     `branch already exists: {branch}`.
   - If any non-removed registry task has the same `worktree_path` or same
     `branch` → `Err(PlanBlocked)` identifying the claiming task handle.
4. Update `start_plan_observation` in CLI and web to compute the intended
   branch (`ajax/{slugify(title)}`) and set `target_branch_exists` via the
   environment helper when the managed repo exists. Prefer calling a small
   shared core helper if one is needed to avoid duplicating slugify; if
   slugify stays private, duplicate only the `ajax/{handle}` string using the
   same public `start_task_identity` / handle from `task_from_new_request`
   patterns already available (`start_task_identity(repo, title)` yields
   `repo/handle`, branch is `ajax/{handle}`).
5. Update cockpit Start paths to plan with observation that includes the
   branch preflight (use `plan_start_task_operation_with_observation` + same
   observation builder, or fix `plan_start_task_operation` default to probe
   branch when a repo path is available). Path existence remains checked
   inside core via `Path::exists` so cockpit benefits without extra wiring.
6. Update every `StartPlanObservation { … }` literal in allowed files/tests.

Keep messages stable and operator-readable. Do not issue git worktree add when
blocked.

## Verification commands

```bash
rtk cargo test -p ajax-core new_task_plan_blocks_when_worktree_path_already_exists -- --nocapture
rtk cargo test -p ajax-core new_task_plan_blocks_when -- --nocapture
rtk cargo test -p ajax-core --lib
rtk cargo fmt --check
rtk cargo clippy -p ajax-core --all-targets --all-features -- -D warnings
rtk cargo check -p ajax-cli -p ajax-web
```

## Acceptance criteria

- Start planning returns `PlanBlocked` (no worktree-add command) when the
  target path exists, when `target_branch_exists` is true, or when another
  non-removed task claims the path or branch.
- Happy-path `new_task_plan` tests still pass for non-existent fake paths.
- CLI/web observation sets `target_branch_exists` from live git when possible.
- No destructive git/tmux commands are added.
- Focused tests prove RED then GREEN; broader ajax-core lib tests pass.

## Stop conditions

- Need to change Repair, placement, or lifecycle semantics.
- Diff grows beyond ~400 lines or outside Allowed files.
- Existing start tests fail for reasons unrelated to struct-literal updates.
- Temptation to auto-remove occupying worktrees or reuse them in place.
