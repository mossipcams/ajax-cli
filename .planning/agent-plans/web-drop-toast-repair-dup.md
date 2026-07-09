# Plan: web drop-toast + repair button + text duplication

Three reported web issues.

## Scope

1. **Toast (#2)** — result/drop toast persists ~12s and pushes layout down.
   Make it an overlay (out of flow) and cap success toasts at 4s.
2. **Repair button (#1)** — no Repair action when a worktree is missing.
3. **Text duplication (#3)** — zero-lag echo overlay lingers → duplicated text.

## Non-goals

- Rewriting the terminal echo/local-echo model.
- Changing the repair *plan* (already recreates missing worktree when branch
  exists — see `op-recover-missing-worktree.md`).

## Diagnosis

- **#2**: `ResultPanel` is the first child of the sticky `.cockpit-chrome`, so it
  is in-flow and shifts content. `RESULT_AUTO_DISMISS_MS = 12000` for all
  results.
- **#1**: root cause in `ajax-core::recommended::available_operator_actions`.
  `task_is_known_invalid(task)` returns `[Drop]` for any confirmed missing
  substrate (worktree/tmux/task-window/branch), shadowing the Repair branch
  below. The repair *plan* can now recreate a missing worktree when the branch
  exists, but the action is never surfaced. Web already supports the Repair
  action (`slices/actions.rs`); it just never appears.
- **#3**: `createZeroLagEcho` only clears the overlay on exact echo match
  (`clearIfEchoedIn`) or Enter. When the PTY echo arrives wrapped in escape
  sequences (TUI redraw, bracketed paste, readline repaint), neither branch
  matches and the overlay text lingers on top of the real terminal glyphs =
  visible duplication.

## Delegation decision

`Delegation decision: not delegated` — #2 is a Small Fix (CSS + constant +
DOM move); #1/#3 pending user approval before core/terminal edits.

## Task checklist

### Task 1: Toast overlay + 4s (#2)  [Small Fix]  ✅ DONE
- [x] Test: ResultPanel auto-dismiss uses 4s for success, 12s for error.
- [x] `polling.ts`: add `RESULT_SUCCESS_DISMISS_MS = 4000`.
- [x] `ResultPanel.svelte`: pick duration by `isError`.
- [x] `App.svelte`: move ResultPanel out of `.cockpit-chrome` to an
      AppViewport-level overlay sibling.
- [x] `styles.css`: `.result-panel` fixed top-center overlay.
- [x] Rebuilt `dist/` bundle.
- [x] Verify: vitest 431 passed; asset snapshot tests 32 passed.

### Task 2: Surface Repair for repairable worktree-missing (#1)  [Behavior Change]  ✅ DONE
- [x] Tests in `recommended.rs`: worktree-missing + branch exists → actions
      include Repair; branch-missing → Drop only.
- [x] `available_operator_actions`: `worktree_repairable` guard → returns
      `[Repair, Drop]` for git-substrate-missing with intact branch.
- [x] Updated `cockpit.rs` test expectation (drop→repair primary + drop present).
- [x] Verify: ajax-core 758, ajax-web+cli 459, ajax-tui 205 — all pass.

### Task 3: Zero-lag overlay backstops (#3)  [attempt #6 — user-approved]  ✅ DONE (pending device verify)
- User confirmed: duplication happens everywhere (shell + TUI), keystrokes
  double as typed ("hello ...hello") → prediction ghost + real echo both shown.
- User chose "one more tuning attempt": idle-clear + clear-on-cursor-advance.
- [x] `terminalZeroLag.ts`: `ZERO_LAG_IDLE_CLEAR_MS = 300` idle timer clears any
      unmatched prediction; `anchor` cursor captured at prediction start, and
      `clearIfEchoedIn` drops the whole prediction once the real echo advances
      the cursor past the anchor. Both internal — no TerminalRawView wiring change.
- [x] Tests: idle force-clear, cursor-advance clear, no-move keep (3 new).
- [x] Rebuilt dist.
- ⚠️ Heuristic, not a guarantee — needs iOS-simulator confirmation by user.

## Validation ledger

- ajax-core: `cargo nextest run -p ajax-core` → 758 passed
- ajax-web + ajax-cli: `cargo nextest run -p ajax-web -p ajax-cli` → 459 passed
- ajax-tui: `cargo nextest run -p ajax-tui` → 205 passed
- JS: `npm run web:test -- --run` → 431 passed
- assets: install/web_backend/asset tests → 32 passed (against rebuilt dist)

## Deviations

- (none yet)
