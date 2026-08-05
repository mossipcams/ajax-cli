# Plan: Ajax web + backend error handling / recovery / toasts

Mode: Behavior Change.
First migrate surface: **A) Operate** (plan default; no user override).

## Scope

- Additive backend error JSON `code` on failure responses
- Frontend parse + shared recovery copy helper
- Operate surface (`taskMutations` / ActionBar path) uses codes for toast + telemetry
- Document contract in `docs/architecture/web-cockpit.md`

## Non-goals

- Redesign ResultPanel layout / success-toast revival
- Browser-owned policy or second task truth
- Migrating Diff/Terminal/Speech/Settings in this PR (inventory only; follow-ups)
- Large `WebError` enum hierarchy

## Delegation decision

`Delegation decision: delegated via model-router` — sequential packets:
1. Backend contract (`cursor-delegate` / `composer-2.5`) — ACCEPT (report wrapper FAILED; parent gated via diff + re-verify)
2. Frontend map + operate migrate + docs (`cursor-delegate` / `composer-2.5`) — ACCEPT (same); parent tightened empty-message fallbacks in `errorRecovery.ts`

## Phase 0 inventory

| Surface | Status today | JSON today | UI today | Desired recovery |
| --- | --- | --- | --- | --- |
| Operate `/api/operations` | 409 | `{ok,error,state_changed,cockpit}` (+`code`) | ResultPanel error toast | done this PR |
| Start `/api/tasks` | 409 | same | NewTaskSheet inline / toast policy | contract parse done; sheet keeps inline |
| Optimistic conflict | 409 | `{ok,error,code:conflict}` | toast/conflict | done |
| WebError generic | 500 | `{ok,error,code}` | connection / toast | helpers done |
| Session missing | 401 | `{ok,error,code:stale_session}` | renew / stale-session | done |
| Task not found (detail/diff/live) | 404 | `{ok,error}` | TaskLoadError / Diff / WS | follow-up |
| Tmux missing | 409 | `{ok,error}` | terminal attach fail | follow-up |
| Diff unobservable | 502 | `{ok,error}` | Diff hard error | follow-up |
| Cockpit load network | n/a | ApiError network | ConnectionStatus Retry | keep |
| Speech STT | WS events | stt.error | Mic recoverable | leave |
| Settings restart/push | various | `{ok,error}` | Settings toasts | leave |

## Task checklist

- [x] Phase 0 inventory (this file)
- [x] Phase 1 backend contract + tests
- [x] Phase 2 ApiError code + recovery helper
- [x] Phase 3 operate migrate (`taskMutations`)
- [x] Docs in web-cockpit.md + validation

## Validation

```bash
cargo nextest run -p ajax-web
# 247 passed

cargo nextest run -p ajax-web -p ajax-cli -- operate_error_code operation_endpoint_returns_refreshed_cockpit_on_bridge_error start_task_endpoint_returns_refreshed_cockpit_on_bridge_error
# 3 passed

npm run web:test -- --run src/shared/lib/errorRecovery.test.ts src/shared/lib/api.test.ts src/features/task/ActionBar.test.tsx
# 60 passed

npm run web:check
# exit 0
```

## Deviations

- Delegate `DELEGATE_REPORT` schema extraction failed both rounds (`MISSING_STRUCTURED_REPORT`); accepted on parent-reviewed delta + independent verification (same pattern as prior toast plans).
- Parent post-gate: empty-message fallbacks for conflict/task_not_found/confirmation_required/unsupported/unknown in `errorRecovery.ts`.

## Validation ledger

- Backend focused + full `ajax-web` nextest: pass
- Frontend focused vitest 60: pass
- `npm run web:check`: pass
- Packets: `.planning/packets/web-error-recovery-backend.md`, `.planning/packets/web-error-recovery-frontend.md`
- Router runs: `.planning/router-runs/web-error-recovery-backend/`, `...-frontend/`
