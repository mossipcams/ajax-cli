PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Add Ajax Web Session AST-aware context: lightweight worktree symbol search API, composer add-context sheet with multi-select, removable chips, and include selected symbol source in the prompt sent to Agent P.

## Scope

### Allowed
- crates/ajax-web/src/slices/web_session.rs (extend; split only if approaching LOC max)
- crates/ajax-web/src/runtime/task_routes/cockpit.rs
- crates/ajax-web/src/runtime/task_routes/live.rs
- crates/ajax-web/src/runtime/tests/suite_4.rs
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.test.tsx
- crates/ajax-web/web/src/features/session/SymbolSearchSheet.tsx
- crates/ajax-web/web/src/features/session/SymbolSearchSheet.test.tsx
- crates/ajax-web/web/src/features/session/types.ts
- crates/ajax-web/web/src/features/session/webSessionTransport.ts
- crates/ajax-web/web/src/features/session/webSessionTransport.test.ts
- crates/ajax-web/web/src/shared/lib/api.ts
- crates/ajax-web/web/src/shared/lib/api.test.ts
- crates/ajax-web/web/src/styles.css
- .planning/agent-plans/ajax-web-session-poc.md
- .planning/packets/ajax-web-session-w4-symbols.md

### Forbidden
- Full indexer / tree-sitter dependency / language servers
- Response symbol tapping / detail sheet (Wave 5)
- Terminal integration
- Enabling for non-Cursor agents
- Commits / branch changes
- Changing Pi RPC spawn wiring except prompt message composition if needed

## Acceptance

1. `GET /api/tasks/{handle}/symbols?q=` (cookie auth) searches the task worktree with lightweight heuristics (prefer host `rg` if available, else walk+string match). Return JSON `{ ok: true, symbols: [{ id, name, kind, path, start_line, end_line, preview }] }` for kinds: function, method, struct, class, type, interface, file. Cap results (~30).
2. `GET /api/tasks/{handle}/symbols/{id}` OR include source in search results — enough source text to attach (prefer a detail endpoint or `source` field on search hits; keep POC small).
3. Composer has an Add context control opening a mobile sheet: search input, matching rows (name + path + kind), multi-select, confirm.
4. Selected symbols show as removable chips (e.g. `SessionManager.start_session() ×`).
5. On Send, prompt text sent to `session.prompt` includes a clear context section with each attached symbol’s path/kind/source before the user question.
6. Focused Rust tests for search helpers (temp dir fixtures) + route auth smoke; vitest for chips/sheet/prompt composition.
7. Wave 4 checklist done in the plan.

## Constraints

- No new cargo deps if std + existing crates suffice; shelling to `rg` is OK with graceful fallback.
- Stay under ~600 LOC/file; peel tests if needed.
- Do not build a permanent index.
- Mobile sheet/drawer, not a desktop multi-panel IDE.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web web_session
      expected: pass
    - type: test
      command: npm run web:test -- --run src/features/session src/shared/lib/api.test.ts
      expected: pass
  broader_checks:
    - cargo check -p ajax-web
  reason: Symbol search + composer context are covered by fixture unit tests and UI tests.
```

## Stop if

- Would add tree-sitter/new indexer architecture
- Edits outside Allowed
- Exceed ~400 changed lines — split and stop
- Wave 5 response linking creeps in

## Code anchors

- `prepare_web_session` / wire types: `crates/ajax-web/src/slices/web_session.rs`
- Route dispatch: `crates/ajax-web/src/runtime/task_routes/cockpit.rs`
- Chat UI: `crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx`
- API helpers: `crates/ajax-web/web/src/shared/lib/api.ts`

## Edit instructions

1. Add symbol search (+ source extract) in web_session slice; HTTP route beside web-session.
2. Frontend SymbolSearchSheet + chips on AjaxWebSessionView; compose prompt with context.
3. Tests + plan checklist.
