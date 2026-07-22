PACKET_STATUS: READY
TASK_KIND: mechanical
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Revert the xterm zero-lag typed-echo overlay from commit `4d2cdc1`
(`feat(web): add xterm zero-lag typed-echo overlay` / #661). After this change,
`TaskTerminal` must not import, create, paint, or clear any zero-lag overlay;
`xtermZeroLag.ts` / `xtermZeroLag.test.ts` must be gone; CSS and docs must not
claim the React surface owns typed-echo prediction.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/xtermZeroLag.ts` (delete)
- `crates/ajax-web/web/src/shared/lib/xtermZeroLag.test.ts` (delete)
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/TERMINAL.md`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `crates/ajax-web/web/dist/app.css` (via `npm run web:build` only)
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/revert-zero-lag-and-fix-scroll-history.md`

## Forbidden changes

- Do not commit, push, merge, rebase, or change branches.
- Do not resurrect banned legacy paths (`src/terminalZeroLag.ts`,
  `e2e/terminal-zero-lag.test.ts`); keep `legacyTerminalRemoval.test.ts` bans.
- Do not change scrollback, history seed, PTY bridge, scroll sync, paste/copy,
  expand, or geometry beyond removing zero-lag wiring.
- Do not hand-edit `dist/*`; rebuild with `npm run web:build`.
- Do not edit files outside Allowed files.

## Context evidence

Desired behavior: remove #661 overlay entirely. Typing waits for PTY echo again
on the React xterm surface.

Anchors from `git show 4d2cdc1`:
- Added `xtermZeroLag.ts` + tests; wired in `TaskTerminal.tsx` (import,
  `zeroLagNoteRef`, painter/echo, `beforeinput`, write `clearIfEchoedIn`,
  reconnect `reset`, cleanup).
- Added `.terminal-host .xterm-zerolag-input` CSS.
- `TERMINAL.md` ownership row for typed-echo overlay.
- Product contract row evidence retargeted from legacy `terminalZeroLag.ts` to
  `xtermZeroLag.ts`.

Pre-#661 Product contract evidence (restore this wording):
`terminalZeroLag.ts`; tests: `terminalZeroLag.test.ts:151,171,186`; e2e:
`e2e/terminal-zero-lag.test.ts:55,65`

## Code anchors

- `TaskTerminal.tsx` import block lines ~20–27 (`createZeroLagEcho` …)
- `TaskTerminal.tsx` `zeroLagNoteRef` / `onTermData` ~428–436
- `TaskTerminal.tsx` overlay setup ~914–956 and cleanup ~1057–1060
- `TaskTerminal.test.tsx` describe `TaskTerminal zero-lag typed echo` ~316–336
- `styles.css` `.terminal-host .xterm-zerolag-input` ~1473–1486
- `TERMINAL.md` ownership row with `xtermZeroLag.ts`
- `TERMINAL_BEHAVIOR_CONTRACT.md` Product typed-echo row (~130)

## Test-first instructions

NOT_APPLICABLE — mechanical revert of a known commit surface. Do not invent new
tests. Delete the zero-lag describe block from `TaskTerminal.test.tsx` and delete
`xtermZeroLag.test.ts` with the module.

## Edit instructions

1. Delete `crates/ajax-web/web/src/shared/lib/xtermZeroLag.ts` and
   `crates/ajax-web/web/src/shared/lib/xtermZeroLag.test.ts`.
2. In `TaskTerminal.tsx`, remove the `xtermZeroLag` import and all overlay
   wiring. Restore `onTermData` to:
   `const onTermData = useEffectEvent((data: string) => { sendKey(consumeCtrl(data)); });`
   Restore Space handler to `sendKey(" ")` only (no `noteZeroLagTerminalData`).
   Restore `term.write(text, scrollSync.applyOutput)` (no `clearIfEchoedIn`).
   Remove `zeroLag.reset()` from `onOpen` and overlay dispose/reset from cleanup.
   Keep fitAddon/open/onHardenTextarea order as before overlay insertion
   (open host, then Space handler is fine if open stays before Space).
3. Delete the entire `describe("TaskTerminal zero-lag typed echo", …)` block from
   `TaskTerminal.test.tsx`.
4. Remove `.terminal-host .xterm-zerolag-input { … }` from `styles.css`.
5. Remove the Typed-echo zero-lag ownership row from `TERMINAL.md`.
6. Restore the Product typed-echo contract row evidence to the pre-#661 legacy
   anchors (`terminalZeroLag.ts` / `terminalZeroLag.test.ts` / e2e). Leave the
   separate Legacy Ghostty zero-lag row (#139) unchanged.
7. Run `npm run web:build` so `dist/app.css` and `dist/terminal.js` drop overlay
   code.
8. Update the plan checklist Task 1 notes if helpful; do not mark parent review done.

## Verification commands

```bash
rg -n 'xtermZeroLag|createZeroLagEcho|xterm-zerolag-input|zeroLagNoteRef' crates/ajax-web/web/src crates/ajax-web/web/TERMINAL.md crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
# expect: no matches in src/TERMINAL.md; contract Product row should not cite xtermZeroLag

npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/legacyTerminalRemoval.test.ts
npm run web:lint
npm run web:build
```

## Acceptance criteria

- No `xtermZeroLag` module or test file remains.
- `TaskTerminal` has no zero-lag imports/refs/listeners.
- Overlay CSS class is gone from source styles.
- Docs no longer claim React owns typed-echo prediction via `xtermZeroLag`.
- Focused tests + lint pass; dist rebuilt.

## Stop conditions

- Any required edit outside Allowed files.
- Need to change scrollback / PTY / history seed to complete the revert.
- `legacyTerminalRemoval` bans would need weakening.
