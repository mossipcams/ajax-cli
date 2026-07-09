# TDD Packet: Resize dedupe / debounce (PR4)

## 1. Goal

Debounce PTY resize at 100ms and skip sending when cols/rows are unchanged, so iOS viewport bursts and duplicate fits do not spam SIGWINCH.

## 2. Allowed files

- `crates/ajax-web/web/src/terminalRefit.ts`
- `crates/ajax-web/web/src/terminalRefit.test.ts`
- `crates/ajax-web/web/src/terminalOutputPolicy.ts` (add pure resize-send helper if cleaner)
- `crates/ajax-web/web/src/terminalOutputPolicy.test.ts` (if helper lives here)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte` (wire dedupe around `sendResize`)
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` (only if required)
- `.planning/agent-plans/web-mobile-power-optimizations.md` (optional checklist)

## 3. Forbidden changes

- Do not change WebSocket resize JSON schema.
- Do not batch input or change output batching.
- Do not change keyboard-open resize freeze logic beyond wrapping the actual `connection.sendResize` call with dedupe.
- No Rust edits. No `web/dist/`. No drive-by refactors.

## 4. Architecture context

Browser shell may resize the ephemeral PTY; duplicate resize frames are waste. Local fit stays cheap; server resize is expensive. Existing `createRefitScheduler` already debounces at `RESIZE_DEBOUNCE_MS` (currently 300).

## 5. Code anchors

```ts
// terminalRefit.ts
export const RESIZE_DEBOUNCE_MS = 300;
```

```ts
// TerminalRawView.svelte
const sendResize = () => {
  if (isKeyboardOpen() && !pinchFlushPending && !expandFlushPending) return;
  if (!term) return;
  const size = validTerminalSize(term.cols, term.rows);
  if (!size) return;
  connection.sendResize(size.cols, size.rows);
};
```

Reuse `validTerminalSize` from `terminalOutputPolicy.ts`.

## 6. Test-first instructions

1. Update/add in `terminalRefit.test.ts`: assert `RESIZE_DEBOUNCE_MS === 100` (change constant from 300 → 100; existing timer tests should keep using the constant).
2. Add pure helper tests (prefer `terminalOutputPolicy.test.ts`):

   - `createResizeDedupe skips send when cols and rows unchanged`
   - `createResizeDedupe sends when cols or rows change`
   - `createResizeDedupe reset clears last-sent so same size can send again`

Helper API:

```ts
export function createResizeDedupe(send: (cols: number, rows: number) => void): {
  sendIfChanged(cols: number, rows: number): void;
  reset(): void;
};
```

**Fail first** then implement.

```bash
npm run web:test -- crates/ajax-web/web/src/terminalRefit.test.ts crates/ajax-web/web/src/terminalOutputPolicy.test.ts --run
```

## 7. Production edit instructions

1. Set `RESIZE_DEBOUNCE_MS = 100`.
2. Implement `createResizeDedupe` in `terminalOutputPolicy.ts` (or `terminalRefit.ts` if you prefer co-location — pick one; prefer `terminalOutputPolicy.ts` next to `validTerminalSize`).
3. In `TerminalRawView.svelte`, wrap `connection.sendResize` with the dedupe: `resizeDedupe.sendIfChanged(size.cols, size.rows)`.
4. Call `resizeDedupe.reset()` on connection open/reconnect (`onOpen`) so a fresh attach can re-send the current size.
5. Keep keyboard-open / pinch / expand guards unchanged and outside the dedupe.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/terminalRefit.test.ts crates/ajax-web/web/src/terminalOutputPolicy.test.ts --run
npm run web:test -- crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
```

## 9. Acceptance criteria

- Debounce is 100ms.
- Unchanged cols/rows do not call `connection.sendResize`.
- Changed size does send.
- Reset on open allows re-send of same size after reconnect.
- Existing refit scheduler tests pass.

## 10. Stop conditions

- Need to change server resize handling or protocol.
- TerminalRawView tests require large rewrites — stop and report.
