# Web terminal scroll yank fix + OpenCode agent option

## Scope

1. Fix web cockpit terminal scrollback: view jumps to bottom while reading
   scrollback, and on busy tasks scrolling up appears impossible.
2. Add `opencode` as an agent option when creating a new task from the web.

## Non-Goals

- No ghostty-web version change (the #356 rollback to npm 0.4.0 stands).
- No route-scroll / viewport-band layout changes (just landed in 053a104).
- No scrollback history seeding on attach (pre-connection tmux history is
  still absent from the browser buffer — follow-up if still wanted after this
  fix).
- No CLI/TUI agent-picker changes; web only for opencode.

## Root cause (scroll)

ghostty-web 0.4.0 `Terminal.writeInternal` ends with
`this.viewportY !== 0 && this.scrollToBottom()` — the library force-scrolls
to bottom on EVERY write when the user is scrolled up. Confirmed present in
the served bundle (`crates/ajax-web/web/dist/app.js` contains the minified
`viewportY!==0&&this.scrollToBottom()`). Consequences:

- Busy tasks (constant tmux redraw): every output frame resets the viewport →
  scroll-up looks completely dead ("certain tasks I can't scroll up").
- Quieter tasks: reading scrollback until the next output frame lands → jump.
- The forced scroll also fires `onScroll(0)`, flipping Ajax's
  `pinnedToBottom` back to `true`, so Ajax's own guard never engages.

Fix in `TerminalRawView.svelte` (same instance-patch pattern already used to
silence `showScrollbar`): capture the library `scrollToBottom`, replace the
instance method with a no-op so the write path can't yank, and route all of
Ajax's intentional bottom-snaps through the captured original. Additionally,
while the user is scrolled up, compensate viewport drift when writes grow the
scrollback (viewportY is measured from the bottom, so keeping content
stationary requires `scrollLines(-delta)`).

## Approval

- User instruction: "Scrolling is still broken … Implement opencode as an
  option when creating a new task from the web as well." Direct
  implementation; not delegated to Cursor because the scroll fix's diff is
  smaller than the work order needed to describe the library-internals
  semantics, and the opencode change is three one-liners plus tests.

## Tasks

- [x] Task 1 (test): teach the ghostty-web mock in
  `TerminalRawView.test.ts` to mimic 0.4.0's write-time force-scroll
  (write → `this.scrollToBottom()` when viewportY ≠ 0; scrollToBottom resets
  viewportY and fires onScroll). Confirm the existing "does not yank" test
  goes red.
- [x] Task 2 (impl): capture + no-op `term.scrollToBottom` after
  `term.open()`; route Ajax's bottom-snaps through the captured original;
  add scrollLines drift compensation for unpinned writes. Green.
- [x] Task 3 (test+impl): opencode backend gate — failing test that
  `start_task` accepts `agent: "opencode"` and sends
  `… -- opencode` to tmux; then add `"opencode"` to
  `supported_start_agent` in `crates/ajax-web/src/slices/operate.rs`.
- [x] Task 4 (test+impl): `NewTaskSheet.svelte` — add
  `<option value="opencode">OpenCode</option>` and a component test that the
  agent select offers codex/claude/cursor/opencode.
- [x] Task 5: rebuild the vendored web bundle (`npm run web:build`) so the
  served assets include both changes; keep install.rs/web_backend.rs asset
  snapshot tests green.
- [x] Task 6: validation — `npm run web:test -- --run` (focused first),
  `cargo nextest run -p ajax-web`, fmt/check/clippy. Record results.

## Deviations

- Discovered the committed `crates/ajax-web/web/dist` bundle was STALE: the
  viewport refactor (053a104) changed web sources without rebuilding, so the
  served UI was still pre-refactor. Rebuilt via `npm run web:build`.
- The rebuild surfaced one stale asset-snapshot assertion in
  `crates/ajax-web/src/slices/install.rs`
  (`html.terminal-expanded .task-detail .terminal-panel`) that tracked the
  pre-refactor selector; updated it to the refactor's intentional selector
  (`html.terminal-expanded [data-testid=task-terminal-panel].is-expanded`)
  and made the compaction quote-insensitive for minified attribute selectors.
  Equal-strength contract update, not a weakening.
- No changes to the mock resize expectations were needed.

## Validation

- `npm run web:test -- --run TerminalRawView.test.ts` — red first (3 failed:
  the two yank tests + new drift test), then 114/114 after the fix.
- `cargo nextest run -p ajax-web start_task_opencode` — red first
  (unsupported agent), green after allowlist change.
- `npm run web:test -- --run` — 28 files, 340 tests, all passed.
- `npm run web:check` — 0 errors, 0 warnings.
- `cargo fmt --check`, `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo nextest run -p ajax-web -p ajax-cli` — 455/455 passed.
- `npx playwright test e2e/layout-scroll.test.ts` — 12 passed, 0 failed.

## Follow-up (not in scope)

- Pre-connection scrollback seeding: the browser terminal still only holds
  output streamed during the current websocket's life; tmux history from
  before the page was opened is not scrollable. If wanted, seed via
  `tmux capture-pane -p -e -J -S -<N> -E -1` on attach in
  `bridge_task_terminal_socket`.
