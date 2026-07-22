# TDD Packet: iOS hotbar key hold-to-repeat

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Make the Web Cockpit terminal hotbar send iOS-like accelerated key repeats for
Backspace and arrow keys while keeping one-shot keys single-fire, with no
trailing click after a hold and immediate stop on cancel/disconnect/unmount.

Concrete behaviors:

1. Add hotbar Backspace (`⌫`, aria-label `Backspace`) that sends `\x7f` via
   existing `sendKey` → `TerminalConnection.sendInput` (not local edit).
2. Tap Backspace or any arrow → exactly one PTY input frame.
3. Press-and-hold Backspace or any arrow → one immediate frame, pause
   `KEY_REPEAT_INITIAL_DELAY_MS`, then accelerated repeats down to
   `KEY_REPEAT_MIN_INTERVAL_MS` using the named stage curve.
4. Esc / Tab / ⌃C / Ctrl / Paste remain one-shot (click path).
5. After a hold ends, no extra frame from a trailing `click`.
6. Repeat stops immediately on pointerup/cancel/lost capture, window blur,
   visibility hidden, unmount, and when the terminal socket is not open.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/keyRepeat.ts` (new)
- `crates/ajax-web/web/src/shared/lib/keyRepeat.test.ts` (new)
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `.planning/agent-plans/ios-hotbar-key-repeat.md`
- `.planning/packets/ios-hotbar-key-repeat.md`

## Forbidden changes

- Do not redesign hotbar CSS/layout beyond what is required to render one new
  Backspace button in the existing `CONTROL_KEYS` map.
- Do not change directional-drag repeat on the interaction surface
  (`DIRECTIONAL_REPEAT_INTERVAL_MS` / `armDirectionalGesture`).
- Do not edit Rust / PTY protocol / architecture.md.
- Do not manipulate xterm buffer text locally for deletion.
- Do not repeat Esc, Tab, ⌃C, Ctrl, or Paste.
- Do not add Page Up/Down or Space to the hotbar.
- No new dependencies.
- No commit / push / branch / worktree changes.
- Do not weaken unrelated tests; rewrite only the held-Left-arrow case that
  currently asserts one-shot hold.

## Context evidence

- Desired behavior: user request for iOS-like hotbar hold-to-repeat;
  plan `.planning/agent-plans/ios-hotbar-key-repeat.md`.
- Hotbar is click-only today:

```1151:1164:crates/ajax-web/web/src/features/task/TaskTerminal.tsx
          {CONTROL_KEYS.map((key) => (
            <button
              key={key.label}
              type="button"
              className="terminal-key"
              aria-label={key.ariaLabel}
              onPointerDown={onToolbarPointerDown}
              onClick={(event) => {
                const ownedFocus = consumeToolbarPointerOwnedFocus(event);
                sendKey(consumeCtrl(key.data));
                refocusTermIfOwned(ownedFocus);
              }}>
              {key.label}
            </button>
```

- CONTROL_KEYS currently has Esc/Tab/⌃C/arrows only (no Backspace):

```118:127:crates/ajax-web/web/src/features/task/TaskTerminal.tsx
  const CONTROL_KEYS = [
    { label: "Esc", ariaLabel: "Escape", data: "\x1b" },
    { label: "Tab", ariaLabel: "Tab", data: "\t" },
    { label: "⌃C", ariaLabel: "Control C", data: "\x03" },
    { label: "←", ariaLabel: "Left arrow", data: "\x1b[D" },
    { label: "↑", ariaLabel: "Up arrow", data: "\x1b[A" },
    { label: "↓", ariaLabel: "Down arrow", data: "\x1b[B" },
    { label: "→", ariaLabel: "Right arrow", data: "\x1b[C" },
  ];
```

- Input path: `sendKey` → `connection.sendInput` binary WS frames
  (`terminalConnection.ts` ~240–247); PTY adapter writes bytes unchanged.
- Backspace byte: use `\x7f` (DEL). Ghostty/xterm convention documented in
  `.planning/packets/ghostty-terminal-behavior-task5.md` (`Backspace \x7f`).
- Existing e2e that must change (today asserts one frame on 550ms hold):

```606:614:crates/ajax-web/web/e2e/terminal-behavior.test.ts
test("held terminal back sends one left-arrow frame per activation", async ({ page }) => {
  await openTaskTerminal(page);
  const back = terminalToolbar(page).getByRole("button", { name: "Left arrow" });
  const baseline = await inputFrameCount(page);

  await back.click({ delay: 550 });
  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
```

- Toolbar pointerdown already `preventDefault`s to keep term focus
  (`onToolbarPointerDown` ~284–287). Reuse capture + owned-focus pattern.
- Architecture: browser UI must not own task truth; sending bytes through
  existing adapter is correct (`architecture.md` § terminal slice).

## Code anchors

- `CONTROL_KEYS` + hotbar render: `TaskTerminal.tsx` ~118–127, ~1149–1193
- `sendKey` / `onToolbarPointerDown` / `consumeToolbarPointerOwnedFocus`:
  `TaskTerminal.tsx` ~223–226, ~284–293
- Directional repeat (do not change): `TaskTerminal.tsx` ~116, ~686–718
- `TerminalConnection.sendInput`: `terminalConnection.ts` ~240–247
- E2E helpers: `inputFrameCount`, `terminalInputFrames`, `terminalToolbar`,
  `settleNoNewFrames` in `terminal-behavior.test.ts`
- Characterization style for TaskTerminal source tests:
  `TaskTerminal.test.tsx` (raw source / CSS contract tests)

## Test-first instructions

1. Add `crates/ajax-web/web/src/shared/lib/keyRepeat.test.ts` proving:
   - `nextRepeatInterval(stage)` stays within
     `[KEY_REPEAT_MIN_INTERVAL_MS, KEY_REPEAT_INITIAL_INTERVAL_MS]` and
     decreases across stages to the min.
   - `createHeldKeyRepeater` (or equivalent) with injectable timers:
     - first `emit` on start
     - no second emit before initial delay
     - subsequent emits at accelerating intervals
     - `stop()` prevents further emits
     - calling emit callback that observes `isActive()===false` stops
2. Extend `TaskTerminal.test.tsx` (source contract) to assert:
   - CONTROL_KEYS / render includes Backspace aria-label and `\x7f`
   - repeatable keys list includes Backspace + four arrows
   - Esc/Tab/Paste are not in the repeatable set
3. Rewrite e2e `held terminal back…` into hold-repeat coverage, and add
   Backspace tap/hold cases using pointer down/up (not only `click({delay})`
   if that cannot drive the new pointer path).

Focused RED command:

```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/keyRepeat.test.ts crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
```

Expect nonzero exit with the new assertions failing before production edits.

## Edit instructions

1. Create `keyRepeat.ts` exporting:
   - `KEY_REPEAT_INITIAL_DELAY_MS = 500`
   - `KEY_REPEAT_INITIAL_INTERVAL_MS = 100`
   - `KEY_REPEAT_MIN_INTERVAL_MS = 30`
   - stage intervals constant array e.g. `[100, 70, 50, 30]`
   - `createHeldKeyRepeater({ emit, isActive?, setTimeout, clearTimeout })`
     returning `{ start(), stop() }`. `start` emits once immediately, waits
     initial delay, then loops with stage acceleration. `stop` clears all
     timers. Never schedule when `isActive` is false; stop if `emit` runs
     while inactive.
2. In `TaskTerminal.tsx`:
   - Add Backspace to `CONTROL_KEYS` (place near arrows; label `⌫`,
     aria `Backspace`, data `"\x7f"`). Prefer inserting before the arrows so
     delete sits with navigation without reshuffling unrelated keys more than
     needed.
   - Mark repeatable keys (`\x7f` and CSI arrow sequences).
   - For repeatable buttons: on `pointerdown` (primary button only),
     `preventDefault`, `setPointerCapture`, stash owned focus, start repeater
     with `emit: () => sendKey(consumeCtrl(data))` but **consume Ctrl only on
     the first emit of a press** (subsequent repeats send the already-modified
     or unmodified payload from the first decision — do not re-arm Ctrl mid
     hold). Simplest correct approach: compute `payload = consumeCtrl(data)`
     once at pointerdown, then repeater emits `payload`.
   - Stop repeater on `pointerup`, `pointercancel`, `lostpointercapture`,
     window `blur`, `visibilitychange` when hidden, and effect cleanup /
     unmount. Also stop when `!connectionRef.current?.isOpen()` before emit.
   - Suppress trailing `click` after a pointer-owned press: if the press was
     handled by the repeater path, `click` must not call `sendKey` again
     (including after a short tap that already emitted on pointerdown).
   - One-shot keys keep current click path.
3. Update e2e:
   - Replace one-shot hold assertion with: Left arrow pointer hold ≥
     initial delay + one interval emits ≥2 `\x1b[D` frames; after pointerup,
     `settleNoNewFrames` shows no growth.
   - Add Backspace tap → exactly one `"\x7f"`; hold → ≥2 `"\x7f"`; stop on up.
   - Keep ordered navigation test green (short clicks still one frame each);
     include Backspace in that ordered list only if you add a short click for
     it without breaking the existing expected sequence — prefer a separate
     Backspace test over enlarging the ordered list if cleaner.
4. Update the persistent plan checklist as tasks complete.

## Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/keyRepeat.test.ts
npm run web:test -- crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
npx playwright test --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit \
  -g "held terminal|Backspace|printable, control, and navigation|supported Ctrl"
```

## Acceptance criteria

- Tap Backspace → one `\x7f` frame; hold → delayed then accelerating repeats;
  release → no further frames and no trailing click frame.
- Tap/hold Left/Right/Up/Down behave the same with their CSI sequences.
- Esc/Tab/⌃C/Ctrl/Paste still single-fire.
- Focus preservation via existing toolbar pointerdown pattern still works.
- No local buffer mutation; only `sendInput` path.
- Unit tests cover acceleration bounds and stop; e2e covers hold cardinality
  and post-up silence.
- Directional-drag e2e still green (untouched production path).

## Stop conditions

- Need to change Rust/PTY protocol or architecture boundaries.
- Required visual redesign beyond one Backspace key in CONTROL_KEYS.
- Directional-drag path must change to make tests pass.
- Cannot suppress trailing click without breaking keyboard/Enter activation
  for accessibility — stop and report (prefer pointer+click split that still
  allows non-pointer activation via click/keyboard for one-shot and for
  repeatable keys as a single emit).
- Edits outside Allowed files.
- Unrelated test failures that are not caused by this change.
