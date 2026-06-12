# Stable Drop Confirmation Selection Plan

## Goal

Remove the variable extra Enter presses after the drop drawer is already open.
Once the operator selects `drop` and arms confirmation, ordinary live refreshes
must keep the cursor and confirmation attached to that same task/action row even
when inbox membership, row ordering, or drawer action ordering changes.

The intended risky-drop interaction after opening the drawer remains:

1. Enter arms confirmation.
2. Enter confirms and starts the drop.

## Task 1: Preserve Armed Drop Action Identity Across Refresh

Estimated time: 10-15 minutes

### Failing behavior tests

Update `crates/ajax-tui/src/lib.rs` with focused input-flow tests:

- Open a drawer, select and arm `drop`, then refresh while the task moves from
  the inbox section to the normal task section. Verify the cursor remains on
  `drop` and the next Enter dispatches the confirmed action.
- Open a drawer, select and arm `drop`, then refresh while drawer action ordering
  changes. Verify the cursor remains on `drop` and the next Enter dispatches the
  confirmed action.
- Preserve the existing behavior that confirmation is invalidated when `drop`
  actually disappears from the refreshed task actions.

Run the focused tests and show that the refresh/reordering tests fail before
implementation.

### Minimal implementation

Update `crates/ajax-tui/src/cockpit_state.rs` refresh reconciliation to:

- Search refreshed selectable drawer actions for the armed task/action identity,
  rather than checking only the row at the old numeric selection index.
- Move the cursor to the matching refreshed `drop` action row.
- Rebind and preserve confirmation when that action still exists.
- Invalidate confirmation only when the armed task/action is no longer
  available.

Do not change drawer expansion behavior, confirmation requirements, drop
execution, or non-drop action behavior.

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

Confirm that after the drawer is open, risky drop requires exactly the intended
two Enter presses even when live refresh changes row positions.
