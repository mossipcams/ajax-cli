# Rethink history seed: settle rows, no UX regression

## Scope

Replace the UX-worsening `-E -` + doubled pad approach with a settle-window
resize debounce so pad stays exactly `client_rows` and capture stays
history-only (`-E -1`). Keep `scheduleImmediate(true)` on open.

## Non-goals

- Do not add seed-end protocol frames or client-side pad.
- Do not change `seed=0` policy, hostile filters, or scroll-sync.
- Do not keep blank-gap (2× pad) or duplicate-screen (`-E -`) behavior.

## Why rethink

IWDP + review showed the prior patch fixed wipe risk but hurt scroll-up UX:
doubled pad leaves ~1 screen of blanks in scrollback; `-E -` leaves a
near-duplicate viewport. Root race remains: pad from *first* resize while the
client grid can still grow.

## New approach

1. Revert capture end to `-E -1`.
2. Remove `seed_pad_rows` / 2× floor; pad with exact settled `client_rows`.
3. After the first resize, keep reading client frames until **150ms quiet**
   (or the existing 500ms overall wait elapses). On each resize, apply to PTY
   and set `client_rows = max(client_rows, size.rows)`.
4. Then keep the existing 100ms reflow sleep + seed.
5. Keep `scheduleImmediate(true)` on open.

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  REASON: GLM weekly limit; PTY settle rethink via cursor-delegate.
```

## Checklist

- [x] Task 1 — Failing tests for settle debounce + reverted capture/pad
- [x] Task 2 — Implement settle loop; revert `-E -` and doubled pad
- [x] Task 3 — Contract doc matches
- [x] Task 4 — Parent review + validation

## Approval status

Authorized by user: rethink because UX got worse.

## Deviations

- Cursor report envelope failed schema validation again; parent reviewed diff
  and re-ran validation.

## Validation

Parent-run (accepted):

```bash
rtk cargo test -p ajax-web resize_settle_deadline -- --nocapture  # PASS 1
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture  # PASS 1
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture  # PASS 2
rtk cargo test -p ajax-web terminal_pty -- --nocapture  # PASS 27
rtk cargo fmt --check  # PASS
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings  # PASS
rtk git diff --check  # PASS
rtk cargo nextest run -p ajax-web --lib  # PASS 176
```
