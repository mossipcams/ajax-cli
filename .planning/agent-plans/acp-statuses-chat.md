---
context: default
slug: acp-statuses-chat
status: in-progress
approval: user-directed (2026-08-18) — implement ACP statuses in web/chat; do not use ACP v2
last_updated: 2026-08-18
correction: statuses are the agent's run state (Running/Waiting/Idle), not activity labels
---

# ACP v1 statuses in Ajax web/chat

## Constraint (user)

**Do not use ACP v2.** Stay on the existing v1 host: no `protocolVersion: 2`, no
`agent-client-protocol` v2 schema types, no `_status*` / `_status_badge`
special-case, no v2 `StateUpdate` enum. Unknown v1 session updates still arrive
as raw JSON (`UnknownSessionUpdate`) and may be mapped there.

## Direction

Wire ACP v1 status-like `session/update` values into Ajax web chat, and tighten
how tool-call marks/status read. Browser stays a projection. Do not redo #695
(do not replace core `LiveStatusKind` / operator status).

ACP `status.state` drives the live-head primary label (Working / Needs you /
Ready — same vocabulary as cockpit cards, plus Error tone from task detail).
Human labels (`Indexing workspace`) are stored but not shown in the head.
Tool marks stay mono glyphs (no icon library).

## Tasks

- [x] T1 Host: map v1 `sessionUpdate: "status"` (and keep existing raw
      `state_update`) to `SessionServerEvent::Status`; ignore typed
      `CurrentModeUpdate` like capability announcements
- [x] T2 Browser: store `status.state` on `SessionState`; pass into `headState`
- [x] T3 LiveHead: agent-status label precedence; remove quiet-line status UI;
      keep thought / Thinking… fallback
- [x] T4 ToolCard look: 20px mark column, drop kind word, status chip, less box
- [x] T5 Docs + session-chat-regression e2e

## Non-goals

- No ACP v2 protocol or schema
- No lifecycle / registry / supervisor parser changes
- No permission buttons in the transcript
- No `fs/*` / `terminal/*` capabilities
- No drawn icon set

## Validation

- `cargo nextest run -p ajax-web -- acp_map tests`
- `npm run web:test -- --run src/features/session src/shared/lib/webSessionTransport.test.ts`
- `npm run web:check`
- Focused Playwright: `session-chat-regression`

## Validation results (2026-08-18)

| Command | Result |
| --- | --- |
| `cargo nextest run -p ajax-web -- status_update state_update typed_mapper capability_announcements` | pass (4/4) |
| `npm run web:test -- --run src/features/session` | pending delegate |
| `npm run web:check` | pending delegate |
| Playwright `session-chat-regression` | not run — vite webServer timed out in delegate env |

## Remaining

- Playwright e2e not executed locally; run before merge.

## headState precedence (correction)

1. permission decision → Needs you
2. ACP `waiting` / `requires_action` → Needs you
3. ACP `running` OR session busy → Working
4. task detail `waiting` / `error` → Needs you / Error tone
5. else Ready
