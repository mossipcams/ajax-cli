# API Speed Plan: Start, Drop, and Web Backend Latency

## Goal

Cut the wall-clock latency of task creation (`start`), task deletion (`drop`),
and Web Cockpit API calls without weakening the substrate-evidence and
receipt contracts in `architecture.md`.

## Current Bottlenecks (from code reading)

### Task creation (`POST /api/tasks`, `ajax start`)

The start plan runs sequentially and the API responds only after every step
(`crates/ajax-core/src/commands/new_task.rs::new_task_plan`):

1. `git fetch origin <default_branch>` — network round trip, **no timeout**
   (`adapters/git.rs::fetch_origin_branch`). A slow remote stalls the whole
   request and holds the web in-flight task key.
2. Optional `graphify_update` shell command — seconds; its output is not
   consumed by the new worktree, yet it blocks worktree creation.
3. `git worktree add`.
4. Husky install: `npm exec --yes husky` — node startup plus npm resolution in
   a fresh worktree with no `node_modules`; reliably 1s+.
5. Optional repo `bootstrap` (for example `npm install`) — arbitrary seconds.
6. `tmux new-session` + agent send-keys — fast.

Only steps 1, 3, and 6 must finish before the operator can see a live task.

### Task deletion (drop)

`task_operations.rs::drop_task::execute_drop_task_operation` builds per-op
commands in `drop_op_command`; the slow step is `git worktree remove`, which
synchronously deletes worktree files (slow with `node_modules`). The two
observation rounds (plan-time and final, 3 fast commands each) are required
by the architecture and stay.

### Web API calls

- `app.js` polls `GET /api/cockpit` every 1s per client. Every poll clones
  state and runs the Live-tier refresh (tmux list-sessions, list-windows,
  conditional capture-pane, conditional git probes). Two clients/tabs double
  the probe load; near-simultaneous polls do redundant identical work.
- Handler responsiveness under slow probes is already covered by
  `axum_health_stays_responsive_during_slow_cockpit_refresh` and
  `axum_health_stays_responsive_during_slow_pane_capture` on the
  multi-threaded runtime, so no `spawn_blocking` task is included.

## Tasks

### Task 1: External command timing instrumentation

Estimated time: 10 minutes

**Failing behavior tests** — `crates/ajax-core/src/adapters/process.rs`:

- `timing_log_line_renders_program_args_and_elapsed_ms`
  - Arrange: `CommandSpec::new("git", ["-C", "/repo/web", "fetch", "origin", "main"])`
    and `Duration::from_millis(1234)`.
  - Act: call the new pure helper `timing_log_line(&command, elapsed)`.
  - Assert: returns exactly
    `ajax-timing: git -C /repo/web fetch origin main 1234ms`.
- `timing_log_line_truncates_long_argument_lists`
  - Arrange: a spec whose joined args exceed 80 chars.
  - Act: `timing_log_line(...)`.
  - Assert: args section is cut at 80 chars and ends with `…`, elapsed suffix
    still present.

Run `rtk cargo nextest run -p ajax-core timing_log_line` and show both fail
(helper does not exist yet).

**Implementation**: add `timing_log_line` and call it from
`ProcessCommandRunner::run`, writing to stderr only when `AJAX_TIMING` is set
(read once via `OnceLock<bool>`). No behavior change otherwise.

**Verification**:
```sh
rtk cargo nextest run -p ajax-core timing_log_line
AJAX_TIMING=1 ajax ready >/dev/null   # manual: confirm one line per external command
```
Capture a baseline timing of start, drop, and `/api/cockpit` before the later
tasks land.

### Task 2: Bound the start fetch with a timeout

Estimated time: 5 minutes

**Failing behavior test** — `crates/ajax-core/src/adapters/git.rs`:

- `fetch_origin_branch_carries_bounded_timeout`
  - Arrange: `GitAdapter::new("git")`.
  - Act: `git.fetch_origin_branch("/repo/web", "main")`.
  - Assert: `command.timeout == Some(GIT_FETCH_TIMEOUT)` with
    `GIT_FETCH_TIMEOUT = Duration::from_secs(60)`, and args unchanged
    (`["-C", "/repo/web", "fetch", "origin", "main"]`).

Run `rtk cargo nextest run -p ajax-core fetch_origin_branch` and show the
timeout assertion fail (`timeout` is currently `None`).

**Implementation**: `.with_timeout(GIT_FETCH_TIMEOUT)` inside
`fetch_origin_branch`. `ProcessCommandRunner` already kills and reports
timed-out captures (`capture_command_times_out_when_configured`).

**Verification**: `rtk cargo nextest run -p ajax-core fetch_origin_branch`,
then `rtk cargo nextest run -p ajax-core new_task_plan` (plan equality tests
compare against the adapter builder, so they stay green).

### Task 3: Skip the start fetch when origin is fresh

Estimated time: 15 minutes

**Failing behavior tests** — `crates/ajax-core/src/commands/new_task.rs`,
using the existing `context()` fixture (`ManagedRepo::new("web", "/repo/web", "main")`):

- `new_task_plan_skips_fetch_when_origin_fetch_is_fresh`
  - Arrange: `StartPlanObservation { origin_fetch_age: Some(Duration::from_secs(30)) }`.
  - Act: `new_task_plan_with_observation(&context, request, &observation)`.
  - Assert: `plan.commands[0]` is the `git worktree add` command (still
    branching from `origin/main`); no command in the plan has
    `args.contains("fetch")`.
- `new_task_plan_fetches_when_origin_fetch_is_stale`
  - Arrange: `origin_fetch_age: Some(Duration::from_secs(120))`.
  - Assert: `plan.commands[0] == git.fetch_origin_branch("/repo/web", "main")`.
- `new_task_plan_fetches_when_origin_fetch_age_is_unknown`
  - Arrange: `origin_fetch_age: None`.
  - Assert: fetch command present at index 0 (current behavior preserved —
    the existing `new_task_plan` wrapper passes `None`).

`crates/ajax-core/src/adapters/environment.rs`:

- `origin_fetch_age_reads_fetch_head_mtime`
  - Arrange: temp dir with `.git/FETCH_HEAD` written just now.
  - Act: `origin_fetch_age(temp_repo_path)`.
  - Assert: returns `Some(age)` with `age < 5s`.
- `origin_fetch_age_is_none_without_fetch_head`
  - Arrange: temp dir with empty `.git/`.
  - Assert: returns `None`.

Run `rtk cargo nextest run -p ajax-core origin_fetch` and
`rtk cargo nextest run -p ajax-core new_task_plan_skips_fetch` — show failures.

**Implementation**:

- `ORIGIN_FETCH_FRESH_FOR: Duration = 60s` in `commands/new_task.rs`.
- `StartPlanObservation` struct + `new_task_plan_with_observation`; the
  existing `new_task_plan` becomes a wrapper passing
  `origin_fetch_age: None` (always fetches — compatibility preserved).
- `task_operations::start::plan_start_task_operation` gains the observation
  parameter (existing signature kept as a `None` wrapper).
- `adapters/environment.rs::origin_fetch_age(repo_path)` stats
  `<repo>/.git/FETCH_HEAD` (filesystem stays in the adapter layer).
- Callers that should pass real evidence: CLI `execution_dispatch` start path
  and `ajax-web::slices::operate::start_task_with_checkpoint` (both call the
  environment probe just before planning).

**Verification**:
```sh
rtk cargo nextest run -p ajax-core new_task_plan
rtk cargo nextest run -p ajax-core origin_fetch
rtk cargo nextest run -p ajax-web start_task
```
Manual: create two tasks back-to-back with `AJAX_TIMING=1`; second start logs
no `git ... fetch` line.

### Task 4: Move husky install and bootstrap off the API critical path

Estimated time: 15 minutes

**Failing behavior tests** — `crates/ajax-core/src/commands/new_task.rs`,
using the existing `agent_send_keys_line(&plan)` helper:

- `new_task_plan_has_no_standalone_husky_command`
  - Act: `new_task_plan(&context, request)`.
  - Assert: no plan command's args contain `npm exec --yes husky`
    (the current `install_husky_hooks_command` is gone from `plan.commands`).
- `new_task_plan_chains_setup_before_agent_in_task_session`
  - Act: `new_task_plan(&context, request)` for repo without bootstrap.
  - Assert: `agent_send_keys_line(&plan)` starts with the husky guard
    (`if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; `)
    and ends with the unchanged `ajax-cli __agent-runtime ... -- codex --cd ...`
    launch (reuse the exact expectation from
    `new_task_plan_launches_agent_through_runtime_wrapper`).
- `new_task_plan_chains_bootstrap_between_husky_and_agent`
  - Arrange: `repo.bootstrap = Some("npm install".to_string())`.
  - Assert: send-keys line contains `; npm install; ` after the husky guard
    and before `ajax-cli __agent-runtime`; no standalone
    `sh -lc "npm install"` command remains in `plan.commands`.
- Update (not weaken) the two plan-shape tests that index commands
  (`new_task_plan_fetches_origin_and_branches_from_remote_tracking_ref`,
  `new_task_plan_runs_graphify_update_in_repo_root_when_configured`) for the
  shorter command list.

`crates/ajax-core/src/task_operations.rs`:

- `start_operation_records_receipts_for_successful_provisioning_steps`
  must stay green unchanged — `WorktreeCreated`, `TaskSessionCreated`, and
  `AgentCommandSent` receipts still map to the worktree-add, new-session, and
  send-keys commands (`start_provisioning_step_for_command` never matched
  husky/bootstrap, so receipts are unaffected; the
  `is_new_task_husky_hook_command` output filter becomes dead and is removed).

Run `rtk cargo nextest run -p ajax-core new_task_plan` — show the new tests
fail.

**Implementation**: drop the standalone husky/bootstrap commands from
`new_task_plan`; build the send-keys line as
`<husky guard>; [<bootstrap>; ]<agent wrapper launch>`. The worktrunk window
shell already starts in the worktree cwd, so the guard needs no `cd`.
Separator is `;` (not `&&`) so a failed setup step still launches the agent
instead of leaving a dead-looking task.

**Trade-off (flag if undesired)**: setup failures now surface in the task
pane instead of failing the start operation; the agent still only launches
after setup finishes because the commands share one shell line.

**Verification**:
```sh
rtk cargo nextest run -p ajax-core new_task
rtk cargo nextest run -p ajax-core start_operation
rtk cargo nextest run -p ajax-web start_task
```
Manual: start a task in the ajax repo; pane shows husky/bootstrap output then
the agent banner; API response returns before npm finishes.

### Task 5: Background the graphify_update step

Estimated time: 10 minutes

**Failing behavior test** — `crates/ajax-core/src/commands/new_task.rs`:

- `new_task_plan_runs_graphify_update_detached`
  - Arrange: `repo.graphify_update = Some("graphify extract --update".to_string())`
    (same fixture as `new_task_plan_runs_graphify_update_in_repo_root_when_configured`).
  - Act: `new_task_plan(&context, request)`.
  - Assert: the graphify command equals
    `CommandSpec::new("sh", ["-lc", "(graphify extract --update) >/dev/null 2>&1 &"]).with_cwd("/repo/web")`
    so the command exits immediately while the update continues.
  - Update the existing graphify test expectation to the wrapped form.

Run `rtk cargo nextest run -p ajax-core graphify` — show failure.

**Implementation**: wrap the configured command in a detached subshell in
`new_task_plan`. Repo-root graph refresh continues; it no longer blocks
worktree provisioning.

**Verification**: `rtk cargo nextest run -p ajax-core graphify`.

### Task 6: Fast drop — rename worktree to trash, delete in background

Estimated time: 15 minutes

**Failing behavior tests** — `crates/ajax-core/src/task_operations.rs`,
using the existing `RecordingQueuedRunner` to capture executed commands:

- `confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline`
  - Arrange: task in cleanup lifecycle with observation
    `worktree: Present, branch: Present, tmux_session: Present`; confirmed
    drop where `drop_needs_force` is true.
  - Act: `execute_drop_task_operation(...)`.
  - Assert: the worktree op command is
    `sh -c '<fast-remove script>' ajax-fast-worktree-remove <repo_path> <worktree_path> <trash_path>`
    where `<trash_path>` is
    `<worktrees_parent>/.ajax-trash/<handle>-<nonce>`; the script performs
    `mv "$2" "$3" && git -C "$1" worktree prune && { rm -rf "$3" >/dev/null 2>&1 & }`
    (positional args keep paths with spaces safe — same concern as
    `new_task_plan_preserves_paths_with_spaces_as_command_arguments`);
    no `git worktree remove` command was run; the
    `worktree_absent` step receipt is still recorded as `Succeeded`.
- `unforced_dirty_drop_keeps_plain_git_worktree_remove`
  - Arrange: drop where `drop_needs_force` is false (clean, merged, pushed —
    but eligibility below says fast path also applies here, so arrange the
    case that must keep the safety backstop: force false **and** git evidence
    absent/unknown).
  - Assert: command is `git -C <repo> worktree remove <worktree>` exactly as
    today (the `git worktree remove` dirty-tree refusal remains the backstop
    when evidence is missing).
- `fast_drop_mv_failure_marks_teardown_incomplete`
  - Arrange: queued runner returns non-zero with
    `mv: ... No such file or directory` for the fast-remove command; final
    observation still reports the worktree present.
  - Assert: completion is `TeardownIncomplete` with `failed_step ==
    DropOp::EnsureWorktreeAbsent` (failure handling unchanged).

Classifier tests:

- `mark_task_cleanup_step_completed` (`commands/teardown.rs` tests):
  `fast_worktree_remove_command_marks_worktree_cleanup_completed` — the sh
  composite (recognized by its `ajax-fast-worktree-remove` argv[0] marker)
  updates git status the same way `git worktree remove` does today.
- `drop_cleanup_resource_is_already_missing` (`task_operations.rs` tests):
  `fast_worktree_remove_missing_source_counts_as_already_missing` — stderr
  `no such file or directory` for the marked composite returns true, so a
  raced-away worktree records `SkippedObserved` instead of failing the drop.

Run `rtk cargo nextest run -p ajax-core drop` — show the new tests fail.

**Implementation**:

- `drop_op_command` builds the fast composite for `EnsureWorktreeAbsent` when
  force applies **or** git evidence shows clean/pushed; keeps plain
  `git worktree remove` when evidence is absent and force is off.
- Trash dir is a sibling of the worktree
  (`<worktree_parent>/.ajax-trash/`), guaranteeing same-filesystem rename.
- Extend `mark_task_cleanup_step_completed` and
  `drop_cleanup_resource_is_already_missing` to recognize the marked
  composite.
- `native_teardown_commands` (confirmation-plan display) keeps its current
  commands; receipts, step keys, and the final-observation
  `Removed`/`TeardownIncomplete` decision are unchanged.

**Verification**:
```sh
rtk cargo nextest run -p ajax-core drop
rtk cargo nextest run -p ajax-core teardown
rtk cargo nextest run -p ajax-web   # operate drop path
```
Manual: drop a task whose worktree has `node_modules` with `AJAX_TIMING=1`;
the worktree op line drops from seconds to milliseconds.

### Task 7: Sweep leftover trash entries in `tidy`

Estimated time: 10 minutes

**Failing behavior test** — sweep tests in
`crates/ajax-core/src/task_operations.rs`:

- `sweep_cleanup_removes_stale_trash_entries`
  - Arrange: context with one managed repo and rooted/legacy worktree
    placement.
  - Act: `execute_sweep_cleanup_operation(...)`.
  - Assert: the executed commands include
    `sh -c 'if [ -d "$1" ]; then find "$1" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi' ajax-trash-sweep <worktrees_root>/.ajax-trash`
    once per worktree root (covers background deletes interrupted by
    reboot/kill); sweep outcome reporting is otherwise unchanged.

Run `rtk cargo nextest run -p ajax-core sweep` — show failure.

**Implementation**: append the guarded trash-sweep command per distinct
worktree root during sweep planning. The 60-minute age guard avoids racing a
live background delete.

**Verification**: `rtk cargo nextest run -p ajax-core sweep`.

### Task 8: Coalesce Web Cockpit refreshes (single-flight + short TTL)

Estimated time: 15 minutes

**Failing behavior tests** — `crates/ajax-web/src/runtime.rs` tests; extend
`TestBridge` with `refresh_count: usize` (alongside the existing `refreshed`
flag and `refresh_delay`):

- `axum_cockpit_serves_cached_projection_within_refresh_ttl`
  - Arrange: `WebAppState` built with `refresh_ttl: Duration::from_millis(500)`
    (new constructor parameter; `WebAppState::new` defaults to 500ms).
  - Act: two sequential `GET /api/cockpit` via `oneshot`.
  - Assert: both 200 with a projection body; `bridge.refresh_count == 1`.
- `axum_cockpit_refreshes_again_after_ttl_expires`
  - Arrange: `refresh_ttl: Duration::from_millis(20)`.
  - Act: GET, `tokio::time::sleep(40ms)`, GET.
  - Assert: `bridge.refresh_count == 2`.
- `axum_operation_invalidates_cockpit_refresh_cache`
  - Arrange: TTL 500ms; GET once (cache warm).
  - Act: `POST /api/operations` (review action, as in
    `axum_cockpit_refresh_does_not_overwrite_concurrent_operation_state`),
    then GET.
  - Assert: the final GET triggered a fresh bridge refresh
    (`refresh_count == 2`) — mutations must not serve a pre-mutation
    projection.
- `concurrent_cockpit_polls_share_one_refresh`
  - Arrange: TTL 500ms, `refresh_delay: Duration::from_millis(200)`,
    multi-thread test runtime (same pattern as
    `axum_health_stays_responsive_during_slow_cockpit_refresh`).
  - Act: spawn GET #1; after 25ms issue GET #2.
  - Assert: GET #2 returns 200 in `< 150ms` (serves the current in-memory
    projection instead of waiting) and `bridge.refresh_count == 1`.

Run `rtk cargo nextest run -p ajax-web cockpit` — show the new tests fail.

**Implementation**: shared state gains `last_refresh_at: Option<Instant>` and
a `refresh_in_flight: bool` marker. `axum_cockpit`: serve the in-memory
projection without probing when `last_refresh_at` is inside the TTL or a
refresh is already in flight (stale-while-revalidate single-flight);
otherwise run the existing refresh session and stamp `last_refresh_at`.
`run_optimistic` (mutations) clears `last_refresh_at` on commit so the next
poll re-probes. Browser polling cadence and DTOs are unchanged.

**Verification**:
```sh
rtk cargo nextest run -p ajax-web
```
Manual: open Web Cockpit in two browser tabs with `AJAX_TIMING=1`; tmux probe
lines no longer double.

## Dropped from the earlier draft

`spawn_blocking` for web handlers: responsiveness under slow refresh, pane
capture, and concurrent operations is already pinned by
`axum_health_stays_responsive_during_slow_cockpit_refresh`,
`axum_health_stays_responsive_during_slow_pane_capture`, and
`axum_cockpit_refresh_does_not_overwrite_concurrent_operation_state` on the
multi-threaded production runtime. No failing behavior to write.

## Architecture Documentation

Tasks 4, 6, and 8 change documented behavior. In the same work, update
`architecture.md`:

- Substrate Evidence / start provisioning: husky+bootstrap run inside the
  task session before the agent launch; fetch is skipped on fresh
  origin-fetch evidence (< 60s).
- Task Operations / drop: confirmed worktree teardown renames to a sibling
  `.ajax-trash` entry, prunes, and deletes in the background; `tidy` sweeps
  stale trash entries.
- Web Cockpit post-startup refresh: `/api/cockpit` serves the in-memory
  projection inside a short refresh TTL and single-flights concurrent
  refreshes; mutations invalidate the window.

Verification: read-through of the three updated sections against the merged
implementation (Markdown-only, no TDD).

## Full Verification (after all tasks)

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```

Fresh-worktree note: run `npm install` first so the pre-commit jscpd step
works.

## Expected Wins

- Start API response: sheds husky/bootstrap/graphify seconds and, on warm
  repos, the fetch round trip — down to roughly worktree-add + tmux time.
- Drop API response: file deletion moves off the request path; drop becomes
  rename + prune + branch delete + session kill (sub-second in most cases).
- Web cockpit polling: concurrent clients/tabs share one probe cycle per TTL
  window instead of multiplying tmux/git subprocess load.
- Task 1 timing lines give before/after numbers for each claim.
