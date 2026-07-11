# TDD Implementation Packet — terminal clipboard Slice 4

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Own paste-fallback / copy-overlay / copy-fallback / notice-flash *state* in
`terminalClipboard.ts` via `createTerminalClipboardUi`. `TerminalRawView`
mirrors state into `$state` for the template and runs Ghostty/clipboard
*effects* only (`term.paste`, `copyText`, selection clear).

## Allowed files

- `crates/ajax-web/web/src/terminalClipboard.ts` (new)
- `crates/ajax-web/web/src/terminalClipboard.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` (only the
  source-contract test that currently requires names inside the svelte file)
- `crates/ajax-web/web/TERMINAL.md`
- `crates/ajax-web/web/dist/*` (via `npm run web:build`)

## Forbidden changes

- Do not change paste/copy UX behavior (fallback when no clipboard, overlay →
  copyText → fallback on failure, notice flash 2500ms, separate from
  statusDetail).
- Do not edit terminalGestures selection math, layout policy, scroll-follow,
  zero-lag, or diagnostics `copyText` implementation.
- Do not weaken behavioral TerminalRawView paste/copy tests.
- No commits / branch changes.

## Context evidence

### Graphify
`NOT_REQUIRED`: TERMINAL.md ownership extension.

### Serena
`NOT_REQUIRED`: CLI lacks symbol tools; inventory via rg.

### rg inventory (TerminalRawView.svelte)

Paste UI state (~97–143, ~591–607):
- `pasteFallbackOpen`, `openPasteFallback`, `closePasteFallback`,
  `sendPasteFallbackText`, `requestPaste`, `pasteNotice`

Copy UI state (~110–118, ~448–493):
- `copyOverlayOpen`, `copyOverlayText`, `copyFallbackOpen`
- `flashCopyNotice` (2500ms), `dismissCopyUi`, `finishSelection` overlay arm,
  `handleCopyOverlay`

Ghostty/effect stays in component:
- `pasteToTerm` → `term.paste` + focus
- `applySelection` / `selectionCellAt` / `wordRangeAt` / `clearTermSelection`
- `copyText` from diagnostics

Source-contract test `"names paste fallback state transitions"` currently
asserts those names live in the svelte source — update it to assert the new
module (or re-export thin wrappers). Prefer asserting
`terminalClipboard.ts` / policy method names.

## Code anchors — behavior tests that must stay green

In `TerminalRawView.test.ts`:

- owns status, paste/copy fallbacks under bottom controls
- pastes clipboard text through terminal paste path
- keeps server error visible after successful paste
- surfaces clipboard read failure (opens fallback)
- copy overlay / fallback flows already covered in the bottom-controls test

## Test-first instructions

Create `terminalClipboard.test.ts`. Fail before module exists.

API (preferred):

```ts
export const COPY_NOTICE_MS = 2500;

export type ClipboardUiSnapshot = {
  pasteFallbackOpen: boolean;
  copyOverlayOpen: boolean;
  copyFallbackOpen: boolean;
  copyOverlayText: string;
  notice: string;
};

export type TerminalClipboardUi = {
  snapshot(): ClipboardUiSnapshot;
  openPasteFallback(): void;
  closePasteFallback(): void;
  /** Close paste fallback and return trimmed text to paste (may be ""). */
  takePasteFallbackText(raw: string): string;
  dismissCopyUi(): void;
  /** Arm copy overlay with selected text; no-op/dismiss if empty. */
  presentCopySelection(text: string): void;
  /** After overlay Copy tap: hide overlay; caller runs copyText. */
  beginCopyAttempt(): string;
  /** copyText succeeded → flash "Copied", clear text/fallback. */
  noteCopySucceeded(): void;
  /** copyText failed → open copy fallback with current text. */
  noteCopyFailed(): void;
  flashNotice(message: string): void;
  clearNotice(): void;
  dispose(): void;
};

export function createTerminalClipboardUi(options?: {
  noticeMs?: number;
  schedule?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
  onChange?: (snap: ClipboardUiSnapshot) => void;
}): TerminalClipboardUi;
```

Required cases:

1. open/close paste fallback toggles `pasteFallbackOpen`
2. `takePasteFallbackText` closes fallback and returns text
3. `presentCopySelection("")` dismisses; non-empty opens overlay, not fallback
4. `beginCopyAttempt` closes overlay and returns text
5. `noteCopySucceeded` sets notice "Copied", clears text/fallback; notice clears after NOTICE_MS
6. `noteCopyFailed` opens copy fallback
7. `dismissCopyUi` clears overlay/fallback/text
8. `dispose` clears notice timer

Call `onChange` after each mutation when provided (for Svelte sync).

Focused red:

```bash
npm run web:test -- --run src/terminalClipboard.test.ts
```

## Edit instructions

### A. Implement `terminalClipboard.ts` per API

### B. Wire `TerminalRawView.svelte`

- Create `clipboardUi = createTerminalClipboardUi({ onChange: syncClipboardUi })`
  at component scope (or onMount + top-level snapshot sync — prefer component
  scope so template bindings work; dispose on onMount cleanup).
- Replace local open/close/dismiss/flash/finishSelection overlay arming /
  handleCopyOverlay state writes with clipboardUi methods.
- Keep `$state` mirrors updated via `onChange` for template
  (`pasteFallbackOpen`, `copyOverlayOpen`, …, `pasteNotice` ← `notice`).
- `requestPaste`: if no `clipboard.readText`, `openPasteFallback()`; else
  readText → pasteToTerm / on catch openPasteFallback (same as today).
- `sendPasteFallbackText`: `const text = takePasteFallbackText(raw);` clear
  textarea value in component; `if (text) pasteToTerm(text)`.
- `handleCopyOverlay`: `const text = beginCopyAttempt() || term?.getSelection() || ""`;
  then `copyText`; success → `noteCopySucceeded` + `term.clearSelection()`;
  fail → `noteCopyFailed`.
- `finishSelection`: on cancel/empty clear selection + `dismissCopyUi`; else
  `presentCopySelection(text)`.
- Dispose clipboardUi in onMount cleanup (clear notice timer).

Preserve named functions if useful for readability:
`openPasteFallback = () => clipboardUi.openPasteFallback()` etc. OR update the
source-contract test — see C.

### C. Update source-contract test

Change `"names paste fallback state transitions"` to assert ownership in
`terminalClipboard.ts` (readFileSync like other ownership tests) for:
`openPasteFallback`, `closePasteFallback`, `takePasteFallbackText` (or
`sendPasteFallbackText` wrapper name if kept), `createTerminalClipboardUi`,
and that TerminalRawView still has `data-testid="terminal-paste-fallback"`.
Do not delete the coverage — relocate the name assertions.

### D. TERMINAL.md

Add row:

`| Paste/copy UI state (fallback/overlay/notice) | terminalClipboard.ts |`

Keep: Gestures / selection geometry → terminalGestures.ts

## Verification commands

```bash
npm run web:test -- --run src/terminalClipboard.test.ts
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
rg -n 'flashCopyNotice|dismissCopyUi|copyNoticeTimer' crates/ajax-web/web/src/components/TerminalRawView.svelte
# expect: gone or thin wrappers only; timer logic in terminalClipboard.ts
```

## Acceptance criteria

- New clipboard unit tests green
- Existing TerminalRawView paste/copy behavior tests green
- Source-contract updated to new owner, still asserts named transitions
- TERMINAL.md lists terminalClipboard.ts
- Diff limited to Allowed files

## Stop conditions

- UX behavior change required to pass → stop
- Need to edit gestures/layout/zero-lag → stop
- Weakening paste/copy behavior tests → stop
- Two failed delegate rounds → stop
