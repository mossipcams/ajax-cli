# Packet: web-error-recovery-frontend

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact
estimated_changed_lines: 160
```

## Goal

Parse optional backend `code` into `ApiError`, add a shared operator-facing recovery helper, and wire the operate path (`taskMutations`) so error toasts + telemetry use codes. Document the contract briefly in `docs/architecture/web-cockpit.md`. Missing `code` must keep today's behavior.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/types.ts`
- `crates/ajax-web/web/src/shared/lib/contracts.ts`
- `crates/ajax-web/web/src/shared/lib/api.ts`
- `crates/ajax-web/web/src/shared/lib/api.test.ts`
- `crates/ajax-web/web/src/shared/lib/errorRecovery.ts` (new)
- `crates/ajax-web/web/src/shared/lib/errorRecovery.test.ts` (new)
- `crates/ajax-web/web/src/features/task/taskMutations.ts`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx` (only if needed for toast/message assertions)
- `docs/architecture/web-cockpit.md`
- `.planning/agent-plans/web-error-recovery.md` (checklist/ledger only)

## Forbidden

- Rust / ajax-web backend (already done)
- Redesigning ResultPanel layout or adding success toasts
- Migrating Diff Review, Terminal WS, Speech, Settings beyond what `api.ts` already parses for operations
- Changing ConnectionStatus banner behavior
- Commits, pushes, branch changes
- Editing the Cursor plan file under `~/.cursor/plans/`

## Acceptance

1. `OperationResponse` has optional `code?: string | null`.
2. `assertOperationResponse` accepts optional string `code` (same pattern as `error`).
3. `ApiError` gains optional `readonly code: string | null` (constructor arg, default null). When building from mutation/POST failure payloads, set `code` from `payload.code` when it is a non-empty string.
4. New `errorRecovery.ts` exports something like:
   - `type RecoveryHint = "retry" | "open_terminal" | "reload_session" | "none"`
   - `operatorErrorPresentation(error: { message: string; code?: string | null; kind?: string } | ApiError | unknown): { message: string; hint: RecoveryHint; telemetryKind: string }`
   - Mapping (code wins; fall back to ApiError.kind / message):
     | code / kind | toast message preference | hint | telemetryKind |
     | --- | --- | --- | --- |
     | `needs_terminal` | keep server message (or "Use the terminal for this action") | open_terminal | needs_terminal |
     | `stale_session` | server message or "Session expired — reload" | reload_session | stale_session |
     | `conflict` / kind conflict | server message | retry | conflict |
     | `task_not_found` | server message | none | task_not_found |
     | `confirmation_required` | server message | retry | confirmation_required |
     | `unsupported_action` / `unknown_action` | server message | none | operation_failed |
     | `command_failed` / missing | server message or "Action failed" | retry | operation_failed |
     | network kind | "Action failed — network error" | retry | network |
   - For toast text: prefer non-empty server `message`; only replace when message empty. Append a short recovery suffix when hint is `open_terminal` (` — open the terminal`) or `reload_session` (` — reload the page`) if the message does not already mention it.
5. `runTaskAction` in `taskMutations.ts`:
   - On `!result.ok`, use `operatorErrorPresentation(result.error ?? result.response)` for `onResult` message and `error_kind` in telemetry.
   - On catch, use presentation for network (`telemetryKind: network`).
6. Vitest covers: `errorRecovery` mappings; `api` postOperation 409 with `code` populates `ApiError.code`; ActionBar/taskMutations path still toasts errors (extend existing test if one asserts message).
7. `docs/architecture/web-cockpit.md`: short subsection under browser/API contracts documenting failure JSON `{ ok:false, error, code? }` and that `code` is a recovery hint, not browser policy. List starter codes. Do not invent a second task model.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/shared/lib/errorRecovery.test.ts src/shared/lib/api.test.ts src/features/task/ActionBar.test.tsx
      expected: exit 0
    - type: typecheck
      command: npm run web:check
      expected: exit 0
  reason: Frontend contract parse + operate toast/telemetry mapping
```

## Stop if

- Diff exceeds ~200 production LOC (tests excluded)
- Tempted to add ResultPanel Retry button wiring across App — skip; toast copy suffix is enough for this packet
