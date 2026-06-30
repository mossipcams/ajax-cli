# Plan: iOS Terminal Scrolling

## Context

Focused iPhone/WebKit probing showed the main scrolling issue is not page scroll lock anymore:

- The document stays locked while the task terminal is open.
- The xterm viewport has no browser scroll range in the active tmux/TUI screen (`scrollHeight === clientHeight`).
- A touch/drag gesture inside xterm is treated as terminal mouse input and sent to the PTY as escape sequences.
- That makes normal mobile scrolling feel broken and can pollute the running terminal session.

## Task 1: Prevent Touch-Drag Gestures From Polluting Terminal Input

- Failing behavior test to write:
  - Add a `TerminalPanel.test.ts` case that dispatches a touch-style pointer drag on the terminal viewport and asserts no mouse-report escape input is sent to the WebSocket.
  - The test should preserve normal keyboard input and terminal key bar input.
- Code to implement:
  - Add a mobile touch-scroll guard around the xterm viewport.
  - Detect vertical touch gestures and prevent xterm mouse reporting from treating those gestures as PTY input.
  - Keep taps/focus and keyboard typing behavior unchanged.
- Verification:
  - `npm run web:test -- --run src/components/TerminalPanel.test.ts`
  - iPhone/WebKit probe: dragging in the terminal no longer sends mouse escape sequences.

## Task 2: Provide a Mobile-Friendly Scroll/History Interaction

- Failing behavior test to write:
  - Add a `TerminalPanel.test.ts` case for touch vertical drag mapping to an intentional terminal navigation action.
  - Cover both upward and downward gestures.
- Code to implement:
  - Translate deliberate vertical touch drags into safe scroll/navigation input, likely PageUp/PageDown or bounded arrow-key bursts, after validating which works best with the attached tmux/Codex screen.
  - Avoid firing navigation for small taps or horizontal gestures.
  - Keep the existing key bar available for precise control.
- Verification:
  - `npm run web:test -- --run src/components/TerminalPanel.test.ts`
  - iPhone/WebKit smoke: drag up/down in the terminal produces useful history movement or no-op safe behavior, without corrupting the prompt.

## Task 3: Update iOS Smoke Coverage and Assets

- Failing behavior test to write:
  - Update `scripts/ios-terminal-smoke.mjs` so it explicitly fails if touch scroll sends raw mouse-report escape input into the WebSocket.
- Code to implement:
  - Extend the smoke script with terminal drag metrics and WebSocket input assertions.
  - Rebuild `dist` assets after source changes.
- Verification:
  - `npm run web:check`
  - `npm run web:test -- --run`
  - `npm run web:build:check`
  - iPhone/WebKit smoke against local rebuilt assets proxied to the live backend.
