PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Fix attach wipe of seeded history without blank scrollback gaps or a duplicate
viewport copy. Revert `-E -` and `seed_pad_rows` doubling. After the first
client resize, debounce further resizes for 150ms quiet (within the existing
500ms overall wait), keep `client_rows = max(rows seen)`, pad with exactly
that count, capture with `-E -1`. Keep `scheduleImmediate(true)`.

## Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `.planning/agent-plans/fix-history-seed-settle-rows.md`
- `.planning/agent-plans/fix-history-seed-row-mismatch.md`

## Forbidden changes

- Do not change `TaskTerminal.tsx` unless required to keep
  `scheduleImmediate(true)` (already present — leave it).
- Do not change dist, seed=0 policy, hostile filters, or scroll-sync.
- Do not reintroduce `seed_pad_rows` doubling or capture `-E -`.
- Do not add dependencies, commit, push, or switch branches.

## Context evidence

- Desired UX: scroll-up shows contiguous seeded history then live screen — no
  empty band, no duplicate connect-time viewport.
- Desired correctness: pad rows must be ≥ final xterm rows at seed write; race
  was exiting resize-wait on the *first* resize (`terminal_pty.rs` pre-seed
  loop `break` on `FrameOutcome::Resize`).
- Prior patch (row-mismatch) worsened UX via `-E -` + `max(48, 2*rows)` pad;
  IWDP on :8788 showed pad/rows race and confirmed installed binary lacked the
  patch — still rethink before shipping the bad UX.
- Pattern: existing `remaining_resize_wait` + 100ms reflow sleep; extend with a
  settle quiet window constant and pure deadline helper for tests.

## Code anchors

- `RESIZE_WAIT_TIMEOUT` at `terminal_pty.rs` (~500ms). Add
  `RESIZE_SETTLE_QUIET: Duration = Duration::from_millis(150)`.
- Add pure helper e.g. `fn resize_settle_deadline(last_resize_at: Instant, now: Instant) -> Option<Duration>`
  returning remaining quiet time or `None` when settle elapsed (mirror
  `remaining_resize_wait` style).
- Pre-seed loop in `bridge_task_terminal_socket`: on Resize, apply PTY resize,
  `client_rows = client_rows.max(size.rows)` when `size.rows > 0`, set
  `resize_applied = true`, **do not break**; instead arm/reset settle deadline
  from `Instant::now()`. Loop wait should be `min(overall remaining, settle
  remaining)` when settle is armed; when settle fires (`None`), break out to
  seed path. Overall 500ms timeout still aborts wait and proceeds.
- History args: `"-E", "-1"` again (revert `"-"`).
- Delete `seed_pad_rows`. Call
  `captured_history_frame_bytes(output.stdout, client_rows)`.
- Remove test `seed_pad_rows_doubles_client_rows_with_floor_of_48`.
- Restore
  `isolated_attach_plan_seeds_browser_scrollback_from_task_window` expected
  `"-E","-1"`.
- Add unit tests for the settle deadline helper.
- `TERMINAL_BEHAVIOR_CONTRACT.md`: capture line back to `-E -1`; document
  settle quiet + exact `client_rows` pad (replace doubled-pad wording).
- Check off tasks in `fix-history-seed-settle-rows.md`. Note in
  `fix-history-seed-row-mismatch.md` deviations that it was superseded.

## Test-first instructions

1. Restore capture assert to `"-E", "-1"` in
   `isolated_attach_plan_seeds_browser_scrollback_from_task_window` — RED if
   still `"-"`.

2. Delete / stop depending on `seed_pad_rows_*` test; if the symbol remains,
   RED compile until removed.

3. Add `resize_settle_deadline_returns_remaining_until_quiet_elapsed`:
   - `last = t0`, `now = t0 + 0ms` → `Some` ≈ 150ms
   - `now = t0 + 149ms` → `Some` small positive
   - `now = t0 + 150ms` → `None`
   - `now = t0 + 200ms` → `None`

RED:

```bash
rtk cargo test -p ajax-web resize_settle_deadline_returns_remaining_until_quiet_elapsed -- --nocapture
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
```

## Edit instructions

1. RED tests as above.
2. Implement settle helper + pre-seed loop debounce; revert capture and pad.
3. Update contract + both plan files (supersede note).
4. GREEN + verification.

## Verification commands

```bash
rtk cargo test -p ajax-web resize_settle_deadline -- --nocapture
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
rtk cargo fmt --check
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
rtk git diff --check
```

## Acceptance criteria

- Capture plan ends with `-E -1`.
- No `seed_pad_rows`; seed uses exact settled `client_rows`.
- First resize no longer immediately ends the wait; quiet settle applies.
- `scheduleImmediate(true)` remains in TaskTerminal (unchanged this round).
- Contract matches; focused tests / fmt / clippy pass; allowed files only.

## Stop conditions

- Stop if settle alone cannot fix wipe without pad>rows or `-E -` (report).
- Stop on edits outside Allowed files or unrelated failures.
- Stop if loop changes would reorder live PTY reader vs seed send.
