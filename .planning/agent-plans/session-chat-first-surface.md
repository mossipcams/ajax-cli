# Session chat-first surface

## Scope

Make orchestration chat read as a phone conversation, not a cockpit page with chat bolted on.

## Non-goals

- ACP / hub / pump behavior
- New task truth in the browser
- Flag-off terminal default path
- Color redesign of the whole Cockpit

## Delegation decision

`Delegation decision: delegated via model-router` → `USE_NATIVE` (caller=cursor, target=cursor/composer-2.5). Implement with Cursor native Composer path.

## Checklist

- [x] Hide global `cockpit-chrome` on session routes (session owns chrome)
- [x] Thread is the primary surface; task admin UI behind a sheet from header ⋯
- [x] Composer is a single chat bar (input + Send); Terminal/Cancel/Diff in sheet
- [x] Transport artifacts collapsed by default (title summary; body in details)
- [x] Soften bubbles / full-height layout within Ajax tokens
- [x] Update SessionChat tests + App session chrome expectation
- [x] Verify: `npm run web:test -- --run src/features/session/` and `npm run web:check`

## Approval

User said prior visual pass is still awful — authorized stronger chat-first redesign (not architecture change).

## Deviations

_(none)_

## Validation

- `npm run web:test -- --run src/features/session/` — pass (13 tests)
- `npm run web:check` — pass
- `npm run web:test -- --run src/app/App.test.tsx` — pass (45 tests)
- Parent polish: Stop stays on the composer bar (not buried in ⋯)
