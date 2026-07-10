# Web UI color + intuitiveness pass

## Scope

- Separate **selection** color (project filter, bottom-nav current page) from **attention** color (inbox, badges, waiting/needs-you).
- Selection → teal family; attention → mustard family (unchanged meaning).
- Active bottom-nav item gets a visible selected state.
- Rebuild `web/dist` after source CSS/Svelte changes.

## Non-goals

- No backend/API/projection/Rust changes.
- No terminal behavior, routing, or task lifecycle changes.
- No new dependencies or layout restructure.
- Do not change status tone semantics for waiting/error/running.

## Delegation decision

`Delegation decision: delegated via model-router` → `cursor-delegate` / `composer-2.5`

## Checklist

- [x] Add failing source-level tests for selection vs attention and bottom-nav active
- [x] Update `styles.css` bottom-nav `[aria-current]` styles
- [x] Update `TaskList.svelte` active pill → teal; keep badges/attention mustard
- [x] Optional row tint skipped (swipe-reveal bleed risk)
- [x] `npm run web:build`
- [x] `npm run web:test -- --run` (466 passed)
- [x] Parent review gate

## Approval

User requested UI refactor — proceed.

## Deviations

- Skipped optional `.task-row` tone-bg tint to avoid swipe-reveal bleed-through.

## Validation results

- Focused tests: pass
- Full web suite: 34 files / 466 tests pass
- Dist `app.css` contains teal active pill + mustard badges
