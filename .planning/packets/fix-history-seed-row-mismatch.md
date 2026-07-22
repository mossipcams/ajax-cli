PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Keep seeded tmux history out of the browser viewport wipe zone when attach
sends `ED 2J`, even if `term.rows` grows after the bridge’s first resize.
Capture through the visible pane and pad with `max(48, client_rows * 2)`.
Also send the first client resize with `discreteIntent=true` on open.

## Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `.planning/agent-plans/fix-history-seed-row-mismatch.md`

## Forbidden changes

- Do not change `seed=0` reconnect policy, hostile-sequence filter targets,
  live `output_frame_bytes`, scroll-sync follow behavior, or dist bundles.
- Do not add dependencies, commit, push, or switch branches.
- Do not drain/reorder the PTY reader beyond seed framing helpers.

## Context evidence

- Desired behavior: after history seed + attach clear, every captured history
  line that is not the live screen copy must remain in xterm scrollback.
- Root cause: #648 pads with exactly `client_rows` from the first resize while
  capture uses `-E -1`. If the client grid is taller when the seed is written,
  newest history sits in the viewport and `ED 2J` overwrites it. Keyboard-open
  `scheduleImmediate()` (default `discreteIntent=false`) can also skip the
  first resize entirely (`TaskTerminal.tsx` `sendResizeNow` / `scheduleFit`).
- Production anchors: history args at `terminal_pty.rs` `build_isolated_attach_plan_with_token`
  (`-E`, `-1`); pad helper `captured_history_frame_bytes(bytes, rows)`; seed
  send uses `client_rows`; open path `onOpen` → `scheduleImmediate()`.
- Pattern to reuse: existing pad unit test
  `captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback`
  and plan assert in `isolated_attach_plan_seeds_browser_scrollback_from_task_window`.
- Architecture: ajax-web PTY adapter owns seed framing; React terminal owns
  resize dial timing. No ownership change.

## Code anchors

- `terminal_pty.rs`: change history capture end from `"-1"` to `"-"`.
- Add `fn seed_pad_rows(client_rows: u16) -> u16` returning
  `client_rows.saturating_mul(2).max(48)` (use `u16::max` / `Ord::max`).
- `captured_history_frame_bytes` call site: pass `seed_pad_rows(client_rows)`
  instead of raw `client_rows`. Keep helper’s “append exactly `rows` CRLFs”
  semantics.
- `TaskTerminal.tsx` `onOpen` callback: change `scheduleImmediate()` to
  `scheduleImmediate(true)`.
- `TERMINAL_BEHAVIOR_CONTRACT.md`: update the capture-pane row that documents
  `-E -1` to `-E -`, and note the doubled pad briefly if that table mentions
  pad behavior (only if already present; do not invent a new table row unless
  needed for the flag change).

## Test-first instructions

1. Update
   `isolated_attach_plan_seeds_browser_scrollback_from_task_window` so the
   expected args end with `"-E", "-"` (not `"-1"`).

2. Add
   `seed_pad_rows_doubles_client_rows_with_floor_of_48` asserting:
   - `seed_pad_rows(24) == 48`
   - `seed_pad_rows(30) == 60`
   - `seed_pad_rows(0) == 48`

3. Keep existing
   `captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback`
   green (still tests raw pad count passed in).

RED command:

```bash
rtk cargo test -p ajax-web seed_pad_rows_doubles_client_rows_with_floor_of_48 -- --nocapture
```

Expected RED: missing `seed_pad_rows` (or wrong value). Also expect
`isolated_attach_plan_seeds_browser_scrollback_from_task_window` to fail on
`-1` until the capture flag changes — run it in RED if the pad test alone is
insufficient to force the flag edit:

```bash
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
```

No new frontend unit test is required for the one-argument
`scheduleImmediate(true)` call; contract doc + Rust tests cover the behavior
change. Do not add Playwright coverage in this packet.

## Edit instructions

1. RED: add/update the Rust tests above; confirm RED.
2. Implement `seed_pad_rows` and use it at the seed send site.
3. Change capture-pane `-E` argument from `-1` to `-`.
4. Change `onOpen`’s `scheduleImmediate()` to `scheduleImmediate(true)`.
5. Update `TERMINAL_BEHAVIOR_CONTRACT.md` capture flag text from `-E -1` to
   `-E -`.
6. Check off tasks in
   `.planning/agent-plans/fix-history-seed-row-mismatch.md` as you go.
7. GREEN + verification commands.

## Verification commands

```bash
rtk cargo test -p ajax-web seed_pad_rows_doubles_client_rows_with_floor_of_48 -- --nocapture
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
rtk cargo fmt --check
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
rtk git diff --check
```

## Acceptance criteria

- History capture plan ends with `-E -`.
- Seed frames pad with `seed_pad_rows(client_rows)` CRLF pairs.
- Open path requests an immediate discrete resize (`scheduleImmediate(true)`).
- Contract doc matches the new capture end flag.
- Focused tests, `terminal_pty` module tests, fmt, and clippy pass.
- Diff touches only allowed files.

## Stop conditions

- Stop if a correct fix requires changing hostile filters, reader ordering, or
  frontend scroll-sync compensation.
- Stop on edits outside Allowed files or unrelated test failures.
- Stop if tmux on the machine rejects `-E -` (report evidence; do not guess an
  alternate flag).
