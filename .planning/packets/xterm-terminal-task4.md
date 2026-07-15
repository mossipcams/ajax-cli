# Xterm terminal rebuild — Task 4 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Add the permanent renderer-neutral interaction surface, scrollback follow/new
output behavior, gesture containment, fullscreen input continuity, and a
bounded persisted pinch font size to the accepted xterm component.

## 3. Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## 4. Forbidden changes

- Do not edit tests, fixtures, connection/backend code, dependencies, docs,
  TaskDetail, global helpers/styles, or generated assets.
- Do not recreate deleted gesture, geometry, selection, output-policy, or
  clipboard helper modules. Keep the minimum concrete logic in the component.
- Do not add synthetic PTY input for scroll, long press, fullscreen, pinch, or
  New output.
- Do not reconnect/recreate xterm for fullscreen/pinch.
- Preserve Tasks 1–3 behavior and touch no other path.

## 5. Context evidence

- Graphify: `NOT_REQUIRED`; gestures and scroll follow remain presentation
  state and cannot become task/terminal truth.
- Serena: `NOT_REQUIRED`; one existing component contains all exact renderer,
  fit, input, and cleanup anchors.
- ast-grep/API evidence: xterm exposes `onScroll` and `scrollToBottom()` in
  `node_modules/@xterm/xterm/typings/xterm.d.ts:990,1227`; `Terminal.options.fontSize`
  is writable (`:871–876`). No helper dependency is needed.
- Test harness evidence: `scrollInteractionSurfaceAway` directly decreases
  the permanent locator element's `scrollTop` and dispatches `scroll`; the
  locator must therefore be a real scroll owner (or faithfully synchronize one)
  rather than a decorative wrapper.
- Desired behavior: permanent cases at
  `terminal-behavior.test.ts:616–745`.

## 6. Code anchors

- `TaskTerminal.svelte` xterm open path: expose exactly one visible
  `data-testid="terminal-interaction-surface"` on the surface that receives
  terminal focus/gestures and owns/synchronizes scrollback.
- Existing connection `onOutput`: preserve ordered `term.write`; add only
  follow/new-output state around it.
- Existing resize scheduler: pinch changes `term.options.fontSize`, resets
  dedupe, and invokes the accepted post-layout fit/resize path.
- Existing panel controls: render `New output ↓` only while output arrives and
  the user is away from the live bottom.

## 7. Test-first instructions

Before editing, run:

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'stable terminal interaction|reading scrollback|long press|synthetic scroll gesture|fullscreen enter and exit keep|outward pinch'
```

Expected RED: the interaction locator and New output behavior are absent, and
pinch cannot change/persist size. Do not modify tests.

## 8. Edit instructions

1. Expose one stable interaction locator that is visible, focusable via clicks,
   contains synthetic drag/long-press without document scrolling or input, and
   whose `scrollTop` mutation drives xterm scrollback. Prefer the existing xterm
   scroll viewport if it can satisfy all actions; otherwise use the smallest
   synchronized wrapper. Do not add a general gesture layer.
2. Track whether xterm is at the live bottom using xterm scroll/buffer state.
   Normal output while following remains at bottom with no button. Output while
   away shows `New output ↓`; clicking it calls `scrollToBottom`, hides the
   button, and sends no PTY input.
3. Preserve click-to-focus and exact input before/after fullscreen. Long press
   and synthetic drag must emit no connection input and leave document scroll
   unchanged.
4. Use `localStorage.ajax.terminal.fontSize`, default 13, clamp to 7–20, and a
   12px activation deadzone. On a two-touch start record distance/font; on
   outward move prevent default, update font from the distance ratio, refit
   locally; on touch end persist and trigger one deduplicated PTY resize. Avoid
   duplicate adjacent size frames.
5. Load the persisted valid font before creating xterm so reload reports a
   size different from the original default. Invalid/missing values fall back
   to 13.
6. Clean up scroll/touch listeners and state with existing component disposal.
   Use Svelte handlers when simpler.

## 9. Verification commands

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'stable terminal interaction|reading scrollback|long press|synthetic scroll gesture|fullscreen enter and exit keep|outward pinch'
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'task route mounts|delayed socket open|socket close reconnects|navigation away closes|pty output corpus keeps|reopening the task route|printable, control|repeated printable|multiline Unicode|Hide keyboard|typing after manual|supported Ctrl|initial open|portrait-to-landscape|repeated same-dimension|keyboard-open resize|fullscreen enter and exit each|reopen with meaningful'
rtk npm run web:check
```

## 10. Acceptance criteria

- All six focused interaction/scroll/pinch cases pass.
- Stable locator is visible and does not create a second terminal surface.
- Scroll/long-press/New output emit zero PTY input.
- Fullscreen keeps one surface/socket and ordered input.
- Pinch produces a fresh deduplicated resize, persists through reload, and
  keyboard input still works before/after reload.
- Prior 19 cases and web checks remain green.
- Only `TaskTerminal.svelte` changes.

## 11. Stop conditions

- Passing requires a test/backend/shared-helper edit or dependency.
- A gesture path would synthesize PTY input or change task truth.
- Prior behavior regresses, unrelated failure blocks proof, or the component
  delta exceeds roughly 400 lines.
