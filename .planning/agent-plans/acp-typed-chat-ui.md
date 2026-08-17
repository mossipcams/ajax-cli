---
context: default
slug: acp-typed-chat-ui
status: in-progress
approval: user-directed (2026-08-16) — user specified the target model and UI directly
last_updated: 2026-08-16
---

# ACP-typed chat UI (Zed-style agent panel)

## Direction (from the user)

Use ACP v1's event model as the contract; model the UI after Zed's ACP agent
panel. Reduce every `session/update` into a typed conversation state, then
render presentation components from that state. Tool calls, thinking, plan,
permissions, and usage each get their own treatment instead of being flattened
into prose or dropped.

## Findings that change the plan

1. **A typed reducer already exists.** `web/src/features/session/sessionThread.ts`
   folds typed `WebSessionServerEvent`s into `SessionState`; nothing renders raw
   JSON. The real gap is one layer up: the Rust wire event
   (`SessionServerEvent::ToolCall`) drops ACP's `content` array entirely, so
   diffs and tool output never reach the browser. No UI change can show a diff
   until the host forwards it.
2. **`messageId` is optional in ACP v1** (`ContentChunk.message_id:
   Option<MessageId>`). Grouping by it is correct when present; role-adjacency
   stays as the fallback, which is what `appendProse` already does.
3. **`ToolCallContent::Terminal` cannot arrive.** Ajax advertises neither `fs/*`
   nor `terminal/*` client capabilities (`docs/architecture/web-session-behavior.md`),
   so no agent can create a terminal to embed. Execute-tool output arrives as
   `Content` text. A dedicated terminal block is dead code here; execute output
   renders as a mono block inside the tool card.

## Deliberate deviation from the spec

- **Permissions stay in the live head, not inline.** The head is sticky and
  cannot scroll away; on a phone that is strictly more "impossible to miss"
  than an inline card in a scrolling thread. The thread gets a static marker
  row so history reads correctly, but the buttons live in the head.
- This **reverses the DIRECTION CONTRACT** at the top of `SessionChat.tsx`
  ("an instrument with a live head, not a message list" / "settled turns fall
  into a transcript as conversation plus one work summary, not a tool trace").
  That contract and `DESIGN.md` are updated in the same change.

## Scope

- Rust: extend the browser wire event with tool content, `messageId`, and a
  first-class `usage` event; split ACP mapping out of `web_session/mod.rs`.
- TS: reducer emits an ordered `ConversationItem[]` including thought, tool,
  plan, permission, and usage items.
- TS: presentation components — assistant Markdown, collapsible thinking,
  tool cards with kind icons and status-driven default collapse, diff view,
  plan checklist, usage indicator.
- Throttle streaming Markdown re-render to ~50ms.
- Docs: `SessionChat.tsx` direction contract, `DESIGN.md`,
  `docs/architecture/web-session-behavior.md`.

## Non-goals

- No change to lifecycle, registry truth, prompt queue, or host transcript
  ownership. Browser stays a projection.
- No `fs/*` or `terminal/*` client capability.
- No change to the permission JSON-RPC correlation or resolution semantics.

## Tasks

- [ ] T1 Rust: `acp_map.rs` split + tool `content`, `messageId`, `usage` event
- [ ] T2 TS: transport event types match the new wire
- [ ] T3 TS: reducer → `ConversationItem[]`
- [ ] T4 TS: presentation components + CSS
- [ ] T5 Markdown streaming throttle
- [ ] T6 Tests: Rust mapping, reducer, component, e2e/visual
- [ ] T7 Docs: direction contract, DESIGN.md, web-session-behavior.md

## Validation

Nothing verified yet.

| Command | Result |
| --- | --- |
| `cargo clippy --all-targets --all-features -- -D warnings` | not run |
| `cargo nextest run -p ajax-web` | not run |
| `npm --prefix crates/ajax-web/web run test` | not run |
| `npm run web:smoke` (mobile-webkit) | not run |
