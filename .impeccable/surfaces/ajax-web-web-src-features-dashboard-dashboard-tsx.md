---
version: 1
slug: "ajax-web-web-src-features-dashboard-dashboard-tsx"
primary_target: "crates/ajax-web/web/src/features/dashboard/Dashboard.tsx"
related_targets: ["crates/ajax-web/web/src/features/task/ActionBar.tsx","crates/ajax-web/web/src/features/task/taskActions.ts","crates/ajax-web/web/src/styles.css"]
---

# Dashboard v4 — iOS-docked armed channel

## Scope & mode
Operate. Web Cockpit home dashboard only (not task detail, not terminal).

## Job
Answer "what now?" before "what is there?", then let the operator run any safe
intent one-thumbed from the iOS thumb zone without opening the terminal.
Browser presents host/Rust truth; it never owns task records.

## Direction
v4 (2026-07-30, seed `a355aa15`, grounded candidate 6 of 7 — channel focus)
keeps the Ajax Cockpit world unchanged (Soft Charcoal steps / Soft Steel Blue
primary / amber remediation) and reworks v3's fixed peg rail into a raised card
docked above the bottom nav / home indicator, in reach of a thumb resting at
the screen's bottom edge. The fleet above stays thin channel traces (glyph ·
handle · age) with no per-row action — the armed channel is the page's only
control surface and only filled pill. Built:
`.impeccable/mocks/dashboard-v4-mobile.png`,
`.impeccable/mocks/dashboard-v4-mobile-selected.png`,
`.impeccable/mocks/dashboard-v4-desktop.png`. Comp round:
`dashboard-v4-comp-a-lead-top.png` (adopted card language + bottom placement),
`dashboard-v4-comp-b-strip-stage.png`, `dashboard-v4-comp-c-mid-lock.png`.

## Memorable moment
The armed channel riding in the dock: a raised card (`--elev-2`, 2px `--tone`
top rule, `--radius-lg`) sitting directly above the bottom nav, its title at
`--text-display` weight, primary pill full-bleed and every secondary
full-width beneath it — nothing to aim sideways for with a thumb.

## First viewport (as shipped)
1. **Head** — count · fleet shape as words (never a gauge) · native `<select>`
   repo picker; the shape drops to its own line under 430px.
2. **Channel traces** — one flat `.task-list`; each row is glyph · handle ·
   age only (no title, no note, no action). Band rules (Needs you / Running /
   Ready / Recent) tag groups with a count.
3. **System** — a `<details>` at the tail, closed at rest: backend authority,
   control state, warning, per-repo counts, Diagnostics. Kept clear of the
   docked channel by `.fleet-footer`'s bottom margin.
4. **Armed channel** — fixed, docked above the bottom nav: handle · age ·
   `Open ›`, title, tone note, full-bleed primary + full-width secondaries
   (`ActionBar layout="primary-key"`).

## Rules (built)
- **Which task leads:** `cockpit.inbox` is severity-ordered in Rust
  (`commands/projection.rs`); the browser takes the first entry still in view
  and never ranks severity itself. `inbox.reason` is an evidence label — never
  rendered.
- **Trace order:** band rank Needs you → Running → Ready → Recent (Matt's
  operator hierarchy), `sortCards` within a band, order held stable across
  polls so rows do not move under a thumb mid-tap.
- **One filled pill per page:** the armed channel's primary slot is the only
  fill on the dashboard; every trace row and every channel secondary stays
  outlined or unfilled.
- **Selection vs. open:** a row tap arms that task's channel (pins selection);
  it does not navigate. `Open ›` inside the channel is the deliberate route to
  task detail and the terminal.
- **Quiet running:** a running row/channel with no output past
  `QUIET_THRESHOLD_SECS` reads `Stale Nm — no output` in amber and its glyph
  stops pulsing.
- **Link state is not restated:** the header's ConnectionStatus owns it; the
  System footer only tints its summary dot.
- **Docked, not overlapping:** the roster reserves exactly the channel's
  measured height (`ResizeObserver`, fallback `RAIL_HEIGHT_FALLBACK = 240`) at
  its tail via `.rail-clearance`, so the fixed dock never covers the last row.

## Constraints
- Drop never on the dashboard (destructive filtered from channel actions).
- Resume/Open filtered via `visibleTaskActions` (opening a task resumes it;
  `Open ›` is the intentional exception, not a listed action).
- Band membership, band order and action lists come from Rust; no new
  `OperatorAction`; no browser task truth.
- Task detail / terminal untouched (`ActionBar` default layout there).

## Unresolved
Still blocked in Rust, not stubbed here: PR entities, worktree create/cleanup,
session reconnect, interrupt/stop, `diff_task_plan` wiring, and web `Start`
(`slices/operate.rs` rejects it).
