# Drop Confirmation Refresh Plan

## Goal

Remove the Native Cockpit drop-confirmation timing race. After the first Enter
arms a drop confirmation, ordinary background refreshes should keep that
confirmation active while the same task/action remains available. A refresh
that removes or changes the action must still invalidate confirmation.

## Task 1: Preserve Valid Drop Confirmation Across Refresh

Estimated time: 10-15 minutes

### Failing behavior tests

Update `crates/ajax-tui/src/lib.rs` with focused App behavior tests:

- Arm a task action confirmation, apply an ordinary refreshed snapshot where
  the same task/action remains available, then verify the next Enter dispatches
  the confirmed action.
- Arm a task action confirmation, apply a refreshed snapshot where that action
  is no longer available, then verify confirmation is invalidated and cannot
  dispatch as confirmed.

Run the focused tests and show that the refresh-preservation test fails before
implementation.

### Minimal implementation

Update `crates/ajax-tui/src/cockpit_state.rs` refresh handling to:

- Rebind an armed confirmation to the refreshed selectable action when its
  task identity, handle, and action still match.
- Clear/invalidate the armed confirmation when the refreshed cockpit no longer
  exposes that action.
- Preserve the existing confirmation notice only for a still-valid armed
  confirmation.

No CLI drop execution, teardown behavior, public API, or architecture boundary
changes are required.

### Verification

Run:

```sh
rtk cargo nextest run -p ajax-tui task_action_confirmation
rtk cargo nextest run -p ajax-tui
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```

Confirm that a drop confirmation remains armed across normal polling refreshes,
while a removed/changed action cannot be confirmed.
