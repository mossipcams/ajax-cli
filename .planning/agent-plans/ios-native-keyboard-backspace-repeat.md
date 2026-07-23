# Plan: native iOS keyboard backspace hold-to-repeat

## Scope

Hold-to-repeat currently exists only on the custom hotbar (commit 5564985).
Holding Delete on the **native iOS software keyboard** deletes once and stops.
Restore the proven Ghostty-era behavior (#397, lost in the xterm/React
migration) inside `TaskTerminal.tsx` so a held native Delete repeats through the
PTY like the hotbar `⌫`.

Target: iOS Safari, `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`.

## Non-goals

- Hotbar repeat behavior (already shipped; do not change `keyRepeat.ts`).
- Directional-drag arrow repeat (fixed 75 ms; unchanged).
- Hardware/Bluetooth keyboards — the OS already emits repeat keydowns; the fix
  must not regress them.
- Any new local echo / zero-lag overlay.
- Layout, chrome, or geometry changes.

## Root cause

```text
native Delete keydown
  → xterm _keyDown → evaluateKeyboardEvent → \x7f + cancel(event)  [preventDefault]
  → iOS never starts its hold-to-delete repeat loop
  → and the helper textarea is empty, so there is nothing for iOS to delete
```

Two conditions must both hold for iOS to repeat:

1. Backspace keydown is **not** `preventDefault()`ed.
2. The focused textarea has deletable content.

xterm violates (1) in `CoreBrowserTerminal._keyDown` (`node_modules/@xterm/xterm/
src/browser/CoreBrowserTerminal.ts:1099`), and `hardenMobileTextarea`
(`TaskTerminal.tsx:264`) leaves the textarea empty, violating (2).

xterm's `_inputEvent` (same file, :1196) only forwards `insertText`, so it never
emits `\x7f` from the input path — once xterm's keydown handling is skipped,
Ajax must emit `\x7f` itself from `beforeinput`.

xterm clears `textarea.value` on blur (:292) and on Enter / Ctrl-C (:1087), so
the sentinel must be reseeded, not seeded once.

## Design (ported from #397 `TerminalRawView.svelte`)

1. `BACKSPACE_SENTINEL = "\u200B"` (escape, never a raw invisible char in
   source), seeded in `hardenMobileTextarea` and on
   textarea `focus`; idempotent (`if (!value.includes(...))`).
2. Extend the existing `attachCustomKeyEventHandler` (currently Space-only) to
   return `false` for `Backspace` / `Delete` keydown — skips xterm handling
   **without** `preventDefault`, so the iOS repeat loop survives.
3. New `beforeinput` listener on the helper textarea: `deleteContentBackward`
   (and `deleteContentForward`) → `sendKey(consumeCtrl("\x7f"))`, then reseed
   the sentinel so the next hold tick still has deletable content.
4. Remove both listeners in the existing effect cleanup.

Open question to settle during implementation/verification: whether the
`beforeinput` handler must `preventDefault()` to keep the sentinel alive across
repeat ticks, or whether reseeding after the default deletion is enough. Verify
on device/simulator, not by reasoning.

## Delegation decision

`Delegation decision: delegated via model-router` — packet
`.planning/packets/ios-native-keyboard-backspace-repeat.md`.

Lane escalation (both recorded in `~/.ajax-router/log.tsv`):

1. `cursor-delegate` / composer-2.5 → `TOOL_UNAVAILABLE` (Cursor out of usage).
2. `pi-delegate` / glm-5.2 → `TOOL_UNAVAILABLE` (opencode-go weekly limit;
   minimax-m3 probe returned the same 429, resets ~2026-07-26).
3. `codex-delegate` / gpt-5.6-sol → implemented; returned `BLOCKED` only because
   its sandbox denied the loopback listener Playwright needs.

## Task checklist

- [x] Packet: `.planning/packets/ios-native-keyboard-backspace-repeat.md`
      (`scripts/check-packet` passed)
- [x] RED: 4 unit contracts failed as intended (delegate evidence: exit 1,
      "4 intended failures: missing Backspace branch, sentinel, beforeinput
      handler, and cleanup")
- [x] GREEN: sentinel + keydown skip + `beforeinput` DEL in `TaskTerminal.tsx`
- [x] e2e: 2 new mobile-webkit tests — exact DEL cardinality, and DEL still
      sent after Enter (the xterm textarea-clear edge)
- [x] Review gate: parent-applied fix for listener identity + lint (below)
- [ ] Device check on a real iPhone — the one thing automation cannot prove:
      that iOS actually *starts* its repeat loop with the sentinel present
- [ ] Verify gate: `npm run verify` + `.husky/pre-commit` remainder (deferred —
      no commit requested yet; the hook rebuilds and stages `dist/`)

## Validation ledger

Run by the parent, not trusted from the delegate:

| Command | Result |
| --- | --- |
| `npm run web:test -- --run` (whole web suite) | PASS — 47 files, 446 tests |
| `npm run web:check` | PASS |
| `npm run web:lint` | PASS (after the fix below; failed on the delegate's diff) |
| `npm run web:smoke -- e2e/terminal-behavior.test.ts --project=mobile-webkit` | PASS — 72 passed, 1 skipped |
| `npm run web:smoke -- --project=mobile-webkit` (all suites) | PASS — 101 passed, 3 skipped |

## Device findings 2026-07-22 (iOS 26.5 Simulator, iPhone 17 Pro)

**Status: NOT FIXED in the app.** Earlier "shipped" claim was wrong — it rested
on unit/e2e tests, never on a device.

Measured with a standalone probe page (`keytest.html`, since deleted) driven by
`idb ui swipe --duration 4 --delta 1` on the soft keyboard's Delete key:

| Surface | Result for one 4s hold |
| --- | --- |
| Standalone probe page | 80+ events; `keydown rep=true` → `beforeinput` → `input` every ~100ms; escalates to `deleteWordBackward` after ~800ms |
| Real Web Cockpit terminal | exactly **one** cycle, then nothing |

So iOS's repeat loop works with this sentinel design, and the app kills it after
the first tick. Two genuine defects were found and fixed on the way:

1. `queueMicrotask(reseed)` from `beforeinput` is a **no-op** — the microtask
   checkpoint runs *before* the browser applies the deletion, so it always sees
   the sentinel still present. Reseeding moved to the `input` event. Device trace
   confirms the fix: `input len=0` → `keyup len=1`.
2. `deleteWordBackward` was ignored. Every long hold escalates to it, so the tail
   of a hold was silently dropped. Now mapped to `\x17`.

Neither was sufficient. The app trace shows our handlers firing correctly and the
sentinel restored (`len=1`) at the end of tick 1 — and no tick 2 arrives.

### Ruled out by instrumented device runs

An instrumented build (temporary on-screen probe, since removed) logged
keydown/beforeinput/input/blur/resize/PTY-write plus a document-level capture
listener. Each hypothesis below was tested and killed:

| Hypothesis | Test | Result |
| --- | --- | --- |
| PTY echo → `term.write()` re-render cancels the repeat | `?nowrite=1` skipped every `term.write` (input still reached the PTY) | Still one tick. **Not it.** |
| Hidden-textarea styling (`opacity:.01`, transparent text/caret) | probe page with `?harden=1` applying `hardenMobileTextarea`'s exact styles | 67+ ticks, escalated to word-delete. **Not it.** |
| Our handlers swallow later ticks | `document.addEventListener("keydown", …, true)` above xterm | One `DOC-keydown rep=false`, then silence. iOS never sends tick 2. **Not it.** |
| Focus loss / sentinel missing | probe logs `f=TA` and `len=1` at keyup | Focus held, sentinel restored. **Not it.** |
| xterm's `input` handler interfering | read `CoreBrowserTerminal._inputEvent` | Only acts on `insertText`; no mutation, no cancel for deletes. **Not it.** |

So iOS itself declines to start the repeat loop on the app page, for a reason
that is neither our JS nor the textarea styling.

**Remaining untested suspect:** xterm's `_syncTextArea`
(`CoreBrowserTerminal.ts:301`, fires on `onCursorMove`) overwrites
`hardenMobileTextarea`'s geometry every cursor move, shrinking the focused
textarea to a single cell at `zIndex:-5`. A probe run mimicking that
(`?tiny=1`) could not be completed — the one-cell target at `zIndex:-5` is
untappable and the scripted focus tap kept registering as a long-press.

**Next experiment:** in the app, re-apply the harden geometry after every
`onCursorMove` (or on a `render` hook) so the focused textarea keeps a real
44px box, then hold Delete. This is also a standing latent bug regardless of
key repeat: xterm and `hardenMobileTextarea` are fighting over the same styles.

### Environment trap hit during this work

The dev web server's binary was silently **replaced from a different worktree**
(`ajax-cli__worktrees/ajax-terminal-load`) partway through — another session
runs its own installs into `.ajax-dev-web/bin`. One test round therefore
measured foreign code. Always re-verify with a marker grep
(`strings -a .ajax-dev-web/bin/ajax-cli | grep -c <marker>`) immediately before
and after each device run.

## Deviations

1. **Delegate moved `termRef.current = liveTerm` above `onHardenTextarea()`.**
   Necessary and correct: `hardenMobileTextarea` reads `termTextarea()`, which
   dereferences `termRef.current`. That ref was assigned ~20 lines *later*, so
   the whole function silently returned early on every mount — the JS textarea
   hardening (`font-size: 16px`, autocapitalize/autocorrect/spellcheck off,
   transparent caret/text) has never run since the xterm migration. CSS masked
   the visible half. Latent bug, now fixed as a side effect.

2. **Parent-applied fix: listener identity + `react-hooks` lint.** The delegate
   registered the `focus` listener with a component-scope arrow. It is added via
   `hardenMobileTextarea` (reached through an effect event → newest render's
   closure) but removed in the effect cleanup (that effect's closure), so the two
   never name the same function and the listener leaks. `useEffectEvent` is not
   a legal fix — `react-hooks/rules-of-hooks` forbids passing an effect event to
   `addEventListener`. `seedBackspaceSentinel` / `seedSentinelFromFocus` are
   therefore module-scope. Guarded by the reworked cleanup test, which was
   confirmed to fail against the unstable wiring before being kept.

3. **Hotbar repeat cadence retuned** (separate user request, same session):
   `keyRepeat.ts` stage curve `[100,70,50,30]` → `[130,100,80,60]` and 4 → 6
   emits per stage, floor 30 ms → 60 ms. New test pins the floor.
