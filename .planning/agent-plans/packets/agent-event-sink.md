# Packet: identity injection + __agent-event sink (task 1)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

The Ajax launch wrapper injects agent identity env vars into the spawned agent
child, and a new hidden `__agent-event` subcommand translates native client
hook invocations into one atomic task-keyed lifecycle event file. Outside Ajax
sessions the subcommand is a silent no-op. It must never fail or print: always
exit 0 with empty output (a nonzero exit could block the hosting client's
hooks).

## Allowed files

- `crates/ajax-cli/src/agent_event.rs` (new)
- `crates/ajax-cli/src/agent_runtime.rs`
- `crates/ajax-cli/src/cli.rs`
- `crates/ajax-cli/src/lib.rs`

## Forbidden changes

- No edits to `crates/ajax-core/**`, `crates/ajax-web/**`, `crates/ajax-tui/**`.
- No edits under any `tests/` directory; new tests are inline `#[cfg(test)]`
  in `agent_event.rs` / `agent_runtime.rs`.
- No changes to existing `__agent-runtime` CLI arguments or snapshot schema.
- No new dependencies.
- No consumption/ingestion of the event file (that is task 2).

## Context evidence

- Behavior spec: `.planning/agent-plans/native-event-adapters.md` task 1 +
  per-client translation table.
- Subcommand registration: `crates/ajax-cli/src/cli.rs:50`
  (`.subcommand(agent_runtime_command())`); sibling builder pattern at
  `cli.rs:135` (`fn agent_runtime_command()`, hidden command with args).
- Pre-context dispatch arms: `crates/ajax-cli/src/lib.rs:107` and
  `lib.rs:154` — `if let Some(("__agent-runtime", subcommand)) = ...` before
  registry load. `__agent-event` must dispatch the same way (no registry).
- Spawn site for env injection: `crates/ajax-cli/src/agent_runtime.rs:101`
  (`Command::new(program).args(args)...spawn()`).
- Atomic write pattern to mirror: `agent_runtime.rs:168`
  (`write_runtime_snapshot`: serde_json to tmp file `.{stem}.tmp-{pid}` then
  `fs::rename`). Task-id file stem helper: `agent_runtime.rs:213`
  (`task_file_stem`, replaces `/` `\\` with `__`) — make it `pub(crate)` and
  reuse.
- Timestamp helper: `agent_runtime.rs:217` (`now_millis`) — make `pub(crate)`
  and reuse.

## Code anchors

1. `agent_runtime.rs` `run_agent_runtime_with_interval` — on the
   `Command::new(program)` builder add:
   - `AJAX_TASK_ID` = task_id
   - `AJAX_RUN_ID` = `primary`
   - `AJAX_AGENT_EVENTS_DIR` = `state_root.parent().unwrap_or(state_root).join("agent-events")`
2. `cli.rs` — new `fn agent_event_command() -> Command` next to
   `agent_runtime_command()`:
   `Command::new("__agent-event").hide(true)` with required `--client`
   (`claude|codex|cursor|pi`) and required `--event <NAME>` string args.
   Register with `.subcommand(agent_event_command())` beside cli.rs:50.
3. `lib.rs` — add `mod agent_event;` beside `mod agent_runtime;` and a
   dispatch arm for `("__agent-event", sub)` in BOTH `run_with_args` (before
   lib.rs:111 context load) and `run_with_args_to_writer` (before its context
   load), calling `agent_event::run_agent_event_command(sub)` which returns
   `Ok(String::new())` in every non-panicking case.
4. New `agent_event.rs`, designed so logic is pure and env access stays in one
   thin entry point (env vars are process-global; tests must not set env):

```rust
pub(crate) struct AgentEventIdentity {
    task_id: String,
    run_id: String,          // env AJAX_RUN_ID, default "primary"
    events_dir: PathBuf,     // env AJAX_AGENT_EVENTS_DIR
}
// entry point: reads env; None (task id or events dir unset) => silent no-op
pub(crate) fn run_agent_event_command(matches: &ArgMatches) -> Result<String, CliError>;
// pure: (client, event, stdin payload) -> lifecycle value, None = ignore event
pub(crate) fn translate_agent_event(client: &str, event: &str, payload: &serde_json::Value) -> Option<&'static str>;
// atomic write of the snapshot to {events_dir}/{task_file_stem(task_id)}.json
pub(crate) fn write_agent_event(identity: &AgentEventIdentity, value: &str, observed_at_unix_millis: u128) -> std::io::Result<()>;
```

Snapshot JSON fields: `task_id`, `run_id`, `parent_run_id`
(`null` when run_id == "primary", else `"primary"`), `value`,
`observed_at_unix_millis`.

Translation table (exact; any other (client,event) pair → `None`):

| client | event | value |
| --- | --- | --- |
| claude | `UserPromptSubmit` | `working` |
| claude | `PreToolUse` | `working` |
| claude | `PostToolUse` | `working` |
| claude | `Notification` | `ask` if payload `message` string contains `permission` (case-insensitive), else `wait` |
| claude | `Stop` | `working` if payload `background_tasks` is a non-empty array, else `done` |
| codex | `prompt-submit` | `working` |
| codex | `turn-complete` | `done` |
| cursor | `beforeSubmitPrompt` | `working` |
| cursor | `stop` | `done` |
| pi | `before_agent_start` | `working` |
| pi | `agent_end` | `done` |

Stdin handling in the entry point: read all of stdin; empty or unparseable
JSON → `serde_json::Value::Null` (never an error). All failures (missing env,
unknown event, write error) → `Ok(String::new())`, no output, no logging.

## Test-first instructions

Inline `#[cfg(test)]` tests, written first, red command:
`cargo test -p ajax-cli agent_event` (initial red = assertion/compile failure
for the missing module is acceptable evidence).

1. `translate_claude_stop_with_background_tasks_stays_working`:
   payload `{"background_tasks":[{"id":1}]}` → `Some("working")`; empty array
   and missing key → `Some("done")`.
2. `translate_claude_notification_permission_vs_idle`:
   `{"message":"Claude needs your permission to run Bash"}` → `Some("ask")`;
   `{"message":"waiting for your input"}` → `Some("wait")`.
3. `translate_ignores_unknown_events`: `("claude","SessionStart")`,
   `("cursor","subagentStop")`, `("nope","stop")` → `None`.
4. `write_agent_event_is_atomic_and_task_keyed`: temp dir identity, write
   `done` then `working`; final file `{dir}/web__fix-login.json` parses with
   `value=="working"` (newest overwrite wins), `run_id=="primary"`,
   `parent_run_id==null`; no `.tmp` files remain in dir.
5. In `agent_runtime.rs` tests:
   `runtime_wrapper_injects_identity_env` — run wrapper with `/bin/sh -c`
   child writing `$AJAX_TASK_ID|$AJAX_RUN_ID|$AJAX_AGENT_EVENTS_DIR` to a file
   in the temp state root; assert task id, `primary`, and
   `<state_root parent>/agent-events`.
6. Entry-point no-op guard: call `run_agent_event_command` behavior via a
   pure helper `run_agent_event(identity: Option<AgentEventIdentity>, ...)`
   with `None` identity → returns Ok, writes nothing (do NOT set process env
   in tests).

## Edit instructions

Implement exactly the anchors above: env injection on the existing spawn
builder; hidden clap subcommand; two pre-context dispatch arms; new
`agent_event.rs` module with the pure translate/write split and the silent
no-op contract; `pub(crate)` visibility for `task_file_stem` and `now_millis`
in `agent_runtime.rs`. Smallest edit that makes the tests green.

## Verification commands

```bash
cargo test -p ajax-cli agent_event
cargo test -p ajax-cli agent_runtime
cargo clippy -p ajax-cli -- -D warnings
cargo check -p ajax-cli
```

## Acceptance criteria

- All four commands exit 0; new tests cover the six cases above.
- `__agent-event` never exits nonzero and never prints for any input.
- Wrapper child env carries the three identity vars.
- No file outside Allowed files changed.

## Stop conditions

- Any anchor line no longer matches the described code.
- A required edit falls outside Allowed files (e.g. clap arg parsing forces a
  change elsewhere).
- Existing `agent_runtime` tests fail for reasons unrelated to env injection.
- Patch would exceed ~400 changed lines.
