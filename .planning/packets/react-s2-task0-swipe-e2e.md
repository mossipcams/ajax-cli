PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Close the S2 characterization gap: add a mobile-webkit Playwright e2e that touch-drags a dashboard task row, asserts the reveal opens to `SWIPE_REVEAL_WIDTH` (88px), and that tapping the revealed action dispatches (operation fetch / confirming UI). Against the **current Svelte** TaskList — no production migration in this packet.

## Allowed files

- `crates/ajax-web/web/e2e/smoke.test.ts` **or** new `crates/ajax-web/web/e2e/swipe-reveal.test.ts` (prefer new file if smoke is already large)
- `.planning/agent-plans/react-slice-s2.md` (checklist only)

## Forbidden changes

- No production component/gesture edits
- No weakening of existing e2e assertions
- No React TaskList/ActionBar port yet
- Do not commit, push, merge, rebase, or change branches

## Context evidence

- Gap named in `docs/react-migration-plan.md` S2: "no e2e for swipe-reveal — add e2e/ mobile-webkit test … against the Svelte implementation first".
- Constant: `SWIPE_REVEAL_WIDTH = 88` in `crates/ajax-web/web/src/gestures/swipeReveal.ts`.
- Row markup: `TaskList.svelte` — `.task-row-wrap[data-handle]`, `.task-row` with `use:swipeReveal`, `.task-row-reveal` width 88, contains `ActionBar` for first visible action.
- Fixture card `web/fix-login` has Review + Drop actions (`e2e/fixtures.ts` COCKPIT_FIXTURE).
- Touch dispatch pattern: `e2e/terminal-behavior.test.ts` ~174–184 (`touchstart`/`touchmove`/`touchend` via `evaluate` + `Object.defineProperty` for `touches`).
- Unit proof of gesture: `swipeRevealAction.test.ts` left-swipe 200→80 settles open at 88.

## Code anchors

- Add test scoped to `testInfo.project.name === "mobile-webkit"` (skip desktop Chromium).
- Target row: `page.locator('.task-row[data-handle="web/fix-login"]')` or wrap.
- After left swipe of ≥88px horizontal (minimal vertical), assert transform/reveal: e.g. `is-revealed` class or `translateX(-88px)` / reveal ActionBar `[data-action='review']` visible and clickable.
- Click revealed `[data-action='review']`; assert operation path keeps dashboard outlet or matching smoke-style post-action visibility (non-destructive completes without second tap — same as smoke).

## Test-first instructions

NOT_APPLICABLE: tests-only packet; the new e2e is the deliverable. Write the failing-or-green characterization test against live Svelte (should pass if written correctly against current behavior).

## Edit instructions

1. Create `crates/ajax-web/web/e2e/swipe-reveal.test.ts` (preferred) importing `mockFetch` from `./fixtures`.
2. One focused test (or two: open-to-88 + action tap). Use in-page touch event dispatch on the task row like terminal-behavior.
3. Skip when not `mobile-webkit`.
4. Do not edit production files.

## Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/swipe-reveal.test.ts
```

If the npm script does not forward args that way, use the repo's Playwright invocation from `package.json` `web:smoke` with equivalent project+file filters.

## Acceptance criteria

- New e2e file exists; passes on mobile-webkit against Svelte TaskList.
- Asserts reveal width 88 and action tap dispatch.
- Zero production file changes.

## Stop conditions

- Test only passes by weakening other e2e or changing gesture production code.
- Need mouse-only drag instead of touch (wrong — must use touch events).
- Diff escapes allowed files.
