# TDD Packet: Priority 2 — long-press paste targets editable textarea

## 1. Goal

On terminal single-finger `touchstart`, focus `term.textarea` immediately with
`{ preventScroll: true }` so iOS native Paste can target an editable element.
Do not `preventDefault` on the idle long-press path. Soften the fully clipped
hidden textarea (ghostty uses `opacity:0` + `clipPath:inset(50%)` + 1px box)
enough that iOS treats it as a real edit target while paste-capable.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/src/terminalGestures.test.ts` (create only if needed;
  prefer extending TerminalRawView tests + existing gesture tests)

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/terminalGestures.ts` (only if a host callback is
  required; prefer focusing from TerminalRawView via a new optional
  `onTouchStart?` host hook rather than importing term into gestures)

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do not change Priority 1 Send/fallback paste behavior.
- Do not implement Priority 3 Copy overlay in this packet.
- Do not call `preventDefault` on single-finger `touchstart` or on
  sub-threshold `touchmove` before scroll engages.
- Do not remove existing PD for: two-finger pinch touchstart/move, scroll past
  threshold, active selection drag.
- Do not bump/replace ghostty-web.
- Do not edit styles.css layout (P0) except textarea rules inside
  TerminalRawView `<style>`.
- Do not edit Rust / architecture.md.

## 4. Architecture context

Ajax owns touch gestures in `terminalGestures.ts`; Ghostty owns the hidden
textarea. rcarmo MenuHandler already repositions the textarea under the finger
for context menus (`position:fixed`, 1px, opacity 0) — too late for native
long-press Paste. Fix: focus early + reduce clip so iOS can attach Paste to
the focused field. Still UI-only; PTY input path unchanged.

## 5. Code anchors

**Gesture touchstart — no focus today; no PD on single finger:**

```119:164:crates/ajax-web/web/src/terminalGestures.ts
  const onTouchStart = (event: TouchEvent) => {
    ...
    if (event.touches.length === 2) {
      if (event.cancelable) event.preventDefault();
      ...
      return;
    }
    ...
    // long-press timer only — no focusTerm
  };
```

**Host interface — add optional callback:**

```15:38:crates/ajax-web/web/src/terminalGestures.ts
export interface TerminalGestureHost {
  ...
  endSelection?(cancelled: boolean): void;
  // ADD: touchBegan?(): void;  // called on single-finger touchstart
}
```

**Wire in TerminalRawView attachTerminalGestures (~398):** pass
`touchBegan: () => focusTerm()` or `term?.textarea?.focus({ preventScroll: true })`.

**hardenMobileTextarea (~231):** currently only autocapitalize + fontSize 16px.
Extend to clear/soften clip for paste targeting:
- set `clipPath` / `clip` to `none`
- set `opacity` to something iOS accepts (e.g. `"0.01"` not `"0"`)
- ensure non-zero box: width/height at least ~1–2px (library already 1px) —
  prefer also CSS override below

**CSS today:**

```1183:1187:crates/ajax-web/web/src/components/TerminalRawView.svelte
  .terminal-host :global(textarea) {
    user-select: text;
    -webkit-user-select: text;
  }
```

Extend to override ghostty’s clip (from bundle ~`clipPath="inset(50%)"`):
```css
.terminal-host :global(textarea) {
  user-select: text;
  -webkit-user-select: text;
  opacity: 0.01;
  clip-path: none;
  -webkit-clip-path: none;
  /* keep tiny so it does not paint over the canvas */
}
```

**Existing focus helpers:** `focusTerm = () => term?.textarea?.focus({ preventScroll: true })`.

## 6. Test-first instructions

1. Add test in `TerminalRawView.test.ts`:
   `focuses the terminal textarea on touchstart with preventScroll`
   - `mountTerminal()` / open socket as other gesture tests.
   - Spy on `textarea.focus` (create/attach a real textarea on the mock term if
     the mock lacks one — check how other tests stub `term.textarea`; reuse
     that pattern from expand/focus tests ~1100).
   - Dispatch single-finger `touchstart` on host.
   - Expect `focus` called with `{ preventScroll: true }` **before** advancing
     long-press timers.
   - Expect `touchstart` event `defaultPrevented === false`.

2. Add test: `does not preventDefault on touchstart before scroll threshold`
   - touchstart + small touchmove under threshold → not defaultPrevented
     (may already exist; extend if needed).

3. Add source/CSS contract test (or assert in harden path via DOM):
   `terminal textarea CSS does not fully clip the edit target`
   - `terminalRawViewSource` matches `clip-path:\s*none` (or
     `-webkit-clip-path:\s*none`) under `.terminal-host :global(textarea)`.
   - matches `opacity:\s*0\.01` (or similar non-zero).

4. Run RED then implement:
   ```bash
   cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
   ```
   Also run gesture unit tests if present:
   ```bash
   cd crates/ajax-web/web && npm run web:test -- --run terminalGestures
   ```

## 7. Production edit instructions

1. Add optional `touchBegan?(): void` to `TerminalGestureHost`.
2. At end of single-finger branch in `onTouchStart` (after arming long-press
   timer), call `host.touchBegan?.()`.
3. In `TerminalRawView.svelte` gesture host object, set
   `touchBegan: () => { term?.textarea?.focus({ preventScroll: true }); }`.
4. Update `hardenMobileTextarea` to clear clip-path and set opacity 0.01 on
   the live element (in addition to CSS).
5. Extend `.terminal-host :global(textarea)` CSS as in anchors.
6. Do **not** add preventDefault on touchstart for one finger.
7. Do **not** change `finishSelectionCopy` / auto-copy yet (P3).

## 8. Verification commands

```bash
cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
cd crates/ajax-web/web && npm run web:test -- --run terminalGestures
cd crates/ajax-web/web && npm run web:check
cd crates/ajax-web/web && npm run web:build
```

## 9. Acceptance criteria

- Single-finger touchstart focuses textarea with preventScroll.
- That touchstart is not defaultPrevented.
- Pinch/scroll/selection-drag still preventDefault as today.
- Textarea not fully clipped (CSS + harden).
- Existing paste/selection tests still pass (auto-copy still current until P3).

## 10. Stop conditions

- Stop if focusing on every touchstart breaks scroll tests or forces keyboard
  on scroll-only gestures in a way you cannot fix within Allowed files —
  report with evidence.
- Stop if mock Terminal has no `textarea` seam — extend the existing mock in
  the test file only; do not invent a new terminal engine.
- Do not implement Priority 3 here.
