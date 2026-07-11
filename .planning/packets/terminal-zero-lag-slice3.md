# TDD Implementation Packet — terminal zero-lag Slice 3

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Complete zero-lag ownership in `terminalZeroLag.ts` by extracting:

1. Imperative overlay painter (`paint` / clear / dispose)
2. Cursor metrics measure helper from Ghostty term + host container

`TerminalRawView.svelte` keeps only event wiring (`beforeinput`, `onData`,
write-flush `clearIfEchoedIn`, reconnect `reset`). Do not change prediction
algorithm behavior in `createZeroLagEcho`.

## Allowed files

- `crates/ajax-web/web/src/terminalZeroLag.ts`
- `crates/ajax-web/web/src/terminalZeroLag.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/TERMINAL.md`
- `crates/ajax-web/web/dist/*` (only via `npm run web:build`)

## Forbidden changes

- Do not change `createZeroLagEcho` prediction / idle-clear / clearIfEchoedIn
  semantics (behavior-preserving move of paint/measure only).
- Do not edit layout policy, scroll-follow, paste/copy, gestures, or connection.
- Do not weaken existing TerminalRawView zero-lag tests.
- No commits / branch changes.

## Context evidence

### Graphify
`NOT_REQUIRED`: frontend module ownership; TERMINAL.md is the contract.

### Serena
`NOT_REQUIRED`: CLI lacks symbol tools; inventory via rg.

### ast-grep / rg inventory

Already owned in `terminalZeroLag.ts`:
- `zeroLagOverlayStyle`, `createZeroLagEcho`, `ZERO_LAG_IDLE_CLEAR_MS`

Still in `TerminalRawView.svelte` (~765–807):
- `paintZeroLag` DOM create/remove/update for `.terminal-zero-lag-input`
- inline `measure` reading canvas + `term.buffer.active` + renderer metrics

Event wires (stay in component): beforeinput, handleTerminalData onData,
writeBatcher clearIfEchoedIn, reconnect/dispose reset.

## Code anchors

Preserve these TerminalRawView tests (do not rewrite to pass):

- zero-lag overlay text / clear / font-size style
- no flushSync
- no CSS `bottom` stretch anchor
- renderer cell metrics positioning
- e2e zero-lag tests if touched indirectly (do not edit e2e)

Current paint contract:

- class `terminal-zero-lag-input`
- `data-testid="terminal-zero-lag-input"`
- `aria-hidden="true"`
- insert as first child of host
- empty text → remove node (querySelector null)

## Test-first instructions

Add to `terminalZeroLag.test.ts` (fail before helpers exist):

### Painter

`createZeroLagOverlayPainter(host)`:

1. `paint("hi", "left: 1px; top: 2px;")` creates one
   `[data-testid=terminal-zero-lag-input]` with text/style
2. second paint updates same node (still one element)
3. `paint("", "")` removes the node
4. `dispose()` removes node if present

### Measure

`measureZeroLagCursor(input)` (pure; no DOM query inside except via injected
fields — prefer a plain data input):

```ts
export type ZeroLagMeasureInput = {
  cursorX?: number;
  cursorY?: number;
  cols: number;
  rows: number;
  canvasWidth: number;
  canvasHeight: number;
  cellWidth?: number;
  cellHeight?: number;
  fontSize: number;
};

export function measureZeroLagCursor(
  input: ZeroLagMeasureInput | null | undefined,
): ZeroLagCursorMetrics | null;
```

Rules matching current component measure:

- null/undefined input → null
- missing cursorX/cursorY (undefined) → null
- otherwise return ZeroLagCursorMetrics with the provided fields

Optional thin adapter used by the component is fine:

```ts
export function measureZeroLagFromTerminalHost(args: {
  host: HTMLElement | null | undefined;
  term: {
    cols: number;
    rows: number;
    options: { fontSize?: number };
    buffer: { active: { cursorX?: number; cursorY?: number } };
    renderer?: { getMetrics?: () => { width?: number; height?: number } };
  } | null | undefined;
  defaultFontSize: number;
}): ZeroLagCursorMetrics | null;
```

This adapter queries `canvas:not([aria-hidden='true'])` and calls
`measureZeroLagCursor`. Unit-test the pure `measureZeroLagCursor` thoroughly;
one test for the host adapter with a fake host/term is enough.

Focused red:

```bash
npm run web:test -- --run src/terminalZeroLag.test.ts
```

## Edit instructions

### A. `terminalZeroLag.ts`

Add painter + measure helpers as above. Export constants for class/testid if
useful (`ZERO_LAG_OVERLAY_CLASS = "terminal-zero-lag-input"`).

### B. `TerminalRawView.svelte`

Replace inline `paintZeroLag` / measure block with:

```ts
const zeroLagPainter = createZeroLagOverlayPainter(/* host getter or container */);
const zeroLag = createZeroLagEcho({
  onChange: (text, style) => zeroLagPainter.paint(text, style),
  measure: () => measureZeroLagFromTerminalHost({
    host: container,
    term,
    defaultFontSize: DEFAULT_FONT_SIZE,
  }),
});
```

On dispose: `zeroLagPainter.dispose()` (in addition to `zeroLag.reset()`).

Painter needs a live host: either pass `() => container` into the factory, or
create the painter once container is known (same onMount scope as today).
If container can be null at paint time, painter no-ops like today.

Keep CSS for `.terminal-zero-lag-input` in the Svelte file (styling stays with
chrome). Do not move CSS unless required.

### C. TERMINAL.md

Add ownership row:

`| Zero-lag input echo + overlay paint | terminalZeroLag.ts |`

## Verification commands

```bash
npm run web:test -- --run src/terminalZeroLag.test.ts
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
rg -n 'paintZeroLag|terminal-zero-lag-input' crates/ajax-web/web/src/components/TerminalRawView.svelte
# expect: class name may remain in CSS; no local paintZeroLag function;
#         createElement for zero-lag overlay should be gone from svelte
```

Acceptance structural check:

```bash
rg -n 'createElement\("div"\)' crates/ajax-web/web/src/components/TerminalRawView.svelte
# zero-lag overlay createElement should not remain (other createElement ok if any)
```

## Acceptance criteria

- New painter/measure unit tests green
- Existing TerminalRawView zero-lag tests green unchanged in intent
- No inline overlay createElement/paintZeroLag in TerminalRawView
- TERMINAL.md lists terminalZeroLag ownership
- Diff limited to Allowed files

## Stop conditions

- Prediction algorithm change required to pass tests → stop
- Need to edit layout/scroll-follow/paste → stop
- Weakening TerminalRawView assertions → stop
- Two failed delegate rounds → stop
