# Ajax Web Session POC

## Scope

Proof-of-concept mobile-first, AST-aware chat for Ajax Web.

- Settings toggle labeled **Ajax Web Session** (localStorage feature flag).
- When enabled, **Cursor** tasks open Task Detail to **Ajax Web Session** instead of the terminal.
- Chat is driven by host-side **Agent P / ACP** = Cursor `agent acp`
  (JSON-RPC over stdio), not Pi and not `agent -p` stream-json.
- **Not** enabled for Pi-selected tasks (or Claude/Codex) in this POC — they keep the terminal.
- Cursor registry identity stays Cursor; `agent acp` is the structured chat transport.

## Non-goals

- Terminal integration / fallback in the session view
- Ajax Web Session for Pi (or other) agent picker values
- Git / PR / task-management UI
- Full indexer, dependency graphs, code editor, multi-agent orchestration
- Changing default terminal path when the flag is off

## Architecture fit

- Presentation: `crates/ajax-web/web/src/features/session/`
- Flag: `ajax.webSession` localStorage (mirror old surfaceV2 pattern)
- Backend: new `ajax-web` slice `web_session` — authenticated WS + lightweight symbol search over task `worktree_path`
- Browser does not own task truth / lifecycle
- Brief note in `docs/architecture/web-cockpit.md` when the path lands

## Naming

| Surface | Name |
| --- | --- |
| Settings toggle | Ajax Web Session |
| Task detail surface | Ajax Web Session |
| localStorage key | `ajax.webSession` |
| Frontend feature dir | `features/session/` |
| Rust slice | `web_session` |
| Test ids | `ajax-web-session`, `ajax-web-session-toggle` |

## Delegation decision

`Delegation decision: delegated via model-router` (bounded waves; parent plans/reviews/validates).

### Router status

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE: [.planning/router-runs/ajax-web-session-w1 compact wave1]
  REASON: Wave 1 is bounded frontend flag+gate wiring; default CURSOR lane.
  ESCALATE_IF: [scope exceeded, verification failed twice]
```

## Task checklist

### Wave 1 — Flag + Cursor gate
- [x] Settings toggle **Ajax Web Session**
- [x] `isAjaxWebSessionEnabled` / `setAjaxWebSessionEnabled`
- [x] TaskDetail: flag on + `agent` is Cursor → session shell; else terminal
- [x] Pi / Claude / Codex unchanged
- [x] Focused vitest

### Wave 2 — ACP bridge (replaced Pi RPC in Wave 2b)
- [x] Authenticated task-scoped WS (STT-like pattern)
- [x] Host `agent acp` in worktree: prompt / abort / stream / running|waiting
- [x] No terminal attach in this path

### Wave 3 — Chat UI
- [x] Scrollable history, composer, send/stop, streaming, running/waiting
- [x] Mobile-first: large composer, sheets, minimal chrome

### Wave 4 — Symbol context
- [x] Composer add-context → search → chips
- [x] Attach functions/methods/structs/classes/types/interfaces/files
- [x] Prompt includes symbol source; lightweight host search (rg/heuristics)

### Wave 5 — Response symbols
- [x] Interactive known-symbol refs → detail sheet (name, path, source, type)
- [x] Attach-from-sheet to next message
- [x] Brief `docs/architecture/web-cockpit.md` Ajax Web Session subsection

## Deviations

- User: Cursor only for this POC.
- User: do not implement Ajax Web Session for Pi agent tasks.
- User: call it **Ajax Web Session** (not “Ajax Session”).
- User: backend is **`agent acp`** (ACP), not Pi and not `agent -p`.

### Wave 2b — Replace Pi with `agent acp`
- [x] Rewrite `web_session_rpc` to speak ACP over `agent acp` stdio
- [x] Flow: initialize → authenticate(`cursor_login`) → session/new → session/prompt
- [x] Map `session/update` `agent_message_chunk` → `session.assistant_delta`; prompt result → settled
- [x] `session/cancel` on abort; auto `allow-once` for `session/request_permission` (POC)
- [x] Fake ACP peer tests (no live LLM); keep frontend WS wire types
- [x] Remove all `pi --mode rpc` usage from this path

## Validation

```bash
# per wave, focused first
cd crates/ajax-web/web && npm test -- --run src/features/settings src/features/task/TaskDetail src/features/session
npm run verify:slice -- operate   # when Rust slice lands
cargo nextest run -p ajax-web web_session
```

Results: Wave 1 focused vitest — see delegate report. Wave 2 `cargo nextest run -p ajax-web web_session axum_task_web_session architecture` — see delegate report.

Results: Wave 5 session vitest 19 passed; docs note lands `agent acp`.

Results: Wave 1 focused vitest — see delegate report. Wave 2 `cargo nextest run -p ajax-web web_session axum_task_web_session architecture` — see delegate report. Wave 2b replaced Pi with `agent acp` (12 web_session + architecture tests pass). Wave 3–5 session UI/AST vitest 70 passed (parent). Final: no `pi --mode rpc` in adapter.

## Approval

User asked to implement the POC; feature-flagged Cursor-only alternate surface is authorized. Terminal remains default when flag off / non-Cursor.
