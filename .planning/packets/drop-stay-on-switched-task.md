# Packet: Drop stay on switched-to task

PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

```yaml
dispatch_level: compact
estimated_changed_lines: 25
```

## Goal

When Drop commits after the operator has already left the dropped task’s detail
page (e.g. opened another task during the undo window), do **not** navigate to
the dashboard. Only dismiss when Drop completes while still on that task detail.

## Allowed files

- `crates/ajax-web/web/src/features/task/ActionBar.tsx`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx`

## Forbidden changes

- `App.tsx` / routing / `ResultPanel` / Drop undo timing / architecture.md
- Unrelated action UX, commits, pushes, branch changes
- Do not clear the surviving Drop timer-on-unmount behavior

## Code anchors

- `ActionBar.tsx` ~102–103: `if (action.action === "drop") onDismiss?.();`
- `ActionBar.tsx` ~60–69: `mountedRef` already tracks mount for setState guards
- Existing test: `commits a pending Drop after unmount when the undo window elapses`
  — extend or add sibling that asserts `onDismiss` was not called after unmount

## Implementation sketch

In `run`, after successful Drop:

```ts
if (action.action === "drop") {
  if (mountedRef.current) onDismiss?.();
} else {
  onMutated?.();
}
```

## Acceptance criteria

1. Confirm Drop on handle `web/x`, unmount ActionBar, advance `DROP_UNDO_MS`,
   Drop API still posts with `{ action: "drop", confirmed: true }`, and
   `onDismiss` is **not** called.
2. Confirm Drop and remain mounted through the undo window: Drop API posts and
   `onDismiss` is called once (existing behavior preserved).
3. Undo still cancels with neither API nor dismiss (existing coverage may stand).

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk npm run web:test -- --run src/features/task/ActionBar.test.tsx
      expected: all ActionBar tests pass, including new unmount-no-dismiss case
    - type: typecheck
      command: rtk npm run web:check
      expected: exit 0
  broader_checks: []
  reason: Behavior is entirely in ActionBar dismiss-after-Drop; focused unit test covers both mounted and unmounted outcomes.
```

## Stop conditions

- Fix seems to require App-level route awareness or history stack
- Surviving-unmount Drop commit regresses
- Change exceeds Allowed files
