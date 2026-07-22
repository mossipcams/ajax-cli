# Web Cockpit terminal behavior contract

**Status:** Task 12 complete — frontend mount removed; this inventory documents
the **pre-removal** Ghostty/Surface V2 behavior contract for rebuild reference.

Source-backed behavioral inventory of the **pre-removal** Ajax browser terminal,
focused on iOS Safari. Every row cites at least one file or test path; `.svelte`
citations are historical references into files this rebuild deleted, and are
preserved as provenance rather than repointed at the React successor. Primary
classifications are mutually exclusive:

| Label            | Meaning |
| ---              | --- |
| **Product**      | Permanent user-observable outcome the ground-up rebuild must satisfy. The contract. |
| **Legacy**       | Removable rollout scaffolding, experimental renderer choice, test-only seams, and current scheduling/batching organization—not rebuild acceptance criteria. |
| **Legacy Ghostty** | Implementation detail that exists because the `ghostty-web` 0.9.4 fork (rcarmo) or the 0.4.0 npm baseline require it. The library quirk is the reason the row exists. |
| **Bug excluded** | A known iOS / WebKit / Ghostty defect that the shell works around. Phrased as the defect, not as a requirement. |
| **Physical iOS** | A behavior that only manifests on a real iPhone and cannot be proven by Playwright mobile WebKit. |

> **Reading the tables.** Product rows describe what an operator sees; Legacy and
> Legacy Ghostty rows name the current implementation that delivers them. Bug
> excluded rows describe the underlying defect (the "because" clause), not the
> mitigation Ajax layers on top. When a Product outcome and a Legacy mechanism
> share a source line, both rows cite it.
>
> **Compatibility scope.** Permanent automated coverage targets the Playwright
> `mobile-webkit` project (`iPhone 15 Pro`). Existing `desktop-chromium` tests
> are inventory only. Removable characterization is indexed in
> `TERMINAL_LEGACY_SURFACE_TESTS.md`; the permanent suite is
> `e2e/terminal-behavior.test.ts`.

Supporting facts (do not move target):

- Mount surface: `crates/ajax-web/web/src/components/TaskDetail.svelte:69` mounts
  `TerminalSurfaceSelector.svelte` (`crates/ajax-web/web/src/components/TerminalSurfaceSelector.svelte:47-67`).
  Default = Ghostty (`TerminalRawView.svelte`); V2 on = Xterm
  (`XtermTerminalView.svelte`).
- Backend slice ownership: `architecture.md:679-714` (terminal slice + PTY
  adapter) and `crates/ajax-web/web/TERMINAL.md` (frontend ownership).
- PTY route: `crates/ajax-web/src/runtime.rs:678-757`
  (`/api/tasks/{handle}/terminal`, browser-session cookie + same-origin Origin
  check + `Upgrade: websocket`).
- PTY bridge: `crates/ajax-web/src/adapters/terminal_pty.rs` (isolated grouped
  tmux session, history seed, scrollback-hostile filter, binary output,
  resize-wait, async child cleanup).
- Frontend socket lifecycle: `crates/ajax-web/web/src/terminalConnection.ts`.
- Tests: Vitest `*.test.ts`, Playwright `crates/ajax-web/web/e2e/*.test.ts`,
  Rust unit tests in `crates/ajax-web/src/`, plus
  `scripts/ios-terminal-smoke.mjs` (deployed iPhone-sized Playwright WebKit proxy).
- Playwright config: `crates/ajax-web/web/playwright.config.mts` (projects
  `desktop-chromium` + `mobile-webkit` = iPhone 15 Pro emulation).

## 1. Mount, readiness, disposal

| Behavior | Class | Evidence |
| --- | --- | --- |
| Task detail exposes one functioning terminal surface | Product | `TaskDetail.svelte:69`; `TerminalSurfaceSelector.svelte:47-67`; `TERMINAL.md:26-35` |
| Default Ghostty vs opt-in experimental xterm is selected by the Dev-only Surface V2 flag; xterm init failure surfaces error + Retry with no auto-fallback while the flag stays on | Legacy | `TerminalSurfaceSelector.svelte:17-67,27-44`; `terminalSurfaceSetting.ts:1-50`; `SettingsView.svelte:113-127`; `TERMINAL.md:33-35` |
| Ghostty is mounted asynchronously inside `onMount` via `mountGhosttyTerminal` (default surface implementation detail) | Legacy Ghostty | `TerminalRawView.svelte:1053-1138` |
| Ghostty WASM is fetched once per page from `/ghostty-vt.wasm` (default surface implementation detail) | Legacy Ghostty | `terminalPreload.ts:4,8-11`; `runtime.rs:319,544-546,1467-1470` |
| Ghostty `open()` targets an inner `.terminal-scale-layer`, never the host (default surface implementation detail) | Legacy Ghostty | `TerminalRawView.svelte:1078-1082,1503-1510` |
| Navigating away from the task releases the terminal, socket, and listeners; no zombie timers or probes remain | Product | `TerminalRawView.svelte:1144-1178` |
| `window.__ajaxTerminalProbe` exposes `cols/rows/viewportY/lines` for Playwright buffer assertions (test seam) | Legacy | `TerminalRawView.svelte:1108-1133,1176-1177`; tests: `e2e/terminal-scroll-garble.test.ts` (removable; see `TERMINAL_LEGACY_SURFACE_TESTS.md`) |
| iOS would autocapitalize, autocorrect, and auto-zoom the terminal input unless the hidden textarea sets `fontSize: 16px`, `autocapitalize=off`, `autocorrect=off`, `autocomplete=off`, `spellcheck=false`, and softens the clip/opacity | Bug excluded | `TerminalRawView.svelte:305-331,1518-1534`; test: `TerminalRawView.test.ts:674,686,699` |
| Ghostty's `Backspace` `keydown.preventDefault` cancels the iOS `beforeinput` key-repeat loop; Ajax returns `false` from `attachCustomKeyEventHandler` for `Backspace`/`Delete` and reseeds a ZWS sentinel on focus/delete so the loop can re-fire | Bug excluded | `TerminalRawView.svelte:292-303,1094-1105`; test: `TerminalRawView.test.ts:674,686,699` |
| iOS treats a fully clipped/opacity:0 textarea as a non-editable target; native Paste is silently rejected without the softened clip | Bug excluded | `TerminalRawView.svelte:318-331,1518-1534` |

## 2. PTY / WebSocket I/O

| Behavior | Class | Evidence |
| --- | --- | --- |
| WebSocket route requires browser-session cookie, `Upgrade: websocket`, and same-origin Origin | Product | `runtime.rs:498-523,678-757`; tests: `axum_task_terminal_requires_browser_session_cookie`, `axum_task_terminal_rejects_non_upgrade_requests`, `axum_task_terminal_rejects_cross_site_websocket_origin`, `websocket_origin_policy_accepts_same_origin_host` |
| URL is `/api/tasks/{handle}/terminal`; `?seed=0` opts out of history seed | Product | `api.ts:247-254`; `terminal_pty.rs:336-342`; tests: `terminalConnection.test.ts:141-175` |
| Bridge builds a per-client grouped tmux session `…-m<12hex>` and tears it down on disconnect | Product | `terminal_pty.rs:163-215,634-642`; tests: `isolated_attach_plan_creates_grouped_session_then_attaches`, `isolated_attach_sessions_are_unique_per_call_and_never_the_shared_session`, `isolated_attach_cleanup_kills_ephemeral_session` |
| First dial seeds history via `capture-pane -p -e -S -10000 -E -1` (no `-J`); after client resizes, bridge waits 150ms quiet (within 500ms overall); TaskTerminal sets `scrollOnEraseInDisplay: true` so attach `CSI 2 J` pushes seeded viewport into scrollback (no CRLF pad); auto-reconnect / visibility-redial dial `?seed=0`; manual `Reconnect` re-seeds | Product | `terminal_pty.rs:199-210,336-342,505-521`; `TaskTerminal.tsx` (`scrollOnEraseInDisplay`); `terminalConnection.ts:154-180,198-202`; tests: `seed_history_query_parsing`, `isolated_attach_plan_seeds_browser_scrollback_from_task_window`, `resize_settle_deadline_returns_remaining_until_quiet_elapsed`, `history_capture_preserves_display_wrapping`, `captured_history_frame_bytes_does_not_append_pad_crlfs`, `TaskTerminal.test.tsx` (`scrollOnEraseInDisplay`), `terminalConnection.test.ts:141,146,175,207` |
| Quiet status options (`status-interval 5`, `visual-activity off`, `visual-bell off`) are set on the ephemeral session only | Product | `terminal_pty.rs:194-198,799-857` |
| PTY output is sent as raw `Message::Binary`; legacy `{"type":"output","data":base64}` and `{"type":"error","error":...}` are decoded for one-release compat | Product | `terminal_pty.rs:327-334,511-561`; `terminalConnection.ts:95-141`; tests: `terminal_output_frame_bytes_returns_raw_bytes_for_binary_send`, `terminalConnection.test.ts:253,276,300,323` |
| Hostile alternate-screen / mouse / `ED 3J` sequences are stripped on the server to keep `capture-pane` seed clean; filter is byte-split safe | Product | `terminal_pty.rs:25-49,298-325`; tests: `filter_scrollback_hostile_sequences_strips_targets_and_carries_split_sequences`, `filter_strips_hostile_sequences_fed_one_byte_at_a_time_without_prefix_leaks` |
| PTY output reaches the browser in order without loss; bursts stay responsive and the surface does not error while streaming | Product | `terminalConnection.ts:95-141`; tests: `terminalConnection.test.ts` (ordered/split UTF-8/burst corpora); `e2e/terminal-behavior.test.ts` (live surface during corpus) |
| Server output batching: 16ms flush, 16KB max, `Message::Binary` raw; resize frames wait up to 500ms for client resize settle (150ms quiet after last resize), then a 100ms beat lets tmux WINCH + capture settle | Legacy | `terminal_pty.rs:17-23,435-503,540-590`; tests: `terminal_output_flush_constants_match_targets`, `resize_settle_deadline_returns_remaining_until_quiet_elapsed` |
| Max input frame 4KB (text or binary); on PTY setup failure the bridge sends a JSON `error` text frame then `Close` | Product | `terminal_pty.rs:20,288-296,356-365,453-475,595-624,645-677`; tests: `handle_input_frame_accepts_resize_without_data` |
| Child cleanup is async with a 2s timeout; socket close is not blocked by a hung wait | Product | `terminal_pty.rs:66-91,628-642`; tests: `terminal_cleanup_runs_wait_on_blocking_task`, `terminal_cleanup_does_not_wait_forever_after_kill` |
| Orphan ephemeral sessions are reaped on web-server start (after a crash that skipped teardown) | Product | `runtime.rs:354-356`; `terminal_pty.rs:266-285`; test: `reaper_targets_only_ephemeral_grouped_sessions` |
| Front-end socket backs off exponentially (1s, 2s, 4s, …) capped at 15s; `unavailable` after 5 immediate failures or a server `error` frame | Product | `terminalConnection.ts:39-40,143-194`; tests: `terminalConnection.test.ts:88,106,123` |
| Socket redials on `visibilitychange → visible` while `reconnecting` | Product | `terminalConnection.ts:198-202`; test: `terminalConnection.test.ts:175` |
| Re-open handler clears status detail, resets resize dedupe, re-seeds scroll follow, snaps to bottom (only on `isReconnect && seeded`) | Product | `TerminalRawView.svelte:975-989` |
| Manual "Reconnect" button re-dials with a full seed (warm status after long drop) | Product | `TerminalRawView.svelte:1300-1304`; `XtermTerminalView.svelte:215-221`; test: `terminalConnection.test.ts:207,237` |
| `TERMINAL_PLACEHOLDER_KEY` (`localStorage.ajax.debug.terminalPlaceholder`) renders a no-network panel for layout tests (test seam) | Legacy | `TerminalRawView.svelte:48-51,1197-1199`; e2e: `layout-scroll.test.ts:299,321` (removable; see `TERMINAL_LEGACY_SURFACE_TESTS.md`) |

## 3. Resize / fit / viewport sources

| Behavior | Class | Evidence |
| --- | --- | --- |
| The terminal grid keeps at least 80 columns so live and scrollback share an agent-sized layout without mid-token soft wrap on phone viewports | Product | `architecture.md:700-704` (deliberate agent-sized layout) |
| CSS scales the terminal element to the host width; `logicalCols`, `fitScale`, and `fitFontSize` implement the current fit math | Legacy | `terminalGeometry.ts:14,101-127,209-226`; tests: `logicalCols`, `fitScale`, `fitFontSize returns whole pixels only` |
| Pinch-adjusted terminal text density persists across reload | Product | `terminalGeometry.ts:53-73`; tests: `pinchFontSize`, `persistedFontSize`/`persistFontSize` paths; `e2e/terminal-behavior.test.ts` |
| Default cell font 13px, pinch range 7–20px, storage key `localStorage.ajax.terminal.fontSize`, and `persistedFontSize`/`persistFontSize` helpers | Legacy | `terminalGeometry.ts:21-26,46-73`; tests: `fitCapFontSize`, `pinchFontSize` |
| Mobile scrollback is capped at 2000 lines, desktop at 10000, chosen from the same media query the mobile CSS uses (fixed renderer default, not a user setting) | Legacy | `terminalGeometry.ts:7-24`; test: `terminal scrollback limits` in `terminalGeometry.test.ts` |
| Open and meaningful viewport/orientation/fullscreen changes eventually settle on a valid positive-integer PTY size; adjacent duplicate dimension pairs are bounded | Product | `terminalConnection.ts` resize JSON frames; `e2e/terminal-behavior.test.ts` (initial size, orientation, burst dedupe, keyboard, fullscreen, reopen) |
| Vertical scroll over the terminal is native (no synthetic swipe layer); horizontal pan is synthetic via `scrollLeft` and `touch-action: pan-y` | Product | `TerminalRawView.svelte:620-641,1485-1491`; `terminalGestures.ts:129-191`; test: `terminalTouchScroll.test.ts:13` |
| iOS keyboard never SIGWINCH-storms the shared tmux window: the local fit freezes and the PTY resize is withheld while the keyboard is up, except for pinch-end and expand-enter (discrete intent) which re-fit and resize in one pass | Product | `terminalLayoutPolicy.ts:38-50,69-101,104-134`; `TerminalRawView.svelte:651-658,718-731`; `terminalRefit.ts:73-89`; `e2e/terminal-behavior.test.ts` (keyboard burst + settled resize) |
| Fit sources combine Ghostty `FitAddon.proposeDimensions`, `term.renderer.getMetrics()`, and `container.clientWidth`; `hostFitCols` is preferred over the addon when measurements differ | Legacy Ghostty | `TerminalRawView.svelte:400-490,732-814` |
| Resize triggers wired to `ResizeObserver` on the host, `window.resize`, `orientationchange`, `visualViewport.resize` / `scroll` | Legacy | `TerminalRawView.svelte:890-900` |
| Refit scheduler: local fit coalesces per `requestAnimationFrame`; server `sendResize` debounces 100ms after the last event burst; `scheduleImmediate` and `schedulePostLayout` fit + resize together | Legacy | `terminalRefit.ts:16,48-89`; tests: `RESIZE_DEBOUNCE_MS is 100ms`, `immediate: fits and resizes together on the next frame`, `coalesces same-frame immediate requests into one fit`, `debounced: fits per frame but sends one resize after the quiet window`, `restarts the resize debounce on every debounced request` |
| Resize dedupe: equal cols/rows are not re-sent; dedupe resets on socket reopen | Legacy | `terminalOutputPolicy.ts:179-199`; tests: `createResizeDedupe skips send when cols and rows unchanged`, `createResizeDedupe sends when cols or rows change`, `createResizeDedupe reset clears last-sent so same size can send again` |
| `isKeyboardOpen` is the single truth; `viewport.ts` uses 150px open / 100px close thresholds and rebases on `innerWidth` change | Legacy | `viewport.ts:14-17,26-31,76-105` |
| `--app-height` / `--app-top` mirror `visualViewport` so the fixed terminal layer sizes to the truly-visible band; `keyboard-open` class toggles for CSS takeover | Legacy | `viewport.ts:16-18,63-100`; `app.html:5` (`interactive-widget=resizes-content`) |
| Sub-cell scroll offset is a `translateY(px)` on `.terminal-scale-layer`, batched into a single `requestAnimationFrame` | Legacy Ghostty | `TerminalRawView.svelte:377-384,1503-1510` |
| Scrollback spacer height = `scrollbackLines × cellHeight`, kept in sync with writes and the host scroll listener | Legacy Ghostty | `TerminalRawView.svelte:333-344,620-637,806,929-955` |
| `Ghostty FitAddon` reserves 15px for its suppressed scrollbar; Ajax fits from the full host width instead | Legacy Ghostty | `TerminalRawView.svelte:471-490` |
| `scrollbarWidth: 0` is passed to the Ghostty constructor (Ajax synthesizes 100% of scrolling) | Legacy Ghostty | `TerminalRawView.svelte:1063-1064` |
| `smoothScrollDuration: 0` because Ajax owns follow-output policy and 0.9.x animates `viewportY` fractionally | Legacy Ghostty | `TerminalRawView.svelte:1064-1070` |
| `term.scrollToBottom` is rebound to a no-op on the instance to stop 0.4.0's write-time force-scroll from yanking scrollback | Legacy Ghostty | `TerminalRawView.svelte:1087-1088` |
| Selection math maps touch to cells and writes endpoints directly into Ghostty's `SelectionManager` (public `select()` stores rows in the wrong space) | Legacy Ghostty | `TerminalRawView.svelte:53-77,521-541`; `terminalGestures.ts:251-310`; tests: `cellAtPoint`, `orderedSelection` |
| iOS latches page zoom on the second touch before any touchmove guard can run; the touchstart pinch-guard (and `gesturestart`) is what stops the OS from ever owning the gesture | Bug excluded | `viewport.ts:108-134`; tests: `viewport.test.ts` |

## 4. Focus, keyboard, paste, copy, selection, scroll, touch

| Behavior | Class | Evidence |
| --- | --- | --- |
| Meta viewport caps `maximum-scale=1` so the iOS keyboard does not focus-zoom the PWA; `viewport-fit=cover`; `interactive-widget=resizes-content` | Product | `app.html:5`; test: `e2e/fullscreen-refit.test.ts:142` |
| Touch on the terminal host focuses the hidden textarea with `preventScroll: true` and resets document scroll so Safari does not scroll-chase | Product | `TerminalRawView.svelte:602-616`; `viewport.ts:37-47` |
| Long-press (500ms) → drag → lift selects a word range; a second finger or system steal cancels the selection | Product | `terminalGestures.ts:36-42,82-127,201-229` |
| Pinch-zoom: 12px deadzone, gesture scales from start font; pinch-end re-fits in one pass with the PTY resize flushed at finger lift | Product | `terminalGestures.ts:36-42,129-159,201-215`; `terminalLayoutPolicy.ts:69-81,123-134`; `terminalGeometry.ts:234-278` |
| `Ctrl` key is a sticky modifier auto-expiring at 4s; folded into the next key from the bar or the keyboard; converts letters to control codes and cursor keys to `CSI 1;5` | Product | `TerminalRawView.svelte:176-227`; `XtermTerminalView.svelte:27-87` |
| Control-key bar: `Esc`, `Tab`, `⌃C`, arrows; on `pointerdown` `preventDefault` to stop focus jump; on click refocuses only when the terminal already owns focus | Product | `TerminalRawView.svelte:1308-1318,1339-1340`; `XtermTerminalView.svelte:224-257` |
| Paste button uses `navigator.clipboard.readText()`; on insecure (plain-http LAN) origin or read failure a fallback textarea opens for native iOS long-press Paste | Product | `TerminalRawView.svelte:684-710,1260-1286`; e2e: `smoke.test.ts:88` |
| Paste goes through `term.paste(...)` so bracketed-paste mode is honored; failures surface a notice (never silently dropped) | Product | `TerminalRawView.svelte:688-691,1289-1299`; test: `terminalClipboard.test.ts:53,84` |
| Copy: long-press selection → `Copy` overlay → `copyText` (ExecCommand fallback for iOS); failure opens a read-only textarea | Product | `TerminalRawView.svelte:559-568,1231-1258`; `terminalClipboard.ts`; tests: `terminalClipboard.test.ts:61,77,99,110,133` |
| Typed text appears at the cursor before the PTY echo returns; the prediction is cleared the moment the real echo advances the cursor or after an idle window, so it never persists as a duplicate | Product | `terminalZeroLag.ts`; tests: `terminalZeroLag.test.ts:151,171,186`; e2e: `e2e/terminal-zero-lag.test.ts:55,65` |
| Touch horizontal pan is captured in `touchmove` (capture phase) so renderer layers can never swallow it | Product | `terminalGestures.ts:231-247` |
| Touch focuses the hidden textarea early so iOS can target native Paste before the long-press timer fires | Product | `terminalGestures.ts:124-127` |
| Hide-keyboard button (`⌄`) blurs the hidden textarea; fullscreen exit also blurs | Product | `TerminalRawView.svelte:672-675,1334-1338`; e2e: `actions.test.ts:205` |
| Expand (fullscreen) toggle adds `.terminal-expanded` on `<html>`; the panel becomes a fixed visual-viewport layer with safe-area padding and the global chrome is hidden | Product | `TerminalRawView.svelte:130-141,1183-1230,1384-1449`; `styles.css:631-700`; tests: `e2e/fullscreen-refit.test.ts:79,105,122,148` |
| Output follow: when pinned to the bottom the view snaps; when unpinned new output is marked unseen and the `New output ↓` pill appears | Product | `terminalOutputPolicy.ts:99-159,372-454`; `TerminalRawView.svelte:620-637,1202-1209`; tests: `createScrollFollowPolicy`, `e2e/terminal-behavior.test.ts` |
| While reading scrollback, new output preserves the read row (it does not crawl upward) | Product | `terminalOutputPolicy.ts:91-96`; `TerminalRawView.svelte:944-955`; test: `compensates positive scrollback growth while preserving reader position`; `e2e/terminal-behavior.test.ts` |
| Output paints at most once per animation frame; leading-edge flush is enabled only while the user is pinned, so reading scrollback never triggers a forced repaint | Legacy | `terminalOutputPolicy.ts:8-88,328-331`; tests: `createTerminalWriteBatcher defers the first chunk when leading edge is disallowed`, `stays trailing-edge across a quiet window when leading edge is disallowed`, `flushes immediately again after a quiet window`, `gates write-batcher leading-edge flush on scroll-follow pin state` (`TerminalRawView.test.ts:559`) |
| Pinch-end calls `host.pinchEnded()` → `layoutPolicy.pinchEnded()` + `schedulePostLayoutRefit()` so the rewrap lands in one pass | Legacy Ghostty | `TerminalRawView.svelte:583-586,820-846` |
| Zero-lag echo overlay: a positioned `<div>` painted from `term.buffer.active.cursorX/Y` mirrors typed text before the PTY echo; idle-cleared at 300ms if the real echo never advances the cursor; `clearIfEchoedIn` drops the prediction when the real echo moves the cursor off the prediction anchor or matches the pending text | Legacy Ghostty | `terminalZeroLag.ts`; tests: `append then text`, `beforeinput then matching onTerminalData`, `clearIfEchoedIn clears when pending is a substring`, `consumes matching prefixes from sequential chunks`, `force-clears an unmatched prediction after the idle window`, `clears the prediction once the real echo advances the cursor`, `keeps the prediction while the cursor has not moved`; e2e: `e2e/terminal-zero-lag.test.ts:55,65` |
| iOS raises a text-magnifier loupe and system edit callout from the contenteditable terminal host; both fight the synthesized long-press and need `user-select: none` + `-webkit-touch-callout: none` to be suppressed | Bug excluded | `TerminalRawView.svelte:1492-1498` |
| Ghostty's `touchend` focuses the hidden textarea and pops the iOS keyboard on every copy; Ajax `stopPropagation`s after a copy gesture so a long-press stays a copy | Bug excluded | `terminalGestures.ts:201-215` |
| iOS animates the visual viewport over several frames after a layout change; a single fullscreen snap reads pre-animation values, so the snap is repeated through ~260ms / 2 rAFs | Bug excluded | `TerminalRawView.svelte:823-887` (uses `EXPAND_REWRAP_MS`); `terminalLayoutPolicy.ts:83-101` |

## 5. Reconnect / restoration

| Behavior | Class | Evidence |
| --- | --- | --- |
| Capped exponential backoff 1s→15s, reset on every open | Product | `terminalConnection.ts:39,143-180` |
| 5 immediate failures before going to `unavailable` (manual Reconnect resets) | Product | `terminalConnection.ts:40,186-194,237`; test: `gives up after repeated immediate failures` |
| Server `{"type":"error"}` frame → `unavailable` immediately, no reconnect | Product | `terminalConnection.ts:109-114,186-191`; test: `treats a server error frame as unavailable` |
| `visibilitychange → visible` while `reconnecting` → immediate redial (mobile Safari kills the socket on background) | Product | `terminalConnection.ts:198-202`; test: `foreground visibility reconnect dials with seed=0` |
| Re-open (isReconnect && seeded): `term.reset()`, scroll follow reset, snap to bottom; resize dedupe reset | Product | `TerminalRawView.svelte:975-989` |
| Re-open (isReconnect && !seeded): keep the local buffer, no reset | Product | `TerminalRawView.svelte:980-986` |
| Orphan ephemeral grouped sessions are reaped on web server start | Product | `runtime.rs:354-356`; `terminal_pty.rs:266-285` |
| Status banner shows observable connection semantics: connecting, connected, reconnecting, unavailable, and no live session | Product | `terminalConnection.ts:39-194`; `e2e/terminal-behavior.test.ts` |
| Renderer-specific status label text differs (`No live session` on Ghostty vs `Unavailable` on xterm) | Legacy | `TerminalRawView.svelte:165-170`; `XtermTerminalView.svelte:48-53` |
| Bridge `error` JSON frame is shown in the status detail; `Copied` / paste notices are a separate channel and never overwrite it | Product | `TerminalRawView.svelte:91-108,1289-1305`; `terminalClipboard.ts:130-148`; test: `terminalClipboard.test.ts:84,122` |

## 6. Settings

| Setting | Class | Surface | Evidence |
| --- | --- | --- | --- |
| Pinch-adjusted terminal text density persists across reload | Product | No UI; observable after reload | `terminalGeometry.ts:53-73`; `e2e/terminal-behavior.test.ts` |
| `ajax.terminal.fontSize` storage key, default 13px, pinch range 7–20px, and `persistedFontSize`/`persistFontSize` helpers | Legacy | `localStorage` | `terminalGeometry.ts:21-26,46-73`; tests: `fitCapFontSize`, `pinchFontSize` |
| `ajax.terminal.surfaceV2` (default **off**) | Legacy | Dev settings toggle; `localStorage`; `storage` event for cross-tab | `terminalSurfaceSetting.ts:1-50`; `SettingsView.svelte:113-127,134`; tests: `terminalSurfaceSetting`, `SettingsView.test.ts` |
| `ajax.terminal.surfaceV2.lastError` (sessionStorage) | Legacy | Read-only on Settings; set by `TerminalSurfaceSelector` on xterm init failure | `TerminalSurfaceSelector.svelte:30,40-41`; `SettingsView.svelte:96,135` |

**Fixed defaults (not user-configurable from the UI):** theme
`#1c1714 / #f4eee0 / #52a095` and `cursorBlink: true`
(`TerminalRawView.svelte:1071-1075`; `XtermTerminalView.svelte:40-46`);
scrollback 2000/10000 from media-query match
(`terminalGeometry.ts:7-24`); default font 13
(`terminalGeometry.ts:25`); pinch bounds 7–20
(`terminalGeometry.ts:21-22`); `TERM=xterm-256color` for the tmux child
(`terminal_pty.rs:24,116-117`); capture-pane depth 10000
(`terminal_pty.rs:209-219`); `status-interval=5`, `visual-activity=off`,
`visual-bell=off` on the ephemeral session only
(`terminal_pty.rs:194-198`).

## 7. Ghostty integrations and workarounds

| Integration | Class | Evidence |
| --- | --- | --- |
| Library: `github:rcarmo/ghostty-web#v0.9.4` (fork of coder's unmaintained npm 0.4.0) | Legacy Ghostty | `architecture.md:694-696` |
| WASM asset `/ghostty-vt.wasm` served by `ajax-cli web`; `Cache-Control: no-store` | Legacy Ghostty | `runtime.rs:319,544-546,1467-1470` |
| `term.scrollToBottom` is replaced on the instance so the library cannot yank scrollback during output | Legacy Ghostty | `TerminalRawView.svelte:1087-1088` |
| Public `term.select()` bypassed — `selectionManager.selectionStart/End` written directly because the library reads in `scrollbackLength + row − viewportY` space | Legacy Ghostty | `TerminalRawView.svelte:59-77,534-541` |
| `attachCustomKeyEventHandler` returns `false` for `Backspace`/`Delete` so the library's `keydown.preventDefault` does not cancel the iOS `beforeinput` repeat loop | Legacy Ghostty | `TerminalRawView.svelte:1094-1097`; test: `TerminalRawView.test.ts:674` |
| `term.open(scaleLayer)` never opens into the host (scale on host crushed the viewport to the top-left and broke expand) | Legacy Ghostty | `TerminalRawView.svelte:1078-1082` |
| Renderer cell metrics (`term.renderer.getMetrics()`) preferred over the FitAddon when measuring cell width; FitAddon reserves 15px for the suppressed scrollbar | Legacy Ghostty | `TerminalRawView.svelte:400-490` |
| `smoothScrollDuration: 0` because Ajax synthesizes 100% of scrolling and reads `getViewportY()` as an integer | Legacy Ghostty | `TerminalRawView.svelte:1064-1070` |
| `scrollbarWidth: 0` (Ajax synthesizes the scroll spacer) | Legacy Ghostty | `TerminalRawView.svelte:1063` |
| Cross-realm / jsdom `ArrayBuffer` decoded via `Object.prototype.toString.call === "[object ArrayBuffer]"` because `MessageEvent.data instanceof ArrayBuffer` fails in jsdom (test seam) | Legacy | `terminalConnection.ts:76-93,253-298` (tests) |
| Private Ghostty casts go through `unknown`; selection internals live in one place (`TerminalSelectionInternals`) so no Ghostty-private API is scattered across components | Legacy Ghostty | `TerminalRawView.svelte:53-77` |
| iOS treats a fully clipped/opacity:0 textarea as a non-editable target; the softened clip (`opacity:0.01`, `clip-path:none`, `caret-color:transparent`) lets native Paste reach the element without exposing typed characters | Bug excluded | `TerminalRawView.svelte:318-331,1518-1534` |
| Ghostty is **not** mounted or preloaded while `surfaceV2` is on; the experimental mount lives in `XtermTerminalView.svelte` | Legacy | `terminalPreload.ts:22-27`; `TerminalSurfaceSelector.svelte:48-67`; `architecture.md:690-705` |
| xterm init failure surfaces an error + `Retry`; **no** auto-fallback to Ghostty while the flag stays on | Legacy | `TerminalSurfaceSelector.svelte:27-44,49-60`; `TERMINAL.md:33-35` |

## 8. Existing tests and infrastructure

**Permanent compatibility suite (keep):**

- `e2e/terminal-behavior.test.ts` — engine-neutral `mobile-webkit` behavior
  contract (lifecycle, I/O, resize outcomes, scroll/touch, settings persistence).
- `src/terminalConnection.test.ts` — public WebSocket decode, reconnect, and
  ordered output transport.
- Rust PTY/runtime tests below — backend boundary, not renderer-specific.

Removable Ghostty/xterm characterization is indexed in
`TERMINAL_LEGACY_SURFACE_TESTS.md`.

**Rust unit tests (`crates/ajax-web/src/`) — keep:**

- `slices/terminal.rs`: `prepare_task_terminal_returns_registered_session_and_task_target`, `prepare_task_terminal_returns_task_not_found_for_unknown_handle`, `prepare_task_terminal_returns_session_missing_for_empty_tmux_session`.
- `adapters/terminal_pty.rs`: `tmux_attach_command_plan_uses_registered_session_and_task_target`, `tmux_attach_target_never_uses_browser_handle`, `tmux_attach_command_uses_clear_capable_terminal_type`, `isolated_attach_plan_creates_grouped_session_then_attaches`, `seed_history_query_parsing`, `remaining_resize_wait_deadline`, `isolated_attach_plan_seeds_browser_scrollback_from_task_window`, `history_capture_preserves_display_wrapping`, `reaper_targets_only_ephemeral_grouped_sessions`, `isolated_attach_cleanup_kills_ephemeral_session`, `isolated_attach_sessions_are_unique_per_call_and_never_the_shared_session`, `terminal_output_flush_constants_match_targets`, `terminal_output_frame_bytes_returns_raw_bytes_for_binary_send`, `filter_scrollback_hostile_sequences_strips_targets_and_carries_split_sequences`, `filter_strips_hostile_sequences_fed_one_byte_at_a_time_without_prefix_leaks`, `handle_input_frame_accepts_resize_without_data`, `cleanup_spawned_child_kills_and_waits`, `terminal_cleanup_runs_wait_on_blocking_task`, `terminal_cleanup_does_not_wait_forever_after_kill`.
- `runtime.rs`: `axum_api_access_policy_classifies_public_and_protected_routes`, `axum_router_serves_static_shell_and_cockpit_json`, `axum_task_terminal_requires_browser_session_cookie`, `axum_task_terminal_rejects_non_upgrade_requests`, `axum_task_terminal_rejects_cross_site_websocket_origin`, `websocket_origin_policy_accepts_same_origin_host`, `server_health_remains_responsive_after_terminal_disconnect_cleanup`.

**Vitest — permanent boundary tests (keep):**

- `terminalConnection.test.ts`: backoff, foreground redial, manual reconnect, JSON/Blob/ArrayBuffer decode, error frames, JSON resize, ordered/split UTF-8 corpora.
- `terminalClipboard.test.ts`: paste fallback, copy overlay, notice, dispose, onChange emission.
- `terminalOwnership.test.ts`: `TERMINAL.md` and `architecture.md` are present and linked.

**Vitest — removable renderer characterization (see legacy index):**

- `terminalGeometry.test.ts`, `terminalRefit.test.ts`, `terminalLayoutPolicy.test.ts`, `terminalOutputPolicy.test.ts`, `terminalZeroLag.test.ts`, `terminalPreload.test.ts`, `terminalSurfaceSetting.test.ts`, `terminalSelection.test.ts`, `terminalTouchScroll.test.ts`, `viewport.test.ts`, `TerminalRawView.test.ts`, `TerminalSurfaceSelector.test.ts`, `XtermTerminalView.test.ts`, `SettingsView.test.ts` (surface V2 portions).

**Playwright (`playwright.config.mts`):**

- **In scope:** `mobile-webkit` (`iPhone 15 Pro`) permanent suite
  `e2e/terminal-behavior.test.ts`.
- **Inventory only:** `desktop-chromium` and legacy engine-specific files
  (`terminal-scroll.test.ts`, `terminal-scroll-garble.test.ts`,
  `terminal-zero-lag.test.ts`, `fullscreen-refit.test.ts`,
  `layout-scroll.test.ts`, `smoke.test.ts`, `actions.test.ts`,
  `visual.test.ts`) — removable characterization; see
  `TERMINAL_LEGACY_SURFACE_TESTS.md`.

**Deployed iPhone-sized Playwright WebKit proxy:** `scripts/ios-terminal-smoke.mjs` — Playwright
WebKit, `iPhone 15` device emulation, headless; requires `CF_ACCESS_CLIENT_ID`
+ `CF_ACCESS_CLIENT_SECRET` against a deployed `ajax-cli web`. Asserts
dashboard chrome, scroll lock on task open, fullscreen expand/collapse, history
swipe, typing, control keys, Ship/Drop confirmation, Back. **This is a proxy,
not physical Safari** (see §9).

## 9. Automation gaps (physical-iOS-only behavior)

These cannot be proven by Playwright mobile WebKit. Mark them Physical iOS.

| Gap | Why it is physical-iOS | Evidence / where to verify |
| --- | --- | --- |
| Native long-press loupe + iOS edit callout suppression | Loupe/callout are system overlays WebKit does not emulate. | `TerminalRawView.svelte:1492-1498`; manual on a real iPhone. |
| Native Paste targeting of the softened textarea | iOS Paste eligibility is a Safari-engine behavior; WebKit driver emulates a touch but the OS paste sheet is not part of headless. | `TerminalRawView.svelte:318-331,684-710,1518-1534`; `scripts/ios-terminal-smoke.mjs` exercises the button, not the loupe. |
| `beforeinput deleteContentBackward` key-repeat on held Backspace | iOS-only input mode. | `TerminalRawView.svelte:292-303,1000-1006,1094-1097`; tests: `TerminalRawView.test.ts:674,686,699`. |
| Address-bar drift + iOS 26 ~24px visual/layout discrepancy around the keyboard | Real keyboard animates; WebKit driver has none. | `viewport.ts:12-15,76-105`; the `KEYBOARD_OPEN_DELTA_PX=150` / close `100` hysteresis exists for that. |
| iOS `interactive-widget=resizes-content` actually shrinking the layout viewport | Safari honors it; other engines may not. | `app.html:5`; assertion only meaningful on a real iPhone. |
| `maximum-scale=1` actually capping iOS focus-zoom | iOS-only zoom path. | `app.html:5`; test: `e2e/fullscreen-refit.test.ts:142` only checks the served meta. |
| Two-finger pinch latching iOS page zoom on second touch | WebKit driver does not always reproduce the page-zoom latch. | `viewport.ts:108-134`; `terminalGestures.ts:82-99`. |
| Alt-screen TUIs (`less`, `vim`, etc.) inside the live tmux-attach under a real iPhone Safari session | Engine + WebGL canvas + viewport layout are not 1:1 with WebKit emulation. | Bake-off checklist in `TERMINAL.md:38-48` is iPhone-targeted. |
| `visibilitychange` race when iOS suspends the tab while a reconnect is in flight | WebKit driver can force background, but the suspended-socket close timing differs from real iOS. | `terminalConnection.ts:198-202`; `TerminalRawView.svelte:201-202` (smoke). |

> **Note for physical-iOS verification:** Playwright mobile WebKit—including
> `scripts/ios-terminal-smoke.mjs`, which is itself only a deployed WebKit
> proxy—is not physical Safari. Anything that depends on the OS (loupe,
> callout, paste sheet, address bar, focus-zoom, page-zoom latch,
> soft-keyboard animation, native selection, touch momentum) is confirmed only
> by a manual session on a real iPhone in Safari, not by CI Playwright or the
> smoke script.
