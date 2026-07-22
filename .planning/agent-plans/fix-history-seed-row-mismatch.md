# Fix history seed wipe under row mismatch

## Scope

Stop attach `ED 2J` from still erasing newest seeded history when the browser
terminal has more rows than the first resize used for the #648 CRLF pad.

## Non-goals

- Do not change reconnect `seed=0` policy, hostile-sequence filter list, or
  scroll-sync / follow-live behavior.
- Do not drain or reorder the live PTY reader beyond seed framing.
- Do not change architecture ownership.

## Root cause

#648 pads the history-only seed (`-E -1`) with exactly `client_rows` CRLFs from
the *first* resize frame. The bridge then exits the resize-wait loop, sleeps,
captures, and sends the seed. Meanwhile the client can refit to a taller grid
(or skip the first resize when the keyboard is open and `scheduleImmediate()`
uses `discreteIntent=false`). If `term.rows > pad`, the newest seed lines stay
in the viewport and attach clear overwrites them. Symptom matches “scrollback
history still overwritten.”

## Fix (smallest)

1. Capture through the visible pane (`-E -`) so true history sits above the
   wipe zone; attach clear only replaces the seeded screen copy.
2. Pad with `seed_pad_rows(client_rows)` = `max(48, client_rows * 2)` so mild
   post-resize growth still pushes the seed into scrollback (covers short
   history too).
3. On terminal `onOpen`, call `scheduleImmediate(true)` so the first resize is
   not suppressed while the keyboard is open.

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
  ALLOWED_SCOPE:
    - crates/ajax-web/src/adapters/terminal_pty.rs
    - crates/ajax-web/web/src/features/task/TaskTerminal.tsx
    - crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
    - .planning/agent-plans/fix-history-seed-row-mismatch.md
  REASON: GLM weekly Go usage limit; reroute READY PTY packet to cursor-delegate composer-2.5.
  ESCALATE_IF: [cursor unavailable, empty diff, edits outside scope]
```

## Checklist

- [x] Task 1 — Failing tests: capture ends with `-E -`; pad helper doubles rows
- [x] Task 2 — Implement capture + pad + discrete open resize
- [x] Task 3 — Update `TERMINAL_BEHAVIOR_CONTRACT.md` capture flag note
- [x] Task 4 — Parent review + focused validation

## Approval status

Authorized by user bug report (“scroll back history is still being overwritten”);
bounded behavior fix, no architecture change.

## Deviations

- First delegate (`pi` / `opencode-go/glm-5.2`) failed: weekly Go usage limit.
  Rerouted once to `cursor-delegate` / `composer-2.5`.
- Cursor wrote a correct scoped diff but wrapped `DELEGATE_REPORT` in a markdown
  fence (`INVALID_STRUCTURED_REPORT`). Parent reviewed the diff and re-ran
  validation; not trusted on claim alone.
- Parent rebuilt `crates/ajax-web/web/dist/*` after accepting so the
  `scheduleImmediate(true)` client change is baked into `include_bytes` assets
  (packet forbade the delegate from touching dist).
- **Superseded:** user flagged UX regression (blank scroll gap from 2× pad,
  duplicate viewport from `-E -`). Replaced by
  `.planning/agent-plans/fix-history-seed-settle-rows.md` (settle debounce +
  exact pad + `-E -1`).
- **Superseded** by `fix-history-seed-settle-rows.md`: `-E -` + doubled pad
  worsened scroll-up UX (blank band + duplicate viewport). Replaced with 150ms
  resize settle debounce, exact `client_rows` pad, and restored `-E -1`.

## Validation

Parent-run (accepted):

```bash
rtk cargo test -p ajax-web seed_pad_rows_doubles_client_rows_with_floor_of_48 -- --nocapture  # PASS 1
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture  # PASS 1
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture  # PASS 2
rtk cargo test -p ajax-web terminal_pty -- --nocapture  # PASS 27
rtk cargo fmt --check  # PASS
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings  # PASS
rtk git diff --check  # PASS
rtk cargo nextest run -p ajax-web --lib  # PASS 176
npm run web:build  # PASS
tmux capture-pane -E -  # PASS (local smoke)
```
