---
version: 1
slug: "ajax-web-web-src-features-dashboard-dashboard-tsx"
primary_target: "crates/ajax-web/web/src/features/dashboard/Dashboard.tsx"
related_targets: ["crates/ajax-web/web/src/features/task/ActionBar.tsx","crates/ajax-web/web/src/features/task/taskActions.ts","crates/ajax-web/web/src/styles.css"]
---

# Dashboard v2 — Next card + band-tagged queue

## Scope & mode
Operate. Web Cockpit home dashboard only (not task detail, not terminal).

## Job
Answer "what now?" before "what is there?", then let the operator run any safe
intent in one tap without opening the terminal. Browser presents host/Rust truth;
it never owns task records.

## Direction
v2 (2026-07-29) was designed from the Rust contract alone — `slices/cockpit.rs`,
`slices/actions.rs`, `core/output.rs` — not from the v1 page. Ajax Cockpit world
unchanged (Soft Charcoal steps / Soft Steel Blue primary / amber remediation).
v1's four stacked band sections, project-pill row, `RepoPanel` and `SystemPanel`
are gone; every field they projected is still rendered, in fewer places. Built:
`.impeccable/mocks/dashboard-v2-mobile.png` (mobile-webkit, iPhone 15 Pro). The v1
comp/build shots in that folder (`dashboard-comp-b-primary-key.png`,
`dashboard-built-*.png`) are history, not the current target.

## Memorable moment
The Next card: the host's highest-severity attention item raised off the page
(`--elev-2`, 2px `--tone` top rule) with a full-bleed primary intent. It is the
page's only filled pill and its only raised surface.

## First viewport (as shipped)
1. **Next card** — NEXT label, dot · handle · time, title (`--text-heading`),
   tone note, full-bleed primary (`min-height: 46px`) + natural-width secondaries.
2. **Filters** — attention chips (`All n` plus one chip per populated band, counts
   inline) sharing a wrap row with a native `<select>` repo picker.
3. **Queue** — one `.task-list`; each row: dot + handle · uppercase band tag in
   `--tone` · time, then title, note, then every safe action as an outlined pill.
4. **System** — a `<details>` at the tail, closed at rest: backend authority,
   control state, warning, per-repo counts, Diagnostics.

## Rules (built)
- **Which task leads:** `cockpit.inbox` is severity-ordered in Rust
  (`commands/projection.rs`); the browser takes the first entry still in view and
  never ranks severity itself. `inbox.reason` is an evidence label — never rendered.
- **Queue order:** band rank Needs you → Running → Ready → Recent (Matt's
  operator hierarchy), `sortCards` within a band, order held stable across polls
  so rows do not move under a thumb.
- **One filled pill per page:** `.task-row-actions .action.primary` is overridden
  to outline + accent ink, so the Next card keeps the only fill. Remediation ids
  (`fix-ci`, `resolve-merge-conflicts`) still fill amber in the Next card's
  primary slot.
- **Quiet running:** a running row with no output past `QUIET_THRESHOLD_SECS`
  reads `Stale Nm — no output` in amber and its dot stops pulsing.
- **Link state is not restated:** the header's ConnectionStatus owns it; the
  footer only tints its summary dot.
- **Tap-through:** the identity block opens task detail; pills mutate via host
  ops and replace the projection with the response.

## Constraints
- Drop never on the dashboard (destructive filtered per row).
- Resume/Open filtered via `visibleTaskActions` (opening a task resumes it).
- Band membership, band order and action lists come from Rust; no new
  `OperatorAction`; no browser task truth.
- Task detail / terminal untouched (`ActionBar` default layout there).

## Unresolved
Still blocked in Rust, not stubbed here: PR entities, worktree create/cleanup,
session reconnect, interrupt/stop, `diff_task_plan` wiring, and web `Start`
(`slices/operate.rs` rejects it).
