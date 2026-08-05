# Ajax Web Session defect-review fixes

## Scope

Fix root causes from the local defect review of the Ajax Web Session POC, then open a PR with the full POC + fixes.

## Non-goals

- Redesign ACP bridge architecture
- Replace auto-allow permission policy (intentional POC)
- New indexer / AST backend
- Pi Ajax Web Session

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Packet READY + delegate ACP/session/symbol/UI defect fixes
- [x] Review gate ACCEPT (delegate report schema wrap failed; parent verified diff + tests)
- [x] Parent validation (`cargo nextest -p ajax-web web_session` 15 pass; `suite_4` 30 pass; session vitest 23 pass)
- [ ] Commit (Conventional Commits) + local verify gate
- [ ] Push + `gh pr create`

## Fixes (root cause)

1. **Cancel**: send ACP `session/cancel` as notification (no id / no wait); do not use `rpc()`.
2. **Unhandled requests**: reply with JSON-RPC error when agent method+id has no auto-handler.
3. **Async blocking**: run ACP handshake (and any remaining blocking rpc) off the Tokio worker via `spawn_blocking` (mirror terminal_pty).
4. **Event queue stall**: never block the stdout reader on a full event channel (`try_send` / drop-oldest for deltas).
5. **Cursor-only**: `prepare_web_session` rejects non-`AgentClient::Cursor`; routes map new error.
6. **rg**: pass `-F` so symbol query is fixed-string.
7. **UI**: optimistic `running` on send; delete dead `WorktreeSymbol` helpers.

## Validation

```bash
cargo nextest run -p ajax-web web_session   # 15 passed
cargo nextest run -p ajax-web suite_4       # 30 passed
npm run web:test -- --run src/features/session src/shared/lib/ajaxWebSessionSetting  # 23 passed
# before PR:
npm run verify
```

## Deviations

- Delegate wrapped `DELEGATE_REPORT` in a markdown fence → schema FAILED; parent accepted after inspecting delta + re-running verification.

## Results

Review Gate: ACCEPT. Defect root causes landed in allowed scope.
