# Terminal rebuild acceptance matrix

**Status:** Task 12 complete — old browser surfaces removed; matrix rows that
require a mounted terminal are **red** until the rebuild lands. This file is
**pre-removal acceptance evidence**; preserve the full matrix and checklist.

Acceptance criteria for the ground-up iOS Safari terminal rebuild. Rows map
permanent automated coverage, backend boundaries, and physical-iPhone checks.
See `TERMINAL_BEHAVIOR_CONTRACT.md` for source-backed inventory.

For surface-dependent rows, **current result** means the state after Task 12.
The successful pre-removal result remains named so the rebuild has evidence
that the contract was executable against the former Ghostty surface.

## Scope and status

| Item | Status |
| --- | --- |
| **Target browser** | Normal iOS Safari tab (not Home Screen / standalone PWA) |
| **Automated regression proxy** | Playwright `mobile-webkit` (`iPhone 15 Pro` emulation) |
| **Not acceptance targets** | Desktop Chromium, desktop WebKit, deployed `ios-terminal-smoke.mjs` |
| **PWA requirement** | None; standalone PWA may be used only as an optional diagnostic comparison |
| **Before Task 12** | Permanent suite passes against current Ghostty surface |
| **Task 12 onward** | Old Ghostty/xterm surfaces are removed; matrix rows go **red** until the rebuild satisfies them |
| **Playwright ≠ physical Safari** | Mobile WebKit proves regression proxy only; it does not prove OS keyboard, loupe, paste sheet, or address-bar behavior |

## Rebuild acceptance matrix

| Required behavior | Test location | Automated or manual | Current result | Physical iOS required |
| --- | --- | --- | --- | --- |
| Task detail exposes one functioning terminal surface and opens one WebSocket | `e2e/terminal-behavior.test.ts` — `task route mounts one terminal surface and opens one socket` | Automated (mobile-webkit proxy) | **Red after Task 12** (pre-removal pass) | No |
| Connection status shows Connecting then Connected on delayed open | `e2e/terminal-behavior.test.ts` — `delayed socket open shows Connecting then connects` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Socket close reconnects; server error → unavailable; manual Reconnect recovers | `e2e/terminal-behavior.test.ts` — `socket close reconnects, server error becomes unavailable, and manual reconnect recovers`; `src/terminalConnection.test.ts` — `gives up after repeated immediate failures`, `treats a server error frame as unavailable`, `manual reconnectNow retries from unavailable` | Automated | **Red after Task 12**; retained boundary unit tests pass | No |
| Navigating away disposes surface, socket, and listeners (no zombie timers) | `e2e/terminal-behavior.test.ts` — `navigation away closes the active socket and removes the surface` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Reopening the task yields one surface and one active socket (deduped mount) | `e2e/terminal-behavior.test.ts` — `reopening the task route yields one surface and one active socket` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Stable interaction locator for black-box tests (engine-neutral) | `e2e/terminal-behavior.test.ts` — `task route exposes a stable terminal interaction surface locator` | Automated | **Red after Task 12** (pre-removal pass) | No |
| First connect dials without `?seed=0`; auto/visibility reconnect dials `?seed=0`; manual Reconnect re-seeds | `src/terminalConnection.test.ts` — `first connect dials without a seed opt-out (seed)`, `automatic backoff reconnect dials with seed=0 (seed)`, `foreground visibility reconnect dials with seed=0 (seed)`, `manual reconnectNow dials a full seed (seed)` | Automated | Pass | No |
| Foreground `visibilitychange` redials while reconnecting (mobile Safari socket kill) | `src/terminalConnection.test.ts` — `foreground visibility reconnect dials with seed=0 (seed)` | Automated | Partial proxy | Yes |
| Re-open with seed resets scroll follow and snaps to bottom; without seed keeps local buffer | Product contract (`TERMINAL_BEHAVIOR_CONTRACT.md` §5); no permanent e2e row yet | Manual + contract | Not automated | No |
| Capped exponential backoff (1s→15s) and reconnect-after-open semantics | `src/terminalConnection.test.ts` — `keeps reconnecting after a socket has opened` | Automated | Pass | No |
| PTY output reaches browser in order; live surface stays connected during corpus | `e2e/terminal-behavior.test.ts` — `pty output corpus keeps surface connected without application errors`; `src/terminalConnection.test.ts` — `preserves unicode and control corpus order through binary frames`, `preserves rapid burst and large payload order without loss or duplication` | Automated | **Red after Task 12**; retained transport tests pass | No |
| PTY output during delayed socket initialization keeps surface stable | `e2e/terminal-behavior.test.ts` — `pty output corpus during delayed socket open keeps surface stable without application errors`; transport exactness in `src/terminalConnection.test.ts` | Automated | **Red after Task 12**; retained transport tests pass | No |
| PTY output during viewport transition eventually settles without errors | `e2e/terminal-behavior.test.ts` — `rapid pty output during viewport transition eventually settles resize without application errors`; transport exactness in `src/terminalConnection.test.ts` | Automated | **Red after Task 12**; retained transport tests pass | No |
| Split UTF-8 / emoji bytes reassemble across binary frames | `src/terminalConnection.test.ts` — `reassembles split UTF-8 emoji bytes across consecutive binary frames` | Automated | Pass | No |
| ANSI, CR/LF, combining/wide glyphs in transport corpora (socket boundary) | `src/terminalConnection.test.ts` — `preserves unicode and control corpus order through binary frames` | Automated | Pass | No |
| Output is visibly painted on the live surface (not just socket delivery) | Physical checklist § Output visibility | Manual | Not proven by Playwright | Yes |
| Printable, Enter/Tab/Escape, and arrow keys produce ordered PTY input | `e2e/terminal-behavior.test.ts` — `printable, control, and navigation keys produce ordered PTY input` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Ctrl combinations (sticky bar / control codes) reach PTY | `e2e/terminal-behavior.test.ts` — `supported Ctrl toolbar combinations send exact control codes and disarm sticky Ctrl` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Repeated printable browser events produce exact cardinality (no duplicate frames) | `e2e/terminal-behavior.test.ts` — `repeated printable browser events produce exact cardinality` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Multiline Unicode paste preserves content in one input frame | `e2e/terminal-behavior.test.ts` — `multiline Unicode paste preserves content in one input frame` | Automated (synthetic paste) | **Red after Task 12** (pre-removal proxy passed) | Yes |
| Hide-keyboard blur and focus transitions add no spurious PTY input | `e2e/terminal-behavior.test.ts` — `Hide keyboard focus blur adds no PTY input` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Typing after manual reconnect sends exactly one input frame | `e2e/terminal-behavior.test.ts` — `typing after manual reconnect sends exactly one input frame` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Single Backspace deletes one character with correct PTY cardinality | Physical checklist § Backspace | Manual | Not proven by Playwright | Yes |
| Held Backspace repeat fires `beforeinput deleteContentBackward` at native cadence | Physical checklist § Backspace | Manual | Not proven by Playwright | Yes |
| Initial open sends at least one valid positive-integer PTY size | `e2e/terminal-behavior.test.ts` — `initial open eventually sends at least one valid positive-integer PTY size` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Grid keeps ≥80 columns (agent-sized layout; not an arbitrary floor) | `architecture.md` § terminal slice; resize outcomes in permanent suite | Automated + architecture | **Red after Task 12** (architecture requirement retained) | No |
| Portrait↔landscape produces fresh valid resize without adjacent duplicate sizes | `e2e/terminal-behavior.test.ts` — `portrait-to-landscape eventually produces a fresh valid resize without adjacent duplicate sizes` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Viewport burst dedupes identical dimension pairs; meaningful change still resizes | `e2e/terminal-behavior.test.ts` — `repeated same-dimension viewport burst then meaningful change deduplicates resize outcomes` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Keyboard-open resize burst does not storm PTY; close settles without adjacent duplicates | `e2e/terminal-behavior.test.ts` — `keyboard-open resize burst does not storm PTY resize; closing eventually settles without adjacent duplicates` | Automated | **Red after Task 12** (pre-removal proxy passed) | Yes |
| Fullscreen enter/exit each produce fresh valid resize and retain one socket | `e2e/terminal-behavior.test.ts` — `fullscreen enter and exit each produce a fresh valid resize and retain one active socket`, `fullscreen enter and exit keep one socket, one surface, and ordered PTY input` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Reopen after meaningful viewport change: one surface, deduped resize outcomes | `e2e/terminal-behavior.test.ts` — `reopen with meaningful viewport change yields one surface and deduplicated resize outcomes` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Resize JSON frames sent from frontend (`terminalConnection.sendResize`) | `src/terminalConnection.test.ts` — `resize still sends JSON text` | Automated | Pass | No |
| `visualViewport` / keyboard-open policy (`--app-height`, hysteresis, pinch guard) | `src/viewport.test.ts` (deterministic viewport policy tests) | Automated (jsdom) | Pass unit | Yes |
| Reading scrollback shows New output pill; restoring live output sends no PTY input | `e2e/terminal-behavior.test.ts` — `reading scrollback shows New output and restoring live output sends no PTY input` | Automated | **Red after Task 12** (pre-removal pass) | No |
| While reading scrollback, new output does not crawl the read row upward | Product contract (`TERMINAL_BEHAVIOR_CONTRACT.md` §4); legacy `terminalOutputPolicy` tests are inventory only | Manual + contract | Legacy characterization | No |
| Long press on interaction surface sends no PTY input | `e2e/terminal-behavior.test.ts` — `long press on the interaction surface sends no PTY input` | Automated | **Red after Task 12** (pre-removal pass) | No |
| Synthetic vertical scroll on terminal does not move document or emit PTY input | `e2e/terminal-behavior.test.ts` — `synthetic scroll gesture on the interaction surface sends no PTY input and does not move the document` | Automated | **Red after Task 12** (pre-removal proxy passed) | Yes |
| Surrounding page does not scroll or zoom while interacting with terminal | Physical checklist § Page scroll/zoom | Manual | Not proven by Playwright | Yes |
| Native long-press selection, loupe suppression, handles, and Copy overlay | Physical checklist § Selection/copy | Manual | Not proven by Playwright | Yes |
| Paste button / fallback textarea after scroll and fullscreen transitions | `e2e/terminal-behavior.test.ts` — `Paste stays available after synthetic scroll gesture and fullscreen transitions` | Automated (button path) | **Red after Task 12** (pre-removal proxy passed) | Yes |
| Touch vertical momentum and horizontal pan ownership | Physical checklist § Touch momentum | Manual | Not proven by Playwright | Yes |
| Outward pinch changes PTY size and pinch-adjusted density persists across reload | `e2e/terminal-behavior.test.ts` — `outward pinch on the interaction surface changes PTY size and persists across reload` | Automated | **Red after Task 12** (pre-removal proxy passed) | Yes |
| Pinch-adjusted text density is the only user-persisted terminal setting | `e2e/terminal-behavior.test.ts` (pinch persistence row); `terminalGeometry.test.ts` (legacy inventory) | Automated + contract | **Red after Task 12** (pre-removal pass) | No |
| Missing or invalid persisted text-density preference at startup | Missing: `e2e/terminal-behavior.test.ts` — `initial open eventually sends at least one valid positive-integer PTY size` (valid-size at startup); invalid fallback: legacy `TerminalRawView.test.ts` — `ignores an out-of-range persisted font size and uses the default` (characterization only); rebuild requires new state-boundary default test without `ajax.terminal.fontSize` storage key | Automated (missing); legacy characterization (invalid) | **Red after Task 12** (missing preference passed pre-removal); invalid fallback remains legacy only | No |
| Safari system browser shortcuts remain browser-owned; terminal keys do not duplicate | No repo-wide shortcut interception policy beyond owned terminal controls; physical external-keyboard Safari session | Manual (physical verification) | Not automated | Yes |
| Fixed theme, font family, cursor blink, scrollback line caps — not user settings | `TERMINAL_BEHAVIOR_CONTRACT.md` §6 fixed defaults | Not applicable (documented constants) | N/A | No |
| Dev-only Surface V2 / xterm experimental toggle — legacy rollout, not acceptance | `TERMINAL_BEHAVIOR_CONTRACT.md` §1, §7; `TERMINAL_LEGACY_SURFACE_TESTS.md` | Not applicable (removable) | Legacy only | No |
| WebSocket requires browser-session cookie, `Upgrade: websocket`, same-origin Origin | `crates/ajax-web/src/runtime.rs` — `axum_task_terminal_requires_browser_session_cookie`, `axum_task_terminal_rejects_non_upgrade_requests`, `axum_task_terminal_rejects_cross_site_websocket_origin`, `websocket_origin_policy_accepts_same_origin_host` | Automated (Rust) | Pass | No |
| PTY bridge targets registered tmux session / task window (never browser handle) | `crates/ajax-web/src/adapters/terminal_pty.rs` — `tmux_attach_command_plan_uses_registered_session_and_task_target`, `tmux_attach_target_never_uses_browser_handle`; `crates/ajax-web/src/slices/terminal.rs` — `prepare_task_terminal_returns_registered_session_and_task_target` | Automated (Rust) | Pass | No |
| Per-client grouped ephemeral tmux session; cleanup on disconnect | `terminal_pty.rs` — `isolated_attach_plan_creates_grouped_session_then_attaches`, `isolated_attach_cleanup_kills_ephemeral_session`, `isolated_attach_sessions_are_unique_per_call_and_never_the_shared_session` | Automated (Rust) | Pass | No |
| History seed via `capture-pane`; `?seed=0` opt-out parsing | `terminal_pty.rs` — `seed_history_query_parsing`, `isolated_attach_plan_seeds_browser_scrollback_from_task_window`, `history_capture_preserves_display_wrapping` | Automated (Rust) | Pass | No |
| Hostile alternate-screen / mouse / `ED 3J` sequences stripped (byte-split safe) | `terminal_pty.rs` — `filter_scrollback_hostile_sequences_strips_targets_and_carries_split_sequences`, `filter_strips_hostile_sequences_fed_one_byte_at_a_time_without_prefix_leaks` | Automated (Rust) | Pass | No |
| Binary PTY output frames; legacy JSON base64 compat decode in frontend | `terminal_pty.rs` — `terminal_output_frame_bytes_returns_raw_bytes_for_binary_send`; `src/terminalConnection.test.ts` — `decodes binary ArrayBuffer output via TextDecoder into onOutput`, `still accepts legacy JSON base64 output frames` | Automated | Pass | No |
| Resize and text input frame handling (4KB cap, resize-without-data) | `terminal_pty.rs` — `handle_input_frame_accepts_resize_without_data` | Automated (Rust) | Pass | No |
| Async child cleanup with timeout; server stays responsive after disconnect | `terminal_pty.rs` — `terminal_cleanup_runs_wait_on_blocking_task`, `terminal_cleanup_does_not_wait_forever_after_kill`; `runtime.rs` — `server_health_remains_responsive_after_terminal_disconnect_cleanup` | Automated (Rust) | Pass | No |
| Orphan ephemeral sessions reaped on web-server start | `terminal_pty.rs` — `reaper_targets_only_ephemeral_grouped_sessions` | Automated (Rust) | Pass | No |

## Physical iPhone checklist

Run on a **real iPhone in normal Safari** (not Playwright, not standalone PWA
unless noted as optional comparison). Record pass/fail per session.

### Browser chrome and keyboard

- [ ] Address bar and Safari chrome: terminal layer sizes to truly visible band;
  no clipped header/footer under keyboard or rotation.
- [ ] Virtual keyboard opens on terminal tap; printable keys echo at cursor.
- [ ] Special keys: Enter, Tab, Escape, arrows reach PTY in order.
- [ ] External keyboard / Safari system shortcuts: browser shortcuts remain
  browser-owned; terminal-owned keys do not duplicate system actions.
- [ ] Ctrl bar / sticky Ctrl: `Esc`, `Tab`, `⌃C`, arrows produce control
  sequences without focus jump.
- [ ] **Single Backspace**: exactly one `deleteContentBackward` / one PTY
  deletion per tap.
- [ ] **Held Backspace repeat**: native repeat cadence; no stuck or missing
  repeats after Ghostty `keydown.preventDefault` interaction.
- [ ] Focus/blur: Hide-keyboard (`⌄`) and fullscreen exit blur without spurious
  input; refocus does not scroll-chase the page.
- [ ] Native multiline Unicode paste (system paste sheet → hidden textarea):
  one bracketed paste to PTY, content preserved.
- [ ] Keyboard open/close: no clipping, offset jump, or resize/SIGWINCH loop
  with tmux window.
- [ ] Portrait ↔ landscape: layout settles; PTY size updates without duplicate
  storm.
- [ ] Fullscreen (expand) with keyboard up: fresh dimensions; safe-area padding;
  chrome hidden; exit restores tab layout.
- [ ] Background → foreground: socket restores or reconnects; scroll position
  sane; status banner truthful.

### Selection, copy, paste, scroll, pinch

- [ ] Long press → drag → lift selects word range; system loupe/callout do not
  fight custom selection (`user-select: none` effective).
- [ ] Copy overlay → clipboard; failure opens read-only fallback textarea.
- [ ] Paste button on insecure origin opens fallback textarea for native long-
  press Paste.
- [ ] Vertical momentum scroll inside terminal; horizontal pan owned by
  terminal (page does not pan).
- [ ] **No surrounding page scroll or pinch-zoom** while gesturing on terminal.
- [ ] Two-finger pinch adjusts text density; value persists after reload.
- [ ] Reading scrollback: `New output ↓` appears; snap to live sends no PTY
  input.

### Optional diagnostic (not acceptance)

- [ ] Home Screen / standalone PWA: compare keyboard and viewport behavior vs
  normal Safari tab (differences noted; PWA is not a merge gate).

## Deliberately excluded bugs (not compatibility requirements)

These are known WebKit / Ghostty / shell defects Ajax works around. The rebuild
must not treat them as product requirements or permanent-test assertions.

| Excluded defect | Why excluded |
| --- | --- |
| Unwanted focus/page zoom on terminal focus | iOS focus-zoom path; mitigated via `maximum-scale=1` meta — not a feature |
| Viewport clipping / Safari chrome offset drift (~24px iOS 26 keyboard band) | Address-bar and `visualViewport` animation mismatch |
| Page scroll conflicts and scrollback yank from renderer `scrollToBottom` | Ghostty library behavior; mitigated, not required |
| Keyboard-driven resize / SIGWINCH storms on shared tmux window | Mitigated by keyboard-open resize withhold policy |
| Ghostty `Backspace` `keydown.preventDefault` canceling iOS `beforeinput` repeat | Workaround returns `false` from custom key handler + ZWS sentinel |
| Incorrect soft-keyboard / visual-viewport offsets | Hysteresis and `--app-top` rebasing are mitigations |
| Renderer garble, private-selection API quirks, arbitrary timer/cadence/listener layout | Implementation-specific; not user-visible contract |
| Old xterm Surface V2 rollout, auto-fallback, and Ghostty-vs-xterm selector | Legacy rollout scaffolding (Task 12 removes) |
| Arbitrary dimension floors not backed by architecture | **Exception:** documented 80-column agent layout in `architecture.md` **is** deliberate and remains in the matrix |

## Untestable without a real device

Playwright mobile WebKit—including `scripts/ios-terminal-smoke.mjs`—cannot
prove these. Each requires the physical checklist above.

| Behavior | Why automation is insufficient |
| --- | --- |
| Native long-press loupe and iOS edit callout suppression | System overlays; headless WebKit has no loupe |
| Native Paste sheet targeting softened hidden textarea | OS paste eligibility and clipboard UI |
| `beforeinput deleteContentBackward` repeat on held Backspace | iOS-only input mode; synthetic key repeat ≠ Safari |
| Address-bar drift and keyboard animation band | Real keyboard animates over multiple frames |
| `interactive-widget=resizes-content` shrinking layout viewport | Safari-only meta behavior |
| `maximum-scale=1` actually blocking focus-zoom | iOS-only zoom path (meta tag presence is not enough) |
| Two-finger page-zoom latch on second touch | WebKit driver does not reproduce latch reliably |
| Touch momentum deceleration and rubber-band boundaries | OS compositor physics |
| Native selection handles and clipboard after long-press | System selection UI |
| `visibilitychange` suspend timing vs in-flight reconnect | Real iOS tab suspension timing differs from driver |
| Alt-screen TUIs (`less`, `vim`) under real phone Safari | Canvas + viewport + tmux attach not 1:1 with emulation |
| Visible output painting during rapid/large streams | Socket tests prove delivery; pixel/canvas paint needs eyes |

## References

- `TERMINAL_BEHAVIOR_CONTRACT.md` — source-backed product vs legacy inventory
- `TERMINAL_LEGACY_SURFACE_TESTS.md` — removable characterization index
- Permanent browser suite: `e2e/terminal-behavior.test.ts` (27 tests)
- Permanent frontend boundary: `src/terminalConnection.test.ts` (16 tests)
- Viewport policy unit tests: `src/viewport.test.ts`
- Backend: `crates/ajax-web/src/adapters/terminal_pty.rs`, `runtime.rs`, `slices/terminal.rs`
