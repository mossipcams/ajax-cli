# TDD Implementation Packet: Fix xterm dual background + resize spam

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Stop Surface V2 xterm from showing two background colors and from spamming /
duplicating text. Root causes: (1) default xterm theme vs Ajax host colors;
(2) undebounced `ResizeObserver` → `fit()` → `sendResize` feedback (and
scrollbar-induced size oscillation). Match Ghostty theme colors and only
`sendResize` when cols/rows actually change, with a short debounce on RO.

## 3. Allowed files

- `crates/ajax-web/web/src/components/XtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/XtermTerminalView.test.ts`
- `.planning/agent-plans/fix-xterm-visual-spam.md`

## 4. Forbidden changes

- Do not edit `TerminalRawView.svelte`, connection protocol, Rust, package.json
- Do not add zero-lag / gesture ports
- Do not commit / push / rebase / change branches

## 5. Context evidence

### Graphify
`NOT_REQUIRED`: single-component visual fix.

### Serena
`NOT_REQUIRED`: anchors from source.

### ast-grep / code anchors
- Ghostty theme in `TerminalRawView.svelte` ~L1002-1006:
  `background: "#1c1714"`, `foreground: "#f4eee0"`, `cursor: "#52a095"`
- Ghostty host CSS uses `background: #1c1714` (~L1411)
- Current `XtermTerminalView.svelte` constructs Terminal **without** `theme`
- Current `reportResize` always `fit()` + `sendResize(Math.max(cols, MIN_TERMINAL_COLS), rows)` on every RO callback and on open — no last-dims guard, no debounce
- Ghostty uses `scheduleDebouncedRefit` on ResizeObserver (~L834)

## 6. Code anchors

### Theme (Terminal options + CSS)
```ts
theme: {
  background: "#1c1714",
  foreground: "#f4eee0",
  cursor: "#52a095",
},
```
CSS on `.xterm-host` and `:global(.xterm), :global(.xterm-viewport), :global(.xterm-screen)`:
`background: #1c1714` (and height 100% on `.xterm` / viewport so the host does
not show a second color around the canvas).

### Resize spam fix
```ts
let lastSentCols = 0;
let lastSentRows = 0;
let resizeTimer: ReturnType<typeof setTimeout> | undefined;
const RESIZE_DEBOUNCE_MS = 50;

const reportResize = () => {
  fitAddon?.fit();
  if (!term) return;
  const cols = Math.max(term.cols, MIN_TERMINAL_COLS);
  const rows = term.rows;
  if (cols === lastSentCols && rows === lastSentRows) return;
  lastSentCols = cols;
  lastSentRows = rows;
  connection?.sendResize(cols, rows);
};

const scheduleReportResize = () => {
  if (resizeTimer) clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    resizeTimer = undefined;
    reportResize();
  }, RESIZE_DEBOUNCE_MS);
};
```
- `ResizeObserver` and `window.resize` → `scheduleReportResize`
- Immediate `reportResize()` once after open / on connection `onOpen` is OK
- Clear `resizeTimer` on unmount

## 7. Test-first instructions

Update `XtermTerminalView.test.ts`:

1. **Theme**: after mount, assert `Terminal` was constructed with
   `theme.background === "#1c1714"` and `theme.foreground === "#f4eee0"`
   (capture ctor args in the mock).

2. **No resize spam**: make mock Terminal `cols`/`rows` mutable; wire
   ResizeObserver callback; fire RO twice with same dims after fit — expect
   `sendResize` called once for that size (not twice). Use fake timers if
   debounce is involved.

3. Keep existing I/O / dispose / init-failure tests green.

RED command:
```bash
cd crates/ajax-web/web && npx vitest run src/components/XtermTerminalView.test.ts
```

## 8. Edit instructions

1. Write failing tests (section 7)
2. Add theme to Terminal options + host/global xterm CSS backgrounds
3. Add last-dims guard + debounce as in section 6
4. GREEN + web:check
5. Update plan checklist/results

## 9. Verification commands

```bash
cd crates/ajax-web/web && npx vitest run src/components/XtermTerminalView.test.ts
npm run web:check
```

## 10. Acceptance criteria

- Terminal options include Ghostty-matching theme
- Host/xterm layers share `#1c1714` background
- Repeated ResizeObserver with unchanged cols/rows does not re-send resize
- Existing XtermTerminalView tests still pass

## 11. Stop conditions

- Need to change PTY/server protocol
- Fix requires editing TerminalRawView
