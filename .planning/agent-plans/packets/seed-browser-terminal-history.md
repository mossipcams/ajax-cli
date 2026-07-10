# Seed browser terminal history

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Before forwarding the live tmux attach stream, seed the browser terminal with
up to 2,000 lines already present in the task pane's tmux history. This makes
the top of Ghostty's local scrollback match the available pre-connection tmux
history instead of stopping at the WebSocket-open boundary.

Keep the scrollbar hidden and preserve the existing keyboard-open snap.

## 3. Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte` only to remove
  the superseded uncommitted `touchScrollOpenedKeyboard` changes
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` only to remove
  the superseded uncommitted no-snap regression test
- `.planning/agent-plans/fix-inline-terminal-scrollback.md`

## 4. Forbidden changes

- Do not expose Ghostty's scrollbar; retain `scrollbarWidth: 0` and its test.
- Do not change keyboard-open snapping, touch gestures, fullscreen behavior,
  terminal geometry, scrollback caps, tmux window sizing, or reconnect reset.
- Do not change `SCROLLBACK_HOSTILE_SEQUENCES` or the raw terminal model.
- Do not add a dependency, commit, push, branch, rebase, or edit generated
  `dist` assets.
- Do not alter unrelated pre-existing worktree content.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — no project graph is present, and the complete
  behavior boundary is confined to the authoritative `terminal_pty` adapter
  plus its in-module tests; `architecture.md` confirms tmux owns durable
  interactive sessions and `ajax-web` owns the browser terminal adapter.
- Serena: `NOT_REQUIRED` — the exact symbols, callers, and tests are all in one
  Rust module; semantic-index output would not add a boundary or reusable
  pattern beyond the inspected source.
- ast-grep: `bridge_task_terminal_socket` has one definition/call path in the
  adapter; `build_isolated_attach_plan` is consumed by that bridge and its
  in-module tests; every `run_tmux_command_blocking` call is in
  `terminal_pty.rs`. No sibling history-seeding implementation exists.
- Existing project plan evidence:
  `.planning/agent-plans/web-terminal-scroll-yank-and-opencode.md` records the
  limitation verbatim: browser scrollback only contains output streamed during
  the current WebSocket, and names `tmux capture-pane -p -e -J -S -<N> -E -1`
  as the follow-up.
- Installed Ghostty evidence: `scrollLines` clamps to
  `getScrollbackLength()`, so frontend gesture changes cannot reveal history
  that was never sent to Ghostty.

## 6. Code anchors

- `IsolatedAttachPlan` in `terminal_pty.rs`: add one explicit
  `history: TmuxCommand` plan alongside `setup`, `attach`, and `teardown`.
- `build_isolated_attach_plan_with_token`: build the history command against
  `<ephemeral-session>:<task-window>` with exact args:
  `capture-pane -p -e -J -t <target> -S -2000 -E -1`.
- `bridge_task_terminal_socket`: after the isolated session and PTY-backed
  attach child exist but before the PTY reader is forwarded to the socket, run
  the planned capture. On successful non-empty stdout, send it as one binary
  WebSocket message before live PTY bytes. Capture failure or empty history is
  best-effort and must not block live attach.
- Existing test anchor:
  `isolated_attach_plan_creates_grouped_session_then_attaches` in the same
  module. Prefer a separate focused history-plan test so the existing grouped
  session assertion stays readable.
- Superseded frontend anchors are visible only in the current uncommitted diff:
  `touchScrollOpenedKeyboard` and
  `does not yank inline scrollback down when touch focus opens the keyboard`.

## 7. Test-first instructions

Add
`isolated_attach_plan_seeds_browser_scrollback_from_task_window` to the
in-module Rust tests. Assert the planned command program is `tmux`, the exact
args are the capture command above, the target is the ephemeral task window,
and neither browser handle nor shared-session target is used.

Run RED before production edits:

```bash
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
```

The intended failure is a missing `history` field/plan (compile failure or
failed exact command assertion), not an unrelated environment failure.

## 8. Edit instructions

1. Add the tested history command to `IsolatedAttachPlan` and its builder.
2. In `bridge_task_terminal_socket`, capture after spawning the isolated tmux
   attach so live bytes can queue in the PTY, but send captured stdout before
   starting/forwarding the PTY reader. This preserves capture-before-live
   ordering without losing output produced during capture.
3. Treat capture command failure and empty stdout as no seed; continue the live
   terminal. If sending a non-empty seed fails, clean up the spawned child and
   ephemeral grouped session before returning.
4. Remove only the superseded uncommitted no-snap flag/condition changes and
   their newly added contradictory test. Restore the original keyboard-open
   block exactly; leave existing snap and hidden-scrollbar tests unchanged.
5. Update the persistent plan with RED/GREEN evidence and any deviation.

## 9. Verification commands

```bash
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
rtk npm run web:test -- --run TerminalRawView.test.ts
rtk npm run web:check
rtk cargo fmt --check
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
rtk git diff --check
```

## 10. Acceptance criteria

- A new browser terminal receives up to 2,000 existing tmux history lines
  before live attach output.
- Capture targets the isolated grouped session's task window, never the browser
  handle.
- Capture failure does not prevent live terminal attachment.
- Failed seed delivery does not leak the child or ephemeral grouped session.
- The hidden scrollbar and keyboard-open snap remain exactly as before.
- Focused Rust tests, the full terminal component suite, type/Svelte checks,
  formatting, clippy, and diff checks pass.

## 11. Stop conditions

- Stop if tmux in the supported environment rejects `-p -e -J -S -2000 -E -1`.
- Stop if preserving capture-before-live ordering requires changing the
  WebSocket protocol or frontend terminal model.
- Stop if cleanup cannot be guaranteed on seed-send failure within
  `terminal_pty.rs`.
- Stop on edits outside Allowed files, unrelated test failures, or a need to
  change shared tmux session/window sizing.
