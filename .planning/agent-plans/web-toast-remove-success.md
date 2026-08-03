# Plan: Remove ActionBar success toasts (not silence)

## Scope

Follow-up to `web-toast-noise-trim`: do not keep a conditional “maybe toast on
success” path. **Delete** ActionBar success completion toasts entirely.

Keep:
- Drop undo toast (`Dropping…`)
- ActionBar error toasts
- TestInDev / Settings error & clipboard toasts (already correct)

## Non-goals

- ResultPanel redesign
- Settings toast changes
- Broader onResult plumbing cleanup beyond ActionBar success path

## Delegation decision

`Delegation decision: delegated via model-router` — MiniMax empty diff; GLM
transport `Operation not permitted`; Cursor rejected `--follow-up`. Parent
implemented after delegate tools failed.

## Task checklist

### Task 1: Delete success completion toast path
- [x] Delete `onResult(\`${label} completed\`)` success branch in ActionBar
- [x] Replace “silence / output-gated” tests with “success never toasts”
- [ ] Parent verify + push to PR #745 (pending commit/push)

## Validation

```bash
npm run web:test -- --run src/features/task/ActionBar.test.tsx
```

Result: 14 passed.

## Deviations

- Implemented locally after three delegate failures (see Delegation decision).
