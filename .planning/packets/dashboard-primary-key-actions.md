PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal
Redesign dashboard task-row actions to composition B (primary-key lattice): the first safe action renders as a full-width primary pill; remaining safe actions render as a secondary pill row underneath. Soft Charcoal / Soft Steel Blue tokens unchanged. Drop stays off dashboard; Resume/Open stay filtered out.

## Allowed files
- crates/ajax-web/web/src/features/dashboard/Dashboard.tsx
- crates/ajax-web/web/src/features/dashboard/Dashboard.test.tsx
- crates/ajax-web/web/src/features/task/ActionBar.tsx
- crates/ajax-web/web/src/features/task/ActionBar.test.tsx (create only if needed; prefer extending Dashboard.test.tsx)
- crates/ajax-web/web/src/styles.css
- crates/ajax-web/web/dist/app.css (via npm run web:build only)
- crates/ajax-web/web/dist/app.js (via npm run web:build only)

## Forbidden changes
- No Rust / OperatorAction / supported_web_action changes
- No TaskDetail / TaskTerminal / transport / polling changes
- No DESIGN.md palette replacement; keep Soft Charcoal / Soft Steel Blue / existing `--accent` / `--warn` tokens
- Do not show Drop on dashboard; do not show Resume/Open buttons
- Do not change ActionBar default layout used by TaskDetail (opt-in prop only)
- No commits, pushes, branch changes

## Context evidence
- Desired: approved Impeccable comp B — full-width primary, secondary row below (`.impeccable/mocks/dashboard-comp-b-primary-key.png`).
- Dashboard already mounts `ActionBar` with `visibleTaskActions(...).filter(!destructive)` at `Dashboard.tsx` TaskRow actions block (~L107–116).
- `ActionBar` marks first non-destructive as `.primary` (`actionClassName`, index===0); remediation `fix-ci` gets `.remediation-action`.
- Dashboard CSS `.task-row-actions` currently horizontal scroll strip with 34px compact pills (`styles.css` ~L1245–1266) — this fights composition B.
- Tests already assert Fix CI + Repair buttons and no Drop/Resume (`Dashboard.test.tsx` L88–127).

## Code anchors
- `TaskRow` action render: `crates/ajax-web/web/src/features/dashboard/Dashboard.tsx` (~L59–119)
- `ActionBar` root `.action-row`: `crates/ajax-web/web/src/features/task/ActionBar.tsx` (~L159–176)
- Dashboard action CSS: `crates/ajax-web/web/src/styles.css` `.task-row-actions` block (~L1245–1266)
- Tests: `crates/ajax-web/web/src/features/dashboard/Dashboard.test.tsx` one-tap describe block (~L85–127)

## Test-first instructions
1. In `Dashboard.test.tsx`, add:
   - `primary action is full-width on the row` — within `web/a`, the `fix-ci` button has class `primary` and its parent action layout uses `data-layout="primary-key"` (or equivalent stable attribute you introduce); assert the primary button is the first `data-action` button and secondary `repair` is present.
   - Prefer asserting structure: `task-row-actions` contains a primary slot and a secondary row (`data-testid="task-row-actions-secondary"`) when >1 action.
2. Red command: `npx vitest run --config crates/ajax-web/web/vite.config.mts src/features/dashboard/Dashboard.test.tsx`
3. Confirm the new assertion fails before production edit.

## Edit instructions
1. Add opt-in `layout?: "default" | "primary-key"` to `ActionBar` (default `"default"`). When `primary-key`:
   - Wrap root with `data-layout="primary-key"` and class `action-row action-row--primary-key`.
   - Render actions[0] alone in `.action-primary-slot` (full width).
   - If more actions, render the rest in `.action-secondary-row` with `data-testid="task-row-actions-secondary"`.
   - Keep existing click/confirm/running/remediation class logic; only first action gets `.primary`.
2. Dashboard `TaskRow`: pass `layout="primary-key"` to `ActionBar`.
3. Add HTML direction contract comment at top of `Dashboard.tsx` (THESIS/OWN-WORLD/STORY/FIRST VIEWPORT/FORM/FINISH) ≤150 words reflecting composition B + seed `a3c11e37`.
4. CSS under `.task-row-actions` / `.action-row--primary-key`:
   - Column stack; primary button `width:100%`, `min-height:44px` (not 34px).
   - Secondary row: flex wrap, gap 8px; secondary pills may stay compact (~34px) but ≥44px touch if already global — prefer primary 44px, secondary match existing `.action` min-height 44px for a11y unless dashboard previously used 34px; for B use primary 44px full-width and secondary `min-height:36px` with padding matching comp, still tappable.
   - Remove horizontal-only scroll that hides secondary actions; allow wrap.
   - Do not invent new colors; remediation primary already uses `.action.remediation-action.primary`.
5. Run `npm run web:build` so `web/dist` matches src.
6. TaskDetail must keep calling ActionBar without `layout` (default).

## Verification commands
```bash
npx vitest run --config crates/ajax-web/web/vite.config.mts src/features/dashboard/Dashboard.test.tsx
npx vitest run --config crates/ajax-web/web/vite.config.mts src/features/task
npm run web:check
npm run web:lint
npm run web:build
```

## Acceptance criteria
- Multi-action dashboard row: first safe action full-width primary; others in secondary row; all still one tap.
- Single-action row: one full-width primary only; no empty secondary row.
- Drop never on dashboard; Resume never shown.
- TaskDetail ActionBar layout unchanged (default).
- Tests green; dist rebuilt.

## Stop conditions
- Need Rust action vocabulary changes
- Touching terminal / TaskDetail behavior beyond ActionBar default
- New color tokens or DESIGN.md rewrite
- Edits outside allowed files
