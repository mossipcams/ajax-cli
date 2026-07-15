# Xterm interaction click focus ownership

## Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Direct terminal-surface clicks focus xterm without scrolling, while clicks on
bubbled child buttons such as `New output` do not refocus the terminal or
reopen hidden keyboard ownership.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

## Forbidden changes

- Do not change toolbar, paste, fullscreen, scroll-follow, keyboard, or PTY
  behavior beyond the wrapper click focus defect.
- Do not add a generic focus abstraction, dependencies, private xterm APIs,
  generated assets, plan/packet edits, branches, or commits.
- Do not weaken existing focus or New-output assertions.

## Context evidence

- Graphify: `NOT_REQUIRED`; this stays within the existing UI interaction owner.
- Serena: `NOT_REQUIRED`; the exact handler, helper textarea lookup, and e2e
  patterns are identified in the current files.
- ast-grep: `NOT_REQUIRED`; one named event handler is the only production
  anchor.

## Code anchors

- Production: `TaskTerminal.svelte::onInteractionClick`, currently raw
  `term?.focus()` for every bubbled click.
- Existing native focus seam: `termTextarea()` and
  `HTMLTextAreaElement.focus({ preventScroll: true })` in
  `refocusTermIfOwned`.
- Tests: New-output case `reading scrollback shows New output...` and nearby
  focus behavior cases in `terminal-behavior.test.ts`.

## Test-first instructions

Add focused black-box coverage proving that with terminal focus absent and the
keyboard-open class absent, clicking visible `New output` keeps terminal focus
absent and keyboard-open absent. Also prove a direct click on the interaction
surface focuses the helper textarea without changing document scroll. Run the
focused test before production edits and capture the unintended refocus as RED.

## Edit instructions

Change `onInteractionClick` to accept the `MouseEvent`; return when
`event.target` is inside a button. Otherwise focus the existing xterm helper
textarea with `{ preventScroll: true }`, falling back to `term?.focus()` only
when the textarea is unavailable. Keep the change local to the handler.

## Verification commands

- Focused new case plus existing New-output/focus cases under mobile-WebKit,
  serial workers.
- Full terminal behavior file under mobile-WebKit, serial workers.
- `npm run web:check`
- `git diff --check`

## Acceptance criteria

- Child button clicks never trigger wrapper refocus.
- Direct surface clicks focus xterm without document scroll.
- New-output still restores live follow and sends no PTY input.

## Stop conditions

- The fix requires focus or keyboard architecture changes.
- Scope expands beyond the two allowed files.
- An unrelated test fails; report it without editing other code.
