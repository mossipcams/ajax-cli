# Packet: task 4 REVISE — align ajax-cli tests with the pane-classification cut

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

The core cut (already applied: pane-text classification, dwell gate, refresh
pane fallthrough all deleted; ajax-core 813/813 green) leaves 22 ajax-cli
tests failing because their fixtures drive status via capture-pane text.
Convert tests that pin surviving behavior to hook/wrapper/lifecycle evidence;
delete tests that pin deleted pane behavior. No production code changes.

## Allowed files

- `crates/ajax-cli/src/cockpit_backend.rs` (tests mod only)
- `crates/ajax-cli/src/web_backend.rs` (tests mod only)
- `crates/ajax-cli/src/lib/tests.rs`
- `crates/ajax-cli/tests/live_cli.rs` (explicitly authorized deviation from
  the no-tests-dir rule, for pane-behavior characterization only)

## Forbidden changes

- No production (non-test) code edits anywhere.
- No changes to ajax-core.
- Never weaken an assertion about surviving behavior: if a test asserts
  "refresh applies evidence and updates status/annotations/inbox", convert
  its evidence source (hook status cache value, wrapper runtime snapshot, or
  lifecycle event file) instead of deleting the assertion. Reference
  conversion: `cockpit_backend::tests::live_refresh_clears_stale_input_when_hook_reports_working`
  (was pane+dwell, now StaticAgentStatusCache hook `working`).
- Delete outright ONLY tests whose point is the deleted mechanism itself
  (pane probe issued, pane text classified, probe-failure recorded, pane+dwell
  two-phase confirmation, structured-pane recognition).

## Context evidence

Failing tests (nextest, 2026-07-21) and disposition guidance:

Delete (pin the deleted pane mechanism):
- cockpit_backend::tests::live_refresh_reprobes_error_task_with_recoverable_conflict_prompt
  (asserts a capture-pane probe is issued)
- tests::live_refresh_nonzero_pane_capture_reports_probe_failure
- tests::cockpit_refresh_marks_agent_running_from_wrapper_plus_structured_pane
  (structured-pane source no longer exists; wrapper alone must NOT assert
  AgentRunning — that invariant is already pinned in core)
- tests::live_refresh_updates_changed_task_window_status_before_pane_failure
  (ordering vs pane failure; pane probe gone — keep only if it still pins
  task-window status updating, else delete)

Convert to hook/wrapper evidence (pin surviving refresh semantics):
- cockpit_backend::tests::live_refresh_updates_cached_annotations_for_cockpit_inbox
  (pane approval prompt → hook `ask` cache value; assert same annotation,
  inbox item, "Waiting for approval")
- web_backend::tests::cockpit_api_refreshes_live_task_status_before_rendering
- web_backend::tests::cockpit_api_reloads_task_state_from_disk_before_rendering
- tests::cockpit_json_refreshes_live_status_from_tmux
- tests::cockpit_json_refreshes_live_status_even_when_projection_is_fresh
- tests::cockpit_json_watch_streams_each_refreshed_frame_to_writer
- tests::cockpit_json_watch_renders_refreshed_live_status_over_iterations
- tests::cockpit_refresh_snapshot_reports_refreshed_tmux_state
- tests::cockpit_watch_renders_refreshed_live_status_in_frame
- tests::live_refresh_lists_tmux_windows_once_for_multiple_active_tasks
- tests::live_refresh_clears_stale_task_window_missing_flag_when_status_matches
- tests::live_refresh_clears_stale_tmux_missing_flag_when_status_matches
- tests::live_refresh_reports_changed_when_same_status_updates_activity
- tests::read_commands_share_live_refresh_contract
- tests::read_json_commands_refresh_live_state_even_when_projection_is_fresh
- tests::status_command_renders_json_from_refreshed_live_state
- tests::status_command_refreshes_live_state_from_tmux
- ajax-cli::live_cli live_cockpit_json_refreshes_recorded_state_from_tmux_without_repair

Conversion mechanics available in the harness:
- cockpit/web tests: `StaticAgentStatusCache` +
  `refresh_runtime_context_with_tier` (see reference conversion) or hook
  status files under the test cache dir.
- lib/tests.rs + live_cli.rs: tests run refresh through real cache reads —
  write a hook status file (`{session}.status` with `working`/`ask`) or an
  agent-events JSON / wrapper runtime snapshot into the harness cache dir,
  matching how `agent_status_cache` reads them. Wrapper `done`/`failed`
  snapshots may replace pane fixtures where a test pins terminal status.
- Where a converted test asserted an exact explanation string produced by
  pane classification, assert the hook-equivalent explanation instead
  (e.g. "agent running" / "Waiting for approval") — same semantic, new
  source.

## Code anchors

Test modules only, listed above. Do not touch fixture runners still used by
passing tests; remove runner structs that become fully unused (dead_code
fails clippy).

## Test-first instructions

NOT_APPLICABLE: tests-only alignment task.

## Edit instructions

Apply the dispositions above. For every deleted test add nothing in its
place; for every converted test keep all original assertions that still
describe surviving behavior.

## Verification commands

```bash
cargo nextest run -p ajax-cli --no-fail-fast
cargo test -p ajax-core --lib
cargo clippy -p ajax-core -p ajax-cli -- -D warnings
cargo fmt -p ajax-core -p ajax-cli -- --check
```

## Acceptance criteria

- ajax-cli: 0 failed, no skipped-by-compile-error; ajax-core stays 813 green.
- Report lists per-test disposition (converted vs deleted) with one line each.
- clippy/fmt clean.

## Stop conditions

- A listed test cannot be converted without production changes (report it,
  do not change production code).
- Any unlisted test starts failing.
