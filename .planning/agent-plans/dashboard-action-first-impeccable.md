# Dashboard action-first redesign (Impeccable)

## Scope

- Redesign Web Cockpit **dashboard** composition so every safe one-tap action
  (Fix CI, Review, Ship, Repair, …) is the primary visual control on each task.
- Keep Ajax Cockpit visual world (Soft Charcoal / Soft Steel Blue / DESIGN.md).
- Mode: Operate. Structure seed: `a3c11e37` → assigned grounded candidate 4
  (**control-panel lattice**).

## Non-goals

- No new visual world / DESIGN.md palette replacement.
- No task detail / terminal / transport / OperatorAction vocabulary changes.
- Drop stays off the dashboard; Resume/Open stay hidden (open-task implies resume).
- No browser-owned task truth; bands/actions still from Rust projection.

## Direction (approved)

THESIS: The dashboard is a button lattice of safe operator intents, not a ledger
with actions tucked under titles.
OWN-WORLD: Soft Charcoal paper steps, Soft Steel Blue primary pills, amber
remediation, status by tone dots — Ajax Cockpit unchanged.
STORY: Operator sees what needs them and taps Fix CI / Review / Ship without
opening the terminal.
FIRST VIEWPORT: Band labels; each task cell has a full-width primary action
pill, then a secondary pill row; identity is a scan line above.
FORM: Control-panel lattice (grounded #4 of 7) + composition B primary-key;
seed `a3c11e37`; approved comp `.impeccable/mocks/dashboard-comp-b-primary-key.png`.
FINISH: unreviewed and undocumented is unfinished; this build ends with the
finish review, the verdict, and surface brief (world preserved — no DESIGN.md replace).

## Delegation decision

`Delegation decision: delegated via model-router` (cursor-delegate / composer-2.5).

## Tasks

- [x] T0 — User approved comp B
- [x] T1 — Write/update failing Dashboard tests for primary-key layout
- [x] T2 — Implement ActionBar `layout="primary-key"` + Dashboard + CSS
- [x] T3 — Rebuild dist; vitest + web:check/lint
- [x] T4 — Screenshot inspect + finish reviewer (pass) + surface brief

## Validation

| Command | Result |
| --- | --- |
| `npx vitest run … Dashboard.test.tsx` | pass — 24 |
| `npm run web:lint` | pass |
| `npm run web:build` | pass |
| Finish reviewer verdict | pass |

## Deviations

- Finish fix #3 (desktop multi-cell lattice) accepted as adaptation: shell max-width 560px.
- Amber filled remediation primary kept (OWN-WORLD + approved Fix CI treatment).
- Parent fixed delegate's broken single-action test (web/r had two actions) and eslint `closest` node-access.

## Validation

Focused vitest Dashboard + ActionBar; `npm run web:check`; `npm run web:lint`;
`cargo nextest run -p ajax-web` if Rust untouched skip; full verify before PR update.
