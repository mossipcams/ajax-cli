# Plan: iOS Safari Terminal Fix

## Context

Live WebKit/iPhone testing against `https://ajax.mossyhome.net` showed the task terminal opens and WebSocket I/O works, but the terminal experience is still buggy:

- The task detail chrome consumes too much of the visible viewport before the terminal.
- The terminal content is horizontally clipped after xterm fits to the mobile container.
- The live shell shows an update banner, so validation must confirm built assets are refreshed after any source change.

## Task 1: Make Mobile Task Detail Terminal-First

- Failing behavior test to write:
  - Update or add a component test for `TaskDetail.svelte` that renders a task with actions and asserts the mobile terminal-first hooks are present:
    - task detail root exposes a stable terminal-first state/class while mounted.
    - the action/status chrome can be targeted by mobile CSS without relying on global element order.
  - This belongs in `crates/ajax-web/web/src/components/TaskDetail.test.ts`.
- Code to implement:
  - Add explicit mobile/full-screen class hooks to the task detail structure.
  - Tighten mobile CSS so the header/action/status chrome is compact and the terminal receives the remaining viewport as the primary flex child.
  - Keep desktop layout and task actions unchanged.
- Verification:
  - `npm run web:test -- --run crates/ajax-web/web/src/components/TaskDetail.test.ts`
  - iPhone/WebKit screenshot check: terminal starts substantially higher and no bottom nav/header overlap is visible.

## Task 2: Stabilize xterm Fit and Prevent Horizontal Clipping

- Failing behavior test to write:
  - Update `crates/ajax-web/web/src/components/TerminalPanel.test.ts` to simulate a container resize after mount and assert:
    - the terminal schedules a second fit/resize after the host is laid out.
    - resize messages are sent after the WebSocket is open.
    - mobile key bar focus still returns to the terminal.
- Code to implement:
  - Defer initial `fitAddon.fit()` until after the terminal host has a stable box.
  - Add a second post-open/post-resize fit tick when needed so WebKit/xterm does not keep stale columns.
  - Add CSS constraints for `.xterm`, `.xterm-screen`, and terminal host width so the canvas/viewport cannot exceed the panel width.
- Verification:
  - `npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts`
  - iPhone/WebKit terminal probe: compare `.terminal-panel`, `.xterm`, and `.xterm-screen` widths and confirm no overflow/clipping.

## Task 3: Refresh Built Assets and Full Web Validation

- Failing behavior test to write:
  - No new test; this is generated asset and validation work after behavior tests pass.
- Code to implement:
  - Run the web build so `crates/ajax-web/web/dist/app.css` and `crates/ajax-web/web/dist/app.js` match source.
- Verification:
  - `npm run web:check`
  - `npm run web:test -- --run`
  - `npm run web:build:check`
  - iPhone/WebKit live/local smoke script with screenshots of the terminal before and after typing.

## Task 4: iOS Safari User-Flow Smoke Coverage

- Failing behavior test to write:
  - Add or update Playwright smoke coverage for the mobile Web Cockpit flow if an existing web smoke harness can run locally without private backend state.
  - If the existing smoke harness is not suitable for the live terminal backend, add a checked-in script or documented smoke command outside `tests/` that exercises the live/private iOS Safari flow with operator-provided Cloudflare Access headers.
- Code to implement:
  - Cover the normal actions a mobile operator takes:
    - load dashboard and verify cockpit cards render.
    - scroll the dashboard/project list without horizontal page movement.
    - open a task detail from the inbox.
    - verify document/page scrolling is locked while the task terminal is open.
    - scroll terminal history inside xterm without the page fighting it.
    - tap terminal, type printable text, press Enter, and verify input reaches the WebSocket.
    - use terminal key bar buttons: Esc, Tab, Ctrl-C, arrows, and sticky Ctrl.
    - use Back to return to the dashboard.
    - exercise non-destructive task actions where safe, or verify action controls are reachable without triggering destructive operations.
    - verify Drop/Ship confirmation behavior is visible but do not complete destructive actions during smoke.
    - verify bottom navigation remains usable after returning from task detail.
  - Capture screenshots and DOM metrics for regressions:
    - task detail initial view.
    - terminal after history scroll.
    - terminal after typing.
    - dashboard after Back.
- Verification:
  - Run the smoke flow against iPhone/WebKit.
  - Confirm no request failures, page errors, unexpected horizontal overflow, document scroll drift, clipped xterm width, or inaccessible controls.
  - Include any live-environment limitations in the final report.
