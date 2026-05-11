# Ajax Native Lifecycle Implementation Plan

## Goal

Ajax should own task lifecycle end to end:

- create task branches and worktrees
- create and open tmux sessions
- ensure the `worktrunk` window points at the task worktree
- launch the selected agent inside the task runtime
- record task state from Ajax-owned lifecycle and live supervisor events
- check, diff, merge, clean, and sweep tasks through Ajax policy
- expose deterministic command plans and Cockpit JSON for every operator action

External tools remain primitives:

- `git` owns repository, branch, merge, and worktree truth.
- `tmux` owns durable terminal sessions, windows, and panes.
- Agent CLIs remain opaque workers.
- Ajax owns naming, planning, registry state, policy, live status, and Cockpit
  workflows.

This is not a compatibility migration. Replace `workmux` lifecycle behavior
directly and delete legacy `workmux` code as each covered slice lands.

## Non-Goals

- Do not reimplement Git internals.
- Do not reimplement terminal/session management.
- Do not add a permanent `workmux` versus native lifecycle backend switch.
- Do not add new dependencies unless the existing standard library, `git`,
  `tmux`, and current project crates cannot reasonably solve the problem.
- Do not modify files under `tests/` unless explicitly approved.

## Design Decisions

- Keep the current sibling worktree layout initially:
  `repo__worktrees/ajax-fix-login`. Rename the helper to Ajax-owned language,
  but avoid path churn while replacing lifecycle behavior.
- Launch the agent inside the `worktrunk` pane with tmux. This preserves the
  existing pane classifier for approval, auth, rate limit, context limit,
  blocked, and done states.
- Make merge conservative at first. Prefer a clear failed command over leaving
  the base repo conflicted. A richer conflict-resolution workflow can be a
  later feature.
- Keep command planning separate from command execution. New lifecycle behavior
  should produce `CommandSpec` values and continue through the existing runner.
- Keep Cockpit as an operator surface over `ajax-core` contracts. Lifecycle
  decisions belong in `ajax-core`.

## Tasks

### 1. Rename Lifecycle Layout Ownership

Test to write:

- Update source-local `ajax-core` tests so `task_from_new_request` proves Ajax
  owns branch, session, worktree, and `worktrunk` naming.

Code to implement:

- Replace `workmux_worktree_path` with an Ajax-owned helper name.
- Keep the current sibling worktree layout.
- Remove `workmux` wording from lifecycle path helpers.

Verify:

```sh
cargo test -p ajax-core task_from_new_request
```

### 2. Add Native Git Command Builders

Test to write:

- Adapter tests for:
  - `git worktree add`
  - `git worktree remove`
  - branch delete
  - switching the base repo to the default branch
  - merging the task branch

Code to implement:

- Extend `GitAdapter` with command builders only.
- Preserve existing status and parser behavior.

Verify:

```sh
cargo test -p ajax-core adapters
```

### 3. Add Native Tmux Command Builders

Test to write:

- Adapter tests for:
  - detached session creation rooted at the task worktree
  - targeting `session:worktrunk`
  - sending an agent command into `worktrunk`
  - attaching or switching to `worktrunk`
  - killing a task session

Code to implement:

- Extend `TmuxAdapter`.
- Keep `CommandMode` choices explicit: captured probes, inherited interactive
  opens, and spawned/detached lifecycle starts.

Verify:

```sh
cargo test -p ajax-core adapters
```

### 4. Replace `new_task_plan`

Test to write:

- Rewrite the current workmux-add plan test to expect native commands:
  - create/add worktree from the default branch
  - create detached tmux session/window rooted at the worktree
  - launch the selected agent in `worktrunk`

Code to implement:

- Remove `WorkmuxAdapter` usage from `new_task_plan`.
- Build the full native provisioning plan in `ajax-core`.

Verify:

```sh
cargo test -p ajax-core new_task_plan
```

### 5. Fix New-Task Execution Flow

Test to write:

- CLI source-local test proving `ajax new --execute` records the task after
  native provisioning and does not run a second open plan.

Code to implement:

- Remove `new_task_open_plan` from `execute_new_task_plan`.
- Record the task and mark it active only after the native plan succeeds.

Verify:

```sh
cargo test -p ajax-cli new_execute
```

### 6. Replace `open_task_plan`

Test to write:

- `open_task_plan` emits direct tmux commands targeting `worktrunk`, not
  `workmux open`.
- Attach and switch-client modes are represented directly.

Code to implement:

- Use `TmuxAdapter` directly.
- Target the recorded Ajax session and `worktrunk` window.

Verify:

```sh
cargo test -p ajax-core open_task_plan
```

### 7. Replace `trunk_task_plan`

Test to write:

- `trunk_task_plan` targets `session:worktrunk`.
- If the recorded state says `worktrunk` is missing or pointed at the wrong
  path, the plan includes a repair step before opening.

Code to implement:

- Make `trunk` a direct tmux workflow.
- Preserve operator-facing blocked reasons when the task worktree is missing.

Verify:

```sh
cargo test -p ajax-core trunk_task_plan
```

### 8. Remove Legacy Workmux Window Discovery

Test to write:

- Live status uses only the recorded Ajax session and `worktrunk` window.
- Legacy `wm-*` windows are not discovered as substitutes.

Code to implement:

- Delete `find_workmux_window`.
- Delete `workmux_window_name`.
- Remove the extra all-windows compatibility lookup if it is no longer needed.

Verify:

```sh
cargo test -p ajax-core live
```

### 9. Replace Merge Planning

Test to write:

- Clean merge candidates emit native git merge commands.
- Tasks with side flags still require confirmation.
- Blocked safety states still block command emission.

Code to implement:

- Remove `workmux merge`.
- Plan direct git commands from the managed repo path.
- Keep the first merge implementation conservative and explicit.

Verify:

```sh
cargo test -p ajax-core merge_task_plan
```

### 10. Replace Clean Planning

Test to write:

- Safe clean emits native cleanup commands:
  - kill task tmux session when present
  - remove task worktree when present
  - delete task branch when safe
- Blocked cleanup emits no commands and preserves blocked reasons.

Code to implement:

- Remove `workmux remove`.
- Reuse cleanup safety as the gate.
- Skip commands for resources known missing from live state.

Verify:

```sh
cargo test -p ajax-core clean_task_plan
```

### 11. Replace Sweep Cleanup

Test to write:

- Sweep reuses the native clean command shape for safe candidates only.
- Sweep candidates and sweep plan remain aligned.

Code to implement:

- Build sweep from the same cleanup helper used by `clean_task_plan`.

Verify:

```sh
cargo test -p ajax-core sweep_cleanup_plan
```

### 12. Update Doctor

Test to write:

- Doctor requires `git`, `tmux`, and the configured/default agent.
- Doctor no longer checks `tool:workmux`.

Code to implement:

- Change required tool detection.
- Update fixtures and expected checks.

Verify:

```sh
cargo test -p ajax-core doctor
```

### 13. Update Cockpit Wording

Test to write:

- Cockpit missing-title errors no longer mention `workmux`.

Code to implement:

- Replace user-facing `workmux` wording in Cockpit action errors.

Verify:

```sh
cargo test -p ajax-cli cockpit_actions
```

### 14. Delete Workmux Adapter

Test to write:

- No new behavior test is needed if previous tasks cover all replaced command
  plans. Compilation should prove no callers remain.

Code to implement:

- Delete `WorkmuxNewTask`.
- Delete `WorkmuxAdapter`.
- Delete workmux adapter tests.
- Remove imports and stale expectations.

Verify:

```sh
cargo test -p ajax-core adapters
rg "Workmux|workmux" crates
```

### 15. Update Smoke Script

Test to write:

- No Rust test unless explicitly approved. The smoke script itself is the
  validation artifact.

Code to implement:

- Remove fake `workmux`.
- Expand fake `git`, `tmux`, and `codex` behavior to cover native lifecycle
  commands.

Verify:

```sh
scripts/smoke.sh
```

### 16. Update Documentation

Test to write:

- No test required.

Code to implement:

- Update `README.md` and `architecture.md` so Ajax owns lifecycle.
- Describe `git`, `tmux`, and agent CLIs as durable substrates.
- Remove statements that `workmux` owns task/worktree/session lifecycle.

Verify:

```sh
rg "workmux" README.md architecture.md
```

## Final Validation

Before considering the lifecycle replacement complete, run:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
scripts/smoke.sh
rg "Workmux|workmux" crates README.md architecture.md scripts
```

Any remaining `workmux` hits should be intentional historical notes. Otherwise,
remove them.
