# Packet — Slice 12 Wave A: mechanical ESLint backlog

```yaml
PACKET_STATUS: READY
TASK_KIND: mechanical
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Clear Wave A of the slice-12 ESLint backlog: enable the two already-clean
testing-library rules, fix the ~32 mechanical violations (unused-vars,
empty-pattern, vitest trio, prefer-const, no-regex-spaces, no-control-regex),
flip those rules from `off` to `error`, and delete their `slice 12 follow-up`
comments. Leave testing-library bulk rules and NewTaskSheet a11y for Wave B.

## Allowed files

- `crates/ajax-web/web/eslint.config.mjs`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx` (prefer-const + one
  control-regex disable only)
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/features/task/keyboardBandPin.test.ts`
- `crates/ajax-web/web/src/app/App.test.tsx`
- `crates/ajax-web/web/src/fixtures.test.ts`
- `crates/ajax-web/web/src/shared/lib/viewport.test.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/e2e/swipe-reveal.test.ts`
- `.planning/agent-plans/react-cleanup-s12-audit.md` (checklist only)

## Forbidden changes

- Do not touch `jsx-a11y/no-noninteractive-element-interactions` or NewTaskSheet
  production a11y (Wave A+ / Wave B).
- Do not enable or fix: `testing-library/no-node-access`,
  `prefer-screen-queries`, `no-container`, `no-await-sync-events`.
- Do not remove permanent exemption blocks for Skeleton / ConnectionStatus /
  TaskDetail / App tests.
- Do not change terminal behavior, PTY framing, or regex meaning for ANSI
  escapes — for `no-control-regex` on `TaskTerminal.tsx:209`, add
  `eslint-disable-next-line no-control-regex` with a one-line reason.
- No commit, push, branch switch, or dependency changes.

## Context evidence

- Desired: rules that Wave A owns are `error` and `web:lint` is clean for them;
  `slice 12 follow-up` markers for those rules are gone.
- Recount 2026-07-19 (`react-cleanup-s12-audit.md`): prefer-presence 0,
  no-wait-for-multiple 0, unused-vars 3, empty-pattern 2, vitest 8,
  regex-spaces 11, prefer-const 6, control-regex 1.
- Prefer-const sites in `TaskTerminal.tsx`: 436 fitAddon, 471 refitController,
  920 selectionDisposable, 925 scrollDisposable, 929 dataDisposable,
  975 resizeObserver — use `eslint --fix` where safe; do not invent refactors.
- Unused: `e2e/terminal-behavior.test.ts:195` unused `surface`;
  `:1259` unused `el`; `App.test.tsx:392` unused `init`.
- Empty pattern: `e2e/swipe-reveal.test.ts:74` and
  `e2e/terminal-behavior.test.ts:368` — `test.beforeEach(({ }, testInfo)` →
  use `_` or omit binding per Playwright style already in repo.
- Vitest: `keyboardBandPin.test.ts` expect-expect / valid-expect;
  conditional expect in `fixtures.test.ts:100,107` and
  `viewport.test.ts:266`.

## Code anchors

- Config markers: `crates/ajax-web/web/eslint.config.mjs` lines with
  `// slice 12 follow-up` for the Wave A rules listed in Goal.
- Control regex: `TaskTerminal.tsx:209` — `/^\x1b\[([ABCD])$/`.

## Test-first instructions

`NOT_APPLICABLE: mechanical lint cleanup; existing suite is the oracle.`

## Edit instructions

1. In `eslint.config.mjs`, set `testing-library/prefer-presence-queries` and
   `testing-library/no-wait-for-multiple-assertions` to `"error"` and remove
   their follow-up comment blocks.
2. Fix unused-vars (prefix with `_`, remove binding, or void — match nearby
   style).
3. Fix empty-pattern beforeEach args.
4. Fix vitest violations with real assertions / non-conditional expects /
   single-arg `expect` — do not delete test coverage.
5. Run eslint `--fix` for `prefer-const` and `no-regex-spaces` on allowed
   files; hand-fix leftovers.
6. Add disable-next-line for control-regex at TaskTerminal:209 only.
7. Flip each fixed Wave A rule to `"error"` and delete its follow-up comments.
8. Check off Wave A in `react-cleanup-s12-audit.md`.

## Verification commands

```bash
npm run web:lint
npm run web:check
npm run web:test -- --run
```

## Acceptance criteria

- `web:lint` exit 0
- No `slice 12 follow-up` comments remain for Wave A rules
- Wave A rules are `"error"`
- Wave B rules still `"off"` with markers
- Tests still pass (387)

## Stop conditions

- Prefer-const fix would require restructuring TaskTerminal mount logic
- Vitest fixes would delete keyboard-band contract coverage
- Any need to touch NewTaskSheet a11y or testing-library bulk rules
