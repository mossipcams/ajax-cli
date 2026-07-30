# Today’s dashboard tech debt

## Scope

Behavior-preserving cleanup of ~35–40 lines of stragglers left by the dashboard
rebuild (#696/#697). ACP leftovers are out of scope (already clean after #701).

## Non-goals

- No ActionBar primary-key layout collapse
- No ACP re-litigation
- No `dev-web-restart.sh` hook-logic changes
- No commit/push unless asked

## Delegation decision

`Delegation decision: not delegated because` the cut list is smaller than a
useful work order (orphan CSS, one unused attr, one dead test, one style move).

## Tasks

- [x] T1 — Delete orphan `.settings-link` CSS; drop dead `.project-nav` scrollbar props; add `.action-row` base rule
- [x] T2 — Drop unused `data-tier` from Dashboard; remove `actionRowStyle` from ActionBar
- [x] T3 — Delete swipe source-grep test; fix stale TaskList/swipe/Open comments
- [x] T4 — Validate: focused vitest + web:check/lint; rebuild `dist`

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/features/dashboard src/features/task/ActionBar.test.tsx
npm run web:check && npm run web:lint
npm run web:build
```

## Deviations

- Rebuilt committed `dist` to match recent dashboard PR practice (CSS/TS changes).

## Validation results

| Command | Result |
| --- | --- |
| `npx vitest run src/features/dashboard src/features/task/ActionBar.test.tsx` | pass — 41 |
| `npm run web:check` | pass |
| `npm run web:lint` | pass |
| `npm run web:build` | pass |
