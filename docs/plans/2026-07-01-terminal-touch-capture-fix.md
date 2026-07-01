# Terminal Scrollback Intercept Fix Plan

## Task 1: Reproduce Wheel And Touch Events Swallowed Or Forwarded By Xterm

- Failing behavior test:
  - Update `crates/ajax-web/web/src/components/TerminalPanel.test.ts`.
  - Add a test that appends a child element inside `.task-terminal-viewport`,
    installs a bubbling `touchmove` listener on that child that calls
    `stopPropagation()`, dispatches the drag from the child, and expects
    `scrollLines(1)` to be called.
  - This models the real xterm screen/layer being the touch target instead of
    the terminal host.
  - Add a wheel test that dispatches a cancelable `WheelEvent` from an xterm-like
    child element and expects Ajax to call `scrollLines(...)` directly and
    `preventDefault()`, without relying on xterm's wheel handlers.
- Code to implement:
  - None in this task.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "captures touch drags"`
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "intercepts wheel"`
  - Expected failure before implementation: `scrollLines` is not called.

## Task 2: Intercept Wheel And Touch In Capture Phase

- Failing behavior test:
  - Use the test from Task 1.
- Code to implement:
  - In `crates/ajax-web/web/src/components/TerminalPanel.svelte`, add a wheel
    handler that converts wheel delta into local scroll lines and calls
    `term.scrollLines(...)` directly.
  - The wheel handler must call `preventDefault()` for real scroll movement so
    xterm/tmux/application mouse handling never receives scroll intent.
  - Change
    `touchstart`, `touchmove`, `touchend`, and `touchcancel` listener options so
    Ajax observes events during capture.
  - Keep `touchmove` as `passive: false` so `preventDefault()` still works after
    a real drag.
  - Use shared option constants so `removeEventListener(...)` uses matching
    capture settings.
  - Comment the behavior plainly: Ajax always uses local scrollback for
    wheel/touch scrolling and does not forward scroll to the terminal
    application.
- Verification:
  - Focused test from Task 1 passes.
  - Existing touch-scroll tests still pass:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "touch"`
  - Wheel focused test passes.

## Task 3: Rebuild And Validate

- Failing behavior test:
  - None; validation only.
- Code to implement:
  - Rebuild web assets if Svelte source changed.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts`
  - `rtk npm run web:check`
  - `rtk npm run web:build`
  - `rtk cargo fmt --check`
  - `rtk cargo check --all-targets --all-features`

Plan ready. Approve to proceed.
