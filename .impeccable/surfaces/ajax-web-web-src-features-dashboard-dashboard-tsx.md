---
version: 1
slug: "ajax-web-web-src-features-dashboard-dashboard-tsx"
primary_target: "crates/ajax-web/web/src/features/dashboard/Dashboard.tsx"
related_targets: ["crates/ajax-web/web/src/features/task/ActionBar.tsx","crates/ajax-web/web/src/features/task/taskActions.ts","crates/ajax-web/web/src/styles.css"]
---

# Dashboard — action-first control panel

## Scope & mode
Operate. Web Cockpit home dashboard only (not task detail, not terminal).

## Job
Operator scans attention bands and runs the next safe intent in one tap without opening the terminal. Browser presents host/Rust truth; it never owns task records.

## Direction
Control-panel lattice (seed `a3c11e37`) + composition B primary-key. Soft Charcoal steps / Soft Steel Blue primaries / amber remediation — Ajax Cockpit world unchanged. Approved comp: `.impeccable/mocks/dashboard-comp-b-primary-key.png`. Built: `.impeccable/mocks/dashboard-built-mobile.png`, `.impeccable/mocks/dashboard-built-desktop.png`.

## Memorable moment
The full-width primary pill is the cell’s largest object — Fix CI / Review / Ship as intent, not a ledger title with actions tucked under it.

## First viewport (as shipped)
1. Project filter pill row (All + repos; fault = rose dot on pill, not a count).
2. Attention band label + count (uppercase tracked micro).
3. Task cell: status dot + one identity scan line (handle · TITLE · note · time).
4. Full-width primary action pill (`min-height: 44px`, Soft Steel Blue — or amber fill when remediation is primary).
5. Secondary pill row underneath (`min-height: 36px`, wrap).

## Lattice rules (built)
- **Bands (operator order):** Needs attention → Running now → Ready for action → Recent (idle as native `<details open>` disclosure).
- **Cell:** `.task-row` column; scan line demoted (`--text-micro` / muted) so actions own the cell.
- **Actions:** `ActionBar layout="primary-key"` — first visible action → primary slot; rest → secondary row. Remediation ids `fix-ci` / `resolve-merge-conflicts` wear amber (`--warn`) fill when primary, amber border when secondary.
- **Empty cell:** Running rows may show scan line only (no action strip) when host offers no safe intents.
- **Tap-through:** Scan line opens task detail; pills mutate via host ops.

## Element inventory (comp → build)
| Comp ingredient | Medium | Shipped |
| --- | --- | --- |
| Soft Charcoal band cards | CSS paper-tint / rule / radius-lg | Match |
| Identity scan line | Semantic button `.task-row-tap` | Match (handle+title+status+time; not ticket IDs) |
| Full-width primary key | `.action-primary-slot .action` | Match |
| Secondary Repair/Ship row | `.action-secondary-row` | Match |
| Amber remediation on Fix CI | `.remediation-action.primary` | Match (amber fill on primary Fix CI) |
| Two-column card grid (comp) | — | Adapted: single-column mobile-first stack (shell ≤560px); lattice thesis preserved |
| Segmented project filter | `.project-pill` | Match (All / repos) |
| Bottom Dashboard / New / Settings | App chrome | Unchanged; not owned by this artifact |

## Constraints
- Drop never on dashboard (destructive filtered in row).
- Resume / Open filtered via `visibleTaskActions` (open is tap-row / detail path).
- Band membership and action lists from Rust; no new OperatorAction; no browser task truth.
- Task detail / terminal layout untouched (`ActionBar` default layout there).

## Unresolved
None.
