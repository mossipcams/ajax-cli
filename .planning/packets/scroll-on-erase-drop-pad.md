PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Stop tmux attach `CSI 2 J` from wiping the newest seeded history lines by
enabling xterm `scrollOnEraseInDisplay: true` (PuTTY-style ED2) and removing the
server CRLF pad that only existed to outrun that erase.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/scrollback-history-rca.md`

## Forbidden changes

- Do not commit, push, merge, rebase, or change branches.
- Do not change capture-pane flags (`-S -10000`, `-E -1`, no `-J`), `seed=0`
  reconnect policy, hostile-sequence list, scroll-sync, or architecture docs.
- Do not add bootstrap-only ED2 latching (that is 1b, not this task).
- Do not hand-edit `dist/*`; rebuild with `npm run web:build` after
  TaskTerminal changes.
- Do not edit files outside Allowed files.

## Context evidence

Desired behavior:
- After history seed write, attach `CSI 2 J` must push viewport content into
  scrollback (not erase it). Newest seed lines remain readable above the live
  screen. No blank CRLF pad band in scrollback.

Source anchors:
- `TaskTerminal.tsx` ~890–900: `new Terminal({ ..., scrollback: terminalScrollbackLines(), ... })` — add `scrollOnEraseInDisplay: true`.
- `terminal_pty.rs` `captured_history_frame_bytes(bytes, rows)` ~441–459: CRLF normalize then append `rows` blank `\r\n` — remove pad; drop unused `rows` param and `client_rows` plumbing at ~574–682.
- Test `captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback` ~1189–1200 currently requires pad — replace with “does not append pad CRLFs”.
- Contract row ~75 still says bridge “pads with exact settled `client_rows` CRLFs” — retarget to `scrollOnEraseInDisplay` + no pad.
- xterm 6 option exists: `node_modules/@xterm/xterm/typings/xterm.d.ts` `scrollOnEraseInDisplay?: boolean`.
- Existing TS test style: `TaskTerminal.test.tsx` uses `TaskTerminal.tsx?raw` source assertions (reuse that; do not mount full React terminal).

Reuse pattern: source-level `expect(taskTerminalSource).toMatch(...)` like other TaskTerminal option/contract tests.

## Code anchors

- `TaskTerminal.tsx`: Terminal constructor options object (~890).
- `TaskTerminal.test.tsx`: add one `it(...)` asserting `scrollOnEraseInDisplay:\s*true` in the constructor options region.
- `terminal_pty.rs`: `fn captured_history_frame_bytes`; call site `captured_history_frame_bytes(output.stdout, client_rows)`; `client_rows` locals/updates in resize wait; pad unit test.
- `TERMINAL_BEHAVIOR_CONTRACT.md` § history seed row citing pad / `client_rows` CRLFs.

## Test-first instructions

1. In `TaskTerminal.test.tsx`, add a failing test that the Terminal constructor
   source includes `scrollOnEraseInDisplay: true` (or `scrollOnEraseInDisplay:\s*true`).
2. RED: `npm run web:test -- --run src/features/task/TaskTerminal.test.tsx`
   must fail on that assertion.
3. In `terminal_pty.rs` tests, replace
   `captured_history_frame_bytes_appends_rows_crlfs_to_push_seed_into_scrollback`
   with a test that history frames are CRLF-normalized only and do **not** grow
   by `rows * 2` trailing pad bytes (e.g. `captured_history_frame_bytes(b"a\nb")`
   equals `b"a\r\nb"` / equivalent after signature change; empty still `None`).
4. RED: `rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture`
   must fail for the intended reason before production edits.
5. Only then implement production edits.

## Edit instructions

1. Add `scrollOnEraseInDisplay: true` to the `new Terminal({...})` options in
   `TaskTerminal.tsx`.
2. Change `captured_history_frame_bytes` to CRLF-normalize only (no blank pad).
   Remove the `rows` parameter; update the call site to pass only stdout.
3. Remove now-dead `client_rows` variable and `.max(size.rows)` updates used
   solely for pad. Keep resize apply, settle quiet loop, and 100ms reflow sleep.
4. Update `TERMINAL_BEHAVIOR_CONTRACT.md` history-seed row: remove pad wording;
   document that TaskTerminal sets `scrollOnEraseInDisplay: true` so attach ED2
   pushes seeded viewport into scrollback; cite TaskTerminal + new/updated tests.
5. `npm run web:build` after TaskTerminal change.
6. Check off Task 2 notes in `.planning/agent-plans/scrollback-history-rca.md`;
   leave parent review for the parent.

## Verification commands

```bash
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture
rtk cargo test -p ajax-web terminal_pty -- --nocapture
rtk cargo fmt --check
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
npm run web:lint
npm run web:build
rg -n 'scrollOnEraseInDisplay|captured_history_frame_bytes|client_rows|\\\\r\\\\n\\\\r\\\\n' crates/ajax-web/web/src/features/task/TaskTerminal.tsx crates/ajax-web/src/adapters/terminal_pty.rs crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
```

## Acceptance criteria

- TaskTerminal constructs xterm with `scrollOnEraseInDisplay: true`.
- History seed bytes are LF→CRLF normalized with no trailing blank-row pad.
- `client_rows` pad plumbing is gone; resize settle/reflow behavior remains.
- Focused TS + Rust tests pass; fmt/clippy/lint/build pass.
- Contract text matches (no pad; documents scroll-on-ED2).

## Stop conditions

- Need to strip or rewrite `CSI 2 J` on the server to make tests pass.
- Need bootstrap-only latching (1b) because permanent option is rejected.
- Patch would exceed ~400 lines or leave Allowed files.
- Ambiguity whether resize settle should also be deleted (out of scope; keep it).
