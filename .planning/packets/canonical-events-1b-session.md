# Packet: session hooks install + translate (Phase 1b)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Install and translate session open/close native events for Claude, Codex, and
Cursor into canonical `SessionOpened` / `SessionClosed`, projecting legacy
`working` / `done`. Keep Pi unchanged. No ajax-core or cache changes.

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-cli/src/agent_hooks.rs`

## Forbidden changes

- Do not edit ajax-core, agent_status_cache, architecture.md, or installers for
  events already present (Cursor pre/postToolUse, Codex PermissionRequest).
- Do not remove existing hook merge idempotency.
- No new dependencies. No commits.

## Context evidence

- Translate lives in `agent_event.rs` `translate_native_event` (post-1a).
- `project_legacy_value` already maps SessionOpened→working, SessionClosed→done
  if kinds exist; wire native names if missing.
- Installers: `agent_hooks.rs` `install_claude_hooks` / `install_codex_hooks` /
  `install_cursor_hooks` event arrays.
- Native names: Claude/Codex `SessionStart`/`SessionEnd`; Cursor
  `sessionStart`/`sessionEnd`.

## Code anchors

1. `translate_native_event`:
   - `("claude"|"codex", "SessionStart")` → SessionOpened
   - `("claude"|"codex", "SessionEnd")` → SessionClosed
   - `("cursor", "sessionStart")` → SessionOpened
   - `("cursor", "sessionEnd")` → SessionClosed
2. Installer event lists append those four names (Claude/Codex pair + Cursor
   pair). Idempotent merge must still pass.
3. Update/extend installer tests that assert event presence so new events are
   required.

## Test-first instructions

Red: `cargo test -p ajax-cli agent_hooks agent_event -- --nocapture`

1. `translate_session_start_end_projects_working_done` — SessionStart→Some("working"),
   SessionEnd→Some("done") for claude; sessionStart/sessionEnd for cursor.
2. Installer test: after install on empty home, Claude settings contain
   SessionStart and SessionEnd ajax commands; Cursor hooks.json contains
   sessionStart and sessionEnd.

Implement until green.

## Edit instructions

Smallest edits to the two allowed files. Reuse merge helpers.

## Verification commands

```bash
cargo test -p ajax-cli agent_hooks agent_event
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- New session events install idempotently and translate correctly.
- Existing hook tests remain green.

## Stop conditions

- Need ajax-core or cache changes.
- Patch > ~200 lines.
- Cursor/Claude event names differ from anchors (stop and report).
