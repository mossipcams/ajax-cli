# Packet: agent-hooks installer + real event names (task 3)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

`ajax agent-hooks install` idempotently installs env-guarded `__agent-event`
forwarders into each client's global hook config (Claude, Codex, Cursor as
JSON hook entries; Pi as a TypeScript extension file). Also correct the
speculative Codex/Pi event names in the task-1 translation table to the
verified real ones.

## Allowed files

- `crates/ajax-cli/src/agent_hooks.rs` (new)
- `crates/ajax-cli/src/agent_event.rs` (translation table + its tests only)
- `crates/ajax-cli/src/cli.rs`
- `crates/ajax-cli/src/lib.rs`

## Forbidden changes

- No writes to the real `$HOME` from tests — all install logic takes an
  explicit `home: &Path` parameter; tests use temp dirs only.
- No edits to `~/.codex/config.toml` handling (do not touch `[hooks.state]`
  trust entries).
- No changes to `write_agent_event`, wrapper, cache, or core crates.
- No edits under any `tests/` directory.
- No new dependencies (serde_json is already available).

## Context evidence

- Verified formats (inspected live configs + official docs, 2026-07-21):
  - Claude `~/.claude/settings.json`: top-level `hooks` object;
    `hooks.<Event> = [ { "matcher"?: string, "hooks": [ { "type": "command",
    "command": string, "timeout"?: n } ] } ]`. Events: `UserPromptSubmit`,
    `PreToolUse`, `PostToolUse`, `Notification`, `Stop`.
  - Codex `~/.codex/hooks.json`: same schema under top-level `"hooks"` with
    the same event names (verified in Matt's live file: `UserPromptSubmit`,
    `PostToolUse`, `Stop`, `SessionStart`). Trust is recorded separately in
    config.toml `[hooks.state]` — new hooks prompt for trust on next run;
    leave that alone.
  - Cursor `~/.cursor/hooks.json`: `{ "version": 1, "hooks": {
    "beforeSubmitPrompt": [ { "command": string } ], "stop": [...] } }`.
    Payload arrives on stdin as JSON; exit 0 = success (our sink always
    exits 0).
  - Pi: TypeScript extensions auto-discovered at
    `~/.pi/agent/extensions/*.ts`, default-export
    `function (pi) { pi.on("<event>", handler) }`; lifecycle events include
    `before_agent_start` and `agent_settled` (= will not auto-continue —
    the correct settle signal; `agent_end` may retry/continue).
- Existing translation table: `crates/ajax-cli/src/agent_event.rs:65-96`
  (`translate_agent_event`); its tests at `agent_event.rs:172+`.
- Subcommand registration pattern: `crates/ajax-cli/src/cli.rs:50` and the
  `agent_event_command()` builder added in task 1; pre-context dispatch arms
  in `lib.rs` `run_with_args` / `run_with_args_to_writer` (see the
  `__agent-event` arms added in task 1).

## Code anchors

1. `agent_event.rs` `translate_agent_event` — replace the speculative rows:
   - remove `("codex", "prompt-submit")` and `("codex", "turn-complete")`;
     add `("codex", "UserPromptSubmit" | "PreToolUse" | "PostToolUse")` →
     `working`, `("codex", "Stop")` → `done`.
   - remove `("pi", "agent_end")`; add `("pi", "agent_settled")` → `done`
     (keep `before_agent_start` → `working`).
   Update the existing translate tests accordingly (speculative names become
   the verified ones; `("pi","agent_end")` now expects `None`).
2. New `agent_hooks.rs`:
   - `pub(crate) fn install_agent_hooks(home: &Path) -> Result<String, CliError>`
     — performs all four installs, returns a short per-client summary line
     ("claude: installed|already installed", ...). Missing parent dirs are
     created only for files Ajax owns outright (the Pi extension file and
     cursor hooks.json); for Claude/Codex settings the file is created if
     absent.
   - JSON merge helper (pure):
     `fn merge_hook_entries(root: &mut serde_json::Value, event: &str, command: &str)`
     — ensures `hooks.<event>` array contains an entry whose inner
     `hooks[].command` equals `command`; appends
     `{ "hooks": [ { "type": "command", "command": command } ] }` when
     missing; never removes or reorders existing entries. Idempotent by
     exact command-string match.
   - Hook command strings (marker = leading `ajax-cli __agent-event`):
     - claude: events `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
       `Notification`, `Stop` → `ajax-cli __agent-event --client claude
       --event <Event>`
     - codex (`~/.codex/hooks.json`): events `UserPromptSubmit`,
       `PreToolUse`, `PostToolUse`, `Stop` → `--client codex`
     - cursor (`~/.cursor/hooks.json`): ensure `"version": 1`; events
       `beforeSubmitPrompt`, `stop`, entries `{ "command": "ajax-cli
       __agent-event --client cursor --event <event>" }`, same
       command-match idempotency.
   - Pi extension: write `~/.pi/agent/extensions/ajax-agent-events.ts`
     (overwrite always — Ajax owns this file) containing a default-export
     function subscribing `before_agent_start` and `agent_settled`, each
     handler calling
     `pi.exec("ajax-cli", ["__agent-event", "--client", "pi", "--event", "<event>"])`
     and swallowing errors.
   - Preserve unrelated JSON keys and formatting-agnostic write (pretty
     JSON via `serde_json::to_string_pretty`).
3. `cli.rs` — visible `Command::new("agent-hooks")` with `install`
   subcommand ("Install agent status hooks for supported clients").
4. `lib.rs` — pre-context dispatch arm for `("agent-hooks", sub)` calling
   `agent_hooks::run_agent_hooks_command(sub)` (resolves `$HOME` itself,
   then calls `install_agent_hooks`).

## Test-first instructions

Red command: `cargo test -p ajax-cli agent_hooks` (plus updated
`agent_event` tests). Inline tests in `agent_hooks.rs` against a temp home:

1. `install_creates_all_configs_in_empty_home`: empty temp home → after
   install, `~/.claude/settings.json` has all five events with the marker
   command; `~/.codex/hooks.json` four events; `~/.cursor/hooks.json` has
   `version == 1` and both events; Pi extension file exists and contains
   `before_agent_start` and `agent_settled`.
2. `install_is_idempotent`: run twice → byte-identical files after second
   run (no duplicate entries).
3. `install_preserves_existing_user_hooks`: seed
   `~/.claude/settings.json` with an unrelated `PostToolUse` entry
   (`workmux set-window-status working`) and a top-level `model` key →
   after install both survive and the ajax entry is appended.
4. In `agent_event.rs`: update translate tests — codex
   `UserPromptSubmit`→working / `Stop`→done, pi `agent_settled`→done,
   `agent_end`→None.

## Edit instructions

Exactly the anchors above. Keep merge logic pure over `serde_json::Value`.
No env reads anywhere except resolving `$HOME` in the command entry point.

## Verification commands

```bash
cargo test -p ajax-cli agent_hooks
cargo test -p ajax-cli agent_event
cargo clippy -p ajax-cli -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- All tests green; idempotency proven byte-for-byte.
- Existing user hook entries and unrelated settings keys survive untouched.
- No test touches the real home directory.

## Stop conditions

- Any anchor mismatch.
- Merge cannot preserve an existing config without restructuring it.
- Patch would exceed ~400 changed lines.
