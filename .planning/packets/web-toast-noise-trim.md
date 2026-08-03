PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Silence redundant Ajax web success toasts. Keep functional Drop-undo, errors (when not duplicated by an open sheet), and success toasts only when an action returns non-empty trimmed output.

## Scope

### Allowed

- `crates/ajax-web/web/src/features/task/ActionBar.tsx`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx`
- `crates/ajax-web/web/src/features/task/NewTaskSheet.tsx`
- `crates/ajax-web/web/src/features/task/NewTaskSheet.test.tsx`
- `crates/ajax-web/web/src/features/task/TestInDevPanel.tsx`
- `crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx`

### Forbidden

- `ResultPanel.tsx` / dismiss timing / CSS
- `SettingsView.tsx` (diagnostics + server-timeout toasts stay)
- `App.tsx` wiring
- Backend / Rust crates
- Unrelated refactors, renames, formatting sweeps
- Commits, pushes, branch changes

## Acceptance

1. **NewTaskSheet success**: after a successful `startTask`, does **not** call `onResult`. Still calls `onOpenTask` / `onClose` / `onCockpit` as today.
2. **NewTaskSheet API failure** (`!result.ok`): sets inline sheet error; does **not** call `onResult` (sheet stays open).
3. **NewTaskSheet network failure** (catch): sets inline sheet error; does **not** call `onResult`.
4. **ActionBar success with empty/null/whitespace-only `response.output`**: does **not** call `onResult` for the completion message.
5. **ActionBar success with non-empty trimmed `response.output`**: calls `onResult(\`${action.label} completed\`, output, false)`.
6. **ActionBar Drop**: still shows the undo toast `Dropping ${handle}…` with `onUndo`/`onCommit`. After the Drop API succeeds, does **not** call `onResult` again (no `"Drop completed"`), still calls `onDismiss` when mounted.
7. **ActionBar errors**: still call `onResult` with `isError: true` (unchanged).
8. **TestInDevPanel success**: does **not** call `onResult("Test in Dev started", …)`. Errors still call `onResult(..., true)`.

## Constraints

- Smallest edit that meets acceptance.
- Prefer updating/adding focused vitest cases in the existing test files.
- Do not invent a shared toast helper unless a one-liner guard is clearly worse; a local `const trimmed = output?.trim()` in ActionBar is fine.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/features/task/ActionBar.test.tsx src/features/task/NewTaskSheet.test.tsx src/features/task/TestInDevPanel.test.tsx
      expected: all listed tests pass
  broader_checks: []
  reason: Behavior is fully expressed by onResult call/non-call assertions in the three feature unit tests.
```

## Stop if

- Need to change Settings, ResultPanel, or App wiring to meet acceptance
- Patch exceeds ~400 changed lines
- Focused tests cannot pass without expanding Allowed scope
- Drop undo window behavior would regress

## Code anchors

- `NewTaskSheet.tsx` `submit`: lines ~127–139 — remove success `onResult("Task started"…)` and both failure `onResult` calls; keep `setError` / navigation.
- `ActionBar.tsx` `run` success branch ~100–107: after Drop success skip completion toast; else toast only when `result.response.output?.trim()` is non-empty.
- `ActionBar.tsx` `armDrop` ~140: keep undo toast unchanged.
- `TestInDevPanel.tsx` `deploy` ~43: remove success `onResult`; keep error path.

## Edit instructions

1. Add/adjust tests first so they fail under current behavior where practical, then implement.
2. ActionBar tests to add (names flexible):
   - Review success with no output → `onResult` not called for completion
   - Review success with output → `onResult` called with `"Review completed"`
   - After Drop undo window + successful API → `onResult` was called once for `"Dropping…"`, never for `"Drop completed"`
3. NewTaskSheet tests: successful start with `onResult` spy → not called; failed `ok: false` → sheet error text present, `onResult` not called; network throw → same.
4. TestInDevPanel: successful deploy → `onResult` not called with started message; failed start → still called with error.
