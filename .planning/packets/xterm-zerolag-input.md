# TDD Implementation Packet — xterm-zerolag-input

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Add the product zero-lag typed-echo overlay to the xterm `TaskTerminal`
surface. Typed printable text must paint at the cursor before PTY echo returns,
using a positioned overlay with class and `data-testid` exactly
`xterm-zerolag-input`. Clear the prediction when the real echo advances the
cursor, when the pending text matches an output chunk, or after the 300ms idle
window — never leave a duplicate ghost.

Port the prediction algorithm from deleted Ghostty `terminalZeroLag.ts`
(`a02fc20`), adapted to measure xterm DOM instead of Ghostty canvas/renderer.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/xtermZeroLag.ts` (new)
- `crates/ajax-web/web/src/shared/lib/xtermZeroLag.test.ts` (new)
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/TERMINAL.md`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`

## Forbidden changes

- Do not recreate `crates/ajax-web/web/src/terminalZeroLag.ts` or
  `crates/ajax-web/web/e2e/terminal-zero-lag.test.ts` (legacy removal hygiene).
- Do not use class/testid `terminal-zero-lag-input` — must be
  `xterm-zerolag-input`.
- Do not change WS protocol, scroll sync, geometry/refit, paste/copy, expand,
  or control-key bar behavior beyond zero-lag bookkeeping on the existing input
  path.
- Do not add dependencies.
- No commits, pushes, merges, rebases, or branch switches.

## Context evidence

### Desired behavior

`TERMINAL_BEHAVIOR_CONTRACT.md` Product row: "Typed text appears at the cursor
before the PTY echo returns; the prediction is cleared the moment the real echo
advances the cursor or after an idle window." Legacy Ghostty mechanism was
`terminalZeroLag.ts` (removed in `6bbef9c`); Product outcome must return on
xterm.

### Source anchors (current)

- Mount owner: `TaskTerminal.tsx` — `hostElRef` / `.terminal-host` (~1052),
  `onTermData` → `sendKey(consumeCtrl(data))` (~426-428, ~930),
  `onOutput` → `term.write(text, scrollSync.applyOutput)` (~944-945),
  Space custom key handler bypasses `onData` (~899-908),
  `termTextarea()` finds `textarea.xterm-helper-textarea` (~238-241),
  reconnect `onOpen` (~956-968), cleanup (~1001-1040).
- Cell-height DOM measure pattern already in
  `terminalScrollSync.ts:28-38` (`.xterm-rows > *` / `getBoundingClientRect`).
- Styles: `.terminal-host textarea.xterm-helper-textarea` block in
  `styles.css:1457-1471` — add sibling rule for `.xterm-zerolag-input`.
- Legacy path ban: `legacyTerminalRemoval.test.ts:29-30,37` forbids recreating
  old `terminalZeroLag*` / `e2e/terminal-zero-lag.test.ts` paths.

### Reuse (deleted reference at `a02fc20`)

Recover algorithm from `git show a02fc20:crates/ajax-web/web/src/terminalZeroLag.ts`:
`createZeroLagEcho`, `createZeroLagOverlayPainter`, `zeroLagOverlayStyle`,
`measureZeroLagCursor`, `ZERO_LAG_IDLE_CLEAR_MS = 300`.

Ghostty host wiring (`TerminalRawView.svelte` ~755-864): painter on host,
`beforeinput` insertText/deleteContentBackward/insertLineBreak, `onData`
printable/`\r`/`\x7f` → `onTerminalData`, write flush → `clearIfEchoedIn`,
reconnect → `reset`.

Old CSS (Svelte scoped → port to `styles.css`):
```css
.terminal-host .xterm-zerolag-input {
  position: absolute;
  z-index: 1;
  max-width: calc(100% - 16px);
  overflow: hidden;
  color: #f4eee0;
  font-family: ui-monospace, SF Mono, Menlo, monospace;
  font-size: 16px;
  line-height: 1.2;
  pointer-events: none;
  text-shadow: 0 0 6px #1c1714;
  white-space: pre;
}
```
(left/top/font-size/line-height come from inline `cssText` via painter.)

### Architecture

Browser terminal UI presents PTY truth; overlay is display-only prediction and
must not become task truth. Keep ownership in `shared/lib` + `TaskTerminal.tsx`
per `TERMINAL.md`.

## Code anchors

Export constants must be:

```ts
export const ZERO_LAG_OVERLAY_CLASS = "xterm-zerolag-input";
export const ZERO_LAG_OVERLAY_TESTID = "xterm-zerolag-input";
```

Painter inserts `<div class="xterm-zerolag-input" data-testid="xterm-zerolag-input"
aria-hidden="true">` as first child of host; empty text removes the node.

Replace Ghostty `measureZeroLagFromTerminalHost` (canvas +
`term.renderer.getMetrics`) with `measureZeroLagFromXtermHost`:

```ts
// Prefer .xterm-screen clientWidth/Height; cellHeight from
// host.querySelector(".xterm-rows > *")?.getBoundingClientRect().height;
// cellWidth from screen.clientWidth / term.cols when row height known.
// Read cursorX/Y from term.buffer.active; fontSize from term.options.fontSize.
```

Keep `createZeroLagEcho` / `clearIfEchoedIn` / idle-clear semantics identical to
`a02fc20` (including cursor-anchor clear when echo advances).

## Test-first instructions

Create `crates/ajax-web/web/src/shared/lib/xtermZeroLag.test.ts` ported from
`git show a02fc20:crates/ajax-web/web/src/terminalZeroLag.test.ts`, with these
required adaptations:

1. Import from `./xtermZeroLag`.
2. Overlay assertions use `xterm-zerolag-input` / `ZERO_LAG_OVERLAY_TESTID`.
3. Replace `measureZeroLagFromTerminalHost` canvas tests with
   `measureZeroLagFromXtermHost` tests that build a host containing
   `.xterm-screen` + `.xterm-rows > div`, plus a fake term with
   `buffer.active.cursorX/Y`, `cols`, `rows`, `options.fontSize`.

Required failing cases before production code exists (RED):

- `createZeroLagEcho`: append then text; beforeinput+matching onTerminalData no
  double-append; clearIfEchoedIn substring/prefix/idle/cursor-advance cases
  (same intent as a02fc20 tests named around lines 151,171,186).
- `createZeroLagOverlayPainter`: paint creates
  `[data-testid=xterm-zerolag-input]`; second paint same node; empty removes;
  dispose removes.
- `measureZeroLagFromXtermHost`: returns metrics from xterm DOM; null when
  cursor missing.
- `zeroLagOverlayStyle`: left/top/font-size/line-height; no `bottom`.

RED command:

```bash
npm run web:test -- --run src/shared/lib/xtermZeroLag.test.ts
```

Expect nonzero exit and missing-module / failing assertions.

Then add source-contract cases in `TaskTerminal.test.tsx`:

- `TaskTerminal.tsx` imports from `@/shared/lib/xtermZeroLag` (or relative
  equivalent) and mentions `xterm-zerolag-input` / `beforeinput` /
  `clearIfEchoedIn`.
- `styles.css` contains `.terminal-host .xterm-zerolag-input` with
  `pointer-events: none` and `position: absolute`.

These TaskTerminal source tests may go RED after the unit module is green but
before wiring — that is expected; implement wiring next.

## Edit instructions

1. Add `xtermZeroLag.ts` with ported algorithm + xterm measure + overlay class
   `xterm-zerolag-input`.
2. Make `xtermZeroLag.test.ts` green.
3. Wire `TaskTerminal.tsx` inside the mount effect (smallest edits):
   - After `liveTerm.open(hostEl)` + harden: create
     `createZeroLagOverlayPainter(() => hostEl)` and `createZeroLagEcho` with
     measure via `measureZeroLagFromXtermHost({ host: hostEl, term: liveTerm,
     defaultFontSize: DEFAULT_FONT_SIZE })`.
   - Attach `beforeinput` on `textarea.xterm-helper-textarea`:
     `insertText` → `noteBeforeInputPrintable`; `deleteContentBackward` →
     `noteBeforeInputBackspace`; `insertLineBreak` → `clear`.
   - In `onTermData` / Space custom handler: for `\r`, `\x7f`, and single
     printable (`charCode >= 32`), call `zeroLag.onTerminalData` (Space path
     currently bypasses `onData` — must still bookkeep). Then send input as
     today.
   - `onOutput`: after write parses, call `zeroLag.clearIfEchoedIn(text)` then
     existing `scrollSync.applyOutput` (use write callback).
   - `onOpen`: `zeroLag.reset()`.
   - cleanup: remove `beforeinput` listener; `painter.dispose()`; `zeroLag.reset()`.
4. Add CSS rule under `.terminal-host .xterm-zerolag-input` as above.
5. Update `TERMINAL.md` ownership table with zero-lag owner
   `xtermZeroLag.ts`.
6. Update `TERMINAL_BEHAVIOR_CONTRACT.md` Product typed-echo row evidence to
   `xtermZeroLag.ts` / `xterm-zerolag-input` (keep Product meaning; Legacy
   Ghostty row may stay historical).

## Verification commands

```bash
npm run web:test -- --run src/shared/lib/xtermZeroLag.test.ts
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/legacyTerminalRemoval.test.ts
```

## Acceptance criteria

- Overlay class and testid are exactly `xterm-zerolag-input`.
- Unit prediction / painter / measure / style tests green.
- TaskTerminal wires beforeinput + onData/Space + clearIfEchoedIn + reset/dispose.
- CSS present; legacy removal hygiene still green.
- No forbidden legacy paths recreated.

## Stop conditions

- Need private xterm `_core` APIs to measure → stop and report (use DOM measure).
- Diff exceeds ~400 lines or spills outside Allowed files → stop.
- Existing TaskTerminal behavior tests fail for unrelated reasons → stop.
- Temptation to recreate Ghostty canvas path or old filenames → stop.
