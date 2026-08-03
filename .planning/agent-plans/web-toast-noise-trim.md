# Plan: Trim noisy Ajax web success toasts

## Scope

Apply the agreed toast policy in Web Cockpit:

- Errors always (except when the sheet already shows the same error inline).
- Drop undo toast always (functional).
- Clipboard / server-timeout toasts stay (Settings — untouched).
- Silence redundant success toasts when the UI already proves the outcome.

## Non-goals

- Redesigning ResultPanel layout/timing.
- Changing ConnectionStatus.
- Changing Settings diagnostics / Test-in-Stable toasts.
- Architecture / backend changes.

## Policy (acceptance)

| Event | Toast? |
| --- | --- |
| New task success → open task | No |
| New task failure (sheet still open) | No — inline sheet error only |
| Action success with empty/null output | No |
| Action success with non-empty trimmed output | Yes (`{label} completed` + output) |
| Drop undo (`Dropping…`) | Yes |
| Drop API success after commit | No |
| Action / Test-in-Dev errors | Yes |
| Test in Dev started | No |

## Delegation decision

`Delegation decision: delegated via model-router` — `cursor-delegate` / `composer-2.5` (frontend UI, 6 files).

## Task checklist

### Task 1: Behavior tests + silence noisy toasts  [Behavior Change]
- [x] Packet + delegate implement
- [x] Parent review gate (ACCEPT — report schema FAILED but delta + verification OK)
- [x] Focused vitest pass (parent re-run: 43 passed)
- [x] Plan ledger updated

## Validation

```bash
npm run web:test -- --run src/features/task/ActionBar.test.tsx src/features/task/NewTaskSheet.test.tsx src/features/task/TestInDevPanel.test.tsx
```

Broader if focused green:

```bash
npm run web:test -- --run
```

## Deviations

- Delegate `DELEGATE_REPORT` failed structured-report schema extraction (`STATUS: FAILED` / empty CHANGED_FILES), but the worktree delta matched Allowed scope and transaction `run_verification` plus parent re-run both passed 43 tests. Accepted on evidence, not the broken report wrapper.

## Validation ledger

- Transaction verification: exit 0, 3 files / 43 tests passed
- Parent re-run: exit 0, 3 files / 43 tests passed
- Broader `npm run web:test -- --run`: skipped (focused coverage sufficient for this toast-call-site change)
- Packet: `.planning/packets/web-toast-noise-trim.md`
- Router run: `.planning/router-runs/web-toast-noise-trim/`
