# Web dashboard redesign (v2, then v3 via impeccable)

Rebuild the dashboard route from the backend contract up. Task page code
(`features/task/*`) is kept as-is and reused.

## Delegation decision

`Delegation decision: not delegated because the user directed an in-session
design rebuild that requires visual iteration (Playwright screenshot loop) in the
same loop as the code; see memory build_dont_interview.` Recorded per AGENTS.md
§Delegation. Validation is run locally by me, not claimed from a delegate.

## Scope

- New `features/dashboard/Dashboard.tsx` (v2), designed only from the Rust
  contract (`slices/cockpit.rs`, `slices/actions.rs`, `output.rs`) — the v1 page
  layout was deliberately not used as a reference (explicit user instruction).
- Fold `RepoPanel.tsx` + `SystemPanel.tsx` into the v2 footer and delete them;
  no projected field is dropped.
- Replace the DASHBOARD / REPOSITORIES / SYSTEM sections of `styles.css`.
- Rewrite `Dashboard.test.tsx`; update the e2e selectors that named v1-only DOM.

## Non-goals

- No change to `App.tsx` props contract, routing, polling, or chrome.
- No change to `features/task/*` (TaskDetail, TaskTerminal, ActionBar,
  NewTaskSheet) beyond importing them.
- No Rust change. No new endpoint, no new DTO field.
- The four terminal-forcing parity gaps (agent output, send-text, interrupt,
  diff) stay blocked in Rust — not stubbed here.

## Backend contract this renders (source of truth)

`GET /api/cockpit` → `BrowserCockpitView`:

- `backend { authority, control_enabled, warning }`
- `repos.repos[] { name, path, active_tasks, attention_items, reviewable_tasks,
  cleanable_tasks }`
- `cards[] { id, qualified_handle, repo, title, status, status_explanation,
  attention, last_activity_unix_secs, actions[] }`
- `inbox.items[] { task_id, task_handle, reason, severity, action }`
- `actions[] = WebAction { action, label, destructive, confirmation_required,
  branch_adoption? }` — the whole capability list; dispatch is
  `POST /api/operations`, which returns a refreshed projection.

Rules carried over from architecture/memory: band membership is `attention`
(never re-derived); ordering inside a band is `sortCards`; never order actions by
`primary_action`; `Drop` never appears on the dashboard; `inbox.reason` is a
snake_case evidence label the browser must not translate — severity is used for
ranking only.

## Design (v2)

1. **Next card** — the highest-severity inbox task promoted to a lead card with
   its full safe action set. First time `cockpit.inbox` is used at all.
2. **One flat queue** instead of four band sections; each row carries its band as
   a tag. Order: needs-you → active → review → idle (Matt's inverted order),
   `sortCards` within.
3. **Filters** — attention chips (rendered only for non-empty bands) + a native
   `<select>` for repo (replaces the pill row; native picker = less furniture).
4. **System footer** — one `<details>`: backend authority/control/warning,
   connection, repo rows with their four counts.

## Tasks

- [x] Read backend slices + core output/ui_state; record the contract above.
- [x] Write v2 `Dashboard.tsx`.
- [x] Replace dashboard CSS.
- [x] Delete RepoPanel/SystemPanel + tests (data folded into the footer).
- [x] Rewrite `Dashboard.test.tsx` (queue order, next card, drop absent, repo
      filter, band filter, empty state, system footer).
- [x] Update e2e selectors that named v1-only DOM.
- [x] `npm run web:test -- --run` green.
- [x] `npm run web:check`, `web:lint`, `web:sg` green.
- [x] `pkill -f vite; CI=1 npm run web:smoke` green + screenshot.

## Design as built

1. **Next card** — highest-severity inbox item, raised (`--elev-2`), 2px tone top
   rule, full-bleed primary + natural-width secondaries. Only filled pill on the
   page. Excluded from the queue so it is never shown twice.
2. **One queue** — flat `.task-list`, band carried per row as an uppercase tone
   tag. Band order needs-you → active → review → idle; `sortCards` within, order
   held stable across polls.
3. **Filters** — attention chips (only for populated bands) + native `<select>`
   repo picker sharing one wrap row. No pill-per-repo.
4. **System `<details>`** at the tail (closed): authority, control, warning,
   per-repo counts, Diagnostics. Link state is NOT restated (the header owns it —
   it also collided with App.test.tsx's `findByText("connected")`).
5. Row actions are outlined even in slot 0, so the list stays quiet.

## Deviations

- `visibleTaskActions` hides `resume`/`open`; v2 keeps that (opening a task
  already dispatches resume) and adds an explicit "Open" affordance only as the
  row tap target, not a button.
- e2e `smoke.test.ts` "project filter" now drives the repo `<select>`; the
  assertion (only matching repo tasks visible) is unchanged.
- `visual.test.ts` row-padding/pill assertions retargeted to the v2 equivalents
  (chip fill, card surface, row padding) — same defect class, new selectors.

## Validation

- `npm run web:test -- --run` → 50 files, 494 tests, pass (24 in the new
  Dashboard.test.tsx).
- `npm run web:check` (tsc), `npm run web:lint` (eslint), `npm run web:sg`
  (ast-grep) → pass.
- `pkill -f vite; CI=1 npm run web:smoke` (Playwright mobile-webkit) → 105
  passed, 3 skipped, exit 0; 1 pre-existing flake in
  `terminal-behavior.test.ts:1400` ("interaction wrap hides scrollbar chrome",
  `page.goto` timeout on the task route, passed on retry, untouched by this
  change). The first run had flagged `layout-scroll.test.ts:333`, which probed the
  deleted `.system-settings`; retargeted to `.fleet-summary` (same assertion).
- No Rust files changed, so the cargo gates were not re-run. Checked that no
  Rust test asserts a dashboard class name (`install.rs` only mentions the legacy
  `new-task-row` id).
- Docs updated: `DESIGN.md` (overview, reading order, chips, signature section,
  flat-by-default exception) and
  `.impeccable/surfaces/ajax-web-web-src-features-dashboard-dashboard-tsx.md`.

---

## Round 2 — impeccable redesign (v3, 2026-07-30)

Matt: "use impeccable in the redesign." Ran the skill's Operate flow on the same
surface. PRODUCT.md + DESIGN.md exist and the Ajax Cockpit world is committed, so
this is *a whole surface inside an established world*: visual system fixed,
structure re-derived from the task.

- **Ask round substituted, disclosed:** no AskUserQuestion round — Matt has
  rejected interview rounds on UI work; built and screenshotted instead.
- **Concept seed:** `concept-seed.mjs --scope surface --mode operate` → key
  `cf0a0deb`, ASSIGNED INDEX 6 of my own grounded, resonance-ordered list:
  1 answer-first queue (= v2), 2 attention bands (= v1), 3 decision deck,
  4 activity river, 5 repo outline, **6 command roster**, 7 watchboard.
- **Staging challengers weighed** (dressed in the committed identity):
  tooling-by-zoom — loses on product clarity (hides tooling behind altitude);
  pivot-fan deck — loses on audience identification (no CLI operator thinks in
  fan decks, and side-by-side compare is not the job);
  **shaker-meeting-room — adopted as the staging** (emptied centre for the work,
  every tool at one ordained height = the fixed action rail; "an empty peg means
  work in progress" maps onto running tasks).
- **Comp round skipped:** no image generation in this session, so visualize.md's
  three rendered options could not run. Disclosed to the reviewer as a gap.

### As built (v3)

1. **Roster** — one 44px line per task (CLI glyph ▸ ? ! ✓ ·, mono handle, age),
   band rules with counts between groups. 6 tasks + rail + head fit one iPhone
   viewport where v2 fit ~2.5 cards.
2. **Peg rail** — fixed above the bottom nav (z 15, under nav 20 / toast 40 /
   sheet 45): selected handle, age, explicit `Open ›`, title, tone note, and
   every safe action with the intent filled and grown. The page's only filled
   pill and only fixed tool surface.
3. **Selection** — a row tap selects (it no longer navigates); the rail opens on
   Rust's severity-ordered inbox lead and falls back to it whenever a pinned task
   leaves the view. `Open ›` is now the deliberate way to reach the terminal.
4. **Head** — count + fleet shape as words (never a proportional gauge) + native
   repo `<select>`; the breakdown takes its own line under 430px.
5. **System `<details>`** unchanged from v2: authority, control, warning, repo
   counts, Diagnostics.
6. Removed with v2: the Next card, attention chips, per-row action pills, the
   eyebrow "NEXT" label (craft-floor bans kickers).

### Round-2 validation

- Batched inspection round (desktop-chromium + mobile-webkit, 3 states each) →
  fixed in one batch: translucent rail letting the roster's tail ghost through
  (now opaque `--paper-tint`), rail inset misaligned with the shell (now 20px),
  row separators inset by the row radius (now a full-width hairline on the `li`,
  radius only on the selected wash), head breakdown truncating behind the repo
  picker (now its own line under 430px). Confirm round clean.
- `detect.mjs` once over the changed files: 1 finding in new code
  (`rgba(0,0,0,0.32)` rail shadow outside DESIGN.md) → replaced with the
  documented chrome lift `rgba(0,0,0,0.28)`. The other 4 advisories are
  pre-existing values in the sheet/terminal blocks and were left alone.
- `npm run web:test -- --run` → 50 files / 494 tests pass (24 rewritten for the
  roster + rail).
- `web:check`, `web:lint`, `web:sg` → pass.
- e2e: `rosterRow()` helper added to `e2e/fixtures.ts` because the handle now
  appears in both the row and the rail (a bare `getByText` matched twice);
  `visual.test.ts` first test rewritten for roster/rail invariants plus a new
  "rail never covers the end of the roster" clearance test;
  `layout-scroll.test.ts` clearance test now measures the last control against
  the rail top, not just the nav.
- Finish review + verdict and the documenter's DESIGN.md pass: see the final
  response.

---

## Round 3 — v4 iOS-docked armed channel, restored (2026-07-30)

Uncommitted v4 work (Dashboard.tsx rework to `ArmedChannel` + primary-key
`ActionBar`, matching CSS, docs, and tests) was lost from the worktree before
it landed on a PR. `Dashboard.tsx` had already been re-applied when this round
started; this round restored the rest of v4 from the surviving code and specs
so the change is coherent again for review.

**Delegation decision:** `Delegation decision: not delegated because this is a
visual-iteration restoration tied directly to already-written v4 component
code (CSS values, test assertions, and doc language all had to match the
specific markup/classes in the restored `Dashboard.tsx`) — the same exception
AGENTS.md §Delegation and the Round 2 entry above record for this surface.`

### Restored in this round
- `styles.css` — DASHBOARD block rewritten for channel traces + the docked
  armed channel (opaque `--paper` floor, `.rail-inner` card carries the
  border/radius/shadow, `--text-display` title, primary-key column layout with
  full-width primary + secondaries); bottom-nav current-page treatment
  softened to an underline so it does not compete with the dock; `.fleet-footer`
  given bottom margin to clear the dock.
- `Dashboard.test.tsx` — clearance assertion updated to the new
  `RAIL_HEIGHT_FALLBACK` (240px).
- `e2e/visual.test.ts` — rail assertions retargeted from the outer dock's
  border to `.rail-inner`'s card border/radius, plus a `[data-layout=
  "primary-key"]` count check; fixed position, opaque background, and primary
  accent fill assertions kept.
- `DESIGN.md` — overview paragraph, Key Characteristics bullet, reading order,
  Flat-By-Default exception, and the Signature section renamed from "Next card
  + band-tagged queue" to "Armed channel + band-tagged traces"; the "Which
  task" inbox rule kept verbatim.
- `.impeccable/surfaces/ajax-web-web-src-features-dashboard-dashboard-tsx.md`
  rewritten for v4 (same frontmatter shape as v2/v3).

### Validation
- Not run as part of this restoration pass; re-run the full v2/v3 validation
  suite (`npm run web:test -- --run`, `web:check`, `web:lint`, `web:sg`,
  `CI=1 npm run web:smoke`) before shipping a PR.
