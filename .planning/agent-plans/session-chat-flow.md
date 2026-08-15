# Session chat flow (stability / usability)

**Date:** 2026-08-13
**Mode:** Behavior Change (presentation + session WS flush)
**Issue:** https://github.com/mossipcams/ajax-cli/issues/875

## Scope

Make Web Cockpit orchestration chat (`#/session/<handle>`) feel like a conversation:

1. Agent prose appears as soon as ACP chunks arrive — no typewriter.
2. Operator turns appear in the transcript on Enter, not after host echo.
3. Newline-only ACP deltas are kept so paragraphs do not smash together.
4. Inbound prompt/cancel/permission flush outbound events immediately (do not wait up to 50ms).

## Non-goals

- Replacing raw xterm/tmux as the default task path
- Redesigning LiveHead / Impeccable direction contract
- Changing ACP host transcript ownership or JSONL persistence
- Follow-up queue / second-Enter-cancels semantics (leave as-is unless a test forces a touch)

## Root causes

1. `useSmoothText` drains already-arrived text at ≤5ms/char over a 250ms window. Live `Markdown` re-parses every character, so blocks pop in line-sized jumps.
2. `sessionReducer` `prompt` only sets `busy`; the user bubble waits for `append_to_log` + the bridge's 50ms poll.
3. `message_event` drops `text.trim().is_empty()`, so a `"\n"` chunk never lands.
4. `bridge_task_session_socket` only `pump`/`read_from` on the timer branch, not after inbound `Prompt`.

## Desired behavior

| Situation | Outcome |
| --- | --- |
| Agent chunk arrives | Full chunk visible on next paint; no RAF typewriter |
| Operator hits Enter | User bubble in thread immediately; host echo of the same text does not duplicate |
| ACP sends `"\n"` | Appended to open agent prose |
| Prompt/cancel/permission inbound | Pending log events flushed on that turn, not the next 50ms tick |
| `prefers-reduced-motion` | Irrelevant once typewriter is gone |

## Task checklist

- [x] T1 — Remove `useSmoothText`; Markdown renders `source` immediately; drop `smooth` prop
- [x] T2 — `prompt` appends user prose; duplicate host echo still skipped; SessionChat test no longer needs a synthetic echo to see the bubble
- [x] T3 — Keep newline/whitespace deltas in `message_event` (`is_empty()`, not `trim()`)
- [x] T4 — Flush outbound after successful inbound in the session WS bridge
- [x] T5 — Focused tests + `npm run web:test` / `web:check` / `cargo test -p ajax-web` for the touched Rust tests

## Validation

```bash
npm run web:test -- --run src/features/session/Markdown.test.tsx src/features/session/Transcript.test.tsx src/features/session/sessionThread.test.ts src/features/session/SessionChat.test.tsx
npm run web:check
cargo test -p ajax-web --lib web_session
cargo test -p ajax-web --lib adapters::web_session_acp::bridge
```

### Results (2026-08-13)

| Command | Exit |
| --- | --- |
| `npm run web:test -- --run …` (4 files, 40 tests) | 0 |
| `npm run web:check` | 0 |
| `cargo test -p ajax-web --lib web_session` | 0 (76 passed) |
| `cargo test -p ajax-web --lib adapters::web_session_acp::bridge` | 0 (3 passed) |

## Deviations

- Updated two `sessionThread.test.ts` cases (`does not invent a summary…`, `ends the turn on error…`) to account for optimistic user prose on `prompt`.
