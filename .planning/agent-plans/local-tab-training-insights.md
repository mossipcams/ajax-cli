# Local tab trim + Training page run insights

Approval: requested directly by Matt on 2026-10-29 ("On the local tab make sure the
page header says local not training. Keep only the Training time and step progress
for the training feature on that page. Update the training page to have useful
information and 3d render visuals if they help"). Immediate implementation requested.

## Scope

1. Local modal (`TrainingModal`, opened by the bottom-nav Local button)
   - Header reads "Local".
   - Training content is reduced to a read-only line while a run is active: run
     label, elapsed time, and step progress (bar + "Step N / M").
   - Removed from the modal: Jobs section, Stop/Train buttons, generation hero,
     idle/runtime hero, "finished" line. Model profiles and serving controls stay.
   - Link to the full Training page stays.
2. Training page (`#/training`) run hero
   - Stat tiles: progress, elapsed, finish time, speed (steps/min).
   - Loss tile: current loss, change since the first logged step, sparkline over
     the loss values parsed from `run.log_tail`.

## Non-goals

- No 3D visuals. The data is one value over time plus single numbers, and 3D
  distorts both. Recorded as a deliberate decision.
- No backend or API changes; everything derives from existing `/api/training/*` fields.
- No new dependencies.

## Tasks

- [x] Slice 1: Local modal trim, modal tests rewritten for the removed controls, `web-cockpit.md` updated
- [x] Slice 2: Training page stat tiles + loss sparkline, with model-helper tests
- [x] Rebuild `dist`, update CSS BASELINE if needed, full web gate
- [x] Simulator screenshots of the Local modal (running/idle) and the Training page hero
- [x] PR on a new `feat/web/...` branch (PR #1520 is already merged)

## Deviations

- Modal tests that pin the removed controls (Stop, Jobs buttons, generation and
  idle heroes) are deleted or rewritten onto the remaining serve/switch mutations.
  This is a direct consequence of the requested removal, not a weakening.

## Validation

- `npm run web:test`, `npm run web:check`, `npm run web:lint`, `npm run web:build`
- Web E2E runs in CI (WebKit cannot start inside the agent sandbox).
- `npm run web:test`: 1887 passed, 9 skipped. `npm run web:check`, `npm run web:lint` and `npm run web:build` all pass (2026-10-29).
- Simulator (iPhone 17 Pro, mocked running and idle data): Local modal shows the "Local" header, the run label, elapsed time and step progress, and nothing when idle. Training page shows the Run stats tiles and the loss trend.
