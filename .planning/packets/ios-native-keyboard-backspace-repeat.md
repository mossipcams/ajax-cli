# Packet: native iOS keyboard backspace hold-to-repeat

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Holding Delete on the **native iOS software keyboard** must repeat deletes
through the PTY, the way the custom hotbar `⌫` already does. Today it deletes
once and stops.

Exactly one bounded behavior: native-keyboard Backspace/Delete input handling in
`TaskTerminal.tsx`. Every press must still send exactly one `\x7f` — no
double-send, no dropped press, on soft keyboard, hardware keyboard, and desktop.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

## Forbidden changes

- `crates/ajax-web/web/src/shared/lib/keyRepeat.ts` and its test — the hotbar
  repeater is a separate, already-shipped path. Do not reuse it here; iOS owns
  the native repeat cadence.
- Hotbar behavior: `CONTROL_KEYS`, `REPEATABLE_KEY_DATA`, `onRepeatableKey*`,
  `onControlKeyClick`.
- The directional-drag arrow gesture (`armDirectionalGesture`,
  `DIRECTIONAL_REPEAT_INTERVAL_MS`).
- Terminal geometry, fit/refit, scroll sync, expand/fullscreen, styles.css.
- The existing Space branch of `attachCustomKeyEventHandler` — extend it, never
  replace it (`attachCustomKeyEventHandler` allows only one handler).
- No new dependency. No `dist/` rebuild. No commits, branches, or pushes.

## Context evidence

**Desired behavior** — iOS starts its hold-to-delete repeat loop only when both
hold: (1) the Backspace keydown was not `preventDefault()`ed, and (2) the focused
field has deletable content. Repeat ticks then arrive as `beforeinput` /
`deleteContentBackward`, **not** as repeated keydowns.

**Why it is broken now** — xterm cancels the keydown and Ajax leaves the helper
textarea empty, so neither condition holds:

- `node_modules/@xterm/xterm/src/browser/CoreBrowserTerminal.ts:1099` —
  `return this.cancel(event, true);` after `triggerDataEvent(result.key)` for
  Backspace; `cancel` calls `preventDefault()`.
- `node_modules/@xterm/xterm/src/browser/CoreBrowserTerminal.ts:1025` — a custom
  key event handler returning `false` makes `_keyDown` return early **without**
  `preventDefault`. This is the required escape hatch.
- `node_modules/@xterm/xterm/src/browser/CoreBrowserTerminal.ts:1196` —
  `_inputEvent` forwards only `insertText`. xterm will never emit `\x7f` from the
  input path, so once its keydown handling is skipped **Ajax must send `\x7f`
  itself** from `beforeinput`.
- `TaskTerminal.tsx:264` `hardenMobileTextarea` — sets autocapitalize, sizing and
  transparency, but seeds no content.

**Sentinel must be reseeded, not seeded once** — xterm clears the textarea:

- `CoreBrowserTerminal.ts:292` on blur.
- `CoreBrowserTerminal.ts:1087` on Enter (`C0.CR`) and Ctrl-C (`C0.ETX`).

The Enter clear is the sharp edge: after every Enter the field is empty, and a
browser fires no `beforeinput` when there is nothing to delete — so without a
seed at keydown time the first Backspace after each Enter is **silently
dropped**. Cover all three seed points (harden/focus, Backspace keydown,
post-deletion) as specified in Edit instructions.

**Proven prior art** — the same fix shipped in the Ghostty era as PR #397 and was
lost in the xterm/React migration. `git show c7d2911 -- '*TerminalRawView.svelte'`
is the reference. Note it does **not** call `preventDefault()` in the
`beforeinput` handler; preserve that.

**Deferred on purpose** — `.planning/agent-plans/ios-hotbar-key-repeat.md:21`
lists this as an explicit non-goal of the hotbar round: "Native system-keyboard
Backspace `beforeinput` Ghostty ZWS sentinel restoration".

**Test pattern to reuse** — `TaskTerminal.test.tsx` is source-regex based
(`import taskTerminalSource from "./TaskTerminal.tsx?raw"`), not a DOM mount; it
asserts wiring via `extractBlock` / `String.match`. `e2e/terminal-behavior.test.ts`
drives a real WebKit terminal against a mocked socket and asserts PTY frames via
`inputFrameCount(page)` / `terminalInputFrames(page)`; see the existing
`"repeated printable browser events produce exact cardinality"` test at :671 for
the exact real-keyboard cardinality pattern to copy.

## Code anchors

| Anchor | File:line | Role |
|---|---|---|
| `hardenMobileTextarea` | `TaskTerminal.tsx:264` | seed sentinel here; add `focus` listener |
| `termTextarea()` | `TaskTerminal.tsx:259` | existing helper returning the xterm textarea |
| `sendKey` / `consumeCtrl` | `TaskTerminal.tsx:238` / `:232` | the only sanctioned PTY input path |
| `attachCustomKeyEventHandler` | `TaskTerminal.tsx:1014-1022` | Space branch; add Backspace/Delete branch |
| `dataDisposable` wiring | `TaskTerminal.tsx:1039` | add the `beforeinput` listener next to this |
| effect cleanup | `TaskTerminal.tsx:1110-1149` | remove both new listeners here |
| unit test tail | `TaskTerminal.test.tsx:315-341` | append new cases inside this describe |
| e2e cardinality test | `e2e/terminal-behavior.test.ts:671` | copy this shape for the new e2e |

## Test-first instructions

Write these tests first and run the red command; each must fail for the stated
reason before any production edit.

**Unit — `TaskTerminal.test.tsx`, appended to the existing describe:**

1. `"skips xterm Backspace keydown so iOS can key-repeat"` — the custom key
   handler block must contain a branch returning `false` for
   `"Backspace"`/`"Delete"`, and must **not** call `preventDefault()` in that
   branch. Red reason: no Backspace branch exists.
2. `"seeds a zero-width space so iOS has deletable content"` — source contains
   `\u200B` as a named constant, seeded in `hardenMobileTextarea` and from a
   `focus` listener. Red reason: no sentinel exists.
3. `"sends DEL from beforeinput deleteContentBackward"` — a `beforeinput`
   listener maps `deleteContentBackward` (and `deleteContentForward`) to
   `sendKey(consumeCtrl("\x7f"))` and reseeds afterwards. Red reason: no
   `beforeinput` listener exists.
4. `"removes the beforeinput and focus listeners on cleanup"` — cleanup block
   contains matching `removeEventListener` calls. Red reason: not wired.

**e2e — `e2e/terminal-behavior.test.ts`:**

5. `"native Backspace presses produce exact DEL cardinality"` — focus the
   terminal surface, `page.keyboard.press("Backspace")` three times, assert
   exactly 3 frames and every frame `=== BACKSPACE`. This is the real-browser
   proof that skipping xterm's keydown neither double-sends nor drops a press.
6. `"native Backspace after Enter still sends DEL"` — press `Enter`, then
   `Backspace`, assert the DEL frame arrives. This locks the xterm
   Enter-clears-the-textarea edge described in Context evidence.

Red command (must fail before the edit):

```bash
cd crates/ajax-web/web && npx vitest run src/features/task/TaskTerminal.test.tsx
```

## Edit instructions

In `TaskTerminal.tsx` only:

1. Add a module-scope constant `const BACKSPACE_SENTINEL = "\u200B";` and an
   idempotent helper beside `termTextarea()` (`TaskTerminal.tsx:259`):

   ```ts
   const seedBackspaceSentinel = () => {
     const input = termTextarea();
     if (input && !input.value.includes(BACKSPACE_SENTINEL)) {
       input.value = BACKSPACE_SENTINEL;
     }
   };
   ```

   Idempotence is required: it must never clobber text xterm has accumulated in
   the textarea.

2. In `hardenMobileTextarea` (`:264`), after the existing style writes, call
   `seedBackspaceSentinel()` and register a `focus` listener on the textarea that
   calls it (covers xterm's blur clear at `CoreBrowserTerminal.ts:292`). Keep a
   reference so cleanup can remove it.

3. Extend the existing `attachCustomKeyEventHandler` at `:1014` — keep the Space
   branch exactly as-is and add, before it:

   ```ts
   if (event.key === "Backspace" || event.key === "Delete") {
     // Skipping xterm's handling avoids its preventDefault, which is what lets
     // iOS start its hold-to-delete repeat loop. beforeinput sends the DEL.
     if (event.type === "keydown" && !event.isComposing) seedBackspaceSentinel();
     return false;
   }
   ```

   The `isComposing` guard is required: seeding mid-IME-composition would clobber
   the composition buffer. The keydown seed is what saves the first Backspace
   after an Enter clear.

4. Add a `beforeinput` listener on the helper textarea, registered next to
   `dataDisposable` (`:1039`):

   ```ts
   const onTextareaBeforeInput = (event: InputEvent) => {
     if (
       event.inputType !== "deleteContentBackward" &&
       event.inputType !== "deleteContentForward"
     ) {
       return;
     }
     // No preventDefault: cancelling here also cancels the iOS repeat loop.
     sendKey(consumeCtrl("\x7f"));
     // The default action deletes the sentinel; restore it after that runs so
     // the next repeat tick still has content to delete.
     queueMicrotask(seedBackspaceSentinel);
   };
   ```

   Register it with `termTextarea()?.addEventListener("beforeinput", ...)`.

5. Remove both listeners in the effect cleanup (`:1110-1149`), next to the
   existing `dataDisposable?.dispose()`.

Do not add local echo, optimistic rendering, or any repeat timer — iOS owns the
native cadence. Do not touch `keyRepeat.ts`.

## Verification commands

```bash
cd crates/ajax-web/web
npx vitest run src/features/task/TaskTerminal.test.tsx
npx vitest run src/shared/lib/keyRepeat.test.ts
npm run web:check
npx playwright test e2e/terminal-behavior.test.ts --project=mobile-webkit
```

The Playwright mobile-webkit run is mandatory, not optional: unit tests here are
source-regex only and cannot prove a real browser still emits exactly one DEL
per press.

## Acceptance criteria

- All six new tests pass; every previously passing test in the three allowed
  files still passes — in particular the existing hotbar tests
  `"Backspace tap sends one DEL frame"` and
  `"held Backspace repeats DEL frames then stops on release"`.
- Exactly one `\x7f` per Backspace press in real WebKit, including immediately
  after Enter.
- The Backspace branch calls no `preventDefault()`.
- `beforeinput` calls no `preventDefault()`.
- The Space branch of the custom key handler is byte-identical to before.
- Changed files are exactly the three Allowed files; diff stays under ~120
  changed lines.

## Stop conditions

- Any anchor above does not match the file (the file changed under you).
- The fix appears to require touching `keyRepeat.ts`, styles, geometry, or the
  hotbar path — that is scope growth; stop and report.
- An existing terminal test fails and the cause is not obviously the new code.
- `npm run web:check` reports pre-existing unrelated failures — report them, do
  not fix them.
- The patch would exceed ~400 changed lines.
- Any need to create commits, branches, or run git write commands.
