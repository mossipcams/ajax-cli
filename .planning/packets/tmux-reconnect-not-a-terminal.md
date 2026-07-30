```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Make ephemeral tmux session setup idempotent for Web Cockpit reconnect without
attaching. After #692, reconnect fails with
`failed to create terminal session: open terminal failed: not a terminal`
because setup uses `new-session -Ad` and `-A` attaches when the session
already exists (requires a TTY). Setup must create detached (`-d`) and treat
an already-existing ephemeral session as success.

## Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `architecture.md`

## Forbidden changes

- Do not change linger-on-disconnect (`teardown: vec![]`) or reaper/
  `destroy_ephemeral_session_commands`.
- Do not change client-id hashing, query parsing, seed/history, or browser JS.
- Do not treat `open terminal failed: not a terminal` as success (that hides
  the bug if `-A` returns).
- Do not edit files outside Allowed files.
- No commits, pushes, merges, rebases, or branch changes.

## Context evidence

- **Desired behavior:** Reconnect to an existing ephemeral grouped session must
  succeed from a non-TTY setup runner. First create still uses detached
  `new-session -d -s <ephemeral> -t <shared>`. Existing session → continue
  setup (set-option…) without error.
- **Source anchor:** `build_isolated_attach_plan_with_token` setup uses
  `"-Ad"` (lines ~237–247). Bridge fails any non-success setup stderr as
  `failed to create terminal session: {failure}` (~610–621).
- **Live repro:** `tmux new-session -Ad -s <existing> …` →
  `open terminal failed: not a terminal` (exit 1). Same session with
  `new-session -d` → `duplicate session: …` (exit 1). `has-session` succeeds.
- **Architecture:** `architecture.md` ~882 claims
  `Setup is idempotent (new-session -A)` — incorrect for non-TTY bridge setup;
  update to match the fixed behavior.
- **Pattern reuse:** Existing unit tests assert exact `isolated.setup` args
  (`isolated_attach_plan_creates_grouped_session_then_attaches`,
  `isolated_attach_plan_is_stable_for_same_client_token`).

## Code anchors

- `crates/ajax-web/src/adapters/terminal_pty.rs`
  - `build_isolated_attach_plan_with_token` setup `new-session` args (`-Ad`)
  - `bridge_task_terminal_socket` setup loop (~610–621)
  - tests asserting `"-Ad"` (~1035, ~1363)
- `architecture.md` ~879–885 (isolated grouped session / `-A` wording)

## Test-first instructions

1. Add a focused unit test helper coverage, e.g.
   `setup_ignores_duplicate_session_on_new_session_only`:
   - `should_ignore_setup_failure` (or equivalent name) returns `true` when
     command is `new-session` and stderr contains `duplicate session`
   - returns `false` for `open terminal failed: not a terminal`
   - returns `false` for `set-option` failures even if stderr mentions
     `duplicate session`
2. Update assertions that expect `"-Ad"` to expect `"-d"` instead (same two
   tests that currently check `-Ad`).
3. Red command:

```bash
cargo test -p ajax-web --lib adapters::terminal_pty -- --nocapture
```

Expect failures on the new ignore-helper assertion and/or `-Ad` vs `-d`
mismatches before production edits.

## Edit instructions

1. In `build_isolated_attach_plan_with_token`, change setup `new-session` flag
   from `"-Ad"` to `"-d"`. Update the comment: do **not** use `-A` here;
   attach-if-exists requires a TTY and breaks reconnect from
   `run_tmux_command_blocking`.
2. Add a small pure helper, e.g.:

```rust
fn should_ignore_setup_failure(command: &TmuxCommand, stderr: &str) -> bool {
    command.args.first().map(String::as_str) == Some("new-session")
        && stderr.contains("duplicate session")
}
```

3. In `bridge_task_terminal_socket` setup loop: on `Ok(output)` with
   non-success status, if `should_ignore_setup_failure(command, stderr)` then
   `continue`; otherwise keep current error path.
4. In `architecture.md`, replace the `new-session -A` idempotency claim with:
   setup uses detached `new-session -d` and treats an already-present
   ephemeral session (`duplicate session`) as success so reconnect reuses the
   viewport without attaching during setup.

## Verification commands

```bash
cargo test -p ajax-web --lib adapters::terminal_pty
cargo check -p ajax-web
```

## Acceptance criteria

- Setup plan uses `-d`, never `-Ad` / `-A`.
- Duplicate-session on `new-session` does not abort the bridge.
- Other setup failures still abort with `failed to create terminal session: …`.
- Teardown remains empty; destroy/reaper unchanged.
- `architecture.md` no longer claims setup uses `new-session -A`.
- Focused tests pass.

## Stop conditions

- Need to change browser/client-id/reaper paths to make this work.
- Live tmux integration test required beyond unit classification (escalate;
  do not invent a flaky harness in this packet).
- Edits outside Allowed files.
- Treating `not a terminal` as benign success.
