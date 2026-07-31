PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
dispatch_level: compact

## Goal

Copy AoE’s non-ACP Cursor wait hooks into Ajax’s **Cursor-specific** native-event
adapter path only: install `Notification(permission_prompt|elicitation_dialog)`
and `ElicitationResult` via `install_cursor_hooks`, translate those events in
the Cursor arms of `translate_native_event`, and mark Cursor wait capabilities
`Native`. Do not touch Claude’s Notification installer.

## Allowed files

- `crates/ajax-cli/src/agent_hooks.rs`
- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-core/src/agent_capability.rs`
- `architecture.md`
- `.planning/agent-plans/cursor-notification-wait-hooks.md` (checklist only)

## Forbidden changes

- Claude / Codex / Pi installers or translate arms (except shared helpers if
  reused unchanged)
- AoE ACP / sidecar status files
- Supervisor `ajax-supervisor/src/agent/cursor.rs` stream-json rewrite
- Web UI, notify adapter logic, pane_fallback rewrite
- Commits, pushes, branch changes

## Context evidence

- Desired: AoE `CURSOR_HOOK_EVENTS` includes
  `Notification` matcher `permission_prompt|elicitation_dialog` → Waiting and
  `ElicitationResult` → Running (non-ACP terminal hooks, not ACP).
- Ajax Cursor installer today: `install_cursor_hooks` at
  `crates/ajax-cli/src/agent_hooks.rs:111-144` writes flat `~/.cursor/hooks.json`
  entries via `merge_cursor_hook_entry` (`:306-328`) — `{ "command": ... }` only;
  no Notification / ElicitationResult.
- Claude matched pattern to mirror (Cursor-scoped): `merge_matched_hook_entries`
  (`:251-279`) and `hook_command_matched`; Cursor format should keep flat shape
  with optional `"matcher"` field (Cursor hooks.json schema).
- Translate: Cursor arms end at `crates/ajax-cli/src/agent_event.rs:146-154`;
  Claude Notification matched arms at `:124-133` are the map to copy for Cursor
  clients only (`permission_prompt` → Permission, `elicitation_dialog` → Question).
- Capability: `cursor_profile` at `crates/ajax-core/src/agent_capability.rs:107-116`
  marks both waits `Unavailable`; tests at `:145-177` assert that — flip to Native.
- Architecture still says Cursor has no native wait/ask (`architecture.md:474-476`,
  `:756-758`).

## Code anchors

- `fn install_cursor_hooks` — add Notification matchers + ElicitationResult
- `fn merge_cursor_hook_entry` / new `merge_cursor_matched_hook_entry` — support
  `"matcher"` on flat Cursor entries
- `translate_native_event` — add only `("cursor", ...)` arms
- `const fn cursor_profile` + capability tests
- `architecture.md` Cursor wait sentences

## Test-first instructions

1. In `agent_event.rs` tests, add:
   - `cursor_notification_permission_prompt_requests_permission_attention`
   - `cursor_notification_elicitation_dialog_requests_question_attention`
   - `cursor_elicitation_result_starts_turn` (or clears wait → Working via
     `turn_started`)
   Assert via `translate_native_event("cursor", ...)`.
2. In `agent_hooks.rs` tests, extend/add Cursor install coverage so
   `~/.cursor/hooks.json` contains:
   - `hooks.Notification[]` entries with matchers `permission_prompt` and
     `elicitation_dialog` and commands
     `ajax-cli __agent-event --client cursor --event Notification:<matcher>`
   - `hooks.ElicitationResult[]` with command
     `ajax-cli __agent-event --client cursor --event ElicitationResult`
3. In `agent_capability.rs` tests, change Cursor wait assertions from
   `Unavailable` to `Native` (both hook-client and AgentClient::Cursor).

Red commands:

```bash
cargo test -p ajax-cli --lib -- cursor_notification_permission cursor_notification_elicitation cursor_elicitation_result cursor_install
cargo test -p ajax-core --lib -- cursor_profile_marks_wait cursor_agent_client_profile
```

Confirm red for expected missing arms / Unavailable asserts before edits.

## Edit instructions

1. `install_cursor_hooks`: after existing events loop, install matched
   Notification hooks for `permission_prompt` and `elicitation_dialog` using
   Cursor flat JSON (`command` + `matcher`). Add unmatched `ElicitationResult`
   via existing `merge_cursor_hook_entry`.
2. Add `merge_cursor_matched_hook_entry` (or extend merge) that idempotently
   inserts `{ "matcher": ..., "command": ... }` and detects duplicates by
   command (and matcher).
3. `translate_native_event` Cursor-only arms:
   - `Notification:permission_prompt` → `attention_requested(Permission)`
   - `Notification:elicitation_dialog` → `attention_requested(Question)`
   - `ElicitationResult` → `turn_started()` (AoE maps to Running)
4. `cursor_profile`: set `permission_wait` and `question_wait` to `Native`.
5. Update `architecture.md` so Cursor is listed among structured wait/ask
   sources (Claude Notification, Codex PermissionRequest, **Cursor Notification
   permission/elicitation**); remove “Cursor has no native wait/ask” claims.

## Verification commands

```bash
cargo test -p ajax-cli --lib -- agent_hooks agent_event
cargo test -p ajax-core --lib -- agent_capability
cargo check -p ajax-cli -p ajax-core
```

## Acceptance criteria

- Cursor hooks install is idempotent and writes Notification matchers +
  ElicitationResult without changing Claude Notification install shape
- Cursor translate maps those events to attention / turn_started
- Cursor capability reports Native waits; pane fallback for Cursor waits is
  no longer allowed solely via Unavailable
- architecture.md matches the new Cursor native wait claim
- Focused tests green; no unrelated file edits

## Stop conditions

- Edits outside Allowed files
- Changing Claude Notification matchers or Codex PermissionRequest
- Inventing ACP / sidecar status files
- Broad supervisor cursor.rs rewrite
- Empty diff with success claim
