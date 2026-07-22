# Packet: Cursor hook identity via cwd index + sessionStart env

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Cursor hooks fire but write nothing because `AJAX_TASK_ID` /
`AJAX_AGENT_EVENTS_DIR` are absent from the hook process. Publish a
cwd→identity index from `__agent-runtime`, let Cursor `__agent-event`
resolve identity from `CURSOR_PROJECT_DIR` / stdin `workspace_roots[0]` when
env is missing, and have Cursor `sessionStart` print session `env` JSON so
subsequent hooks get identity the Cursor-documented way. Keep the
`runtime_hooks_accepted` gate.

## Allowed files

- `crates/ajax-cli/src/agent_runtime.rs`
- `crates/ajax-cli/src/agent_event.rs`
- `architecture.md` (one short sentence under canonical agent-event / Cursor
  hooks only if needed for the cwd-index contract)

## Forbidden changes

- No `ajax-core` / `ajax-web` / pane recognizer / live.rs Other-policy changes
- No SQLite migration
- No changes to Claude/Codex/Pi translation tables beyond shared helpers
- No commits, pushes, or branch switches
- Do not weaken `runtime_hooks_accepted`

## Context evidence

| Category | Finding | Anchor |
|---|---|---|
| Desired | Cursor hooks fire; `agent-events/` empty; `__agent-event` no-ops without env | Live probe 2026-07-21; `agent_event.rs` `read_agent_event_identity` |
| Desired | Cursor session env for hooks is `sessionStart` `{"env":{...}}` | Cursor docs: sessionStart output `env` |
| Desired | Runtime injects AJAX_* into agent child only; Cursor hook runner may not inherit | `agent_runtime.rs` `.env("AJAX_TASK_ID"...)`; empty events despite Running wrappers |
| Pattern | Atomic JSON write via tmp+rename | `agent_runtime.rs` `write_runtime_snapshot`; `agent_event.rs` `write_agent_event` |
| Pattern | Events dir sibling of runtime | `agent_runtime.rs`: `state_root.parent()/agent-events` |
| Pattern | Command always exits 0; stdout empty today | `agent_event.rs` `run_agent_event_command` → `Ok(String::new())` |
| Architecture | Lifecycle evidence accepted even for Other; empty files are the blocker | `live.rs` `AgentEvidenceSource::Lifecycle` (no Other skip) |

## Code anchors

1. `agent_runtime.rs` `run_agent_runtime_with_interval` — after computing
   `agent_events_dir`, before spawn: write cwd-index entry from
   `std::env::current_dir()` → `{task_id, run_id:"primary", events_dir}`.
   On successful/failed exit path (both return sites after final snapshot):
   remove that cwd-index entry.
2. Index path helper (same file): under `agent_events_dir.join("cwd-index")`,
   file stem = same style as `task_file_stem` but for an absolute cwd string
   (replace `/` `\\` `:` with `__`, or hash if path is huge — prefer
   reversible `task_file_stem`-like encoding of the absolute path).
3. `agent_event.rs` `read_agent_event_identity` — keep env-first; add
   `resolve_agent_event_identity(payload: &Value) -> Option<Identity>` used by
   `run_agent_event_command` / `run_agent_event`:
   - env identity if present
   - else if client is `cursor` (case-sensitive match as today): project dir
     from `std::env::var("CURSOR_PROJECT_DIR")` or
     `payload["workspace_roots"][0]` string; look up
     `{events_dir candidate}` — **problem**: without env we do not know
     events_dir root.
   - Fix: store index under a discoverable Ajax cache. Prefer writing index at
     `{agent_events_dir}/cwd-index/{stem}.json` AND also record absolute
     `events_dir` inside the JSON. Discovery without env: derive cache root
     from well-known siblings of common Ajax homes is fragile.
   - **Required discovery rule (implement exactly):**
     1. Env identity wins.
     2. Else read `AJAX_AGENT_EVENTS_DIR` alone if set (partial) — still need
        task_id from index under that dir.
     3. Else for cursor only: try these roots in order for
        `{root}/cwd-index/{stem}.json` where stem encodes absolute project
        dir:  
        - `PathBuf::from(project_dir).join(".cache/ajax/agent-events")`  
        - if `AJAX_HOME` set: `{AJAX_HOME}/cache/agent-events`  
        - `{home}/.ajax-dev/cache/agent-events` and `{home}/.ajax/cache/agent-events`
          (home from `HOME`)  
        First readable valid JSON wins. JSON schema:
        `{"task_id":"...","run_id":"...","events_dir":"...","cwd":"..."}`.
4. `run_agent_event` — use resolved identity; unchanged gate + append.
5. `run_agent_event_command` — after `run_agent_event`, if `client=="cursor"`
   and `event=="sessionStart"` and identity resolved, return stdout
   `{"env":{"AJAX_TASK_ID":"...","AJAX_RUN_ID":"...","AJAX_AGENT_EVENTS_DIR":"..."}}`
   (compact or pretty; must be valid JSON object). Otherwise keep empty
   string. Still exit success.
6. Index publish must use the **absolute** `agent_events_dir` path (canonicalize
   or absolute from state_root) so hook lookup matches.

## Test-first instructions

Add tests in `agent_runtime.rs` / `agent_event.rs` `#[cfg(test)]` (no new
integration crates).

1. `runtime_publishes_and_clears_cwd_index` — run wrapper with `/bin/sh -c
   'exit 0'` from a temp cwd (use `std::env::set_current_dir` only inside the
   test process carefully, or pass cwd by running a shell that writes
   `pwd`); assert index file exists while running is hard — instead unit-test
   publish/clear helpers directly: `publish_cwd_index` + `clear_cwd_index`
   round-trip, and assert `run_agent_runtime_with_interval` clears index after
   exit (check file absent after return).
2. `cursor_event_resolves_identity_from_cwd_index_without_ajax_env` — build
   temp events dir + cwd-index for project path P; unset AJAX_* in the test
   by calling an internal `run_agent_event_with_identity_resolver` **or**
   test `resolve_cursor_identity(project_dir, payload, home, ajax_home)` pure
   function; then `run_agent_event(Some(identity), ...)` with Running
   runtime snapshot; assert jsonl/snapshot written. Prefer pure resolver
   unit test + one `run_agent_event` integration with explicit identity to
   avoid mutating process env.
3. `cursor_session_start_stdout_includes_session_env` — call the command
   helper that formats sessionStart stdout given identity; assert JSON has
   `env.AJAX_TASK_ID` etc. Wire `run_agent_event_command` path if practical
   without global env races; otherwise test `session_start_env_stdout(identity)
   -> String` used by the command.
4. `cursor_without_index_still_noops` — resolver returns None → no write.
5. Existing runtime gate tests remain green (exited runtime rejects
   beforeSubmitPrompt).

Red commands:

```bash
cargo test -p ajax-cli agent_event -- --nocapture
cargo test -p ajax-cli agent_runtime -- --nocapture
```

## Edit instructions

1. Extract `publish_cwd_index` / `clear_cwd_index` / `cwd_index_path` in
   `agent_runtime.rs`; call publish after events dir known, clear on both
   exit paths (and spawn-failure path if an index was published).
2. Add `resolve_cursor_identity(...)` in `agent_event.rs`; use it when env
   identity is missing and client is `cursor`.
3. Add `session_start_env_stdout`; use in `run_agent_event_command` for
   cursor/`sessionStart` only.
4. Keep all failure paths silent exit 0.

## Verification commands

```bash
cargo test -p ajax-cli agent_event -- --nocapture
cargo test -p ajax-cli agent_runtime -- --nocapture
cargo check -p ajax-cli --all-targets --all-features
cargo clippy -p ajax-cli --all-targets -- -D warnings
```

## Acceptance criteria

- Cursor hook path can write lifecycle events with no AJAX_* in the process
  env when a cwd-index entry exists for `CURSOR_PROJECT_DIR` / workspace root
- `sessionStart` stdout carries Cursor session `env` when identity resolves
- Missing index → still no-op
- `runtime_hooks_accepted` still drops post-exit non-settle events
- Focused tests prove RED then GREEN; clippy/check clean

## Stop conditions

- Need registry/DB lookup to map worktree → task
- Need changes outside allowed files
- Cursor requires a different sessionStart schema than `{"env":{...}}`
- Index discovery roots insufficient for Matt's AJAX_HOME layout (report and
  stop rather than guessing more paths)
