# TDD Implementation Packet: Ajax-Owned Touch Scrollback

## 1. Goal

Touch dragging inside the web task terminal should scroll xterm's local
scrollback with `terminal.scrollLines(...)` instead of dispatching synthetic
wheel events that xterm may forward into tmux or the foreground app.

## 2. Allowed files

Production:

- `crates/ajax-web/web/src/components/TerminalPanel.svelte`

Test:

- `crates/ajax-web/web/src/components/TerminalPanel.test.ts`

Do not edit `crates/ajax-web/web/src/terminalTouchScroll.ts`; reuse its existing
`wheelNotchesFromDrag(...)` helper unchanged.

## 3. Forbidden changes

- Do not change Rust websocket, PTY, tmux, or route code.
- Do not change `crates/ajax-web/src/adapters/terminal_pty.rs`.
- Do not replace `tmux attach-session` in this packet.
- Do not change terminal input, zerolag input, control-key bar, output decoding,
  or websocket message framing behavior.
- Do not edit generated files under `crates/ajax-web/web/dist/`.
- Do not edit smoke tests, especially
  `crates/ajax-cli/tests/smoke_user_flows.rs`.
- Do not weaken or delete existing assertions; update the touch-scroll
  assertions to the new behavior.

## 4. Architecture context

`ajax-web` owns the browser Cockpit adapter. `TerminalPanel.svelte` is the
browser presentation component for the live terminal path. It opens a websocket
with `openTaskTerminalSocket(handle)`, renders xterm, and handles local browser
input/scroll behavior before frames cross the websocket boundary.

This packet is presentation-only. It must not change the backend contract:
input frames remain `{type:"input",data}`, resize frames remain
`{type:"resize",cols,rows}`, and output frames continue to be decoded and
written into xterm.

Graphify architecture map was not available in this session. Architecture
boundaries above are from `architecture.md`, direct file reads, and Serena
pattern search. If a Graphify-derived map is required before execution, stop and
generate or request it before editing.

## 5. Code anchors

Serena pattern context:

- `crates/ajax-web/web/src/components/TerminalPanel.svelte:6` imports
  `wheelNotchesFromDrag` from `../terminalTouchScroll`.
- `TerminalPanel.svelte:78-90` comment currently describes touch drags being
  translated into synthetic wheel events and possibly forwarded to tmux.
- `TerminalPanel.svelte:99-112` defines `dispatchWheel(...)`, which calls
  `term.element?.dispatchEvent(new WheelEvent("wheel", ...))`.
- `TerminalPanel.svelte:129-139` calls `wheelNotchesFromDrag(...)`, computes
  `step`, loops over `notches`, and calls `dispatchWheel(step, ...)`.
- `TerminalPanel.test.ts:30-43` defines `MockTerminal`; add `scrollLines` there.
- `TerminalPanel.test.ts:446-464` currently asserts a touch drag dispatches
  three `WheelEvent`s with `deltaY === 1`.
- `TerminalPanel.test.ts:466-477` currently asserts a downward drag dispatches
  wheel events with `deltaY === -1`.
- `TerminalPanel.test.ts:479-491` currently asserts a stationary tap sends no
  wheel events and does not prevent default.

ast-grep anchors:

- Helper export confirmed with:
  `rtk ast-grep -p 'export function wheelNotchesFromDrag($$$ARGS): WheelNotches { $$$BODY }' --lang ts crates/ajax-web/web/src/terminalTouchScroll.ts`
  matching `crates/ajax-web/web/src/terminalTouchScroll.ts:25`.
- Existing test cases confirmed with:
  `rtk ast-grep -p 'it($NAME, async () => { $$$BODY })' --lang ts crates/ajax-web/web/src/components/TerminalPanel.test.ts`
  matching the touch-scroll tests at `TerminalPanel.test.ts:447`,
  `TerminalPanel.test.ts:467`, and `TerminalPanel.test.ts:480`.
- ast-grep did not parse `TerminalPanel.svelte` as TypeScript in this setup for
  the optional pattern `term.element?.dispatchEvent($EVENT)`. Use the Serena and
  `rg` anchors above for the Svelte edit. If an executor requires ast-grep
  matches inside Svelte before editing, stop.

## 6. Test-first instructions

Edit `crates/ajax-web/web/src/components/TerminalPanel.test.ts` first.

1. Add a top-level mock:

   ```ts
   const scrollLines = vi.fn();
   ```

2. Add `scrollLines = scrollLines;` to the `MockTerminal` class next to
   `scrollToBottom = scrollToBottom;`.

3. Reset `scrollLines` through the existing `vi.restoreAllMocks()` path; no
   separate reset is required unless the focused tests become order-sensitive.

4. Rename the test
   `"translates a touch drag into wheel events on the xterm element"` to:

   ```ts
   "scrolls local terminal scrollback on touch drag"
   ```

5. In that test, remove `wheelEvents` and `lastElement.addEventListener(...)`.
   Keep the same touch sequence. Assert:

   ```ts
   expect(scrollLines).toHaveBeenCalledWith(1);
   expect(scrollLines).toHaveBeenCalledTimes(3);
   expect(move.defaultPrevented).toBe(true);
   ```

6. In `"scrolls back into history when the finger drags downward"`, remove
   `wheelEvents` and assert:

   ```ts
   expect(scrollLines).toHaveBeenCalledWith(-1);
   expect(scrollLines.mock.calls.length).toBeGreaterThan(0);
   ```

7. In `"leaves a stationary tap untouched so it can focus and open the keyboard"`,
   remove `wheelEvents` and assert:

   ```ts
   expect(scrollLines).not.toHaveBeenCalled();
   expect(move.defaultPrevented).toBe(false);
   ```

Run this focused command and confirm it fails before production edits:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "touch drag"
```

Expected failure: `scrollLines` was not called because production code still
dispatches synthetic `WheelEvent`s.

Also run:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "finger drags downward"
```

Expected failure: `scrollLines` was not called for downward drag.

## 7. Production edit instructions

Edit only the touch-scroll section in
`crates/ajax-web/web/src/components/TerminalPanel.svelte`.

1. Replace the comment at `TerminalPanel.svelte:78-90` with wording that says
   touch drags scroll Ajax-owned xterm scrollback directly and do not forward
   wheel events into tmux or the foreground terminal app.

2. Delete the local `dispatchWheel(deltaY, clientX, clientY)` function at
   `TerminalPanel.svelte:99-112`.

3. In `onTouchMove`, keep:

   ```ts
   const { notches, remainderPx } = wheelNotchesFromDrag(touchAccumPx, cellHeightPx());
   touchAccumPx = remainderPx;
   if (notches === 0) return;
   if (event.cancelable) event.preventDefault();
   ```

4. Replace the `step` loop that calls `dispatchWheel(...)` with direct local
   scrollback movement:

   ```ts
   const step = notches > 0 ? 1 : -1;
   for (let i = 0; i < Math.abs(notches); i += 1) {
     term.scrollLines(step);
   }
   ```

Keep the one-line-at-a-time behavior so the new implementation preserves the
current helper's clamping and line-notch semantics.

## 8. Verification commands

Focused failure before implementation, then pass after implementation:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "touch drag"
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "finger drags downward"
```

After both focused tests pass:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts
rtk npm run web:test -- --run crates/ajax-web/web/src/terminalTouchScroll.test.ts
```

Do not run or claim full Rust validation for this packet unless web build or
generated asset assertions are touched, which this packet forbids.

## 9. Acceptance criteria

- The focused touch-drag test fails before the Svelte production edit because
  `scrollLines` is not called.
- The focused downward-drag test fails before the Svelte production edit because
  `scrollLines` is not called.
- After production edit, upward touch drag calls `scrollLines(1)` three times
  for the existing 60px/18px fixture and prevents default.
- After production edit, downward touch drag calls `scrollLines(-1)` at least
  once.
- Stationary tap still calls no scroll method and does not prevent default.
- No synthetic wheel-event assertions remain in the touch-scroll tests.
- `wheelNotchesFromDrag(...)` remains unchanged and its helper tests still pass.

## 10. Stop conditions

- Stop if `TerminalPanel.svelte` no longer imports `wheelNotchesFromDrag`.
- Stop if the touch-scroll code no longer has an `onTouchMove` branch using
  `wheelNotchesFromDrag`.
- Stop if xterm's mock terminal cannot expose `scrollLines` without broader
  test harness changes.
- Stop if the new `scrollLines` tests pass before editing production code.
- Stop if satisfying this packet requires changes outside the allowed files.
- Stop if unrelated tests fail in a way that is not caused by this packet.
- Stop if a Graphify-generated architecture map is mandatory before execution;
  one was not available when this packet was written.
