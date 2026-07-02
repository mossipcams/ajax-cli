# Terminal small refactor — remove/align behavior that doesn't make sense

Mode: Refactor/Cleanup with one small Behavior Change slice. Scope:
`TerminalRawView.svelte`, `viewport.ts`, and their tests. No PTY/WebSocket
contract changes, no CSS takeover changes, raw-first contract untouched.

## Findings (what's messy and why)

1. **Two competing "keyboard open" detectors that can disagree.**
   - `viewport.ts` owns the CSS side: baseline-rebased `visualViewport.height`
     delta with open/close hysteresis (150px open / 100px close), toggling
     `html.keyboard-open` (which collapses the task chrome).
   - `TerminalRawView` has its own private `keyboardOpen()` used for the PTY
     lockstep (freeze local grid, withhold server resize, bottom-anchor crop):
     `window.innerHeight - visualViewport.height > 150` — no hysteresis, no
     rebasing, and a different reference height.
   - Consequence: iOS address-bar drift or the documented ~24px iOS 26
     visual/layout discrepancy can put the two detectors in different states —
     chrome collapsed while the grid still resizes (SIGWINCH spray the freeze
     exists to prevent), or grid frozen under full chrome. One keyboard truth
     should drive both.

2. **The `disconnected` status is unreachable.** `TerminalRawView` declares a
   four-state connection status but only ever assigns `connecting`,
   `connected`, and `reconnecting`. The `Disconnected` label, the
   `status === "disconnected"` arm of the visibility handler, and the same arm
   of the Reconnect-button condition are dead. A UI state that can never
   render.

3. **Sticky Ctrl + Enter skips the overlay clear.** In `term.onData`, an armed
   Ctrl short-circuits before the `"\r"` branch. `controlModify("\r")` returns
   `"\r"` unchanged, so Ctrl-armed Enter sends a normal Enter **without**
   `zerolag.clear()` — the local input overlay lingers as ghost text until the
   next output frame. Inconsistent with every other Enter. (This is the one
   real behavior fix; everything else is behavior-preserving.)

4. **Dead `FileReader` fallback in `readMessageData`.** The Blob branch
   already returns via `data.text()`, which exists in every supported runtime
   (iOS Safari ≥14, desktop browsers, jsdom — the existing "decodes Blob
   websocket messages" test exercises it). The 8-line FileReader path can
   never run.

Noted but NOT in scope:
- The unused `Message::Binary` input path in `terminal_pty.rs` (the bundled
  client only sends JSON text frames) — removing it narrows the WebSocket
  contract; flag for a separate decision.
- Connect-time `term.focus()` (desktop-desirable; iOS won't pop a keyboard
  without a user gesture).
- 80-col floor / pan / pinch / fling / debounce mechanics — recently
  consolidated and fully pinned; leave alone.

## Slices (in order)

### Slice 1 — single keyboard-open source of truth
1. Extract the detector into `viewport.ts` as pure, exported helpers:
   `updateKeyboardState(baseline, current, wasOpen)` (rebasing + hysteresis as
   today) and export `isKeyboardOpen(): boolean` that reads the
   `html.keyboard-open` class — the class `initViewport` already maintains.
2. Failing behavior tests first (`viewport.test.ts`): hysteresis (opens >150,
   stays open until <100), baseline rebasing after address-bar drift,
   `isKeyboardOpen` reflects the class.
3. `TerminalRawView.keyboardOpen()` becomes `isKeyboardOpen()` — delete the
   private `KEYBOARD_OPEN_THRESHOLD_PX` and `window.innerHeight` math.
4. Update `TerminalRawView.test.ts` keyboard tests: instead of stubbing
   `window.innerHeight` + vv height, tests set/clear the `keyboard-open`
   class on `document.documentElement` (and keep dispatching vv resize for the
   refit path). Assertions unchanged: grid frozen while open, bottom-anchored
   crop, exactly one flushed resize after close.
   Risk note: the component no longer detects the keyboard when `initViewport`
   isn't running; App mounts it unconditionally, and the component's behavior
   without it (never frozen) matches desktop reality.

### Slice 2 — remove the unreachable `disconnected` state
- Narrow the status union to `connecting | connected | reconnecting`, drop the
  dead label and the two dead condition arms.
- Mechanical dead-code deletion proven unused (no assignment site); existing
  reconnect tests (backoff, foreground reconnect, manual button) stay green
  unchanged — they all operate in `reconnecting`.

### Slice 3 — Ctrl-armed Enter clears the overlay (behavior fix)
1. Failing test: arm Ctrl via the key bar, send `"\r"` through `onData`,
   assert `zerolag.clear()` was called AND the socket got `"\r"`.
2. Smallest fix: in `onData`, consume Ctrl first (`data = consumeCtrl(data)`)
   and let the transformed byte flow through the existing `"\r"` / backspace /
   printable branches. Control codes (<32) are already excluded from
   flushed-overlay tracking, so Ctrl+letter behavior is unchanged; existing
   sticky-Ctrl tests must stay green.

### Slice 4 — delete the dead FileReader fallback
- `readMessageData` keeps `string | Blob.text() | ArrayBuffer` handling; the
  FileReader branch goes. Covered by the existing Blob decode test; no new
  test (dead-code deletion).

### Finishing
- `npm run web:build` (dist snapshot tests pin the bundle), then full
  validation: `cargo fmt --check`, check/clippy `-D warnings`,
  `cargo nextest run --all-features`, `npm run web:check`,
  `npm run web:test -- --run`.

## Risks
- Slice 1 changes *when* the grid freezes in edge cases (address-bar drift no
  longer mis-detected as a keyboard; hysteresis prevents freeze/unfreeze
  flapping near the threshold). That is the point — the aligned detector is
  strictly more sensible — but it is technically observable, hence tests
  first.
- Slice 3 is a deliberate small behavior fix (ghost overlay after Ctrl+Enter);
  guarded by a failing-first test.
- Slices 2 and 4 are pure dead-code deletions with zero observable change.

Estimated size: ~40–60 lines net removed; 3–5 tests added/updated.
