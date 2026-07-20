PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Stop attention webhooks from firing while a task lifecycle is `Removing` or
`Removed`. Drop teardown intentionally removes tmux/worktree; that missing
substrate currently projects as `Error` and pings before the task settles or
deletes. `TeardownIncomplete` must still notify once.

## Allowed files

- `crates/ajax-core/src/attention.rs`

## Forbidden changes

- Do not change `ui_state::derive_operator_status` precedence or UI Error
  projection.
- Do not change dwell / episode-clear constants or metadata key names.
- Do not edit `notify.rs`, web tick, runtime_refresh, or drop_task.
- Do not suppress `TeardownIncomplete` notifications.
- No renames, formatting sweeps, or unrelated cleanup.

## Context evidence

- Desired behavior: Dropping a task must not phone-ping for expected substrate
  gaps during teardown. Durable `TeardownIncomplete` still pings.
- Status precedence: missing substrate is checked before `Removing → Idle` in
  `derive_task_status` (`ui_state.rs:47-50` then `:79-87`), so Removing +
  TmuxMissing → `Error`. Architecture allows error to override cleanup
  lifecycles for UI; notifications must still ignore in-flight drop.
- Detector: `take_attention_transition_at` (`attention.rs:40-78`) fires on any
  actionable Waiting/Error with a new episode stamp. No lifecycle guard today.
- Side-flag path: `Task::apply_tmux_status` (`models.rs:461-468`) calls
  `mark_resource_missing(TmuxMissing)` even when live observation apply is
  skipped for Removing (`runtime_refresh.rs:256-263`).
- Pattern to reuse: early `None` returns already used for non-actionable
  Waiting (`is_actionable_attention`) and Ready-for-review /
  delegated-waiting filters.

## Code anchors

- `crates/ajax-core/src/attention.rs:40` — `take_attention_transition_at`
- `crates/ajax-core/src/attention.rs:45-72` — Waiting/Error fire arms;
  Running/Idle clear arm
- `crates/ajax-core/src/attention.rs:573-578` — `waiting_task` / `active_task`
  test helpers
- `crates/ajax-core/src/ui_state.rs:41-43` — TeardownIncomplete → Error
- `crates/ajax-core/src/ui_state.rs:47-50` — missing substrate → Error (before
  Removing Idle)
- `crates/ajax-core/src/models.rs:39` — `LifecycleStatus::Removing`

## Test-first instructions

Add two tests in `attention.rs` `#[cfg(test)]` next to existing transition
tests (~after `running_and_idle_never_fire`):

1. `removing_with_missing_substrate_does_not_notify`
   - Build active waiting-capable task OR active + `mark_resource_missing(TmuxMissing)`.
   - Set `task.lifecycle_status = LifecycleStatus::Removing` (direct assign is
     fine in tests; lifecycle module already does this in projection tests).
   - Assert `take_attention_transition(&mut task) == None`.
   - Assert no `LAST_NOTIFIED_STATUS_KEY` written.

2. `teardown_incomplete_still_notifies`
   - Active task, set `lifecycle_status = LifecycleStatus::TeardownIncomplete`.
   - Assert transition is `Some` with `TaskStatus::Error`.
   - Second call returns `None` (dedup).

Red command:

```bash
cargo nextest run -p ajax-core removing_with_missing_substrate_does_not_notify teardown_incomplete_still_notifies
```

Expect nonzero exit and the Removing test assertion failure before production
edit.

## Edit instructions

In `take_attention_transition_at`, before computing/firing on Waiting|Error
(immediately after deriving `operator_status` is fine, or before the match),
if `task.lifecycle_status` is `Removing` or `Removed`, return `None` without
writing notify metadata. Import `LifecycleStatus` if not already available via
existing `models` import (it is already imported at `attention.rs:2`).

Do not clear episode stamps on this path (task is leaving or already gone).

## Verification commands

```bash
cargo nextest run -p ajax-core removing_with_missing_substrate_does_not_notify teardown_incomplete_still_notifies
cargo nextest run -p ajax-core attention
cargo fmt --check
cargo clippy -p ajax-core --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- Removing + missing substrate does not fire and does not stamp metadata.
- TeardownIncomplete still fires once then dedups.
- Existing attention tests remain green.
- Diff limited to `attention.rs`.

## Stop conditions

- Need to change `derive_operator_status` to make the test pass.
- Need to touch notify.rs / web / drop_task.
- Diff exceeds ~80 lines or spreads beyond Allowed files.
- Unrelated test failures in ajax-core.
