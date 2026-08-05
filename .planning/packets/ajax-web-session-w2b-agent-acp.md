PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Replace the Ajax Web Session Pi RPC backend with Cursor **`agent acp`** (ACP JSON-RPC over stdio). Keep the existing browser WebSocket wire types and UI.

## Scope

### Allowed
- crates/ajax-web/src/adapters/web_session_rpc/mod.rs
- crates/ajax-web/src/adapters/web_session_rpc/bridge.rs
- crates/ajax-web/src/adapters/web_session_rpc/tests.rs
- crates/ajax-web/src/adapters/mod.rs (only if rename needed; prefer keep module path)
- crates/ajax-web/src/architecture.rs (only if adapter rename)
- .planning/agent-plans/ajax-web-session-poc.md
- .planning/packets/ajax-web-session-w2b-agent-acp.md

### Forbidden
- Frontend chat/symbol UI changes (except if a comment mentions Pi — fix that)
- Reintroducing `pi --mode rpc`
- Using `agent -p` / stream-json instead of ACP
- Terminal/PTY integration
- Commits / branch changes
- Changing `/web-session` URL or `session.*` WS wire types

## Acceptance

1. Adapter spawns `agent acp` (resolve `agent` or `cursor-agent` on PATH) with cwd = task worktree.
2. On WS connect, run ACP: `initialize` → `authenticate` `{ methodId: "cursor_login" }` → `session/new` `{ cwd, mcpServers: [] }`, then emit `session.ready` + status waiting.
3. On `session.prompt` from browser: send ACP `session/prompt` with `prompt: [{ type: "text", text: message }]`; set status running.
4. On ACP notification `session/update` with `update.sessionUpdate == "agent_message_chunk"` and text content, emit `session.assistant_delta`.
5. When `session/prompt` RPC returns (stopReason), emit `session.settled` + status waiting.
6. On browser `session.abort`: send ACP `session/cancel` (and/or kill child if needed).
7. On `session/request_permission`: auto-respond allow-once (POC). For blocking Cursor extension methods (`cursor/ask_question`, `cursor/create_plan`), respond cancelled/skipped so the turn cannot hang indefinitely.
8. No references to Pi RPC remain in this adapter. Tests use a fake ACP JSONL peer script — no live LLM.
9. Existing route/WS tests and `cargo nextest run -p ajax-web web_session` still pass; architecture tests pass.
10. Plan Wave 2b checklist marked done.

## Constraints

- Prefer long-lived `agent acp` process per WebSocket (one session/new per connection).
- JSON-RPC 2.0 newline-delimited on stdio (see Cursor ACP docs minimal client).
- Keep files under ~600 LOC; rename Pi* types to Acp* / AgentAcp*.
- Module path may stay `web_session_rpc` for churn control (ponytail: name is historical).

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web web_session
      expected: pass
    - type: test
      command: cargo nextest run -p ajax-web architecture
      expected: pass
    - type: static_analysis
      command: rg -n 'pi --mode rpc|spawn_default_pi|DEFAULT_PI_ARGS|PiRpc' crates/ajax-web/src/adapters/web_session_rpc
      expected: no matches
    - type: build
      command: cargo check -p ajax-web
      expected: pass
  reason: Adapter swap is backend-only; fake ACP peer covers protocol without live Cursor auth/LLM.
```

## Stop if

- ACP handshake cannot be tested without live Cursor login and no fake peer is workable
- Edits outside Allowed
- Exceed ~400 changed lines
- Would change frontend wire protocol

## Code anchors

- Current Pi bridge: `crates/ajax-web/src/adapters/web_session_rpc/{mod,bridge,tests}.rs`
- WS wire types: `crates/ajax-web/src/slices/web_session.rs`
- Cursor ACP docs flow: initialize → authenticate(cursor_login) → session/new → session/prompt; session/update agent_message_chunk; session/cancel; session/request_permission
- Minimal client reference: https://cursor.com/docs/cli/acp.md

## Edit instructions

1. Replace Pi process/protocol with ACP client over `agent acp`.
2. Keep bridge_task_web_session_socket public entry and WS event mapping.
3. Rewrite tests around fake ACP peer.
4. Check off Wave 2b in the plan.
