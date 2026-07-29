# Dashboard rebuild — one tap, no terminal

## Scope

Rebuild the Web Cockpit **dashboard page** from scratch so every safe operation a
task offers is a visible, single-tap button on its row. Task pages
(`features/task/TaskDetail`, `TaskTerminal`, `ActionBar`, `NewTaskSheet`, …) are
untouched.

Driving instruction (Matt, 2026-07-29): *"rethink ajax web dashboard. The goal
should be 1 tap actions from the dashboard to use the terminal the least
possible. Restart the dashboard page from scratch but keep the tasks pages."*

## Non-goals

- No new backend endpoint, no new `OperatorAction`, no change to
  `supported_web_action`. The action vocabulary stays exactly what Rust projects.
- No browser-side task truth: band membership stays `card.attention`, ordering
  stays Rust's, action list stays `card.actions`.
- No batch/band-level "ship all" operation (speculative).
- Task detail, terminal, settings, new-task sheet: unchanged.

## Diagnosis of the current dashboard

`features/task/TaskList.tsx` renders `visibleTaskActions(...)[0]` inline and hides
`slice(1)` behind `useSwipeReveal`. Consequences against the stated goal:

1. Any task with 2+ safe actions needs a **hidden horizontal gesture** to reach
   the rest — the opposite of one tap, and undiscoverable on iOS Safari.
2. A row whose only actions are `resume`/`drop` shows **no control at all**, so
   its sole path is tapping through to the terminal.
3. Row tap + chevron + inline button = three affordances competing in ~40px.

## Design

Row (whole row is the unit; tap the text block to open the task):

```
● Fix login                                  2m ago
  web/fix-login · Waiting for review
  [ Review ]  [ Ship ]  [ Fix CI ]
```

- **Every** non-destructive action from `visibleTaskActions` renders as its own
  button on an action line under the text. `Drop` still never appears on the
  dashboard (task detail only).
- Swipe-reveal deleted (`shared/gestures/swipeReveal.ts`,
  `shared/hooks/useSwipeReveal.ts`, `e2e/swipe-reveal.test.ts`).
- Chevron deleted — the text block is the navigation target and carries the
  accessible name.
- Bands (`needs-you` / `review` / `active` / `idle`) and project pills kept as-is;
  they are Rust-owned and already one tap.
- Rows with no safe action render no action line (nothing to fake).

## Delegation decision

`Delegation decision: not delegated because` the session harness forbids spawning
subagents unless the user asks (`Do not call the AgentTool unless the user
requested it`), and this is design-direction UI work, which AGENTS.md lists on the
do-not-delegate side (a design choice, not a bounded mechanical work order).
Implemented directly, reviewed and validated here.

## Tasks

- [x] T1 — Read `AGENTS.md`, current dashboard, action vocabulary, e2e selectors.
- [x] T2 — Write `features/dashboard/Dashboard.test.tsx` first: port the surviving
      contracts from `TaskList.test.tsx` (band order, band membership from
      `attention` not `status`, no `Drop`, no `Resume`, project pills + fault dot,
      quiet flag, empty state, relative time) and add the new ones (every safe
      action gets its own button; multi-action row exposes all of them).
- [x] T3 — Implement `features/dashboard/Dashboard.tsx`; delete
      `features/task/TaskList.tsx` + test.
- [x] T4 — Delete orphaned swipe modules + their tests + `e2e/swipe-reveal.test.ts`.
- [x] T5 — Point `app/App.tsx` at `Dashboard`.
- [x] T6 — Replace the `TASK LIST (dashboard)` CSS block with the new dashboard
      block; keep the `.task-row` / `.project-pill` hooks that e2e asserts.
- [x] T7 — Playwright screenshot review (mobile-webkit) → found and fixed a real
      grid overflow (see Deviations) → added an e2e containment regression test.
- [x] T8 — Validate: vitest, eslint, tsc, `web:smoke` on a freshly started vite.

## Deviations

- `sortCards`/`statusRank` reused unchanged from `shared/lib/state.ts` rather than
  rewritten — presentation-only stable ordering, already correct.
- `taskActions.visibleTaskActions` and `features/task/ActionBar` reused as-is; the
  dashboard needed no new dispatch/confirm/undo machinery.
- **Layout bug found by the screenshot, not by any test.** `.task-list` is a CSS
  grid and `.task-row` had the default `min-width: auto`, so the widest row's
  action strip sized the whole track and pushed *every* row's timestamp ~172px
  past the clipped card edge. Fixed with `min-width: 0` on `.task-row` (plus on
  `.task-row-title` so a long title ellipsises against the timestamp). New e2e
  test `a wide action row never pushes any row past the task list edge` in
  `e2e/layout-scroll.test.ts` measures containment, not pixels; verified it fails
  with the fix reverted (`overhang: 172.4375`).
- The action line only renders when a row has safe actions, so
  `e2e/layout-scroll.test.ts`'s existing 96px row cap (its fixture cards carry
  `actions: []`) still holds.
- Row time moved from a stacked `.task-row-side` column onto the title line, so
  the title truncates against the timestamp instead of against a chevron.
- **Worktree reaped mid-task.** `ajax-cli__worktrees/ajax-web-ajax` was emptied and
  dropped from `git worktree list` partway through validation; the work was
  recreated in `ajax-web-ajax-new-control` (same base commit `40b0f28`) and
  committed immediately. Cf. the standing note about slice worktrees being reaped.

## Validation

| Command | Result |
| --- | --- |
| `npx vitest run src/features/dashboard` | pass |
| `npx vitest run` (full web suite) | pass |
| `eslint src e2e` / `tsc -p tsconfig.check.json` | pass |
| `pkill -f vite; CI=1 npm run web:smoke` | pass |
