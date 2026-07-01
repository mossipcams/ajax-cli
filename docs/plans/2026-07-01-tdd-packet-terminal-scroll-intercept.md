# TDD Implementation Packet: Terminal Scroll Intercept

## 1. Goal

Wheel and touch scrolling inside the web terminal must always be intercepted by
Ajax and mapped directly to `terminal.scrollLines(...)`. Scroll intent must use
xterm local scrollback and must not be forwarded to tmux or the foreground
terminal application.

## 2. Allowed files

Production:

- `crates/ajax-web/web/src/components/TerminalPanel.svelte`

Test:

- `crates/ajax-web/web/src/components/TerminalPanel.test.ts`

Generated, only after implementation passes:

- `crates/ajax-web/web/dist/app.css`
- `crates/ajax-web/web/dist/app.js`

Do not edit `crates/ajax-web/web/src/terminalTouchScroll.ts`; reuse
`wheelNotchesFromDrag(...)` unchanged.

## 3. Forbidden changes

- Do not change Rust websocket, PTY, tmux, or route code.
- Do not change `crates/ajax-web/src/adapters/terminal_pty.rs`.
- Do not replace `tmux attach-session` in this packet.
- Do not dispatch synthetic `WheelEvent`s into xterm.
- Do not rely on xterm wheel handlers or application mouse mode for scrolling.
- Do not change terminal input, zerolag input, control-key bar, output decoding,
  websocket message framing, resize debounce, or `New output ↓` behavior.
- Do not edit smoke tests, especially
  `crates/ajax-cli/tests/smoke_user_flows.rs`.
- Do not delete or weaken existing assertions.

## 4. Architecture context

`ajax-web` owns the browser Cockpit adapter. `TerminalPanel.svelte` is the
browser presentation component for the live terminal path: it opens the task
terminal websocket, renders xterm, and handles browser-local input/scroll
behavior before frames cross the websocket boundary.

This packet is presentation-only. It must preserve the backend contract:
input frames remain `{type:"input",data}`, resize frames remain
`{type:"resize",cols,rows}`, and output frames continue to be decoded and
written into xterm.

Graphify architecture map was not generated in this session. Architecture
boundaries above are reconstructed from `architecture.md`, direct file reads,
and the existing `ajax-web` component/test layout. If a Graphify-derived map is
mandatory before execution, stop and generate or request it before editing.

## 5. Code anchors

Existing helper and tests:

- `crates/ajax-web/web/src/terminalTouchScroll.ts:25` defines
  `export function wheelNotchesFromDrag(...)`.
- `TerminalPanel.test.ts:7` defines the existing `scrollLines` mock.
- `TerminalPanel.test.ts:42` exposes `scrollLines` on `MockTerminal`.
- `TerminalPanel.test.ts:515` defines `makeTouch(...)`.
- `TerminalPanel.test.ts:523` has `"scrolls local terminal scrollback on touch drag"`.
- `TerminalPanel.test.ts:540` has `"scrolls back into history when the finger drags downward"`.
- `TerminalPanel.test.ts:551` has `"leaves a stationary tap untouched so it can focus and open the keyboard"`.

Current production scroll section:

- `TerminalPanel.svelte:7` imports `wheelNotchesFromDrag`.
- `TerminalPanel.svelte:80-87` comments on Ajax-owned touch scrollback.
- `TerminalPanel.svelte:90-126` defines touch state, `cellHeightPx`,
  `onTouchStart`, `onTouchMove`, and calls `term.scrollLines(step)`.
- `TerminalPanel.svelte:134-137` registers touch listeners on `container` in
  bubble phase.
- `TerminalPanel.svelte:316-319` removes those touch listeners without options.

ast-grep anchors:

- Helper export confirmed with:
  `rtk ast-grep -p 'export function wheelNotchesFromDrag($$$ARGS): WheelNotches { $$$BODY }' --lang ts crates/ajax-web/web/src/terminalTouchScroll.ts`
- Existing async tests confirmed with:
  `rtk ast-grep -p 'it($NAME, async () => { $$$BODY })' --lang ts crates/ajax-web/web/src/components/TerminalPanel.test.ts`

Note: ast-grep was not used for `TerminalPanel.svelte` because the current setup
does not parse Svelte component script blocks reliably. Use the `rg` anchors
above for the Svelte edit. Stop if Svelte AST anchors are mandatory.

## 6. Test-first instructions

Edit `crates/ajax-web/web/src/components/TerminalPanel.test.ts` first.

Add this helper near `makeTouch(...)`:

```ts
function appendXtermLayer(host: HTMLElement): HTMLElement {
  const layer = document.createElement("div");
  layer.className = "xterm-screen";
  host.appendChild(layer);
  return layer;
}
```

Add this test after the existing touch-scroll tests:

```ts
it("captures touch drags from xterm child layers before they can be swallowed", async () => {
  const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
  const host = container.querySelector(".task-terminal-viewport") as HTMLElement;
  const layer = appendXtermLayer(host);
  layer.addEventListener("touchmove", (event) => event.stopPropagation());

  layer.dispatchEvent(makeTouch("touchstart", 200));
  const move = makeTouch("touchmove", 140);
  layer.dispatchEvent(move);

  expect(scrollLines).toHaveBeenCalledWith(1);
  expect(scrollLines).toHaveBeenCalledTimes(3);
  expect(move.defaultPrevented).toBe(true);
});
```

Add this wheel test after the touch tests:

```ts
it("intercepts wheel scroll from xterm child layers into local scrollback", async () => {
  const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
  const host = container.querySelector(".task-terminal-viewport") as HTMLElement;
  const layer = appendXtermLayer(host);
  layer.addEventListener("wheel", (event) => event.stopPropagation());

  const wheel = new WheelEvent("wheel", {
    deltaY: 3,
    deltaMode: WheelEvent.DOM_DELTA_LINE,
    bubbles: true,
    cancelable: true,
  });
  layer.dispatchEvent(wheel);

  expect(scrollLines).toHaveBeenCalledWith(1);
  expect(scrollLines).toHaveBeenCalledTimes(3);
  expect(wheel.defaultPrevented).toBe(true);
});
```

Run focused tests before production edits:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "captures touch drags"
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "intercepts wheel"
```

Expected failures before implementation:

- Touch child-layer test: `scrollLines` is not called because the host listener
  is bubble-phase and the child stops propagation.
- Wheel child-layer test: `scrollLines` is not called because no Ajax wheel
  intercept handler exists.

## 7. Production edit instructions

Edit only the scroll section in
`crates/ajax-web/web/src/components/TerminalPanel.svelte`.

1. Update the comment at `TerminalPanel.svelte:80-87` to say Ajax intercepts
   both wheel and touch scrolling, always uses local xterm scrollback, and does
   not forward scroll to the terminal application.

2. Add a local helper inside `onMount` near `cellHeightPx`:

   ```ts
   const scrollLocalLines = (lines: number) => {
     const step = lines > 0 ? 1 : -1;
     for (let i = 0; i < Math.abs(lines); i += 1) {
       term.scrollLines(step);
     }
   };
   ```

3. Replace the touch `step` loop in `onTouchMove` with:

   ```ts
   scrollLocalLines(notches);
   ```

4. Add a wheel handler near the touch handlers:

   ```ts
   const onWheel = (event: WheelEvent) => {
     const lineDelta =
       event.deltaMode === WheelEvent.DOM_DELTA_PIXEL
         ? Math.trunc(event.deltaY / cellHeightPx())
         : Math.trunc(event.deltaY);
     if (lineDelta === 0) return;
     if (event.cancelable) event.preventDefault();
     scrollLocalLines(lineDelta);
   };
   ```

   Keep pixel-wheel conversion intentionally simple and local. Do not dispatch a
   wheel event. Do not call websocket send APIs.

5. Replace listener registration at `TerminalPanel.svelte:134-137` with shared
   options:

   ```ts
   const touchStartOptions = { passive: true, capture: true };
   const touchMoveOptions = { passive: false, capture: true };
   const scrollEndOptions = { passive: true, capture: true };
   const wheelOptions = { passive: false, capture: true };

   container?.addEventListener("touchstart", onTouchStart, touchStartOptions);
   container?.addEventListener("touchmove", onTouchMove, touchMoveOptions);
   container?.addEventListener("touchend", onTouchEnd, scrollEndOptions);
   container?.addEventListener("touchcancel", onTouchEnd, scrollEndOptions);
   container?.addEventListener("wheel", onWheel, wheelOptions);
   ```

6. Replace cleanup at `TerminalPanel.svelte:316-319` with matching options:

   ```ts
   container?.removeEventListener("touchstart", onTouchStart, touchStartOptions);
   container?.removeEventListener("touchmove", onTouchMove, touchMoveOptions);
   container?.removeEventListener("touchend", onTouchEnd, scrollEndOptions);
   container?.removeEventListener("touchcancel", onTouchEnd, scrollEndOptions);
   container?.removeEventListener("wheel", onWheel, wheelOptions);
   ```

If TypeScript rejects the inferred listener option object types, annotate them
with `AddEventListenerOptions` and keep the same runtime values.

## 8. Verification commands

Focused red-before/green-after:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "captures touch drags"
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "intercepts wheel"
```

Regression coverage:

```sh
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts -t "touch"
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts
rtk npm run web:check
rtk npm run web:build
```

After rebuilding generated assets:

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
```

## 9. Acceptance criteria

- The new child-layer touch test fails before production edits and passes after.
- The new child-layer wheel test fails before production edits and passes after.
- Touch drags from inside xterm child layers call `scrollLines(1)` three times
  for the existing 60px/18px fixture and prevent default.
- Wheel events from inside xterm child layers call `scrollLines(1)` three times
  for `deltaMode: DOM_DELTA_LINE, deltaY: 3` and prevent default.
- Existing upward drag, downward drag, and stationary tap tests still pass.
- No code dispatches synthetic `WheelEvent`s for terminal scrolling.
- No scroll handling sends websocket input or resize frames.
- Web build regenerates `dist/app.css`/`dist/app.js` if Svelte output changes.

## 10. Stop conditions

- Stop if `TerminalPanel.svelte` no longer imports `wheelNotchesFromDrag`.
- Stop if touch handling no longer lives in `onTouchMove` inside
  `TerminalPanel.svelte`.
- Stop if `TerminalPanel.test.ts` no longer exposes a `scrollLines` mock.
- Stop if the new tests pass before production edits.
- Stop if satisfying this packet requires Rust backend changes.
- Stop if satisfying this packet requires changing terminal input, websocket
  frame formats, resize behavior, or output decoding.
- Stop if unrelated tests fail in a way that is not caused by this packet.
- Stop if a Graphify-generated architecture map is mandatory before execution;
  one was not available when this packet was written.
