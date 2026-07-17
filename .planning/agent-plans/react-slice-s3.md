# React migration S3 — Settings + ResultPanel

Source of truth: `docs/react-migration-plan.md` §S3.

Status: in progress on branch `ajax/react-s3` (from `origin/main` after S2 #573 merged).

Delegation decision: delegated via model-router.

## Scope

- Port `ResultPanel` and `SettingsView` to React with RTL ports.
- Island-swap both consumers in `App.svelte`.
- Delete Svelte components + tests in the same slice.
- Repoint `legacyTerminalRemoval.test.ts` path `SettingsView.svelte` → `SettingsView.tsx` (same symbols).
- Move SettingsView scoped CSS into `styles.css` verbatim.
- Bespoke restart confirm (no shadcn unless class-parity is free — default bespoke).

## Non-goals

- No api.ts / session / restart polling helper rewrites.
- No TaskDetail / TestInDev / terminal / sheet / shell migration.
- Compact Test-in-Dev UX remains stashed (`stash@{0}`) — separate from S3.

## Task checklist

- [x] **Task 1 — ResultPanel React port + App island swap.**
  - Test first: port `ResultPanel.test.ts` → RTL `.test.tsx`.
  - Implement: `ResultPanel.tsx`; swap App ResultPanel → ReactIsland; delete Svelte ResultPanel + test.
  - Verify: focused RTL + App tests + settings/actions e2e subset if any touch the panel.
  - Result: `ResultPanel.test.tsx` (8 tests) + `App.test.ts` green; `web:check` green.

- [x] **Task 2 — SettingsView React port + App island swap.**
  - Test first: port `SettingsView.test.ts` → RTL `.test.tsx` (restart confirm + poll mocks).
  - Implement: `SettingsView.tsx`; CSS → `styles.css`; App swap; delete Svelte; repoint legacyTerminalRemoval path.
  - Verify: focused RTL + `web:check` + settings e2e from `actions.test.ts`.
  - Result: `SettingsView.test.tsx` (9 tests) + guard + `App.test.ts` green; `web:check` + settings e2e green.

- [x] **Task 3 — full automated validation.**
  - PASS — parent 2026-07-17: web:test / smoke / ajax-web / verify green; serviceWorker 0.

- [x] **Task 4 — on-device + PR.**
  - Matt: restart from phone, diagnostics copy, result Undo — **validated 2026-07-17**.
  - PR: https://github.com/mossipcams/ajax-cli/pull/575 (opened 2026-07-17). CI/review/merge/baseline-restore still pending.

## Escalate instead of guessing

- Any `api.ts` session-renewal change.
- Any e2e weakening.
- Restart poll helper reimplementation.

## Deviations

_(none yet)_

## Validation results

- `npm run web:test -- --run crates/ajax-web/web/src/components/ResultPanel.test.tsx crates/ajax-web/web/src/components/App.test.ts` — exit 0 (42 tests)
- `npm run web:check` — exit 0
- `npm run web:test -- --run crates/ajax-web/web/src/components/SettingsView.test.tsx crates/ajax-web/web/src/legacyTerminalRemoval.test.ts crates/ajax-web/web/src/components/App.test.ts` — exit 0 (44 tests)
- `npm run web:check` — exit 0
- `npm run web:smoke -- crates/ajax-web/web/e2e/actions.test.ts` — exit 0 (24 passed)
