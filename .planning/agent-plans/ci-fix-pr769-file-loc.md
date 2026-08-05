# CI fix: File LOC — peel App sheet tests

## Failure

PR #769 File LOC: `App.test.tsx` is 1045 lines (limit 1000).

## Fix

Peel the new-task sheet regression tests into `App.sheet.test.tsx`.

## Delegation decision

`Delegation decision: not delegated because mechanical peel smaller than work-order overhead`

## Checklist

- [ ] Extract sheet suite to `App.sheet.test.tsx`
- [ ] `App.test.tsx` under 1000
- [ ] Focused vitest pass
- [ ] Commit + push
