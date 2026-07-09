# Plan: three separate Web Cockpit regressions

## Scope

Three independent iOS Web Cockpit regressions reported 2026-07-08:

1. **Fullscreen blank column** under the ⛶ button (canvas narrower than panel).
2. **Duplicate typing off the terminal** in inline (non-fullscreen) mode.
3. **iOS backspace hold** does not send repeated deletes.

Non-goals: ghostty-web bump, architecture changes, unrelated paste/copy redesign,
desktop layout changes beyond what these fixes require.

## Root causes (inspected)

### R1 — blank column under ⛶

`beginExpandFlush` clears `expandFlushPending` after two rAFs, but
`snapExpandedView` keeps scheduling `schedulePostLayoutRefit` through a 260ms
settle window. After the flag clears, `fitNow`/`sendResize` hit the
keyboard-open guard and skip — the grid stays at the pre-expand column count,
left-aligned, empty strip under the corner button.

Also: expand focuses the textarea → keyboard opens a few frames later, so the
flush can finish *before* `keyboard-open` is set; later settle refits then
freeze.

### R2 — duplicate typing off-terminal

`#393` softened the hidden textarea (`opacity: 0.01`, `clip-path: none`) so
iOS native Paste can target it. Ghostty still paints that textarea at
`left:0; top:0` with real text color — typed characters become a faint/off-canvas
echo beside the canvas, while the zero-lag overlay / PTY echo also show the
same text.

### R3 — backspace hold

ghostty-web `handleKeyDown` always `preventDefault()`s Backspace. On iOS,
hold-to-delete repeats via `beforeinput` `deleteContentBackward`, and
preventDefault on the initial keydown cancels that loop. An empty textarea
also has nothing to delete, so repeats never start. Fix: skip Ghostty's
Backspace keydown via `attachCustomKeyEventHandler` returning `false`, seed a
ZWS sentinel so iOS always has content to delete, and let Ghostty's existing
`beforeinput` path emit `\x7f`.

## Delegation decision

`Delegation decision: delegated via model-router` — one behavior at a time,
Cursor / Grok 4.5 High (complex frontend / terminal / iOS viewport).

Order: R1 → R2 → R3. Each gets its own TDD packet and review gate.

## Approval

User reported regressions and asked for fixes — authorized to implement.

## Task checklist

### R1 — expand flush survives settle

- [x] Packet: `.planning/agent-plans/packets/r1-expand-flush-settle.md`
- [x] Test: settle-window keyboard-open expand still refits after ~260ms
- [x] Impl: keep `expandFlushPending` through snap settle; clear after final refit
- [x] Verify focused TerminalRawView tests + web:check + web:build

### R2 — invisible pasteable textarea

- [x] Packet: `.planning/agent-plans/packets/r2-invisible-textarea-echo.md`
- [x] Test: textarea text/caret paint is transparent; paste unclip contracts hold
- [x] Impl: transparent text/caret fill; keep opacity/clip paste target
- [x] Verify focused tests + web:check + web:build

### R3 — iOS backspace key-repeat

- [x] Packet: `.planning/agent-plans/packets/r3-ios-backspace-repeat.md`
- [x] Test: Backspace custom handler returns false; ZWS seeded; reseed after delete
- [x] Impl: attachCustomKeyEventHandler + ZWS seed/reseed on focus and beforeinput
- [x] Verify focused tests + web:check + web:build

## Validation (after each, and once at end)

```bash
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

Broader when all three land:

```bash
npm run web:test -- --run
cargo nextest run -p ajax-web --all-features --test-threads=1
```

## Deviations

- R1: expand flush timer is cleared on dispose / restart of `beginExpandFlush`,
  not inside `cancelExpandedSnap` (that runs at the start of `snapExpandedView`
  and would cancel the timer immediately).
- R3: parent added reseed on `deleteContentBackward` after review — without it,
  the first iOS delete can empty the textarea and stop the hold-repeat loop
  because keydown is no longer preventDefaulted.

## Validation results

- PASS R1 settle-window + early expand + full TerminalRawView (125→ then grew) + web:check + web:build
- PASS R2 transparent paint + clip/opacity + full TerminalRawView + web:check + web:build
- PASS R3 Backspace skip + ZWS + existing backspace + full TerminalRawView (130) + web:check + web:build
- Delegation: Cursor / Grok 4.5 High for R1–R3; parent reviewed diffs and re-ran validation
