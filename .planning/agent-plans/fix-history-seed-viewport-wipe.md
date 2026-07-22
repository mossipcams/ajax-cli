# Fix history seed wiped by attach clear

## Scope

Stop tmux attach's initial clear/redraw from erasing the newest lines of the
browser history seed. After writing capture-pane history (`-E -1`, viewport
excluded), pad the seed with `rows` CRLFs so those lines sit in scrollback
before live PTY bytes arrive.

## Non-goals

- Do not change capture-pane flags, reconnect `seed=0` policy, CRLF
  normalization of bare LF, or scroll-sync / frontend terminal code.
- Do not drain or reorder the live PTY reader beyond appending the pad to the
  seed frame.
- Do not change architecture ownership.

## Root cause

History seed uses `-S -2000 -E -1` (scrollback only). Writing that into xterm
leaves the newest history rows in the viewport. Attach then sends ED 2J / CUP
redraw, which clears viewport cells in place — those seeded lines are
overwritten. Older scrollback survives; recent history looks like it "ate
itself."

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE: [crates/ajax-web/src/adapters/terminal_pty.rs, .planning/agent-plans/fix-history-seed-viewport-wipe.md]
  REASON: GLM opencode-go weekly limit hit; reroute READY PTY packet once to cursor-delegate composer-2.5.
  ESCALATE_IF: [cursor unavailable, empty diff, edits outside scope]
```

## Checklist

- [x] Task 1 — Failing unit test: seed frame appends `rows` CRLFs after history
- [x] Task 2 — Implement pad using client resize rows (fallback 24)
- [x] Task 3 — Parent review + focused ajax-web validation

## Approval status

Authorized by user bug report ("scrollback history still overwrites itself");
bounded behavior fix, no architecture change.

## Deviations

- First delegate (`pi` / `opencode-go/glm-5.2`) failed: weekly Go usage limit.
  Rerouted once to `cursor-delegate` / `composer-2.5` per model-router.
- Cursor delegate wrote a correct scoped diff but failed the structured report
  envelope (`MISSING_STRUCTURED_REPORT`). Parent reviewed the diff and re-ran
  validation; not trusted on claim alone.

## Validation

Parent-run (accepted):

```bash
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture  # PASS 2
rtk cargo test -p ajax-web terminal_pty -- --nocapture                  # PASS 26
rtk cargo fmt --check                                                     # PASS
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings # PASS
rtk git diff --check                                                      # PASS
rtk cargo nextest run -p ajax-web                                         # PASS 175
```
