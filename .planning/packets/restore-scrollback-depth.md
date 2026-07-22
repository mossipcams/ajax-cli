PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Restore scrollback depth lost when React xterm hard-coded `scrollback: 2000`
for every viewport. Client buffer must again be mobile 2000 / desktop 10000 via
`terminalScrollbackLines()`, and `capture-pane -S` must seed up to `-10000`
lines so desktop history is not truncated at 2000 on first dial.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/terminalGeometry.ts`
- `crates/ajax-web/web/src/shared/lib/terminalGeometry.test.ts`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/revert-zero-lag-and-fix-scroll-history.md`

## Forbidden changes

- Do not commit, push, merge, rebase, or change branches.
- Do not change history-seed settle/pad logic, `seed=0` policy, hostile
  filters, scroll-sync, zero-lag (already reverted), or architecture docs.
- Do not hand-edit `dist/*`; rebuild with `npm run web:build` if TaskTerminal
  changes require it.
- Do not edit files outside Allowed files.

## Context evidence

Desired behavior (from `TERMINAL_BEHAVIOR_CONTRACT.md` §3 and prior Ghostty
`terminalGeometry.ts` at `b83e544`):

```ts
export const MOBILE_SCROLLBACK_LINES = 2000;
export const DESKTOP_SCROLLBACK_LINES = 10000;
const MOBILE_MEDIA_QUERY =
  "(max-width: 767px), (pointer: coarse) and (max-height: 500px)";
export function terminalScrollbackLines(): number {
  if (typeof window !== "undefined" && window.matchMedia?.(MOBILE_MEDIA_QUERY).matches) {
    return MOBILE_SCROLLBACK_LINES;
  }
  return DESKTOP_SCROLLBACK_LINES;
}
```

Current bug anchors:
- `TaskTerminal.tsx` ~902: `scrollback: 2000` (hard-coded for all viewports)
- `terminal_pty.rs` ~214–216: `capture-pane … -S -2000` with ponytail comment
  saying raise both caps if deeper history matters
- Test `isolated_attach_plan_seeds_browser_scrollback_from_task_window` asserts
  `"-2000"`

Same media query already used in `TaskTerminal.tsx` `isPhoneTerminalLayout`.

## Code anchors

- `terminalGeometry.ts` top exports (add constants + helper after font constants)
- `terminalGeometry.test.ts` (add scrollback limits describe)
- `TaskTerminal.tsx` Terminal constructor `scrollback: 2000` →
  `scrollback: terminalScrollbackLines()`
- `terminal_pty.rs` history capture `-S`, `"-2000"` → `"-10000"`; update
  ponytail comment; update test assertion
- Contract rows citing `-S -2000` and scrollback 2000/10000 evidence paths

## Test-first instructions

1. Add failing TS tests in `terminalGeometry.test.ts`:
   - `MOBILE_SCROLLBACK_LINES === 2000`
   - `DESKTOP_SCROLLBACK_LINES === 10000`
   - `terminalScrollbackLines()` returns mobile when `matchMedia` matches the
     mobile query; desktop otherwise (mock `window.matchMedia`).
2. RED: `npm run web:test -- --run src/shared/lib/terminalGeometry.test.ts`
   must fail because exports/helper are missing.
3. Change Rust test expectation from `"-2000"` to `"-10000"` in
   `isolated_attach_plan_seeds_browser_scrollback_from_task_window`.
4. RED: `rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture`
   must fail on the expected `-S` value.
5. Only then implement production edits.

## Edit instructions

1. Restore `MOBILE_SCROLLBACK_LINES`, `DESKTOP_SCROLLBACK_LINES`, and
   `terminalScrollbackLines()` in `terminalGeometry.ts` (xterm, not Ghostty,
   in the doc comment).
2. Import and use `scrollback: terminalScrollbackLines()` in `TaskTerminal.tsx`.
3. In `terminal_pty.rs`, change capture `-S` from `-2000` to `-10000`; update
   the ponytail comment to note it matches `DESKTOP_SCROLLBACK_LINES`.
4. Update `TERMINAL_BEHAVIOR_CONTRACT.md` evidence that still says `-S -2000`
   and that points scrollback helper evidence at missing
   `terminalGeometry.ts:28-44` — retarget to the restored helper/tests.
5. `npm run web:build` after TaskTerminal change.
6. Check off Task 2 notes in the plan; leave Task 3 for parent.

## Verification commands

```bash
npm run web:test -- --run src/shared/lib/terminalGeometry.test.ts
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
npm run web:lint
npm run web:build
rg -n 'scrollback: 2000|-S",|"-2000"|terminalScrollbackLines' crates/ajax-web/web/src/features/task/TaskTerminal.tsx crates/ajax-web/src/adapters/terminal_pty.rs crates/ajax-web/web/src/shared/lib/terminalGeometry.ts
```

## Acceptance criteria

- Desktop viewports get 10000-line xterm scrollback; mobile media query gets 2000.
- History seed captures `-S -10000`.
- Focused TS + Rust tests pass; lint + build pass.
- Contract text matches the new depths.

## Stop conditions

- Need to change settle/pad / `-E` seed framing to make tests pass.
- Ambiguity about whether capture should be unlimited (`-S -`) instead of 10000.
- Patch would exceed ~400 lines or leave Allowed files.
