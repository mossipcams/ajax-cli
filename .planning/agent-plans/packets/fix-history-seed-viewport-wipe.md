PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Prevent tmux attach's viewport clear/redraw from erasing the newest lines of
the capture-pane history seed. After CRLF-normalizing captured history, append
exactly `rows` CRLF pairs (from the client's first resize, else 24) so the
seed sits entirely in scrollback before live PTY bytes arrive.

## Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `.planning/agent-plans/fix-history-seed-viewport-wipe.md`

## Forbidden changes

- Do not change `capture-pane` args (`-S -2000 -E -1`, no `-J`).
- Do not change `seed_history_from_query`, reconnect `seed=0` client policy,
  hostile-sequence filter, live `output_frame_bytes`, frontend, or dist.
- Do not add dependencies, commit, push, or switch branches.
- Do not drain or reorder the PTY reader beyond padding the seed frame.

## Context evidence

- Desired behavior: history-only seed (`-E -1`) must survive attach ED 2J /
  CUP redraw. Newest seeded lines currently occupy the viewport and are wiped
  in place; pad with `rows` CRLFs first.
- Production anchor: `captured_history_frame_bytes` at
  `terminal_pty.rs:440-451`; seed send at `terminal_pty.rs:631-644`; resize
  applied at `terminal_pty.rs:573-576` with initial PTY `rows: 24` at
  `terminal_pty.rs:512-514`.
- Existing pattern: CRLF helper already wraps `output_frame_bytes`; extend it
  (or a thin caller) to take `rows` and append `\r\n` × rows. Keep empty
  history as `None`.
- Architecture: ajax-web PTY adapter owns seed framing; no ownership change.

## Code anchors

- Rename or extend `captured_history_frame_bytes(bytes) -> Option<Vec<u8>>` to
  `captured_history_frame_bytes(bytes, rows: u16) -> Option<Vec<u8>>` (or add
  a wrapper called only at the seed send site). After LF→CRLF normalization,
  if the normalized payload is non-empty, append `rows` copies of `\r\n`.
  `rows == 0` must still avoid panics; treat as no pad (or clamp to at least
  the openpty default 24 at the call site — prefer clamping at the call site
  so the helper stays pure: append exactly `rows` CRLFs).
- In `bridge_task_terminal_socket`, track `client_rows: u16` defaulting to
  `24`. On `FrameOutcome::Resize(size)` in the pre-seed wait loop, set
  `client_rows = size.rows` when applying the resize. Pass `client_rows` into
  the history-frame helper at the seed send site.
- Update
  `captured_history_frame_bytes_converts_lf_to_crlf_without_doubling_crlf` to
  pass a rows argument (use `0` so existing byte assertions stay stable), and
  add a new test for the pad.

## Test-first instructions

Add
`captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback`
in the `terminal_pty` tests module.

Assert:

- `captured_history_frame_bytes(b"a\nb".to_vec(), 3)` is
  `Some(b"a\r\nb\r\n\r\n\r\n")` — note: bare LF after `b` becomes CRLF, then
  three pad CRLFs. Input `b"a\nb"` has no trailing LF on `b`, so normalized
  form is `a\r\nb` then three `\r\n` → `a\r\nb\r\n\r\n\r\n`.
- `captured_history_frame_bytes(b"x\n".to_vec(), 2)` ends with exactly two
  pad CRLFs after the normalized `x\r\n`.
- `captured_history_frame_bytes(Vec::new(), 8)` is `None`.
- Existing CRLF test still passes with `rows: 0` (no pad).

RED command:

```bash
rtk cargo test -p ajax-web captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback -- --nocapture
```

Expected RED: missing symbol / wrong arity / failed assertion, not unrelated
compile errors in other crates.

## Edit instructions

1. Change `captured_history_frame_bytes` to accept `rows: u16`. After building
   the CRLF-normalized buffer, if it is non-empty, append `rows` times
   `b"\r\n"`, then pass through `output_frame_bytes`.
2. In `bridge_task_terminal_socket`, introduce `let mut client_rows: u16 = 24;`
   Before the pre-seed loop. On each pre-seed `FrameOutcome::Resize(size)`,
   set `client_rows = size.rows` when `size.rows > 0` (resize handler already
   requires `rows > 0`).
3. Call `captured_history_frame_bytes(output.stdout, client_rows)` at the seed
   send site.
4. Update the existing CRLF unit test to pass `0` for rows.
5. Add the new pad unit test from Test-first instructions.
6. Check off tasks in
   `.planning/agent-plans/fix-history-seed-viewport-wipe.md`.

## Verification commands

```bash
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
rtk cargo fmt --check
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
rtk git diff --check
```

## Acceptance criteria

- Non-empty history seed frames end with exactly `client_rows` CRLF pairs
  after the normalized capture text.
- Empty capture still yields no seed frame.
- Bare-LF normalization behavior is unchanged aside from the trailing pad.
- Resize before seed uses the client's row count; missing resize keeps 24.
- Focused and module tests, fmt, and clippy pass.
- Diff touches only allowed files.

## Stop conditions

- Stop if fixing the wipe requires changing capture-pane flags, frontend
  reset/write behavior, or draining the PTY reader.
- Stop on edits outside Allowed files or unrelated test failures.
- Stop if xterm semantics appear to need a different pad (e.g. `rows-1`);
  report evidence rather than guessing.
