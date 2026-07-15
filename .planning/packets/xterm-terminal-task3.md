# Xterm terminal rebuild — Task 3 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Make the accepted xterm surface report valid, coalesced, deduplicated PTY sizes
on open and meaningful viewport/fullscreen changes, while suppressing keyboard
resize storms and preserving one renderer/socket across transitions.

## 3. Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## 4. Forbidden changes

- Do not edit tests, fixtures, connection/backend code, dependencies, docs,
  TaskDetail, global viewport helpers/styles, or generated assets.
- Do not recreate deleted geometry/refit/output-policy modules or add a generic
  controller/helper abstraction.
- Do not reconnect/recreate xterm when entering fullscreen.
- Do not change Task 1 lifecycle or Task 2 input behavior.
- Do not commit, branch, or touch any other path.

## 5. Context evidence

- Graphify: `NOT_REQUIRED`; PTY resize remains the existing browser-to-backend
  connection message and no ownership boundary changes.
- Serena: `NOT_REQUIRED`; all state/listeners live in one accepted component
  and use exact `FitAddon`, `Terminal`, and connection APIs.
- ast-grep evidence: `TerminalConnection.sendResize(cols, rows)` at
  `terminalConnection.ts:213–216` sends one JSON frame per call; dedupe must
  therefore happen in the component before calling it.
- Architecture: `architecture.md` requires raw xterm/tmux-first behavior and
  retains terminal truth in the backend; browser code may fit/render only.
- Desired behavior: permanent resize cases at
  `terminal-behavior.test.ts:429–615`.

## 6. Code anchors

- `TaskTerminal.svelte` `onMount`: extend the existing local `FitAddon`
  lifecycle with one `ResizeObserver`, the named viewport listeners, one
  debounce timer, and last-sent cols/rows.
- Existing connection `onOpen`: reset resize dedupe and schedule/send one valid
  current size after fit.
- Existing panel `<section>`: add `class:is-expanded` and one `Expand terminal`
  button with boolean `aria-pressed`.
- Component cleanup: remove every new listener/timer/observer and the
  `terminal-expanded` document class.

## 7. Test-first instructions

Before editing, run:

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'initial open|portrait-to-landscape|repeated same-dimension|keyboard-open resize|fullscreen enter and exit each|reopen with meaningful'
```

Expected RED: no resize frames and no Expand terminal control. Do not modify
tests.

## 8. Edit instructions

1. Define only component-local constants needed now: 80 minimum columns and a
   short quiet-window debounce (prefer the documented 100ms unless a smaller
   existing value is required by the cases).
2. Fit before reading dimensions. Send only positive integer rows and
   `max(term.cols, 80)` columns; skip an adjacent duplicate pair.
3. On successful socket open, reset dedupe and ensure one immediate fitted
   resize is sent while the socket is open.
4. Coalesce/delay event bursts from `ResizeObserver`, `window.resize`,
   `orientationchange`, and `visualViewport.resize`. If the root currently has
   `keyboard-open`, do not generate a resize storm; after it clears, the next
   viewport event must settle a fresh size.
5. Add one expand toggle. It changes only component/document classes and
   schedules a post-layout fit/resize. Enter and exit each produce a fresh
   resize without recreating xterm or the connection. If dedupe would suppress
   a required post-layout report, reset it only for the explicit fullscreen
   toggle.
6. Keep styling component-scoped and minimal: expanded mode must produce a
   meaningful host dimension change on mobile and keep the panel visible.
7. Dispose all resize resources and remove `terminal-expanded` on unmount.

## 9. Verification commands

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'initial open|portrait-to-landscape|repeated same-dimension|keyboard-open resize|fullscreen enter and exit each|reopen with meaningful'
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'task route mounts|delayed socket open|socket close reconnects|navigation away closes|pty output corpus keeps|reopening the task route|printable, control|repeated printable|multiline Unicode|Hide keyboard|typing after manual|supported Ctrl'
rtk npm run web:check
```

## 10. Acceptance criteria

- All six focused resize/fullscreen cases pass.
- Every resize pair is positive/integer and adjacent duplicates are absent.
- Same-dimension bursts add no immediate duplicate frames.
- Keyboard-open bursts add at most one frame; closing settles a fresh frame.
- Fullscreen enter/exit each resize without adding a socket or surface.
- Prior 13 lifecycle/input cases and web checks remain green.
- Only `TaskTerminal.svelte` changes.

## 11. Stop conditions

- Passing requires backend/test/global-helper edits or a new dependency.
- Fullscreen requires recreating the terminal/socket.
- Prior cases regress, unrelated failures block proof, or the component delta
  exceeds roughly 400 lines.
