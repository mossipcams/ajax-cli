# Web Slice Tech-Debt Cleanup Plan

Scope: `crates/ajax-web/src` + `crates/ajax-web/web/src` (excluding `web/dist`).
Baseline LOC (find … | wc -l): **15,611**. Target: ≥30% net reduction
(≤ ~10,928) without deleting behavior, weakening tests, or hiding complexity in
generated code.

## 1. Behavior contracts to preserve (from the last week of PRs)

- **Raw-first terminal** (#263/#264): task terminal is raw xterm/tmux on mobile
  and desktop; no Live/snapshot/composer default; no mode tabs; guarded
  prompt/approval flows stay separate dashboard paths.
  Pinned by `TerminalPanel.test.ts` ("defaults_to_raw_terminal…", "does not
  render snapshot viewer or mode tabs") — these tests survive any wrapper
  removal, retargeted at the surviving component.
- **PTY output filtering** (#273): alternate-screen (?47/?1047/?1049),
  mouse-tracking (?1000–?1007), and scrollback-erase (CSI 3 J) sequences are
  stripped, including sequences split across read chunks with no prefix leaks;
  normal erase/cursor/SGR/text passes through.
  Pinned by `filter_scrollback_hostile_sequences_*` tests; strengthen with a
  byte-at-a-time chunking test before touching the scanner.
- **80-column PTY floor + pan/pinch/fling** (#278): cols never < 80; horizontal
  drag pans the oversized canvas with clamping; pinch maps to a 7–20px font
  with localStorage persistence; fling decays, is bounded, and is cancelled by
  a new touch. Pinned in `TerminalRawView.test.ts`, `terminalGeometry.test.ts`,
  `terminalTouchScroll.test.ts`. Missing: wheel-cancels-fling — add it.
- **Keyboard-open lockstep** (#278): while the iOS keyboard is open the local
  grid is frozen (no fit/resize), the canvas is bottom-anchored, and exactly
  one server resize is flushed after the keyboard closes; resize debounce is
  300ms with immediate local refit. All pinned in `TerminalRawView.test.ts`.
- **Scroll interception** (#259/#261/#270/#275): all wheel/touch scroll maps to
  `term.scrollLines`; `touch-action: none` + hidden native viewport scrolling
  so iOS Safari cannot steal or desync the gesture; taps stay taps; pinned to
  bottom auto-follow only when at bottom.
- **Mobile polish** (#280): 13px readable font on mobile (incl. landscape
  phones via the combined media query), xterm DOM scrollbar hidden on coarse
  pointers, tighter mobile chrome, task-details disclosure hidden on mobile,
  expand toggle never focuses the terminal and refits immediately.
- **Reconnect**: capped exponential backoff, immediate foreground reconnect,
  overlay cleared on reconnect only, manual reconnect button.
- **Sticky Ctrl**: armed Ctrl folds into exactly the next key from keyboard or
  key bar, auto-expires after 4s.
- **Paste**: clipboard.readText() → term.paste(), visible failure message.
- **Hide keyboard key**: blurs the terminal.
- **Backend contracts**: WebSocket frame shapes (input/resize/output/error),
  ephemeral grouped tmux session per client + reaping, browser-session cookie
  gate on /api routes, cockpit refresh cache TTL + single-flight, operation
  idempotency by request_id, one-mutation-at-a-time 409s, TLS accept
  isolation. All pinned in `runtime.rs`/`terminal_pty.rs` tests.
- **Serving contract**: `install.rs` + `ajax-cli/web_backend.rs` snapshot tests
  pin the built `web/dist` bundle — any `web/src` change requires
  `npm run web:build`.

## 2. Cleanup targets (ranked by expected LOC yield)

1. **`runtime.rs` tests (~2,320 lines)** — extreme boilerplate: every test
   hand-builds an authenticated oneshot request, reads the body, parses JSON,
   and re-implements the shared-lock guard; `TestBridge` implements
   `RuntimeBridge` twice verbatim for two runner types (~130 duplicated
   lines); the condvar release dance is copy-pasted four times.
   → one generic `impl<R: CommandRunner> RuntimeBridge<R> for TestBridge`,
   `get/post` request helpers returning `(status, headers, json)`, a
   `bridge(&state)` accessor, a `release()` helper. Every assertion is kept.
2. **`runtime.rs` production (~940 lines)** — in-flight gate + 409 conflict
   JSON duplicated between `axum_start_task` and `axum_action`; repeated
   5-line `shared.lock().unwrap_or_else(PoisonError::into_inner)` blocks.
   → small `OperationGate` acquire/release helper + `shared()` accessor.
3. **`terminal_pty.rs` (932)** — six duplicated "send error frame, close,
   return" blocks; four near-identical `TerminalChild` test mocks; repeated
   attach-plan fixtures. → one `send_error_and_close` helper, one configurable
   `MockChild`, one `plan()` fixture.
4. **Rust test fixtures** — the 12-line `Task::new("web/fix-login", …)`
   literal is repeated ~14× across `runtime.rs`, `slices/terminal.rs`,
   `slices/cockpit.rs`, `slices/operate.rs`. → `pub(crate)` `test_support`
   builders.
5. **`TerminalPanel.svelte` (41) + `TerminalPanel.test.ts` (121)** — a pure
   pass-through wrapper added when the snapshot/composer host was deleted
   (#264); its `.terminal-host-shell` duplicates `.terminal-panel` chrome
   (nested double border/margin — patch-layering artifact). → delete the
   wrapper, mount `TerminalRawView` directly from `TaskDetail`, move the
   raw-first contract tests into `TerminalRawView.test.ts`, drop the duplicate
   CSS block and the `.terminal-host-shell` selectors in `styles.css`.
6. **`TerminalRawView.test.ts` (1,314)** — repeated mount/open/settle
   scaffolding in ~50 tests. → `mountTerminal()` + `settleOpen()` helpers;
   assertions untouched.
7. **`TerminalRawView.svelte` (1,002)** — duplicated touch-state reset in
   touchend/touchcancel; comments restating code (iOS-specific rationale is
   kept); CSS duplicated with the deleted wrapper.
8. **Small**: duplicated `postOperation`/`startTask` bodies in `api.ts`;
   `.task-detail .terminal-panel` twin selectors in `styles.css`.

Explicit non-targets: gesture modules (all in use), `viewport.ts` (owns
--app-height / keyboard-open), `styles.css` structure (recently deduplicated),
`slices/terminal.rs` (already minimal), CSS source-shape tests in
`TerminalRawView.test.ts` (they are the only executable pin for coarse-pointer
scrollbar hiding and mobile chrome sizing — jsdom cannot evaluate media
queries, so no equivalent behavior test exists to replace them).

## 3. Execution order (small slices, tests first)

1. Green baseline: npm install (fresh worktree), `npm run web:test -- --run`,
   `cargo nextest run -p ajax-web`.
2. Safety-net additions (must pass before refactors):
   a. byte-at-a-time chunked hostile-sequence test (`terminal_pty.rs`);
   b. wheel-cancels-fling test (`TerminalRawView.test.ts`);
   c. fling total-line cap test (`terminalTouchScroll.test.ts`) if absent.
3. `terminal_pty.rs` consolidation (prod helper, then test mocks) — focused
   tests after each.
4. `runtime.rs` production gate helper — focused tests.
5. `runtime.rs` test-module consolidation in batches — full `-p ajax-web`
   nextest after each batch.
6. Shared `test_support` fixtures across the four Rust files.
7. Wrapper removal (TerminalPanel → TerminalRawView direct) with contract
   tests retargeted first, then component + styles.css cleanup, then
   `TerminalRawView.test.ts` helper consolidation.
8. `npm run web:build` (dist snapshot tests), then full validation:
   fmt/check/clippy/nextest/doc + web:check/test/build + final LOC count.

Every slice keeps production behavior identical; the only intentional visual
delta is removing the double-border/double-margin artifact created by the
redundant wrapper (nested identical panel chrome), which no test pins.
