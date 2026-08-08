# Fix orchestration chat Act-on findings

## Scope

Ship the four Act-on items from the chat UX critique, sequenced into verifiable units on this branch.

1. Unblock ACP event pump during `session/prompt`
2. One backend-facing brief (title + constraints + outcome)
3. Minimal conversation model (coalesce chunks; render artifacts as cards, not system text)
4. Hub retain across disconnect (refcount; do not destroy ACP on first socket close)

## Non-goals

- Full durable transcript persistence / replay store
- Markdown rendering, agent CSS polish as the headline
- Multi-agent ACP (Codex/Claude/Pi)
- Moving task truth into the browser
- Fake Retry/Try-another auto-send (follow-up)

## Delegation decision

`Delegation decision: USE_NATIVE` (subagent harness; model-router not used).

## Checklist

- [x] Architect arena (2+ designs) → pick recorded
- [x] Unit 1: fire-and-forget prompt/cancel + error-via-events; tests
- [x] Unit 4: hub refcount (with unit 1)
- [x] Unit 2: one brief including title
- [x] Unit 3: coalesce + artifact cards; Cancel button
- [x] Focused verify: `cargo test -p ajax-web`, web session tests
- [x] Open/update PR 779

## Deviations

- Did not add `clientMessageId` / `PromptAccepted` wire events (deferred per synthesis).
- Hub refcount helps concurrent holders; single-tab iOS reconnect still drops at holders==0 (grace deferred).
- `npm ci` was required once in this worktree before vitest was available.

## Validation

- Parent: `cargo test -p ajax-web --lib` — 258 passed
- Parent: `npm run web:test -- --run src/features/session/` — 12 passed
