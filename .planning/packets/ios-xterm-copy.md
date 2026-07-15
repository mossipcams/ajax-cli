# TDD Packet: xterm selection Copy overlay

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Terminal Copy works again on the xterm surface:

1. When the terminal has a non-empty selection, show a Copy control.
2. Tapping Copy writes via existing `copyText` (clipboard API + execCommand
   fallback). Success flashes a short "Copied" notice; failure opens a
   read-only textarea fallback the user can long-press to copy natively.
3. Long-press on the interaction surface that yields a word selection must not
   send PTY input (existing long-press case stays green) and should present
   the Copy control when selection text is non-empty.

Do not restore the deleted `terminalClipboard.ts` file (legacy removal suite
forbids it). Keep copy UI state local to `TaskTerminal.svelte` or a new small
helper that is not on the removal list.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/diagnostics.ts` (only if `copyText` needs a tiny
  synchronous-gesture-safe tweak; prefer no change)
- `.planning/agent-plans/ios-xterm-mobile-bugs.md`
- `.planning/packets/ios-xterm-copy.md`

## Forbidden changes

- Do not recreate `terminalClipboard.ts` / Ghostty selection managers.
- Do not edit `legacyTerminalRemoval.test.ts` to allow deleted files back.
- Do not change Rust / architecture / PTY protocol.
- Do not undo Task 1 fullscreen/chrome/compact-key fixes.
- No commit / push / branch changes.
- No new dependencies.

## Context evidence

- Graphify: `NOT_REQUIRED` — UI-only presentation of existing terminal I/O.
- Serena: `NOT_REQUIRED` — anchors below.
- ast-grep: `NOT_REQUIRED` — Svelte/TS string anchors.

## Code anchors

Paste + `copyText` import target:

```4:10:crates/ajax-web/web/src/diagnostics.ts
export async function copyText(text: string): Promise<boolean> {
```

Existing paste fallback UI pattern in `TaskTerminal.svelte` (mirror for copy
fallback readonly textarea).

xterm public APIs to use only:

- `term.onSelectionChange`
- `term.getSelection()` / `term.hasSelection()` / `term.clearSelection()`
- `term.select(column, row, length)` if needed for long-press word select
- buffer line cell reads via `term.buffer.active.getLine` / `translateToString`

Do not cast into private selection managers.

Existing case to keep green:

- `long press on the interaction surface sends no PTY input`

Prior product contract (behavior target, not file restore):

> Copy: long-press selection → Copy overlay → copyText; failure opens read-only
> textarea (`TERMINAL_BEHAVIOR_CONTRACT.md`).

## RED

Add Mobile WebKit case(s) that fail before the impl:

1. Programmatically create a non-empty xterm selection (via page.evaluate calling
   public `select` / writing known text then selecting), assert a Copy control
   is visible (`getByRole('button', { name: 'Copy' })` inside the terminal
   panel).
2. Click Copy with `navigator.clipboard.writeText` mocked to succeed; assert
   clipboard received the exact selected text and a Copied/status notice
   appears (or selection clears / overlay dismisses — pick one observable).
3. With clipboard write failing / unavailable, Copy opens a read-only fallback
   containing the selected text (accessible name ok).

Run RED before production edits.

## Implementation

Smallest path:

1. Subscribe to `onSelectionChange`; when `getSelection()` is non-empty, set
   local `copyOverlayText` / open overlay button; when empty, dismiss.
2. Copy button: `await copyText(text)`; on success flash notice + clear
   selection/overlay; on failure open readonly fallback textarea + keep text.
3. Optional long-press word select: if a 500ms single-finger press without
   move ends and there is no selection yet, map touch to cell and
   `term.select` the word — without sending PTY input. Skip if this exceeds
   scope; selection via evaluate in tests + desktop drag is the minimum bar,
   but phone long-press should work if cheap.

Reuse paste-fallback layout tokens; do not add a new abstraction module unless
it stays under ~80 lines and is not named `terminalClipboard.ts`.

## Verification

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 \
  --grep 'Copy|long press on the interaction|Paste stays available|clipboard fallback'

rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 \
  --grep 'fullscreen band|keyboard-open hides|compact terminal keys|phone fullscreen keeps background'

rtk npm run web:check
rtk git diff --check
```

## Stop conditions

- Would recreate deleted Ghostty clipboard module name/path
- Exceeds ~400 lines
- Requires private xterm APIs
- Breaks Task 1 cases

## Return

Exact `DELEGATE_REPORT` with RED/GREEN/VERIFY evidence.
