# Packet — Slice 10 Round 1: useEffectEvent, delete the last suppression

```yaml
PACKET_STATUS: READY
TASK_KIND: mechanical
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Route the four in-effect call sites of `consumeCtrl`, `hardenMobileTextarea` and
`scheduleBandSettle` through `useEffectEvent`, so `TaskTerminal.tsx`'s mount
effect satisfies `react-hooks/exhaustive-deps` with `[handle]` honestly, and the
last react-hooks suppression in the tree can be deleted.

Behaviour must not change in any way. This is a lint-correctness change, not a
restructuring: the 705-line effect stays one unit and round 2 owns splitting it.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/eslint.config.mjs` — deletion of one block only

## Forbidden changes

- Any edit under `crates/ajax-web/web/e2e/`, or to
  `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`. These are the
  net for this surface and are not part of the change.
- Any other file, including `styles.css`, `package.json`, and any other
  component.
- **Converting `consumeCtrl` or `scheduleBandSettle` themselves into
  `useEffectEvent`.** Both are called from JSX handlers outside the effect
  (`:1233` and `:406,:409`); an Effect Event called outside an effect is a React
  error. They keep their current plain definitions.
- Changing the effect's dependency array to anything other than `[handle]`.
- Splitting, reordering, or extracting any part of the 705-line effect. No new
  module, no controller, no moved ownership — that is round 2.
- Changing terminal behaviour: socket setup/teardown, gesture handling,
  clipboard, fullscreen, geometry, refit, or status.
- Enabling React `StrictMode`.
- Adding, removing, weakening, or reordering any test.
- Any commit, branch, push, rebase, or branch switch.

## Context evidence

**The suppression to delete** lives in `crates/ajax-web/web/eslint.config.mjs`,
a block whose `files` is `["**/features/task/TaskTerminal.tsx"]` and whose rules
are `{"react-hooks/exhaustive-deps": "off"}`. Its comment opens
`// REMOVE IN SLICE 10 (terminal controller extraction).` and states it is "the
last react-hooks suppression in the tree".

**The effect**: `TaskTerminal.tsx` has exactly one `useEffect`, opening at line
412 and closing at line 1117 with `}, [handle]);`.

**The four in-effect call sites** — the complete list:
- `:735` — `scheduleBandSettle();`
- `:989` — `hardenMobileTextarea();`
- `:1006` — `dataDisposable = liveTerm.onData((data) => sendKey(consumeCtrl(data)));`
- `:1070` — `scheduleBandSettle();`

**The outside-effect call sites that constrain the design** — these must keep
working through the plain functions:
- `:406` and `:409` — `scheduleBandSettle()` inside `toggleExpanded`
- `:1233` — `sendKey(consumeCtrl(key.data));` inside a `.terminal-key` `onClick`

**Definitions**: `scheduleBandSettle` at `:150`, `consumeCtrl` at `:213`,
`hardenMobileTextarea` at `:240`.

**The established pattern in this codebase** — `crates/ajax-web/web/src/app/App.tsx`:
line 1 imports `useEffectEvent` from `react`; lines 95, 99, 103 define
`onShellMount`, `onShellResume`, `onShellVisibilityChange`. Slice 2's plan
describes this as "the sanctioned use — an external subscription that must mount
once but needs the latest non-reactive callbacks — not concealment of a
dependency". React is **19.2.7**, so the API is available unflagged.

## Code anchors

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx:1` — import site.
- `:150`, `:213`, `:240` — the three plain definitions (unchanged).
- `:406`, `:409`, `:1233` — outside-effect callers (unchanged).
- `:412` — effect opens. `:1117` — `}, [handle]);`.
- `:735`, `:989`, `:1006`, `:1070` — the four call sites to route.
- `crates/ajax-web/web/src/app/App.tsx:95-105` — the pattern to imitate.
- `crates/ajax-web/web/eslint.config.mjs` — the block to delete.

## Test-first instructions

`NOT_APPLICABLE: mechanical — four exact call sites, exact replacements, expected
match count 4, and no behaviour changes, so there is no new assertion to write
first. The oracle is the existing suite plus the terminal e2e — in particular the
single-socket cardinality assertions, which fail loudly if the effect starts
re-running. Adding a test here would assert the lint configuration, not the
product.`

## Edit instructions

**1. `TaskTerminal.tsx` — add the Effect Events.**

Import `useEffectEvent` from `react` alongside the existing hooks.

Define three Effect Events in the component body, after the plain definitions
they delegate to and before the `useEffect` at `:412`:

- `onHardenTextarea` — calls `hardenMobileTextarea()`.
- `onBandSettle` — calls `scheduleBandSettle()`.
- `onTermData` — takes `data: string` and performs `sendKey(consumeCtrl(data))`.

They must **delegate to** the live functions, not copy their bodies, so the
latest state is always read.

**2. Route the four call sites, and only those four.**

- `:735` → `onBandSettle();`
- `:989` → `onHardenTextarea();`
- `:1006` → pass `onTermData` as the `onData` handler.
- `:1070` → `onBandSettle();`

Leave `:406`, `:409`, `:1233` calling the plain functions.

**3. Replace the debt marker.**

Add a comment immediately above the `useEffect` at `:412`, in the repo's
`ponytail:` style, recording that this single effect still owns ~700 lines of
terminal lifecycle, that the deps are now honest rather than suppressed, and
that splitting it into a disposable controller is round 2. The suppression block
being deleted is currently the only marker of this debt, so it must not vanish
silently.

**4. `eslint.config.mjs` — delete the suppression block**, including its comment.
Delete nothing else.

## Verification commands

```bash
npm run web:lint
npm run web:check
npm run web:test -- --run
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit -g "terminal"
```

Run from the repository root. `web:lint` is the primary signal: it must pass
**with the suppression block gone**.

## Acceptance criteria

- `web:lint` exits 0 and `eslint.config.mjs` no longer contains
  `react-hooks/exhaustive-deps": "off"` anywhere. State the grep result.
- The effect's dependency array is still exactly `[handle]`.
- Full suite: **380** passing across 43 files, 0 failing, no test file modified.
- `web:check` and `web:build:check` exit 0.
- Terminal e2e passes, including the single-socket assertions.
- `git diff --stat` lists exactly two paths: `TaskTerminal.tsx` and
  `eslint.config.mjs`.
- Outside the four routed lines and the added definitions/import/comment, no
  line inside the effect changed — state the changed-line count for
  `TaskTerminal.tsx`; it should be roughly 15 or fewer.

## Stop conditions

- `web:lint` still reports `exhaustive-deps` on `TaskTerminal.tsx` after the
  change — the three functions were not the only blockers. Stop and report the
  full rule output rather than re-adding a suppression or widening the deps.
- React errors that an Effect Event was called outside an effect — a plain
  function was converted when it should have been wrapped. Stop and report.
- Any terminal e2e fails, especially socket cardinality — stop immediately and
  report; do not adjust the test.
- The change appears to require touching the effect's internals beyond the four
  call sites — stop and report.
- The patch exceeds roughly 60 changed lines.
