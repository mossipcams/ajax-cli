# TDD Implementation Packet: iOS Safari Terminal Scroll And Size

## 1. Goal

Make the Web Cockpit raw terminal usable on iPhone 15 Pro Safari by ensuring
touch drags over xterm child layers are intercepted and translated into local
xterm `term.scrollLines()` calls, and by tightening mobile-only terminal sizing
so more terminal rows fit without changing desktop behavior.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

Production files:

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

Generated files, only after source tests pass and only if `npm run web:build`
changes them:

- `crates/ajax-web/web/dist/app.js`
- `crates/ajax-web/web/dist/app.css`
- `crates/ajax-web/web/dist/index.html`

## 3. Forbidden changes

- Do not edit `crates/ajax-cli/tests/smoke_user_flows.rs`.
- Do not edit unrelated tests.
- Do not delete, weaken, or skip existing terminal tests.
- Do not reintroduce terminal mode tabs, Live/snapshot/composer terminal mode,
  or tmux scroll forwarding.
- Do not synthesize browser wheel events as the implementation path.
- Do not send scroll gestures over the websocket as input frames.
- Do not change backend terminal websocket routing, PTY bridge code, task
  lifecycle code, registry code, or `architecture.md`.
- Do not reduce global `input`, `textarea`, or `select` font size below 16px.
- Do not perform unrelated styling cleanup or palette/layout refactors.

## 4. Architecture context

`architecture.md` is the required source of truth for this worktree. It says
`ajax-web` owns the browser Cockpit adapter, browser shell assets, HTTP/WebSocket
routing, and shell assets. It also says Web Cockpit task terminals are raw
xterm/tmux-first on mobile and desktop, and raw terminal hardening must preserve
reconnect, resize debounce, sticky Ctrl, scroll interception, and readable mobile
font.

This change stays inside the browser presentation adapter:

- `TerminalRawView.svelte` owns xterm setup, websocket input/output, resize
  debounce, local scrollback interception, sticky Ctrl, and the mobile terminal
  CSS surface.
- `TerminalRawView.test.ts` already mocks xterm, FitAddon, ZerolagInputAddon,
  WebSocket, visualViewport, and terminal scroll methods.

Graphify input is missing in this turn because no Graphify query tool is
exposed. Do not broaden the change beyond these files without first obtaining a
Graphify map or stopping for guidance.

## 5. Code anchors

Serena anchors in `TerminalRawView.svelte`:

- `const MOBILE_FONT_SIZE = 14;` near line 96.
- `const DESKTOP_FONT_SIZE = 13;` near line 97.
- `const onTouchMove = (event: TouchEvent) => { ... }` near line 180.
- `container?.addEventListener("touchmove", onTouchMove, touchMoveOptions);`
  near line 219.
- `.terminal-host { ... padding: 8px; }` near line 570.
- `.terminal-keys { ... gap: 6px; padding: 6px 8px; }` near line 589.
- `.terminal-key { min-width: 44px; min-height: 40px; padding: 6px 10px;
  font-size: 13px; }` near line 598.
- `:global(.terminal-panel .xterm-viewport) { overflow-y: auto;
  -webkit-overflow-scrolling: touch; overscroll-behavior: contain; }` near
  line 682.

Existing helper to reuse:

- `wheelNotchesFromDrag` from `crates/ajax-web/web/src/terminalTouchScroll.ts`.
  It converts accumulated drag pixels into whole line notches and carries a
  remainder.

ast-grep anchors gathered from `TerminalRawView.test.ts`:

- Pattern: `it($NAME, async () => { $$$BODY })`
- Existing tests:
  - `uses a readable font size on a mobile viewport`
  - `uses a compact font size on a desktop viewport`
  - `scrolls local terminal scrollback on touch drag`
  - `scrolls back into history when the finger drags downward`
  - `leaves a stationary tap untouched so it can focus and open the keyboard`
  - `captures touch drags from xterm child layers before they can be swallowed`
  - `intercepts wheel scroll from xterm child layers into local scrollback`

ast-grep limitation:

- `ast-grep --lang tsx` did not produce useful anchors inside the mixed Svelte
  component script/style sections. Use the Serena/text anchors above for the
  Svelte edit locations.

Live browser observation from `http://ajax.mossyhome.net` with iPhone 15 Pro
emulation and Cloudflare Access headers:

- Dashboard loads at `393x659`, DPR 3.
- Opening `ajax-cli/release-please` mounts the terminal at
  `#/t/ajax-cli%2Frelease-please`.
- Observed xterm font size is `15px`, line height `22.5px`, xterm rect
  `341x404`, and 26 rendered rows.
- Touch drag observation was inconclusive because `.xterm-viewport` had
  `scrollHeight === clientHeight === 404`, so the DOM viewport had no overflow
  to visibly move.

## 6. Test-first instructions

Task 1 test:

- In `TerminalRawView.test.ts`, add or strengthen a test named:
  `intercepts iPhone touch drags from xterm child layers with scrollLines only`
- The test must:
  - stub `matchMedia` to match `(max-width: 767px)`
  - render `TerminalRawView.svelte`
  - emit socket `open` so websocket input is active
  - append an `.xterm-screen` child layer under `.task-terminal-viewport`
  - add a bubbling `touchmove` listener on the child that calls
    `stopPropagation()`
  - dispatch `touchstart` and a moved `touchmove` on the child layer
  - assert the moved event is `defaultPrevented`
  - assert `scrollLines` was called once per translated line with the expected
    sign
  - assert `socket.send` was not called with any `{ type: "input", ... }` frame
    caused by the scroll gesture
- Focused command that must fail before implementation if the current code does
  not satisfy the new behavior:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

Task 2 test:

- In `TerminalRawView.test.ts`, change the mobile font-size test to assert the
  intended compact iPhone size exactly after implementation, and keep the
  desktop test asserting desktop is smaller than 14.
- Add a CSS-facing test or component DOM style assertion named:
  `uses tighter mobile terminal chrome without changing desktop sizing`
- The test must assert the mobile terminal CSS contract after rendering:
  - terminal host has a mobile-specific compact padding rule
  - control keys remain tappable but are smaller than the current 40px-high
    mobile key bar contract
  - desktop media query behavior remains untouched
- Focused command that must fail before implementation:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

## 7. Production edit instructions

Task 1 production edit:

- Edit only the touch-scroll section inside `onMount` in
  `TerminalRawView.svelte`.
- Keep `wheelNotchesFromDrag(touchAccumPx, cellHeightPx())`.
- Keep `scrollLocalLines(notches)` calling `term.scrollLines(step)` in a loop.
- If the new test fails because `preventDefault()` happens too late or capture
  is insufficient for iOS/xterm layers, make the smallest change to ensure the
  capture listener owns the gesture after the threshold is crossed.
- Do not dispatch synthetic wheel events.
- Do not send websocket input for scroll gestures.

Task 2 production edit:

- Edit `MOBILE_FONT_SIZE` in `TerminalRawView.svelte` to the compact iPhone
  value proven by the test.
- Add mobile-only CSS under the existing `@media (max-width: 767px)` block in
  `TerminalRawView.svelte`:
  - reduce `.terminal-host` padding from the desktop/default `8px`
  - reduce `.terminal-keys` gap/padding
  - reduce `.terminal-key` min-height/padding/font-size while preserving
    practical touch targets
- Do not alter the desktop `@media (min-width: 768px)` height rule.
- Do not alter global input sizing in `styles.css`.

## 8. Verification commands

Focused TDD:

- `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

Web type/style validation:

- `rtk npm run web:check`

Build validation:

- `rtk npm run web:build`

Live browser validation:

- Use Playwright Chromium with iPhone 15 Pro emulation and Cloudflare Access
  headers against `http://ajax.mossyhome.net`.
- Open a task detail route so `[data-testid="task-terminal-panel"]` exists.
- Verify terminal sizing, font size, row count, key bar height, and touch-drag
  interception. If the selected task has no scrollback overflow, record that
  visible DOM scroll movement is inconclusive and rely on the component
  `scrollLines` test for the functional assertion.

Recommended broader validation when time permits:

- `rtk npm run verify`

## 9. Acceptance criteria

- The new touch test fails before the production edit if current behavior does
  not intercept the iPhone-style child-layer drag as specified.
- After implementation, touch drags over xterm child layers call
  `term.scrollLines()` locally and prevent default browser scrolling once the
  movement threshold is crossed.
- Scroll gestures do not produce websocket input frames.
- Stationary or tiny jitter taps still do not prevent default, so tapping can
  focus the terminal and open the keyboard.
- Mobile xterm font size and terminal chrome are more compact than the current
  live observation of `15px` font / `22.5px` line height / 40px key buttons.
- Desktop terminal sizing behavior remains unchanged.
- Focused web tests and `web:check` pass.
- `web:build` passes and generated dist changes, if any, are included.

## 10. Stop conditions

- Stop if a required edit would touch files outside the allowed list.
- Stop if the new behavior test passes before any production code change; report
  that the bug may be in deployed/generated assets, live environment, or manual
  interaction conditions rather than source behavior.
- Stop if fixing the issue requires websocket, PTY bridge, tmux, or backend
  runtime changes.
- Stop if the only way to make scrolling visible in the live browser is to alter
  task state or inject terminal output into a real user task.
- Stop if Graphify-derived architecture information is required to broaden the
  scope; no Graphify tool is available in this turn.
- Stop if focused tests fail for unrelated reasons after reverting only the
  in-task edits is not possible without touching unrelated user changes.
