PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Expand native hooks: Codex `PermissionRequest` → `ask`; Cursor
`preToolUse`/`postToolUse` → `working`. Add an e2e test that a lifecycle
`wait` event file causes exactly one attention webhook on
`refresh_cockpit(..., deliver_notifications=true)`.

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-cli/src/agent_hooks.rs`
- `crates/ajax-cli/src/web_backend.rs`

## Forbidden changes

- Do not edit ajax-core, architecture.md, README, notify.rs, or
  agent_status_cache.rs.
- Do not invent Cursor/Pi wait/ask mappings.
- Do not fire curl from `__agent-event`.
- Do not commit, push, merge, rebase, or change branches.
- No drive-by cleanup outside Allowed files.

## Context evidence

- Desired: plan Round 2.
- Translator: `agent_event.rs:60-96` `translate_agent_event` match arms.
- Codex installer events: `agent_hooks.rs:65` currently
  `UserPromptSubmit, PreToolUse, PostToolUse, Stop` — add `PermissionRequest`.
- Cursor installer events: `agent_hooks.rs:90` currently
  `beforeSubmitPrompt, stop` — add `preToolUse`, `postToolUse`.
- Test event lists: `agent_hooks.rs:286-292` `codex_events`/`cursor_events`
  must grow with the new events (array lengths change).
- Existing translation tests: `agent_event.rs:210-226`
  `translate_codex_and_pi_verified_events`; extend for PermissionRequest and
  Cursor tool hooks.
- E2E pattern: `web_backend.rs:623-641` `write_agent_status_event`;
  `web_backend.rs:744-825` `web_refresh_cockpit_notify_respects_deliver_flag`
  (RecordingRunner + `CliRuntimeBridge::refresh_cockpit` + notify config).
- Status refresh already proven by
  `cockpit_api_refreshes_live_task_status_before_rendering` (`web_backend.rs:644`).

## Code anchors

1. `translate_agent_event`: add
   `("codex", "PermissionRequest") => Some("ask")` and
   `("cursor", "preToolUse" | "postToolUse") => Some("working")`.
2. `install_codex_hooks` events array include `PermissionRequest`.
3. `install_cursor_hooks` events array include `preToolUse`, `postToolUse`.
4. Update `codex_events()` / `cursor_events()` helpers in hooks tests.
5. New test in `web_backend.rs` tests module (near notify test):
   - temp `cache_dir` on context.runtime_paths
   - Active task with present-enough fixture (reuse reviewable/active pattern
     from existing cockpit refresh tests so probe applies events)
   - `write_agent_status_event(&cache_dir, task_id, "wait")`
   - notify config set; RecordingRunner
   - `refresh_cockpit(..., true)` → exactly one curl
   - second `refresh_cockpit(..., true)` → still one curl (episode stamp)
   - body contains Waiting and optional `(codex)` client from Round 1

If the Active fixture requires tmux/session stubs, mirror
`cockpit_api_refreshes_live_task_status_before_rendering` setup exactly, then
assert via `notify_attention` side effect (curl count), not HTTP status card.

## Test-first instructions

1. Add failing translation asserts for Codex PermissionRequest → ask and
   Cursor preToolUse/postToolUse → working; red:
   `cargo nextest run -p ajax-cli agent_event -- translate_codex translate_cursor PermissionRequest preToolUse`
2. Extend `codex_events`/`cursor_events` first so install tests fail missing
   hooks; red:
   `cargo nextest run -p ajax-cli agent_hooks`
3. Add e2e test that fails until hooks/events wired (or until refresh applies
   wait); red focused filter on the new test name.
4. Implement production edits; green all three filters.

## Edit instructions

Implement translator + installer list changes; add e2e as specified. Keep
idempotent merge behavior unchanged.

## Verification commands

```bash
cargo nextest run -p ajax-cli agent_event agent_hooks
cargo nextest run -p ajax-cli web_refresh_cockpit_notify lifecycle_wait notify
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt --check
```

(Adjust the e2e filter to the exact new test name.)

## Acceptance criteria

- Codex PermissionRequest translates to `ask` and is installed.
- Cursor preToolUse/postToolUse translate to `working` and are installed.
- Lifecycle wait event → one webhook on deliver refresh; second refresh silent.
- agent_hooks install still idempotent.
- Focused tests + clippy/fmt green.

## Stop conditions

- Edits outside Allowed files.
- Need pane classification or ajax-core changes.
- Cannot get refresh to ingest lifecycle events without expanding scope —
  stop and report with the failing e2e rather than widening Allowed files.
- Patch > ~400 lines.
