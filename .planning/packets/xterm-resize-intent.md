# Xterm resize fix — exact discrete intents

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Fullscreen-enter and pinch-end are discrete keyboard-open resize exceptions:
each must settle to exactly one fresh, deduplicated PTY resize without a storm.
While the keyboard is open, pinch movement must not refit the local logical
grid before the single pinch-end fit and PTY resize.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## Forbidden changes

- Do not weaken or delete the exact-one-resize assertions.
- Do not change public terminal behavior outside keyboard-open pinch handling.
- Do not edit plans, packets, generated assets, dependencies, branches, or commits.

## Context evidence

- Graphify: `NOT_REQUIRED`; this is a local interaction scheduling fix that does
  not change Ajax ownership or architecture boundaries.
- Serena: `NOT_REQUIRED`; exact symbols and behavior are already identified in
  the current source and failing Playwright case.
- ast-grep: `NOT_REQUIRED`; the edit is a single named handler, not a structural
  or repeated syntax change.

## Code anchors

- Production: `TaskTerminal.svelte` `onTouchMove`, where the font-size update
  currently calls `fitLocal()` unconditionally.
- Test: `terminal-behavior.test.ts` case `keyboard-open pinch-end produces
  exactly one fresh PTY resize while keyboard stays open`.

## Test-first instructions

The existing pinch-end case is RED in the full mobile-WebKit run: its poll did
not observe exactly one fresh, changed PTY resize within five seconds. Preserve
that assertion and use this observed failure as RED evidence.

## Edit instructions

In `onTouchMove`, keep applying the selected font size, but skip `fitLocal()`
while `isKeyboardOpen()` is true. Let the existing `onTouchEnd` discrete
`schedulePostLayout(true)` perform the single fit and PTY resize. Add no helper
or abstraction.

## Verification

- Two discrete-intent cases plus existing resize/pinch/keyboard group: green.
- Run the pinch-end case five times to catch the observed nondeterminism.
- Full terminal file: green.
- `npm run web:check`; `git diff --check`.

## Acceptance criteria

- Keyboard-open touchmove performs no local fit.
- Pinch end produces exactly one fresh PTY resize and leaves keyboard-open set.
- Existing expand and ordinary pinch behavior remain green.

## Stop conditions

- The exact-one test still fails after the minimal handler change.
- A required fix expands outside the two allowed files.
- An unrelated test fails; report it without editing unrelated code.
