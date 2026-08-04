# CI fix: File LOC — peel App.test.tsx

## Failure

PR #757 File LOC: `App.test.tsx` is 1232 lines (limit 1000).
Touching the file made a pre-existing oversized suite fail the changed-file gate.

## Fix

Peel polling/resume lifecycle tests into `App.polling.test.tsx` with shared
harness helpers in `appTestHarness.ts` (setHash, jsonResponse, beforeEach stubs).
Leave shell/layout/connection tests in `App.test.tsx`.

## Checklist

- [x] Extract polling suite to `App.polling.test.tsx`
- [x] `App.test.tsx` under 1000 (979)
- [x] `node scripts/check-file-loc.mjs` → 0 errors
- [x] Focused vitest App + App.polling — 51 passed

## Delegation

`Delegation decision: not delegated because mechanical peel smaller than work-order overhead`
