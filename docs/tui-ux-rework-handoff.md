# TUI UX Rework — Session Handoff

Source plan: `docs/tui-ux-rework-plan.md`.

## Status — COMPLETE

| Phase | State |
|------|------|
| 1 — Drop brand, extract counts strip | done |
| 2 — Persistent attention line | done |
| 3 — Notice row + severity | done |
| 4 — Realtime correctness for notices | done |
| 5 — Feed polish (section headers, chevron prefix, pinned task summary) | done |

All workspace tests pass (817 / 817). Validation sweep clean:

- `cargo fmt --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `cargo nextest run`
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`

Changes are unstaged in the working tree.

## Phase 5 closeout — this session

### Task 18 — chevron prefix on selected row

`render_row` and `render_selectable` now take an `is_selected: bool`. Selected
rows render `" > "` (ASCII, not `▸`, to honor
`cockpit_render_uses_ascii_chrome_for_tmux_copy`) in `primary_accent()` bold.
Non-selected rows keep the 3-space pad so glyph columns stay aligned.

The failing test inherited from the prior session
(`selected_row_renders_chevron_prefix`) was also strengthened: it now finds
the inbox feed row by matching both `"web/fix-login"` and `"open task"`
(the recommended-action label is unique to the inbox feed row), instead of
the first chunk that contained the handle — which was the attention line
and could never satisfy the chevron assertion.

### Task 17 — pinned TaskActions summary row

`build_feed` now prepends a non-selectable summary row for `AppView::TaskActions`
between the leading blank and the action list. The summary renders the task
glyph + qualified handle + status label + title. It is **not** in `sel_to_row`,
so selectable indices still line up with `app.selectables`. `selectable_row_layout`
sees the summary as a non-selectable row above the first action, so
`selectables[0].start` shifts from 1 to 2 in TaskActions; mouse-click tests
already operate through `selectable_row_layout` and remain green.

Test: `task_actions_view_pins_summary_row_above_action_list`.

### Validation-sweep cleanups (behavior-neutral)

`cargo clippy --all-targets -- -D warnings` flagged five preexisting issues
introduced earlier in the rework:

- `cockpit_state.rs:463` — replaced `match` with `!matches!(..., ... if ...)`.
- `cockpit_state.rs::notify_task` — merged two identical `if` arms with `||`.
- `cockpit_state.rs::notify_system` — same merge.
- `lib.rs::flash_expires_after_final_visible_tick` — replaced
  `assert!(FLASH_TICKS > 0)` (always-true on a const) with
  `assert_ne!(FLASH_TICKS, 0)`, preserving the back-compat intent.
- `lib.rs::error_notice_decays_over_error_lifetime` — replaced
  `app.notices.get(&task_id).is_none()` with `!app.notices.contains_key(...)`.

## Files modified (unstaged)

- `crates/ajax-tui/src/cockpit_state.rs` (Phase 4 prune helpers + clippy fixes)
- `crates/ajax-tui/src/input.rs`
- `crates/ajax-tui/src/lib.rs` (notice row, section headers, chevron prefix,
  pinned summary, Phase 3–5 tests, clippy fixes)
- `crates/ajax-tui/src/rendering.rs`
- `crates/ajax-tui/src/runtime.rs`
- `CLAUDE.md` (added earlier this rework, untracked)

## Next session

This rework is closed. The doc is kept for cross-reference until the change
lands; safe to delete after commit.
