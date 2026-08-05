# Packet: web-error-recovery-backend

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact
estimated_changed_lines: 180
```

## Goal

Add an additive stable `code` field to ajax-web failure JSON for operate/WebError paths so the browser can classify recovery without parsing free-form strings. Do not change task lifecycle or HTTP status numbers.

## Allowed files

- `crates/ajax-web/src/runtime/bridge.rs`
- `crates/ajax-web/src/adapters/http.rs`
- `crates/ajax-web/src/slices/operate/mod.rs`
- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-web/src/runtime/state.rs` (conflict JSON only — add `"code":"conflict"`)
- `crates/ajax-web/src/runtime/mod.rs` (session 401 — add `"code":"stale_session"` only on the browser-session-required response)
- Existing tests that construct `ActionFailure` under:
  - `crates/ajax-web/src/runtime/tests/**`
  - `crates/ajax-cli/src/web_backend/tests*` (if present)
  - any test broken by the new `ActionFailure` field

## Forbidden

- Frontend / `web/` TypeScript
- architecture.md ownership changes beyond what is needed in ajax-web helpers
- Changing operate success JSON
- Renaming existing `error` / `ok` / `state_changed` / `cockpit` fields
- Broad route migration of every `"ok": false` site (only operate path + WebError helpers + the two callouts above)
- Commits, pushes, branch changes

## Acceptance

1. `ActionFailure` has `code: String` (or `&'static str` converted to owned String in the struct — prefer `String` for simplicity) alongside `message` and `state_changed`.
2. `operation_error_response` emits:
   ```json
   { "ok": false, "error": "<message>", "code": "<code>", "state_changed": <bool>, "cockpit": ... }
   ```
3. `web_error_response` and `response_from_web_error` emit `"code"`:
   - `JsonSerialization` → `internal`
   - `CommandFailed` → `command_failed`
4. Mapping from `OperateError` (add `operate_error_code` next to `format_operate_error` in operate/mod.rs):
   - `UnknownAction` → `unknown_action`
   - `UnsupportedCapability` containing "terminal" (case-insensitive) → `needs_terminal`; else `unsupported_action`
   - `Command(CommandError::TaskNotFound, _)` → `task_not_found`
   - `Command(CommandError::ConfirmationRequired, _)` → `confirmation_required`
   - `Command(CommandError::PlanBlocked, _)` → `conflict`
   - other `Command` → `command_failed`
5. `unsupported_operate_action` sets `code: "unsupported_action"`.
6. `persist_operate` / `action_failure_from_cli` in `web_backend.rs` populate `code` (CLI persist failures → `command_failed`).
7. Optimistic conflict in `state.rs` includes `"code": "conflict"`.
8. Browser session required 401 includes `"code": "stale_session"`.
9. All `ActionFailure { ... }` construction sites compile.
10. Focused tests assert operate error JSON includes `code` (extend an existing runtime suite test that already checks 409 operate errors, or add a small unit test on `operation_error_response` / `operate_error_code`).

## Code anchors

- `ActionFailure` + `operation_error_response`: `crates/ajax-web/src/runtime/bridge.rs`
- `web_error_response` / `response_from_web_error`: `crates/ajax-web/src/adapters/http.rs`
- `format_operate_error`: `crates/ajax-web/src/slices/operate/mod.rs` ~454
- `persist_operate`: `crates/ajax-cli/src/web_backend.rs` ~416
- Conflict: `crates/ajax-web/src/runtime/state.rs` ~147-149
- Session 401: `crates/ajax-web/src/runtime/mod.rs` ~290-292

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web -p ajax-cli -- operate_error_code OR filter matching new/updated tests; if no named filter, cargo nextest run -p ajax-web -- suite_2 suite_3 OR full ajax-web if needed
      expected: exit 0; at least one assertion that error JSON contains code
    - type: build
      command: cargo check -p ajax-web -p ajax-cli
      expected: exit 0
  reason: Backend JSON shape + ActionFailure field are locked by compile + focused tests
```

## Stop if

- Need to change ajax-core CommandError semantics
- Estimated diff exceeds ~250 lines of production code (tests excluded) — stop and report
