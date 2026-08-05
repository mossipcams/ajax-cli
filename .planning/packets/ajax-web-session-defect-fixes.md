PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Fix Ajax Web Session defect-review root causes so Stop/cancel, ACP request handling, async bridge I/O, Cursor-only admission, symbol search, and Send double-submit behave correctly.

## Scope

### Allowed

- `crates/ajax-web/src/adapters/web_session_rpc/mod.rs`
- `crates/ajax-web/src/adapters/web_session_rpc/bridge.rs`
- `crates/ajax-web/src/adapters/web_session_rpc/tests.rs`
- `crates/ajax-web/src/slices/web_session.rs`
- `crates/ajax-web/src/runtime/task_routes/cockpit.rs`
- `crates/ajax-web/src/runtime/task_routes/live.rs`
- `crates/ajax-web/src/runtime/tests/suite_4.rs`
- `crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx`
- `crates/ajax-web/web/src/features/session/AjaxWebSessionView.test.tsx`
- `crates/ajax-web/web/src/features/session/webSessionTransport.ts`
- `crates/ajax-web/web/src/features/session/webSessionTransport.test.ts`
- `docs/architecture/web-cockpit.md` (one sentence only if Cursor-only backend gate needs documenting)

### Forbidden

- Commits, push, branch changes
- Changing permission auto-allow policy beyond request-reply correctness
- Rewriting the ACP bridge to a new protocol
- Editing unrelated crates or terminal PTY beyond reading patterns
- Broad CSS/UI redesign

## Acceptance

1. `session/cancel` is written as a JSON-RPC **notification** (method + params, **no** `id`) and `send_cancel` does **not** wait for a response / does not call `rpc()`.
2. When the ACP reader sees a request (`method` + `id`) with no `auto_response_for_method` match, it writes a JSON-RPC **error** response for that `id` (do not silently `continue`).
3. ACP handshake (blocking `rpc` chain) runs via `tokio::task::spawn_blocking` (or equivalent) so the async WS task does not park on `recv_timeout` / stdio. Mirror `terminal_pty` pattern.
4. Stdout reader must not permanently block on a full event queue: use non-blocking send for events (e.g. `try_send`, drop-on-full for deltas) so RPC response delivery cannot deadlock behind a full `sync_channel`.
5. `prepare_web_session` returns a distinct error when `task.selected_agent != AgentClient::Cursor`. Route handlers map it to an HTTP error (403 or 422 with clear JSON). Existing suite_4 / unit fixtures that use default Codex `task_in` must set `selected_agent = AgentClient::Cursor` where web-session/symbols success is expected; add one test that non-Cursor is rejected.
6. `search_with_rg` passes `-F` (fixed string) before the needle.
7. `AjaxWebSessionView.sendPrompt` sets run status to `running` immediately when sending (before server echo) so double-send is blocked by `canSend`.
8. Remove dead `WorktreeSymbol` / `composePromptWithContext` / `symbolChipLabel` from `webSessionTransport.ts` (keep `composeWebSessionPrompt`).

## Constraints

- Smallest safe diff; no drive-by cleanup.
- Keep wire protocol / frontend WS message types unchanged.
- Tests: extend `web_session_rpc/tests.rs` fake peer so cancel is a notification (no reply to cancel); assert notification encoding; assert unknown request gets an error reply if practical.
- Prefer `AgentClient` from `ajax_core` already in scope for the slice.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web web_session
      expected: pass
    - type: test
      command: cargo nextest run -p ajax-web suite_4
      expected: pass (or the specific suite_4 web_session/symbols tests)
    - type: test
      command: npm run web:test -- --run src/features/session src/shared/lib/ajaxWebSessionSetting
      expected: pass
  broader_checks: []
  reason: Focused unit/integration coverage for ACP cancel/request handling, Cursor gate, rg -F, and UI send race.
```

## Stop if

- Change would exceed ~400 lines or require redesigning the ACP process model
- `AgentClient` / task model fields differ from expectation and need architecture decision
- Handshake cannot be moved to `spawn_blocking` without breaking the WS bridge ownership model — escalate with concrete blocker
