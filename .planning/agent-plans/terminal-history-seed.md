# Task 4 — Width-correct history seed + no reseed on auto-reconnect

Follow-up to `.planning/agent-plans/ghostty-smoothness-native-scroll.md`
(PR #504). Matt reports scroll history is still broken/garbled. Two causes,
both in the bridge's history seeding (untouched by PR #504):

1. **Width mismatch:** `capture-pane -p -e -J -S -2000` runs immediately
   after attach (PTY still 80×24, window history wrapped at the task
   window's width — e.g. a 200-col desktop client) and `-J` joins wrapped
   lines to full logical width. ghostty re-wraps at the client's 80 cols →
   different break points → mush, worst for TUI frames.
2. **Reseed on every reconnect:** mobile Safari drops the socket on every
   backgrounding; each reconnect does `term.reset()` + a fresh mis-wrapped
   2000-line seed. Local scrollback continuity is destroyed several times a
   session.

## Design

- **Server (`terminal_pty.rs`, `runtime.rs`)**
  - Parse `seed=0` from the WS URL query in `axum_task_terminal` (pure
    helper + unit tests); default seed=on (old clients unaffected). Pass
    `seed_history: bool` into `bridge_task_terminal_socket`.
  - Defer the seed until the client's first `resize` frame (bounded
    pre-loop over socket frames, 500 ms timeout fallback), apply the PTY
    resize, wait 100 ms for tmux to reflow history to the new width
    (ponytail: fixed beat; event-driven readiness if it ever flakes), THEN
    capture. tmux ≥2.6 reflows soft-wrapped history on width change, so
    rows captured at the client's width render identically in ghostty.
  - Drop `-J`: capture display rows at the (now client-width) window
    instead of joined logical lines.
  - Reader task keeps starting after the seed (same output-ordering
    guarantee as today).
- **Client (`api.ts`, `terminalConnection.ts`, `TerminalRawView.svelte`)**
  - `openTaskTerminalSocket(handle, { seedHistory })` → appends `seed=0`.
  - Automatic reconnects (backoff/foreground) connect with `seed=0`; the
    first connect and the manual Reconnect button seed fully.
  - `onOpen` reports `seeded`; TerminalRawView only does `term.reset()` +
    snap when seeded. Unseeded auto-reconnect keeps the local buffer; the
    tmux attach repaint restores the live screen in place (screen-region
    overwrite, no scrollback push). PTY resize is still re-sent
    (`resizeDedupe.reset()` stays).
  - Accepted trade-off: output missed while disconnected is absent from
    local scrollback on auto-reconnect (the screen repaint shows current
    state); manual Reconnect recovers deep history via a full reseed.

Not an architecture change: attach planning, trust boundaries, and frame
protocol ownership are unchanged (`architecture.md` §ajax-web reviewed).

Delegation decision: delegated via model-router (packet 4A server, packet
4B client).

## Checklist

- [x] 4A server (cursor-delegate composer-2.5, first round, red PROVEN by
      delegate; opencode GLM lane skipped as unavailable — 4 logged hangs /
      0 successes). 136/136 nextest, clippy -D warnings, fmt clean.
- [x] 4B client (cursor-delegate composer-2.5, first round; TEST_FIRST
      red proven by PARENT post-hoc: production stash → 5/7 seed tests
      fail → restore → 164/164). Found+fixed en route: foreground
      reconnects computed isReconnect=false (attempts zeroed before dial),
      so the view never reset while the server reseeded — the primary
      duplicate-history mechanism on mobile. isReconnect now = everOpened.
- [x] Rebuild dist; ajax-web 136/136 + ajax-cli 335/335; vitest 561/561;
      web:check clean; mobile-webkit e2e 46/46.
- [ ] Manual device check by Matt (dev server must run THIS branch's
      binary — dist is baked in via include_bytes).

## Deviations / validation

(recorded during execution)
