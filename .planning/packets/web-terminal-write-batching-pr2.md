# TDD Packet: Client terminal write batching (PR2)

## 1. Goal

Batch decoded terminal output text for ~16ms (or until 32_000 queued chars) before a single `term.write`, and apply scroll-follow / scrollback compensation once per flush — without delaying keystroke input.

## 2. Allowed files

**Production**

- `crates/ajax-web/web/src/terminalOutputPolicy.ts` (add batcher + constants; keep existing helpers)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte` (wire batcher into output path only)

**Tests**

- `crates/ajax-web/web/src/terminalOutputPolicy.test.ts` (extend with batcher tests)
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` (only if a focused behavior assertion is required; prefer pure unit tests)

**Plan (optional checklist only)**

- `.planning/agent-plans/web-mobile-power-optimizations.md`

## 3. Forbidden changes

- Do not delay, coalesce, or batch keyboard / paste / zero-lag input (`sendInput`, `handleTerminalData`, optimistic printable path).
- Do not change WebSocket framing, base64 decode, or `terminalConnection.ts` message parsing (server batching / binary are later PRs).
- Do not change scrollback line limits, resize debounce semantics beyond what flush already does, polling, or Rust PTY code.
- Do not migrate terminal libraries.
- Do not edit `web/dist/`.
- No drive-by refactors outside the output write path.

## 4. Architecture context

Browser shell presents server terminal bytes; Ajax owns follow-output policy via `pinnedToBottom` / `outputFollowEffects` / `scrollbackGrowthCompensation` (`terminalOutputPolicy.ts`). Batching is a presentation coalescing layer: same bytes reach Ghostty, fewer layout/write calls. Core/registry truth is untouched.

Today `writeOutput` in `TerminalRawView.svelte` writes every WS chunk immediately and runs follow/compensation per chunk. Pre-mount uses `pendingOutput: string[]` flushed once when Ghostty mounts.

## 5. Code anchors

Constants / API to add in `terminalOutputPolicy.ts`:

```ts
export const TERMINAL_WRITE_FLUSH_MS = 16;
export const TERMINAL_WRITE_MAX_CHARS = 32_000;

export type TerminalWriteBatcher = {
  push(text: string): void;
  flush(): void;
  dispose(): void;
  /** Test/inspection: queued character count. */
  pendingChars(): number;
};

export function createTerminalWriteBatcher(options: {
  flushMs?: number;
  maxChars?: number;
  now?: () => number;
  schedule?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
  onFlush: (combined: string) => void;
}): TerminalWriteBatcher;
```

Existing helpers to **reuse unchanged** on each flush (not per chunk):

- `scrollbackGrowthCompensation`
- `outputFollowEffects`

`TerminalRawView.svelte` anchors:

- `const pendingOutput: string[] = [];`
- `const writeToTerminal = (text: string) => { ... term.write(text); }`
- `const writeOutput = (text: string) => { ... }` — currently per-chunk write + follow
- `connection = connectTaskTerminal(handle, { onOutput: writeOutput, ... })`
- `flushPendingOutput` before mount completion

Injected timers in tests (do not rely on real 16ms flakiness): pass `schedule` / `clearSchedule` / optional immediate flush via `maxChars`.

## 6. Test-first instructions

Extend `crates/ajax-web/web/src/terminalOutputPolicy.test.ts`.

Add tests (exact names):

1. `createTerminalWriteBatcher coalesces pushes until flush timer fires`
2. `createTerminalWriteBatcher flushes immediately when max chars is reached`
3. `createTerminalWriteBatcher flush delivers one combined string and clears the queue`
4. `createTerminalWriteBatcher dispose cancels a pending timer without flushing`
5. `TERMINAL_WRITE_FLUSH_MS is 16 and TERMINAL_WRITE_MAX_CHARS is 32000`

Behavior details:

- Test 1: use fake `schedule` that records the callback; `push("a"); push("b");` → `onFlush` not called; invoke scheduled callback → `onFlush` called once with `"ab"`.
- Test 2: `maxChars: 5`, `push("123"); push("45");` → flush fires on the push that crosses/reaches max (combined `"12345"`), without needing the timer.
- Test 3: after timer flush, `pendingChars()` is 0; a manual `flush()` with empty queue is a no-op (does not call `onFlush`).
- Test 4: `push("x"); dispose();` → scheduled callback never runs / cleared; `onFlush` not called.
- Empty `push("")` may be ignored (do not schedule).

**Fail first:**

```bash
npm run web:test -- crates/ajax-web/web/src/terminalOutputPolicy.test.ts --run
```

Then implement batcher. Then wire view. Then:

```bash
npm run web:test -- crates/ajax-web/web/src/terminalOutputPolicy.test.ts crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
```

## 7. Production edit instructions

### `terminalOutputPolicy.ts`

Implement `createTerminalWriteBatcher`:

- Append non-empty strings to an internal buffer (string concat or array join on flush — either OK; prefer array + `join("")` on flush to avoid quadratic concat if easy).
- On `push`: if buffer empty, `schedule(flush, flushMs ?? TERMINAL_WRITE_FLUSH_MS)`.
- If `pendingChars >= maxChars`, call `flush()` immediately (cancel timer first).
- `flush`: cancel timer; if buffer empty return; else take combined text, clear buffer, call `onFlush(combined)` once.
- `dispose`: cancel timer; drop buffer without calling `onFlush` (unmount path).

### `TerminalRawView.svelte`

1. Import `createTerminalWriteBatcher` (and constants only if needed).
2. Replace per-chunk `writeOutput` body so WS `onOutput` **pushes** into the batcher.
3. Batcher `onFlush(combined)` must:
   - Capture `pinnedToBottom` / scrollback **before** write (same logic as today’s `writeOutput`).
   - Call `writeToTerminal(combined)` **once**.
   - Apply `scrollbackGrowthCompensation` once if not pinned.
   - Apply zero-lag optimistic clearing using `combined` (same `includes` check as today).
   - Apply `outputFollowEffects` **once** (snap or mark unseen).
4. Keep `writeToTerminal` for the actual `term.write` / pre-mount queue.
5. On dispose/unmount: `batcher.dispose()` in the existing cleanup; also flush or dispose consistently — **prefer `flush()` then `dispose()` on unmount only if term still exists and you want last paint; otherwise `dispose()` is OK**. Prefer: `flush()` if `term` is mounted so last bytes render, then `dispose()`.
6. Pre-mount: either keep `pendingOutput` as today and let batcher flush into `writeToTerminal` (which queues while `!term`), **or** let batcher hold bytes until mount then flush — simplest: keep `writeToTerminal` pendingOutput behavior; batcher `onFlush` → `writeToTerminal` so pre-mount still works; call existing `flushPendingOutput` after open as today.
7. **Do not** batch `appendZeroLagInput` / local echo — only server `onOutput` path.
8. Input `connection.sendInput` remains immediate.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/terminalOutputPolicy.test.ts --run
npm run web:test -- crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
```

## 9. Acceptance criteria

- New batcher tests fail before implementation, pass after.
- Defaults: 16ms / 32_000 chars.
- Multiple rapid `push`es → one `onFlush` with concatenated text (timer path).
- Crossing max chars flushes without waiting for timer.
- `TerminalRawView` uses batcher for WS output; follow/compensation once per flush.
- Input path unchanged (no new timers on sendInput).
- Existing TerminalRawView tests still pass.

## 10. Stop conditions

- Wiring requires changing Ghostty mocks or large TerminalRawView.test rewrites beyond a small fix.
- Scrollback compensation semantics become ambiguous under batching (e.g. tests fail on hold-position) — stop and report with failing test names.
- Need files outside Allowed files.
