# TDD Packet: R3 — iOS backspace hold key-repeat

## 1. Goal

Holding Backspace on the iOS soft keyboard must send repeated `\x7f` deletes to
the PTY. Today ghostty-web's `handleKeyDown` always `preventDefault()`s
Backspace, which cancels iOS's hold-to-delete loop (repeats arrive as
`beforeinput` `deleteContentBackward`, not repeated keydowns). Fix by (a)
skipping Ghostty's Backspace keydown via `attachCustomKeyEventHandler`
returning `false`, and (b) seeding a zero-width-space sentinel in the textarea
so iOS always has content to delete and will start the repeat loop.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do not bump ghostty-web.
- Do not change expand flush (R1), textarea transparent paint (R2), paste/copy,
  zero-lag overlay logic beyond what backspace bookkeeping already does.
- Do not edit `styles.css`, Rust, package.json, gestures.
- Do not `preventDefault` Backspace yourself on keydown.
- No drive-by refactors.

## 4. Architecture context

Ghostty InputHandler:
- `keydown` on the host container — Backspace maps to `\x7f` and always
  `preventDefault()`s (cancels iOS key-repeat).
- `beforeinput` on the textarea — `deleteContentBackward` also maps to `\x7f`,
  with de-dupe against a recent keydown.
- `attachCustomKeyEventHandler(fn)`: return `true` → preventDefault and stop;
  return `false` → return from handleKeyDown **without** preventDefault;
  return `undefined` → continue normal handling.

Ajax already listens to textarea `beforeinput` for zero-lag overlay
(`deleteContentBackward` → `optimisticBackspacesAhead` + trim). Ghostty still
owns sending `\x7f` via its beforeinput handler. Ajax must not double-send.

## 5. Code anchors

Ghostty API (installed):
`node_modules/ghostty-web/dist/index.d.ts` —
`attachCustomKeyEventHandler(customKeyEventHandler: (event: KeyboardEvent) => boolean | undefined): void`

Ghostty keydown custom-handler branch (`ghostty-web.js` ~2079–2087):
`true` → preventDefault+return; `false` → return without preventDefault.

```243:263:crates/ajax-web/web/src/components/TerminalRawView.svelte
    const hardenMobileTextarea = () => { ... }
```

```918:926:crates/ajax-web/web/src/components/TerminalRawView.svelte
      hardenMobileTextarea();
      term.textarea?.addEventListener("beforeinput", handleTextareaBeforeInput);
      terminalSubscriptions.push(
        term.onScroll(...),
        term.onData(handleTerminalData),
      );
```

Mock Terminal in `TerminalRawView.test.ts` (~53–137) — add
`attachCustomKeyEventHandler` spy that stores the handler.

Existing tests: `"always forwards backspace to the PTY"`,
`"updates textarea optimistic input for backspace and enter"`.

## 6. Test-first instructions

### T1 — custom handler skips Backspace keydown

Add test:
`"skips Ghostty Backspace keydown so iOS can key-repeat"`.

1. Extend MockTerminal with:
   `attachCustomKeyEventHandler = vi.fn((handler) => { customKeyHandler = handler; })`
   (module-level `let customKeyHandler`).
2. `mountOpenTerminal()`, wait until `customKeyHandler` is defined.
3. Assert `customKeyHandler({ key: "Backspace" } as KeyboardEvent) === false`.
4. Assert `customKeyHandler({ key: "a" } as KeyboardEvent)` is `undefined`
   (or not `false`) so normal keys still go through Ghostty.
5. Optionally same for `Delete` → `false`.

### T2 — ZWS sentinel seeded

Add test:
`"seeds a zero-width space in the textarea so iOS backspace can repeat"`.

1. `mountOpenTerminal()`.
2. Assert `lastTextarea!.value` includes `\u200B` (or equals `\u200B`).

### T3 — repeated beforeinput deletes forward multiple DEL

Add test:
`"forwards repeated deleteContentBackward events as multiple backspaces"`.

This proves the beforeinput path (what iOS repeat uses) still reaches the PTY
when keydown is skipped:

1. `mountOpenTerminal()`, clear `socket.send`.
2. Simulate what Ghostty's beforeinput would emit after our keydown skip:
   call `onDataHandler?.("\x7f")` three times (Ghostty still owns beforeinput →
   onData; Ajax's handleTerminalData forwards to the socket).
3. Assert `inputPayloadsOf(socket!)` has three `"\x7f"` entries.

Wait — T3 as written only re-tests existing onData forwarding. Stronger T3:

Alternative T3 (preferred if mock can fire beforeinput through Ghostty):
Not available — Ghostty is mocked. Instead:

**T3b** — source/behavior contract that Ajax does not preventDefault Backspace
and does attach the custom handler:

```
expect(terminalRawViewSource).toMatch(/attachCustomKeyEventHandler/);
expect(terminalRawViewSource).toMatch(/key === ["']Backspace["']/);
expect(terminalRawViewSource).toMatch(/\\u200B/);
```

And a runtime test that after three `dispatchTextareaBeforeInput("deleteContentBackward")`
calls, the optimistic overlay trims three times (already partially covered) —
plus assert the custom handler returns false (T1).

Also add: after `dispatchTextareaBeforeInput("deleteContentBackward")`,
textarea still contains `\u200B` (reseed if Ghostty mock doesn't preventDefault
the value change — in jsdom, beforeinput does not mutate value automatically,
so assert seed remains or reseed helper ran).

Minimum required failing tests before impl: **T1 + T2**. T3 source-guard is fine.

Focused failing command:
```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "Backspace|zero-width|key-repeat"
```
(or run the new test names explicitly)

## 7. Production edit instructions

1. After `hardenMobileTextarea()` / terminal open wiring, call:

```ts
term.attachCustomKeyEventHandler((event) => {
  // iOS hold-to-delete repeats via beforeinput deleteContentBackward.
  // Ghostty's keydown always preventDefault()s Backspace, which cancels that
  // loop. Returning false skips Ghostty's keydown handling without
  // preventDefault; beforeinput still emits \x7f.
  if (event.key === "Backspace" || event.key === "Delete") return false;
  return undefined;
});
```

2. In `hardenMobileTextarea`, seed the sentinel if missing:

```ts
const BACKSPACE_SENTINEL = "\u200B";
if (!input.value.includes(BACKSPACE_SENTINEL)) {
  input.value = BACKSPACE_SENTINEL;
}
```

3. Reseed on focus so blur/refocus and Ghostty clears cannot leave it empty:

```ts
input.addEventListener("focus", () => {
  if (!input.value.includes(BACKSPACE_SENTINEL)) input.value = BACKSPACE_SENTINEL;
});
```

(Store/remove the listener in the existing dispose path next to beforeinput
remove, or use `{ once: false }` and removeEventListener on cleanup.)

4. Do **not** send `\x7f` from Ajax's beforeinput handler — Ghostty still does
   that. Ajax beforeinput stays overlay-only.

## 8. Verification commands

```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "skips Ghostty Backspace"
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "zero-width space"
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "always forwards backspace"
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

## 9. Acceptance criteria

- T1/T2 fail before impl, pass after.
- Existing backspace/overlay tests still pass.
- Full TerminalRawView suite + web:check + web:build green.
- Diff limited to Allowed files.
- Holding Backspace on iOS can fire repeated deletes (keydown not
  preventDefaulted; textarea has deletable sentinel).

## 10. Stop conditions

- Mock has no way to capture `attachCustomKeyEventHandler` → extend mock as
  specified; if still blocked, stop and report.
- Attaching the handler breaks printable input tests → stop and report.
- Required edit outside Allowed files → stop.
- Test passes before production edit → stop and report.
