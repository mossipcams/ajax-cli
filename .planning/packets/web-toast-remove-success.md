PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
DISPATCH_LEVEL: compact

## Task

Remove ActionBar success completion toasts entirely. Do not gate/silence them on output — delete the success `onResult` call path. Keep Drop undo toast and error toasts.

## Scope

### Allowed

- `crates/ajax-web/web/src/features/task/ActionBar.tsx`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx`

### Forbidden

- NewTaskSheet / TestInDevPanel / Settings / App / ResultPanel (already correct or out of scope)
- Commits, pushes, branch changes
- Unrelated refactors

## Acceptance

1. On successful `postOperation` (any action including Drop), ActionBar does **not** call `onResult` for a completion message like `"${label} completed"`.
2. Drop undo toast still calls `onResult(\`Dropping ${handle}…\`, null, false, { onUndo, onCommit })`.
3. Failed operations still call `onResult` with `isError: true` (message + optional output).
4. Success with non-empty output also does **not** toast (path removed, not gated).
5. Tests updated: remove the test that expects `"Review completed"` with output; assert successful Review (with or without output) never calls `onResult`; keep Drop undo / Drop-no-completed assertions.

## Constraints

- Smallest deletion. No new helpers.
- NONE otherwise.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/features/task/ActionBar.test.tsx
      expected: all ActionBar tests pass
  broader_checks: []
  reason: Behavior is onResult call/non-call assertions in ActionBar unit tests.
```

## Stop if

- Need files outside Allowed
- Drop undo or error toasts would regress
- Patch grows beyond ActionBar success-path deletion + tests

## Code anchors

- `ActionBar.tsx` ~100–104: delete the entire `if (action.action !== "drop") { const trimmed ... onResult completed }` block; leave cockpit / dismiss / mutate logic.
- `ActionBar.test.tsx`: remove `surfaces a completion toast when Review succeeds with non-empty output`; collapse empty/whitespace success tests into asserting `onResult` not called on success (including with output).
