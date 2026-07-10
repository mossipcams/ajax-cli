# TDD Implementation Packet — terminal reset on reconnect

## Goal

When the task terminal WebSocket reconnects, clear the Ghostty buffer before
the new tmux attach replays history, re-pin follow-output to the bottom, and
snap the viewport to the newest output so post-create reconnects do not stack
duplicate text or leave the view at the top of scrollback.

## Allowed files

Production:
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

Tests:
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

## Forbidden changes

- Do not edit `terminalConnection.ts` reconnect policy (it already passes
  `isReconnect`).
- Do not change zero-lag heuristics, paste fallback, expand/pinch flush, or
  gesture code except as required to call reset from `onOpen`.
- Do not clear/reset on the *first* successful open (`isReconnect === false`).
- Do not edit `styles.css`, TaskDetail, or Rust/PTY code.
- No formatting sweeps or drive-by cleanup.
- Do not hand-edit `crates/ajax-web/web/dist/*`.

## Architecture context

`terminalConnection.ts` owns socket lifecycle and calls
`events.onOpen(isReconnect)` after a successful open (`isReconnect` when
`reconnectAttempts > 0`). `TerminalRawView.svelte` owns the Ghostty instance
and follow-output policy (`pinnedToBottom`, `snapScrollbackToBottom`). A fresh
tmux attach always repaints the pane; replaying into a non-cleared buffer
duplicates history.

Ghostty API (from bundled ghostty-web): `term.reset()` rebuilds the wasm
terminal and clears the renderer; prefer that over `clear()` (ANSI clear only).

## Code anchors

`terminalConnection.ts` already:

```ts
events.onOpen(isReconnect);
```

Current consumer in `TerminalRawView.svelte` (must change):

```ts
onOpen: () => {
  statusDetail = "";
  zeroLag.reset();
  resizeDedupe.reset();
  schedulePostLayoutRefit();
  requestAnimationFrame(() => term?.focus());
},
```

Nearby helpers to reuse:
- `snapScrollbackToBottom`
- `pinnedToBottom` / `hasUnseenOutput`
- `zeroLag.reset()` / `resizeDedupe.reset()` (already called)
- `pendingOutput` array (clear on reconnect if anything is queued)

Existing reconnect tests to extend in `TerminalRawView.test.ts`:
- `"refits and focuses ghostty on reconnect"`
- `"enters reconnecting and opens a new socket after the socket closes"`

Mock `Terminal` in the test file currently lacks `reset`; add
`const reset = vi.fn()` and `reset = reset` on the mock class (same pattern as
`focus` / `blur`).

## Test-first instructions

1. Add `reset` mock to the Ghostty `Terminal` mock in
   `TerminalRawView.test.ts` (hoisted `vi.fn`, assigned on the class, cleared
   in `beforeEach` alongside `focus`/`fit`).
2. Add a test named:
   `"resets the terminal buffer and snaps to bottom on reconnect"`.
3. Flow:
   - `mountOpenTerminal()` (or equivalent) so the first socket is open and
     Ghostty is mounted.
   - Emit some output on the first connection (`socket.emit` / binary helper
     already used by other tests) so `write` has been called at least once.
   - Close the socket to enter reconnecting; advance timers; open the second
     socket.
   - Assert `reset` was called after the second `open`.
   - Assert `scrollToBottom` was called as part of the reconnect open path
     (pin + snap).
   - Assert `reset` was **not** required/called solely from the first open
     (optional: clear `reset` after first open, then only assert on second).
4. Run and confirm FAIL before production edit:
   ```bash
   rtk npm run web:test -- --run src/components/TerminalRawView.test.ts -t "resets the terminal buffer"
   ```
   Expected failure: `reset` is not a function / not called.

## Production edit instructions

Change the `connectTaskTerminal` `onOpen` handler to accept `isReconnect`:

```ts
onOpen: (isReconnect) => {
  statusDetail = "";
  zeroLag.reset();
  resizeDedupe.reset();
  if (isReconnect) {
    pendingOutput.length = 0;
    term?.reset();
    pinnedToBottom = true;
    hasUnseenOutput = false;
    snapScrollbackToBottom();
  }
  schedulePostLayoutRefit();
  requestAnimationFrame(() => term?.focus());
},
```

Keep first-open behavior unchanged aside from the new parameter. Do not reset
when `isReconnect` is false.

## Verification commands

```bash
rtk npm run web:test -- --run src/components/TerminalRawView.test.ts
rtk npm run web:check
```

## Acceptance criteria

- New test fails before the production edit for the expected reason.
- After the edit, full `TerminalRawView.test.ts` passes.
- `web:check` passes.
- Diff only touches Allowed files.
- First open does not call `reset`; reconnect does, and re-pins/snaps bottom.

## Stop conditions

- Stop if Ghostty mock/`term.reset` is unavailable and a different clear API
  is required outside Allowed files.
- Stop if the new test passes before the production edit.
- Stop if unrelated TerminalRawView tests fail for reasons outside this change.
- Stop if the patch grows beyond reconnect `onOpen` handling + test mock/assert.
