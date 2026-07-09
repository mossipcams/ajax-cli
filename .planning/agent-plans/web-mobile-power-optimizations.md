# Web mobile power optimizations

## Scope

Battery-friendly Web Cockpit optimizations that preserve live terminal UX.
Follow the agreed PR sequence; one behavior change per PR.

## Non-goals

- Terminal library migration
- Hard output caps that make streaming feel delayed
- Disabling scrollback entirely
- Slowing restart polling below 500ms during restart
- User-facing “battery mode” UI
- Binary WS frames before batching PRs land

## PR sequence

1. Adaptive polling by route / visibility / terminal-open
2. Client-side terminal write batching
3. Server-side PTY output batching
4. Resize dedupe / debounce
5. Change-only cockpit state + overlapping-poll guard
6. Binary terminal output frames
7. Moderate mobile scrollback + browser-session tmux redraw noise (later)

## Active work: PR1 adaptive polling

**Delegation decision: delegated via model-router** → Cursor / Grok 4.5 High
(complex Svelte/TS frontend polling behavior).

Packet: `.planning/packets/web-adaptive-polling-pr1.md`

### Target constants

```ts
export const REFRESH_INTERVAL_ACTIVE_MS = 1000;
export const REFRESH_INTERVAL_TERMINAL_MS = 5000;
export const REFRESH_INTERVAL_IDLE_MS = 10000;
export const REFRESH_INTERVAL_HIDDEN_MS = 60000;
export const RESTART_POLL_MS = 500; // unchanged; restart path only

export const VERSION_POLL_MS = 30000;
export const VERSION_POLL_TERMINAL_MS = 120000;
export const VERSION_POLL_HIDDEN_MS = 300000;
```

### Interval selection

| Context | Cockpit | Version |
| --- | --- | --- |
| `visibilityState !== "visible"` | 60s | 300s |
| task route (terminal open) | 5s | 120s |
| settings route | 10s (idle) | 30s |
| dashboard / project | 1s | 30s |

### Checklist

- [x] Failing unit tests for interval selectors in `polling.test.ts`
- [x] Implement pure selectors + constants in `polling.ts`
- [x] Wire adaptive timers in `App.svelte` (reschedule on route/visibility)
- [x] Only immediate-resume poll when becoming visible (not on hide)
- [x] Keep `RESTART_POLL_MS = 500` for restart wait path
- [x] Focused vitest pass
- [x] Parent review of diff — **Accepted**

## Deviations

- `App.test.ts` “clears detail failure…” previously relied on the 1s cockpit poll
  (via `applyCockpit`) to clear “disconnected” within the default wait, and on
  `detailCalls <= 2` failing before success. With task-route 5s polling that
  mask is gone; the test now fails the first detail load only, `await tick()`s
  between `#/` and reopen, and asserts recovery from a successful detail load.

## Validation (PR1)

```bash
npm run web:test -- crates/ajax-web/web/src/polling.test.ts crates/ajax-web/web/src/components/App.test.ts --run
# EXIT 0 — 27 passed
```

## Active work: PR2 client terminal write batching

**Delegation decision: delegated via model-router** → Cursor / Grok 4.5 High
(terminal rendering / Svelte output path).

Packet: `.planning/packets/web-terminal-write-batching-pr2.md`

Target:

```ts
const TERMINAL_WRITE_FLUSH_MS = 16;
const TERMINAL_WRITE_MAX_CHARS = 32_000;
```

- Batch decoded output text before `term.write`
- Run scroll-follow / scrollback compensation once per flush
- Keep input immediate (zero-lag path untouched)
- Pre-mount `pendingOutput` queue may merge into the same flusher

### Checklist

- [x] Failing unit tests for write batcher
- [x] Implement batcher helper
- [x] Wire TerminalRawView output path
- [x] Focused vitest pass
- [x] Parent review of diff — **Accepted** (140 tests)

## Validation (PR2)

```bash
npm run web:test -- crates/ajax-web/web/src/terminalOutputPolicy.test.ts crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
# EXIT 0 — 140 passed
```

## Active work: PR3 server PTY output batching

**Delegation decision: delegated via model-router** → OpenCode / GLM 5.2
(tricky Rust PTY/WebSocket select-loop batching). OpenCode hung (empty log;
wrong then corrected model id still stalled) → **re-routed to Cursor / Grok 4.5 High**.

Packet: `.planning/packets/web-server-pty-output-batching-pr3.md`

Target:

```rust
const TERMINAL_OUTPUT_FLUSH_MS: u64 = 16;
const TERMINAL_OUTPUT_MAX_BYTES: usize = 16 * 1024;
```

### Checklist

- [x] Failing Rust tests for output batch buffer
- [x] Implement batch buffer + flush helper
- [x] Wire terminal_pty select loop (timer + max bytes; input immediate)
- [x] Focused cargo test pass
- [x] Parent review of diff — **Accepted**

## Validation (PR3)

```bash
cargo test -p ajax-web terminal_output -- --nocapture
cargo test -p ajax-web filter_scrollback_hostile handle_input_frame -- --nocapture
# EXIT 0
```

## Active work: PR4–PR7 (remaining)

**Delegation decision: delegated via model-router** (Cursor lanes; OpenCode hung on PR3).

| PR | Packet | Lane |
| --- | --- | --- |
| 4 Resize dedupe/debounce | `web-resize-dedupe-pr4.md` | Cursor / Composer 2.5 |
| 5 Change-only cockpit + in-flight guard | `web-cockpit-poll-guard-pr5.md` | Cursor / Composer 2.5 |
| 6 Binary terminal output frames | `web-binary-terminal-output-pr6.md` | Cursor / Grok 4.5 High |
| 7 Mobile scrollback + browser tmux redraw | `web-scrollback-tmux-noise-pr7.md` | Cursor / Grok 4.5 High |

### PR4 checklist

- [x] Failing tests for resize dedupe + 100ms debounce
- [x] Implement + wire
- [x] Verify + parent accept — **Accepted** (153 vitest)

### PR5 checklist

- [x] Failing tests for hash/apply-if-changed + in-flight guard
- [x] Implement + wire App
- [x] Verify + parent accept — **Accepted** (38 vitest)

### PR6 checklist

- [x] Failing tests server+client binary output
- [x] Implement both sides; JSON for control/error
- [x] Verify + parent accept — **Accepted** (cargo + 9 vitest)

### PR7 checklist

- [x] Failing tests scrollback constants + ephemeral tmux options
- [x] Implement Terminal scrollback + ephemeral set-option
- [x] Verify + parent accept — **Accepted** (166 vitest + cargo)

## Leftover / follow-up

- `ajax-web` still lists unused `base64` in `Cargo.toml` after binary frames (safe cleanup later).
- Full optimization sequence (PR1–PR7) is **uncommitted**.

