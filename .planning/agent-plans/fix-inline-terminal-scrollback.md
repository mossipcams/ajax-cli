# Seed inline terminal scrollback

## Scope

Seed the browser terminal with pre-connection tmux history so scrolling upward
does not stop at the WebSocket-open boundary while older pane history exists.
Keep the scrollbar hidden and preserve the intentional keyboard-open snap.

## Non-goals

- Do not change touch-to-keyboard focus, native Paste support, the hidden
  scrollbar, or the keyboard-open snap.
- Do not change fullscreen, tmux, terminal sizing, gesture direction, or
  scrollback limits.
- Do not add dependencies or abstractions.

## Delegation decision

`Delegation decision: model-router selected opencode-delegate / GLM 5.2 because
this is backend tmux/session/PTY work, but that delegate skill is unavailable
in this environment. Implementation is blocked unless the user explicitly
authorizes local Codex execution.`

```yaml
ROUTING_DECISION:
  ACTION: STOP
  LANE: opencode-delegate
  MODE: implement
  MODEL: opencode-go/glm-5.2
  PACKET_STATUS: READY
  ALLOWED_SCOPE: [crates/ajax-web/src/adapters/terminal_pty.rs, crates/ajax-web/web/src/components/TerminalRawView.svelte, crates/ajax-web/web/src/components/TerminalRawView.test.ts, .planning/agent-plans/fix-inline-terminal-scrollback.md]
  REASON: The required backend tmux/session lane is unavailable, and repo rules prohibit silently taking over locally.
  ESCALATE_IF: [The user explicitly authorizes local Codex execution, the opencode-delegate skill becomes available, or the task expands beyond the READY packet]
```

## Task checklist

- [x] Task 1 — Superseded diagnosis: prevent inline keyboard-open snap
  - The user clarified that the snap is intentional; this task's uncommitted
    implementation and newly added contradictory test must be removed.
- [x] Task 2 — Superseded diagnosis: expose a scrollbar
  - The user clarified that the bar should remain hidden; the problem is the
    premature local-history boundary.
- [x] Task 3 — Seed pre-connection tmux history
  - Test: add a failing Rust command-plan test for capture-pane history from
    the isolated task window.
  - Implementation: follow the READY packet at
    `.planning/agent-plans/packets/seed-browser-terminal-history.md`; also
    remove Task 1's uncommitted wrong fix/test and restore snap unchanged.
  - Verification: focused adapter tests, full terminal component tests,
    web check, fmt, ajax-web clippy, and diff check.

## Approval status

Approved by user with an explicit instruction to implement locally until
finished; all checklist items are complete.

## Deviations

- The first one-condition implementation also suppressed the intentional snap
  when the keyboard opens after an older non-touch scroll. The full component
  suite caught this existing contract, so the implementation was narrowed to
  remember only the touch scroll that opened the keyboard.
- The user then clarified that all keyboard-open snapping is desired and that
  snapping was not the bug. Task 1 is superseded.
- The user clarified that the scrollbar must remain hidden. Source and an
  earlier project plan confirm the real boundary: Ghostty never receives tmux
  history that predates the WebSocket. Task 2 is superseded.

## Validation results

- Initial command from the web subdirectory failed before running tests because
  the package manifest is repo-rooted.
- Correct repo-root test command initially failed because npm dependencies were
  absent (`vitest: command not found`); `npm ci` completed successfully.
- RED: focused regression test failed with one unexpected
  `scrollToBottom` call.
- GREEN: focused regression test passed after the initial implementation.
- Full `TerminalRawView.test.ts`: failed 1/142 because the initial
  implementation broke the intentional non-touch keyboard-open snap; corrective
  implementation added without changing the existing assertion.
- Focused regression plus existing keyboard-snap contract: passed 2/2.
- Full `TerminalRawView.test.ts`: passed 142/142.
- `npm run web:check`: passed with 0 errors and 0 warnings after the final edit.
- Task 3 RED: `cargo test -p ajax-web
  isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture`
  failed with three `E0609` errors because `IsolatedAttachPlan.history` did not
  exist.
- Task 3 GREEN: the same focused command passed 1/1.
- `cargo test -p ajax-web terminal_pty -- --nocapture`: passed 16/16.
- `npm run web:test -- --run TerminalRawView.test.ts`: passed 141/141,
  confirming hidden-scrollbar and snap behavior remain intact.
- `npm run web:check`: passed with 0 errors and 0 warnings.
- `cargo fmt --check`: passed.
- `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings`:
  passed with no issues.
- Disposable tmux smoke test accepted the exact capture flags and returned
  2,000 history lines; the isolated tmux server was then removed.
- `cargo test -p ajax-web`: passed 127/127 across two suites.
- `git diff --check`: passed.
