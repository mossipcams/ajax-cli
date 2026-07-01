# Mobile Terminal Scroll And Resize Plan

## Scope

Improve the current xterm-over-WebSocket terminal path without replacing the
tmux attach backend yet. This phase targets the most damaging mobile behaviors:
scroll events being forwarded into tmux/apps, viewport refits yanking the user to
bottom, keyboard-driven resize churn, and unreadable mobile font sizing.

Later architecture work can move mobile toward a tmux capture/send-keys
viewer-controller path and add a command composer. This plan keeps the first
slice small and testable.

## Task 1: Own Touch Scrolling In Ajax

- Failing behavior test:
  - Update `crates/ajax-web/web/src/components/TerminalPanel.test.ts` so touch
    drag assertions expect `Terminal.scrollLines(...)` calls instead of
    dispatched `WheelEvent`s on `term.element`.
  - Add/adjust the mock terminal to expose `scrollLines`.
  - Keep the existing stationary tap assertion: small movement must not scroll
    and must not prevent default.
- Code to implement:
  - In `crates/ajax-web/web/src/components/TerminalPanel.svelte`, replace
    `dispatchWheel(...)` with direct `term.scrollLines(notches)` behavior.
  - Update comments so they describe Ajax-owned scrollback rather than xterm
    wheel forwarding.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "touch drag"`
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "finger drags downward"`

## Task 2: Show New Output Without Yanking Scroll

- Failing behavior test:
  - Add a `TerminalPanel.test.ts` test that simulates the user scrolled away
    from bottom, receives output, and expects a visible `New output` control
    while `scrollToBottom` is not called.
  - Add a second assertion in that test, or a focused companion test, that
    tapping the control calls `scrollToBottom` and hides the control.
- Code to implement:
  - Track a `hasUnseenOutput` UI state in `TerminalPanel.svelte`.
  - When output arrives and `pinnedToBottom` is false, set the state instead of
    scrolling.
  - Render a compact bottom action labeled `New output ↓`; on click, scroll to
    bottom, clear unseen output, and focus the terminal.
  - Clear unseen output when the xterm scroll event reports the viewport is
    pinned to bottom again.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "New output"`

## Task 3: Debounce Keyboard-Driven Server Resize

- Failing behavior test:
  - Replace the existing visualViewport test expectation that resize immediately
    sends a server resize frame.
  - Add a test where `visualViewport.resize` fits locally right away, does not
    call `socket.send` immediately, then sends one resize frame after a stable
    debounce window.
  - Add or adjust a test proving viewport resize does not call
    `scrollToBottom` while the user is not pinned to bottom.
- Code to implement:
  - Split refit scheduling in `TerminalPanel.svelte` into local fit and server
    resize scheduling.
  - On `visualViewport` resize/scroll, call `fitAddon.fit()` locally but delay
    the resize frame until the viewport has been stable for about 300ms.
  - Continue sending resize promptly for socket open, container resize, window
    resize, and orientation changes unless the same debounce path is clearly
    cleaner.
  - Only call `term.scrollToBottom()` after refit when `pinnedToBottom` is true.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "visual viewport"`
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "scrolled up"`

## Task 4: Raise Mobile Terminal Font Size

- Failing behavior test:
  - Add or update a `TerminalPanel.test.ts` mock assertion that the xterm
    terminal is constructed with a readable default `fontSize` of `10`.
- Code to implement:
  - Change `TerminalPanel.svelte` terminal construction from `fontSize: 6` to
    `fontSize: 10`.
  - Keep the existing monospace family and theme unchanged.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "font"`

## Task 5: Report First-Phase Validation

- Failing behavior test:
  - None; validation task only.
- Code to implement:
  - None beyond formatting or generated web build artifacts if required by the
    repo checks.
- Verification:
  - Focused web test file:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts`
  - Helper tests:
    `rtk npm run web:test -- --run crates/ajax-web/web/src/terminalTouchScroll.test.ts`
  - Web checks as available:
    `rtk npm run web:check`
  - If web assets are generated or Rust asset assertions are affected, run:
    `rtk npm run web:build`
    `rtk cargo nextest run -p ajax-web --test-threads=1 -E 'test(install)'`

## Out Of Scope For This Phase

- Replacing `tmux attach-session` with a capture-pane/send-keys
  viewer-controller path.
- Adding the mobile command composer.
- Changing Rust WebSocket/PTTY bridge behavior.
- Editing smoke tests in `crates/ajax-cli/tests/smoke_user_flows.rs`.
