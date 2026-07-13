# Packet: web Drop authorizes empty-registry wipe

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

When Web Cockpit Drop removes the last persistable task (or otherwise leaves
the in-memory registry empty after a non-empty load), `CliRuntimeBridge` must
authorize a one-shot empty-registry save — matching CLI `drop --execute` and
native Cockpit — so operators no longer see:

`refusing to save empty registry over non-empty loaded state; authorize delete-all before saving`

## 3. Allowed files

- `crates/ajax-cli/src/web_backend.rs`
- `.planning/agent-plans/web-drop-defect.md` (checklist only)

## 4. Forbidden changes

- Do not weaken `prevent_accidental_empty_overwrite` for non-Drop paths.
- Do not change `context.rs` guard semantics, SQLite store APIs, or native
  Cockpit paths (already correct).
- No frontend / terminal edits.
- No commit, push, branch, rebase, or merge.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — save-on-operator-action contract is in
  `architecture.md`; this is a missing authorize call on the web bridge only.
- Serena: `NOT_REQUIRED` — anchors located by rg/Read.
- ast-grep: `NOT_REQUIRED` — single `execute_operate` / `persist_operate`
  call chain in `web_backend.rs`.
- Prior round: `OkRunner` always reports the task worktree/branch present, so
  Drop ends in `TeardownIncomplete` and never reaches empty-registry persist.
  Resume must use an absent-resources runner (below).

## 6. Code anchors

- `CliRuntimeBridge::execute_operate` (~239–246):
  `self.persist_operate(operate(...), context)` — missing Drop authorize
- Native mirror: `cockpit_backend.rs` (~168–173)
  `save_state.allow_empty_registry_once()` when action is Drop
- Existing test stub left by prior round:
  `web_bridge_drop_of_last_task_persists_empty_registry` (currently fails with
  TeardownIncomplete under `OkRunner`) — **replace its runner**, keep the name
  and desired-behavior assertions
- Keep `web_bridge_rejects_empty_save_over_non_empty_sqlite_state`

## 7. Test-first instructions

Replace the runner inside existing
`web_bridge_drop_of_last_task_persists_empty_registry` (do not assert the broken
Err path in the test body).

Add a small test-only `AbsentDropRunner` (or equivalent) in the same tests
module that always reports substrate **absent**:

- `git worktree list --porcelain` → only the main repo worktree (`/repo/web`),
  **no** `ajax-fix-login` worktree
- `git branch --format=%(refname:short)` → `main\n` only (no `ajax/fix-login`)
- `tmux list-sessions` / any session list → empty / no `ajax-web-fix-login`
- Other commands → status 0 with empty or harmless stdout

Setup:

1. Persist `reviewable_context()` to sqlite.
2. `CliRuntimeBridge::for_context(Some(&paths), &context)`.
3. `execute_operate(OperateRequest { task_handle: "web/fix-login", action: "drop" }, …, &mut AbsentDropRunner)`.

Desired-behavior assertions (RED fails pre-fix, GREEN after):

- `execute_operate` returns `Ok(_)`
- `context.registry.list_tasks()` is empty
- `load_context(&paths).registry.list_tasks()` is empty

RED command:

```bash
rtk cargo nextest run -p ajax-cli web_bridge_drop_of_last_task_persists_empty_registry
```

Expected RED excerpt contains `refusing to save empty registry` (Drop removes
the task; persist refuses the empty wipe without authorize).

## 8. Edit instructions

In `CliRuntimeBridge::execute_operate`:

1. Let `authorize_empty = request.action == OperatorAction::Drop.as_str()`
   (import `ajax_core::models::OperatorAction` if needed).
2. `let result = operate(context, runner, request);`
3. If `authorize_empty` and the result will persist —
   `Ok(o) if o.state_changed` or `Err(OperateError::Command(_, true))` —
   call `self.save_state.allow_empty_registry_once()`.
4. `self.persist_operate(result, context)`.

Do not authorize when `state_changed` is false.

## 9. Verification commands

```bash
rtk cargo nextest run -p ajax-cli web_bridge_drop_of_last_task_persists_empty_registry
rtk cargo nextest run -p ajax-cli web_bridge_rejects_empty_save
rtk cargo check -p ajax-cli
```

## 10. Acceptance criteria

- Drop of the sole task via the web bridge persists an empty registry.
- Accidental empty save without Drop authorization still fails.
- Diff limited to `web_backend.rs` (+ plan checklist).

## 11. Stop conditions

- Absent-resources runner still cannot complete Drop to `Removed` after a
  reasonable stub — stop and report the exact observation commands.
- Fix would require changing `context.rs` empty-overwrite semantics.
- Scope grows outside Allowed files.
