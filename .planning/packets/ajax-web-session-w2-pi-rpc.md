PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Implement the Ajax Web Session backend bridge: authenticated task-scoped WebSocket that runs host `pi --mode rpc` in the task worktree and exposes prompt/abort/streaming/status for Wave 3 UI.

## Scope

### Allowed
- crates/ajax-web/src/slices/web_session.rs
- crates/ajax-web/src/slices/web_session/ (directory form ONLY if single file would exceed ~600 LOC; prefer one file)
- crates/ajax-web/src/slices/mod.rs
- crates/ajax-web/src/adapters/web_session_rpc/mod.rs
- crates/ajax-web/src/adapters/web_session_rpc/bridge.rs
- crates/ajax-web/src/adapters/web_session_rpc/tests.rs (or inline cfg tests)
- crates/ajax-web/src/adapters/mod.rs
- crates/ajax-web/src/architecture.rs
- crates/ajax-web/src/runtime/task_routes/cockpit.rs
- crates/ajax-web/src/runtime/task_routes/live.rs
- crates/ajax-web/src/runtime/tests/suite_4.rs
- crates/ajax-web/src/runtime/tests/suite_1.rs (only if route table assertions require `/web-session`)
- .planning/agent-plans/ajax-web-session-poc.md

### Forbidden
- Frontend chat UI beyond what already exists (Wave 3 owns UI)
- Terminal/PTY integration
- Enabling Ajax Web Session for Pi agent tasks
- Changing task start/Cursor launch / registry truth
- Symbol search (Wave 4)
- Commits, pushes, merges, rebases, branch changes
- New cargo dependencies unless already present and required for std process I/O

## Acceptance

1. New slice `web_session` with versioned JSON wire types:
   - Client: `session.prompt` { message: string }, `session.abort` {}
   - Server: `session.ready` { sessionId }, `session.status` { state: "running"|"waiting" }, `session.assistant_delta` { text }, `session.settled` {}, `session.error` { code, message }, `session.closed` {}
2. `prepare_web_session(context, handle)` returns worktree path + handle; TaskNotFound when missing; WorktreeMissing when path empty/missing.
3. Authenticated route `GET /api/tasks/{handle}/web-session` mirrors STT: requires browser session cookie, websocket upgrade, same-origin Origin check; resolves task and upgrades to bridge.
4. Bridge spawns `pi --mode rpc` with cwd = task worktree (resolve `pi` via PATH). Speak Pi RPC JSONL on stdin/stdout. Map: prompt→stdin prompt; abort→stdin abort; stdout message text streaming→`session.assistant_delta`; `agent_settled`→`session.settled` + status waiting; prompt accepted→status running; process/socket end→`session.closed` / errors as `session.error`.
5. architecture.rs lists `web_session` in SLICES and `web_session_rpc` in ADAPTERS.
6. Focused tests: prepare_web_session; wire serde round-trips; route auth/upgrade/origin (like STT). Process bridge may use a fake JSONL peer script for unit tests — do not require live LLM calls.
7. Plan Wave 2 checklist items marked done.

## Constraints

- Browser remains presentation-only; no second task registry.
- Adapter must not import sibling slices except `crate::slices::web_session` wire types (same pattern as stt_provider→stt). Prefer keeping Axum WS code in bridge.rs and spawn/protocol in mod.rs if splitting.
- Keep each .rs file under ~600 LOC (hard max 1000).
- Product name remains Ajax Web Session; route path `/web-session`.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web web_session
      expected: pass
    - type: test
      command: cargo nextest run -p ajax-web axum_task_web_session
      expected: pass (or equivalent new route test names)
    - type: test
      command: cargo nextest run -p ajax-web architecture
      expected: pass
  broader_checks:
    - cargo check -p ajax-web
  reason: Locks protocol, route auth, and architecture isolation without needing live Pi/LLM.
```

## Stop if

- Need edits outside Allowed
- Would require new heavy dependencies or a full indexer
- Would attach to tmux/terminal
- Patch would exceed ~400 changed lines — split and stop
- Cannot find a safe way to spawn/kill pi without hanging tests

## Code anchors

- STT route: `crates/ajax-web/src/runtime/task_routes/live.rs` `axum_task_stt`
- STT path dispatch: `crates/ajax-web/src/runtime/task_routes/cockpit.rs` `/stt` branch
- STT wire types: `crates/ajax-web/src/slices/stt.rs`
- STT bridge pattern: `crates/ajax-web/src/adapters/stt_provider/bridge.rs`
- Terminal prepare pattern: `crates/ajax-web/src/slices/terminal.rs`
- Architecture lists: `crates/ajax-web/src/architecture.rs`
- Pi RPC stdin commands: `prompt`, `abort`; stdout events include `message_update` / `agent_settled` (host package `@earendil-works/pi-coding-agent`)

## Edit instructions

1. Add `web_session` slice with wire enums + `prepare_web_session`.
2. Add `web_session_rpc` adapter that bridges WS ↔ `pi --mode rpc` JSONL.
3. Wire `/web-session` next to `/stt` in cockpit/live routes.
4. Update architecture allowlists + suite_4 auth tests.
5. Check off Wave 2 in the plan.
