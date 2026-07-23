# Web dashboard redesign — decision queue

Full redesign of the Web Cockpit dashboard page (`#/`, `#/project/:name`) from a
repo-grouped task inventory into an urgency-ordered decision queue.

Approved by Matt 2026-07-22: **layout = decision queue**, **scope = frontend only**
(no Rust changes), **keep the project pill band** (no test deletions).

The pill-band decision cascaded: once "no test deletions" is the constraint, the
bottom-nav `Dashboard` button also stays (`App.test.tsx:390` pins it with
`aria-current="page"` on the dashboard route), and the IDLE band ships
default-expanded (a collapsed `<details>` hides its rows from the accessibility
tree, breaking four existing row queries). Net test deletions: **zero**.

## Why

`PRODUCT.md` states the dashboard exists to answer "what needs input, what is ready
to review, and which action is safe next". The current page answers "what tasks
exist, grouped by repo". Concrete defects found while reading the code:

- `card.title` is in the payload and never rendered (`TaskList.tsx` renders only
  `qualified_handle`). The human-readable name — the one thing that makes a task
  recognizable on a phone — is discarded.
- Rows print `repo/slug` inside a repo-titled group under an active repo filter.
  Identity is stated three times.
- Status is stated three times (dot, label, explanation); the action is stated zero
  times — it is behind an undiscoverable swipe that reveals exactly one button.
  On a "Needs you" row the action *is* the content.
- The project pill band duplicates the repo group headers.
- The bottom nav's first button is a no-op on the page you are already on.

## Scope

In scope:

- `crates/ajax-web/web/src/features/task/TaskList.tsx` — rewrite of the section
  structure below the pill band.
- `crates/ajax-web/web/src/styles.css` — row/hero/band rules; `.task-group*`
  replaced by `.task-band*`.
- `crates/ajax-web/web/src/app/App.tsx` — header status line only.

Non-goals:

- **No Rust changes.** No `/api/next`, no `agent_activity` on `BrowserTaskCard`,
  no stuck detector. Explicitly deferred (see Deferred below).
- No change to `TaskDetail`, `TaskTerminal`, `NewTaskSheet`, `SettingsView`,
  `ActionBar`.
- **No change to the project pill band** (`.project-nav`, `.project-pill`,
  `.pill-badge`) — markup, CSS, and its five tests stay exactly as they are.
- **No change to `.bottom-nav`.** It is load-bearing for the mobile terminal
  keyboard-band contract (probed by `e2e/terminal-behavior.test.ts` at 488, 540,
  812, 826, 865, 1075, 2025 and `e2e/layout-scroll.test.ts:249`), and
  `App.test.tsx:390` pins the `Dashboard` button's `aria-current` on the dashboard
  route.
- No change to `.task-row` remaining a single `<button>` element — `useSwipeReveal`
  types on `HTMLButtonElement` and `e2e/swipe-reveal.test.ts` selects
  `.task-row[data-handle=…]`.

## Design

### Header (`App.tsx`)

Status line becomes attention-first instead of a raw count:

- inbox non-empty → `N need you`
- else running > 0 → `N running`
- else → `N tasks` (current behaviour)

Live dot, Settings link, update banner unchanged.

### Page body (`TaskList.tsx`)

Project pill band (unchanged), then:

**Region `Needs you`** (`aria-label` preserved), rendered when the inbox is
non-empty:

1. **The hero** — `inbox[0]` rendered as a `.task-row.is-inbox.is-next`: same row
   button (so `data-handle`, tap-to-open and the role query all survive) with
   `title` promoted to a heading line, then a real `ActionBar` as a sibling below
   it carrying that card's `visibleTaskActions`.
   The browser only *selects* index 0 from a list Rust already sorted by severity —
   it does not author priority, so the `types.ts` contract holds.
2. **The rest** — `inbox[1..]` as rows with the same **inline** `ActionBar`.

Rule: **inbox rows show actions inline; calm rows use swipe.** No row ever renders
both, so an action label never appears twice in the tree.

**Region `Tasks`** (`aria-label` preserved) holds two bands in place of the old
per-repo groups:

3. **Active · N** — non-inbox cards with `status !== "idle"`, ordered by the
   existing `sortCards`.
4. **Idle · N** — `status === "idle"`, inside a native `<details>`.
   **Ships `open`.** A closed `<details>` removes its rows from the accessibility
   tree, which would break `TaskList.test.tsx` lines 69, 88, 159 and 216. Flipping
   the default to collapsed is a one-word change plus four test-query updates —
   Matt's call once he has seen it.

Row shape (all bands): tone dot · `title` (primary) · `qualified_handle` (muted
secondary) · `status_explanation` · status label + relative time · chevron.
`title` falls back to `qualified_handle` when absent.

`qualified_handle` stays rendered rather than being reduced to the bare slug: it
keeps repo identity visible as metadata and keeps the `/web\/a/`-style role
queries plus `e2e/smoke.test.ts` and `e2e/visual.test.ts` text assertions on
`web/fix-login` valid.

Empty-state copy unchanged.

### Deletions

- `.task-group`, `.task-group-title` per-repo grouping — markup and CSS, replaced
  by `.task-band` / `.task-band-title`. No test references either.

## Test impact

**No existing assertion is deleted, weakened, or rewritten.** The section
structure was chosen to satisfy the current suite:

| Existing pin | Why it survives |
| --- | --- |
| `TaskList.test.tsx:75-78` — `web/a` is a `.task-row.is-inbox` with `data-handle` | the hero *is* a task row, not a bespoke card |
| `TaskList.test.tsx:81-82` — no `Open` / `Resume` text | no separate Open control; `visibleTaskActions` already strips `resume` |
| `:96-97` — regions named `Needs you` and `Tasks` | both `aria-label`s preserved; Active/Idle are bands *inside* `Tasks` |
| `:69, 88, 159, 216` — idle row `api/c` is queryable | `<details open>` keeps it in the accessibility tree |
| `:107-137, 228-244` — project pills | band untouched |
| `App.test.tsx:390-405` — bottom-nav `aria-current` | nav untouched |
| `e2e` `.bottom-nav` / `.task-row` / `.project-pill` probes | all three selectors preserved |

New tests to add:

- `card.title` is rendered as the primary line on every row.
- The hero renders `inbox[0]` with a clickable primary action, no swipe needed.
- No hero when the inbox is empty.
- Inbox rows expose their action inline; calm rows do not (swipe only), so no
  action label is duplicated.
- Active band excludes idle cards and excludes inbox handles.
- Idle band lists only idle cards and is a `<details>`.
- Attention-first status line: `need you` → `running` → `tasks`.

## Tasks

- [x] 1. Tests first: add the new `TaskList.test.tsx` cases against the redesigned
      markup (red).
- [x] 2. Rewrite `TaskList.tsx`: hero + inline-action inbox rows, Active/Idle
      bands.
- [x] 3. `styles.css`: hero/band/title rules; `.task-group*` → `.task-band*`.
- [x] 4. `App.tsx`: attention-first status line.
- [x] 5. Verify.

## Deferred (needs Rust — out of scope by decision)

- Expose `ajax-core`'s `NextResponse` as `/api/next` or a `next` field on
  `BrowserCockpitView`. Selecting `inbox[0]` gives the same answer today but
  re-states a ranking Rust owns.
- `agent_activity` on `BrowserTaskCard` so ACTIVE rows can say what the agent is
  doing, not just that it is running.
- A "stuck" signal (running with no activity past a threshold). This is the one
  that makes `PRODUCT.md`'s "never miss a stuck task" actually true; without it a
  wedged task is visually identical to a healthy one.

## Validation

Per the migration gate — this slice touches UI and layout, so Playwright is
mandatory, not optional:

```
cd crates/ajax-web/web
npm run lint
npm run typecheck
npm run test            # vitest
npm run web:smoke       # Playwright, mobile-webkit — REQUIRED for layout changes
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
```

## Deviations

1. **`e2e/fixtures.ts` + `e2e/swipe-reveal.test.ts` retargeted.** The swipe test
   drove `web/fix-login`, which is the fixture's *inbox* card — exactly the row
   class that now renders actions inline instead of behind a gesture. The fixture
   had no calm card carrying an action, so `api/add-auth` gained a `review`
   action and `TARGET_HANDLE` moved to it. **The assertion body is unchanged**;
   only the row it points at moved. Coverage of the gesture is fully retained.

2. **Two visual defects caught by screenshot, not by tests.** First pass put the
   `ActionBar` *outside* the row card: the lead entry ballooned with dead space,
   and the second entry's Review/Drop floated orphaned between two cards. Fixed by
   making `.inbox-entry` a single bounded card with the action strip inside it on a
   `border-top` hairline — which is what DESIGN.md's "interact panel is a flat
   hairline strip" already prescribed. The whole e2e suite passed in the broken
   state; only the screenshot showed it.

3. **Added a regression test for the class of bug the suite could not see.**
   `e2e/visual.test.ts` was written (see its header) to catch "the stylesheet
   stopped applying" — it asserts computed styles *per element* and never
   relationships *between* elements, so correctly-styled controls rendering
   detached from their card were invisible to it.

   New test: `dashboard action groups sit on a card, not on the page background`.
   For every `.action-row` on the dashboard it walks up to the nearest ancestor
   that actually paints (background or border) and asserts that surface lies
   *inside* the route outlet. A properly parented group finds its card in 1–3
   levels; the orphaned one escaped the outlet entirely and only stopped at the
   app root's page paper — which is exactly what "floating on the background"
   means. No magic depth constant, no pixel baseline.

   Pixel snapshots were rejected deliberately: CI runs WebKit on `ubuntu-latest`
   while development is macOS, and the design's font stack (Avenir Next /
   Helvetica Neue) does not exist on Linux, so baselines could never agree.

   **Verified by reverting the fix**: with the broken CSS restored the test fails
   with `action group "Review" paints no card of its own — the nearest surface is
   DIV., outside the route outlet`; with the fix it passes. The test needs a
   two-inbox-item fixture, because the single-item default only renders the lead
   entry, which kept its border even when broken.

   Note: one intermediate run of this investigation gave a false "pass" because
   the reused Vite dev server had died and was serving a degraded page. When an
   e2e result is surprising, restart the dev server before trusting it.

## Results

- `web:test` — 433 passed / 46 files (429 before; 4 new cases).
- `web:smoke` (mobile-webkit) — 98 passed, 0 failed, 3 skipped (skips pre-existing).
- `web:lint`, `web:check`, `web:sg`, `web:build:check` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo nextest run` — 1744 passed, 0 failed.
