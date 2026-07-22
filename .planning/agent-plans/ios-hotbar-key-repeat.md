# Plan: iOS hotbar key hold-to-repeat

## Scope

Make Ajax Web Cockpit custom terminal hotbar keys behave like native iOS
keys during long presses: Backspace hold-to-delete and arrow hold-to-repeat,
with bounded acceleration and hard stop on cancel/disconnect/unmount.

Target: iOS Safari and standalone PWA. No visual redesign beyond adding a
missing Backspace key to the existing hotbar.

## Non-goals

- Redesign hotbar layout/chrome.
- Change directional-drag arrow repeat on the interaction surface (already
  repeats at fixed 75 ms; leave alone this round).
- Page Up / Page Down (not present).
- Hotbar Space repeat (Space is owned by the xterm custom key handler for
  physical/native keyboard; not a hotbar key).
- Local terminal buffer editing; all deletes/moves go through PTY input.
- Native system-keyboard Backspace `beforeinput` Ghostty ZWS sentinel
  restoration (physical checklist item; separate from hotbar).

## Input-path audit (current)

```text
Hotbar button
  pointerdown → preventDefault + stash term focus ownership
  click      → sendKey(consumeCtrl(data))
             → TerminalConnection.sendInput(string)
             → WS binary frames (≤4096 B)
             → ajax-web::adapters::terminal_pty → PTY writer

Interaction-surface directional drag (separate path)
  touchstart → (hold) → touchmove past 24px cardinal threshold
             → sendKey(arrow) + setInterval(75ms)
  touchend/touchcancel/unmount → clearDirectionalGesture()

Native/xterm keyboard
  textarea → onData → sendKey(consumeCtrl)
  Space specially owned by attachCustomKeyEventHandler
  No hotbar Backspace today; no React-path Backspace custom handler
```

### Findings

| Issue | Anchor | Risk |
| --- | --- | --- |
| Hotbar keys are click-only; hold does not repeat | `TaskTerminal.tsx` CONTROL_KEYS + `onClick` ~1151–1164 | Arrows feel dead on hold |
| E2E locks one-shot hold for Left arrow | `terminal-behavior.test.ts` `held terminal back sends one left-arrow frame` | Must rewrite with new behavior |
| No Backspace hotbar key | `CONTROL_KEYS` | Cannot satisfy hold-to-delete via custom keyboard |
| Directional drag uses fixed interval, no accel | `DIRECTIONAL_REPEAT_INTERVAL_MS = 75` | Out of scope this round |
| Trailing `click` after pointer-driven fire would double-send | Current `onClick` always `sendKey` | Stuck/extra delete if not suppressed |
| Repeat timers must stop on blur/visibility/disconnect/unmount | Hotbar has no repeater yet | Stuck-key risk once added |
| Established Backspace byte | Ghostty/xterm convention `\x7f` (DEL); path is string `sendInput` | Use `\x7f`, not `\x08` or KeyboardEvent |

## Locked product decisions

1. Add hotbar Backspace (`⌫`, aria `Backspace`) sending `\x7f`.
2. Repeatable: Backspace + ← ↑ ↓ →.
3. One-shot: Esc, Tab, ⌃C, Ctrl, Paste.
4. Acceleration constants (named, exported for tests):
   - `KEY_REPEAT_INITIAL_DELAY_MS = 500`
   - `KEY_REPEAT_INITIAL_INTERVAL_MS = 100`
   - `KEY_REPEAT_MIN_INTERVAL_MS = 30`
   - Stage curve: `100 → 70 → 50 → 30` after 4 emits per stage (or equivalent
     pure function with those bounds).
5. Pointer lifecycle owns repeating keys; suppress trailing synthetic `click`.
6. Stop immediately on: `pointerup`, `pointercancel`, `lostpointercapture`,
   pointer leave (unless capture keeps ownership intentionally — use capture),
   `window` `blur`, `visibilitychange` → hidden, component unmount,
   WS/terminal not open (stop timer; `sendKey` already no-ops).

## Delegation decision

`Delegation decision: delegated via model-router` (cursor-delegate /
composer-2.5 after READY packet).

## Task checklist

- [x] Unit tests for `keyRepeat` helper: delay, acceleration bounds, stop
- [x] Unit/source tests for hotbar: Backspace present, repeatable vs one-shot
- [x] Implement `keyRepeat.ts` helper with named constants
- [x] Wire hotbar repeating keys in `TaskTerminal.tsx`; suppress trailing click
- [x] Rewrite e2e held-Left-arrow case for multi-frame hold + stop-on-up
- [x] Add e2e Backspace tap = one `\x7f`; hold accelerates; no post-up frames
- [x] Parent validation: focused vitest + focused playwright cases

## Approval

Not required (behavior change within existing hotbar/terminal adapter;
architecture boundaries unchanged).

## Deviations

- Delegate report used multi-line YAML for `FILES_CHANGED` / `REMAINING_RISKS`;
  `check-report` expects those keys with a trailing space (inline form). Code
  was still gated from the worktree delta.
- Parent review gate cleared stale `toolbarRepeatHandledRef` after
  `pointercancel`, blur, visibility-hidden, and unmount so a cancelled hold
  cannot swallow a later keyboard activation.

## Validation

```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/keyRepeat.test.ts crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
# parent: exit 0 — 23 passed

npx playwright test --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit \
  -g "held terminal|Backspace|printable, control, and navigation|supported Ctrl"
# parent: exit 0 — 6 passed
```

Results: parent-verified green after review-gate fix.
