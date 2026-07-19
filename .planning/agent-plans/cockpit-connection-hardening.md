# Cockpit connection banner / polling hardening

## Scope

Per approved plan: quiet non-error connection banner states; trailing in-flight poll so Retry/resume cannot no-op; App + Playwright Retry recovery coverage.

## Non-goals

- Terminal WebSocket
- Polling cadence changes
- Removing `reconnecting` from `ConnectionState` type

## Delegation decision

`Delegation decision: delegated via model-router` (frontend UI, >2 files → `cursor-delegate` / `composer-2.5`).

## Task checklist

- [x] Quiet banner CSS + test (`checking` / `reconnecting` hidden like `connected`)
- [x] Trailing in-flight guard + update unit/hook tests
- [x] App Retry recovery test + Playwright connection Retry test
- [x] Parent review + validation commands

## Deviations

- Delegate report schema validation failed (runner); parent accepted via delta inspect + independent verify.
- Parent hardened `createInFlightGuard` finally-path so a dirty bit set after the loop exits still schedules one trailing run.
- Full-suite StrictMode test failed when every overlap trailed (double cockpit fetch). Trailing is now **opt-in** via `loadCockpit({ trailing: true })` / `run(fn, { trailing: true })`; Retry uses it, mount/interval/resume do not.

## Validation

```bash
npm run web:test -- …/cockpitPoll.test.ts …/useCockpitResource.test.tsx …/ConnectionStatus.test.tsx …/App.test.tsx
# exit 0 — 67 passed

npx playwright test …/actions.test.ts --grep 'connection'
# exit 0 — 6 passed (chromium + webkit)
```

## Review Gate

`VERDICT: ACCEPT`
