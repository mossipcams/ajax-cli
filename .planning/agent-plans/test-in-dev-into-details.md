# Move Test in Dev into Task details; remove Open Dev

## Scope

- Relocate the ajax-cli-only **Test in Dev** control from the always-visible task page into the **Task details** `<details>` disclosure.
- Delete the **Open Dev** pill and its `window.open` / hardcoded URL behavior from the web UI.

## Non-goals

- No backend/API changes (`DEV_OPEN_URL`, `open_url` field, deploy endpoints).
- No ActionBar / non-ajax-cli task changes.
- No redesign of Test in Dev deploy/polling behavior beyond removing Open Dev.

## Delegation decision

`Delegation decision: delegated via model-router` (frontend UI, >2 files → `cursor-delegate` / `composer-2.5`).

## Task checklist

- [x] Update failing placement test in `TaskDetail.test.tsx` (panel inside Task details group)
- [x] Update `TestInDevPanel` tests (drop Open Dev assertions)
- [x] Move `<TestInDevPanel>` into `<details className="meta-details">` in `TaskDetail.tsx`
- [x] Remove Open Dev button, `OPEN_URL`, and `openDev` from `TestInDevPanel.tsx`
- [x] Adjust `.test-in-dev` CSS only if nesting needs a margin tweak (skipped — existing spacing OK)
- [x] Parent review gate + focused `npm run web:test` validation

## Approval

Not required (Behavior Change, explicitly requested).

## Deviations

- Delegate returned non-schema report YAML; parent accepted via delta inspect + independent verify (same outcome as ACCEPT).
- `styles.css` untouched.

## Validation

```bash
npm run web:test -- crates/ajax-web/web/src/features/task/TaskDetail.test.tsx crates/ajax-web/web/src/features/task/TestInDevPanel.test.tsx
# exit 0 — Test Files 2 passed; Tests 24 passed
```

## Review Gate

`VERDICT: ACCEPT` — scope clean, Open Dev removed, placement inside Task details, focused tests green.