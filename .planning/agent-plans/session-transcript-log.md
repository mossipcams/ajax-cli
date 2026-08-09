# Session transcript log + per-socket cursors

Status: in progress
Mode: Behavior Change (user-visible)
Delegation decision: **not delegated** — continuation of the impeccable-driven
session work the user has been reviewing turn by turn in this session.

## Problem

`SessionSlot` holds only the ACP client and a holder count. Events are drained
straight into whichever socket polls, through `AcpStdioClient::poll_event` →
`Receiver::try_recv()`, which is **single-consumer**. Three consequences:

1. **Reload loses the transcript.** Nothing retains events, so a refreshed tab
   shows an empty thread while the agent is still mid-turn.
2. **Two devices split the conversation.** Each event goes to whichever socket
   calls `try_recv` first, so each client receives a random half.
3. **Reload kills the agent.** `release` removes the slot the moment holders hit
   zero, dropping the `AcpStdioClient` and with it the `agent acp` child — so a
   refresh terminates work in progress.

All three share one root: no event log, and no per-consumer cursor.

## Approach

- `SessionSlot` gains an append-only `log: Vec<SessionServerEvent>` plus
  `dropped: usize` (count trimmed from the front) so cursors stay absolute.
- One drain path, `WebSessionHub::pump`, moves events out of the ACP receiver
  and appends them to the log under the sessions lock, so ordering is total.
- Each socket keeps its own cursor and reads `log[cursor - dropped..]`. A fresh
  socket starts at 0 and therefore replays the whole session for free.
- `release` no longer tears the slot down; it records `last_released` so the
  ACP process survives a reload. Idle slots are evicted LRU on the next
  `acquire` once they exceed `MAX_IDLE_SESSIONS`, which bounds process count.
- `MAX_LOG_EVENTS` bounds memory per task.

Deliberately **not** in scope: durability across a web-server restart. That
needs a store, and `registry_events` is the natural home, but putting chat in
the registry is a task-truth boundary decision that needs Matt's call.

Lock order is sessions → client throughout; no path takes client → sessions, so
the added nesting cannot deadlock.

## Tasks

- [x] T1 `SessionSlot` log, `dropped`, `append`, `read_from`; bounds.
- [x] T2 `pump` / `read_from` on the hub; LRU idle eviction on acquire.
- [x] T3 `release` keeps the slot alive.
- [x] T4 Bridge holds a cursor; replays from 0 on connect.
- [x] T5 Unit tests: replay, fan-out, trimming, eviction, holder lifecycle.
- [x] T6 Full gate.

## Validation

| Command | Result |
| --- | --- |
| `cargo nextest run -p ajax-web` | pass — 275 |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo fmt --check` | pass |
| `npm run web:check` / `web:lint` | pass |
| `npm run web:test -- --run` | pass — 763 |
| session e2e (both projects) | pass — 18/18 |

## Deviations

- **The log needed the operator's own turns.** ACP never echoes a user prompt,
  so a replayed transcript would have carried the agent's half and none of
  yours. The bridge now records the prompt into the shared log, which makes the
  host the single source of truth for the transcript.
- Consequently the browser's optimistic append had to go, or the sending socket
  would render its own message twice. `{type:"prompt"}` now only marks the turn
  in flight; the entry arrives from the host. Four tests asserted the old
  optimistic behaviour and were updated to the real contract — coverage
  extended, none weakened.
- `TranscriptLog` was extracted from `SessionSlot` so the log semantics are
  testable without spawning an ACP process. The first attempt used
  `mem::zeroed()` for a fake client, which is undefined behaviour for a type
  owning a `Child` and channels; the compiler's `invalid_value` lint caught it.

## Follow-ups

- No reaper: idle sessions are evicted only when a new `acquire` arrives. A
  server with one long-lived task keeps its ACP child indefinitely.
- Durability across server restart (see scope note above).
