# Revert zero-lag + restore scrollback depth

## Scope

1. Revert `feat(web): add xterm zero-lag typed-echo overlay` (`4d2cdc1` / #661):
   remove overlay module, TaskTerminal wiring, CSS, and related tests/docs.
2. Restore scrollback depth that the React xterm path hard-capped at 2000:
   - client: mobile 2000 / desktop 10000 via `terminalScrollbackLines()`
   - seed: `capture-pane -S` raised to `-10000` to match desktop buffer

## Non-goals

- Do not change history-seed settle/pad logic from #657/#648 beyond capture depth.
- Do not restore Ghostty / wterm surfaces.
- Do not commit, push, or open a PR unless asked.
- Do not reintroduce legacy `terminalZeroLag.ts` paths (still banned).

## Root cause

- Zero-lag: product request to remove the typed-echo overlay (#661).
- Scroll history UX: React `TaskTerminal` uses `scrollback: 2000` for every
  viewport; contract and prior Ghostty path used 2000 mobile / 10000 desktop.
  Server seed also uses `capture-pane -S -2000`, so even a deeper client buffer
  cannot receive more than 2000 seeded lines.

## Delegation decision

`Delegation decision: delegated via model-router`

Two sequential rounds (one bounded behavior each):

1. Revert zero-lag → cursor-delegate / composer-2.5 (ACCEPT; invalid report wrap)
2. Restore scrollback → pi-delegate/GLM unavailable (weekly limit) → escalated to
   cursor-delegate / composer-2.5 (ACCEPT after parent contract fix)

## Checklist

- [x] Task 1 — Packet + delegate: revert zero-lag (#661 surface)
  - Test: remove/adjust zero-lag assertions; remaining web unit tests pass
  - Implementation: delete overlay module/wiring/CSS; update contract anchors
  - Verification: focused web tests + lint
  - Notes: deleted `xtermZeroLag.ts` + test; unwired `TaskTerminal.tsx`; removed CSS/docs row; restored Product contract to legacy `terminalZeroLag.ts` anchors
- [x] Task 2 — Packet + delegate: restore scrollback depth
  - Test: failing `terminalScrollbackLines` + capture `-S -10000` assertions
  - Implementation: restore helper; wire TaskTerminal; raise capture depth
  - Verification: focused TS + Rust tests; contract row updated
- [x] Task 3 — Parent review gate + validation for both diffs

## Approval status

Authorized by user request: revert zero lag; fix scroll history cap UX.

## Deviations

- Both Cursor delegates returned invalid/missing report envelopes; parent gated
  on delta + re-ran verification.
- GLM weekly limit on round 1 of scrollback; escalated same packet to Cursor.
- Parent fixed leftover contract prose: capture-pane depth 2000 → 10000 and
  scrollback helper line anchors.

## Validation

Parent-run:

```bash
# Task 1
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/legacyTerminalRemoval.test.ts  # PASS 17
npm run web:lint  # PASS
rg zero-lag anchors in src/dist  # no matches

# Task 2
npm run web:test -- --run src/shared/lib/terminalGeometry.test.ts  # PASS 15
rtk cargo test -p ajax-web isolated_attach_plan_seeds_browser_scrollback_from_task_window -- --nocapture  # PASS 1
rtk cargo test -p ajax-web terminal_pty -- --nocapture  # PASS 27
npm run web:lint  # PASS
rtk git diff --check  # PASS
rtk cargo fmt --check  # PASS
```
