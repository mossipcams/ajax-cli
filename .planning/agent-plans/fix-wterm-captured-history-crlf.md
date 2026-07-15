# Fix Wterm captured-history staircase

## Scope

Normalize only the textual output of `tmux capture-pane` from LF line endings
to terminal CRLF before it is sent as the initial history seed. This fixes the
staircase layout in Wterm's standards-correct built-in core.

## Non-goals

- Do not rewrite live PTY output, WebSocket frames, or frontend terminal writes.
- Do not change normal Ghostty, the Surface V2 flag, grid geometry, tmux
  ownership, or scrollback filtering.
- Do not add a general stream normalizer or dependency.

## Evidence

- `tmux capture-pane -p` is executed through `std::process::Command` and its
  stdout is sent directly as the pre-attach history frame.
- Wterm built-in core renders `one\ntwo\nthree` at columns 0, 3, and 6, while
  `one\r\ntwo\r\nthree` renders every line at column 0.
- The live PTY reader and connection decoder pass bytes through unchanged; they
  are not the defective boundary.

## Decision and approval

- Delegation decision: delegated via model-router after approval; backend PTY
  work routes to the GLM implementation lane with a complete TDD packet.
- Approval status: approved by user (`implement until finished`).

## Task 1 — Normalize the captured history frame (complete)

- Test: add a focused unit test in
  `crates/ajax-web/src/adapters/terminal_pty.rs` proving captured bare LF becomes
  CRLF and an existing CRLF is not doubled. Run it first and capture the
  expected failure.
- Implementation: add one private byte-level helper at the history-frame
  boundary and call it only for `isolated.history` stdout. Preserve empty-frame
  behavior and all escape/text bytes.
- Verify: rerun the focused test, then `cargo nextest run -p ajax-web`.

## Task 2 — Validate and open a follow-up PR (complete)

- Test: no new test; Task 1 is the behavior contract.
- Implementation: no additional product code. Review the scoped diff, update
  this ledger, regenerate nothing because web assets are unchanged, then commit
  and push the fix on a branch based on current `origin/main`.
- Verify: `cargo fmt --check`, `cargo check -p ajax-web --all-targets`,
  `git diff --check`, and GitHub PR checks reach green/mergeable.

## Validation ledger

- RED: delegate ran
  `cargo test -p ajax-web captured_history_frame_bytes_converts_lf_to_crlf_without_doubling_crlf`;
  it exited 101 because the helper did not exist.
- GREEN: the same focused test passed (1 passed, 131 filtered out).
- Parent validation: `cargo nextest run -p ajax-web` passed 132 tests.
- Parent validation: `cargo fmt --check` passed.
- Parent validation: `cargo check -p ajax-web --all-targets` passed.
- Parent validation: `git diff --check` passed.
- PR #495 merged while this fix was in progress. The follow-up PR's post-push
  checks are observed externally and reported in the final handoff so recording
  the result does not create an additional CI-only commit.

## Deviations

- Cargo tooling changed the workspace package version in `Cargo.lock` from
  0.42.12 to 0.43.0 during validation. That unrelated generated drift was
  reverted and is not part of the fix.
