# Xterm fullscreen fix — isolate background interaction

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

When the phone terminal is expanded, background cockpit chrome, task-detail
controls, metadata, and bottom navigation must not remain focusable or
interactive. Restore them on exit and cleanup.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED

Add a black-box phone case that expands the terminal, attempts to focus/click
representative background controls (Back, cockpit chrome/settings, bottom nav,
Task details), and proves none can receive focus or act while expanded. Exit and
prove normal focus/interaction is restored. Current code must fail.

## Implementation

Use the native `inert` platform behavior on the smallest set of terminal sibling
and shell chrome elements necessary while expanded. Do not make an ancestor of
the terminal inert. Restore only state owned by this component on exit and
unmount. Keep fixed overlay, socket, PTY input, geometry, and architecture
unchanged. No new dependency or generic focus-trap abstraction.

## Review revision

The first pass omits a pre-existing `.result-panel`, which is an AppViewport
sibling outside the task-detail/chrome/nav sets and contains a Dismiss button.
Extend the test by triggering the existing non-destructive Review action before
expansion, prove the Result panel is inert and its Dismiss button cannot receive
focus while expanded, then prove restoration on exit. Add `.result-panel` to
the owned inert set without changing already-inert elements.

## Verification

- New case plus fullscreen/focus/input cases: green.
- Full terminal file: green.
- `npm run web:check`; `git diff --check`.

No other files, assertion weakening, plans/packets/generated assets, commits,
branches, dependencies, debug code, or unrelated changes.
