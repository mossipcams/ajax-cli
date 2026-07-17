# Compact Test in Dev under Task details

## Scope

- Move `TestInDevPanel` out of the task-detail action strip into the existing **Task details** `<details>` dropdown.
- Shrink the control: no card chrome, no explanatory note, no occupant metadata dump.
- Keep deploy / Open Dev / phase / error behavior.

## Non-goals

- No React migration slice S2 in this change.
- No API / deploy pipeline changes.
- No changes to frozen terminal modules or ActionBar.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] **Task 1 — failing tests first**
- [x] **Task 2 — move + compact**
- [x] **Task 3 — parent validation**

## Deviations

- Cursor delegate returned a nonconforming report envelope; parent accepted after review + re-validation.
- 2026-07-17: stashed while starting S2 after merging `origin/main` (S1 landed as #571).

## Validation results

- PASS — focused vitest 24/24; `web:check` clean (pre- and post-`origin/main` merge).
