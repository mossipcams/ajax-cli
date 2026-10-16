# Ajax Web Bottom-First Rendering

## Scope

- Use the existing nested transcript structure to make the outer session thread a reversed flex scroll container while preserving chronological DOM order.
- Update viewport bottom coordinates, history prepend restoration, and live-follow behavior for negative offsets in the reversed container.
- Update the owning Web Session behavior contract if its scroll-coordinate description changes.
- Verify focused frontend tests and an iOS WebKit browser path before any commit.

## Non-goals

- No message-order reversal, transcript restructuring, loading phase, dependency, or public API change.
- No changes to backend session ownership or transcript authority.
- No commit, push, or pull request in this task.
- No edits under `tests/` unless the user separately authorizes them.

## Approval

- [x] Immediate implementation explicitly requested by the user on 2026-10-14.

## Implementation

- [x] Inspect the affected scrolling source, callers, and existing tests.
- [x] Apply reverse outer container and normal inner column CSS.
- [x] Treat the live bottom as `scrollTop = 0` in session viewport logic.
- [x] Preserve the reader's position across older-message reveals with negative reversed-container offsets.
- [x] Remove initial bottom-scroll correction work while preserving live-follow, keyboard, and restoration behavior.
- [x] Update the owning behavior documentation when its coordinate contract changes.

## Verification

- [x] Run the focused viewport and full chat-scrolling test suites.
- [x] Run the relevant frontend typecheck.
- [ ] Exercise the session transcript in an iOS WebKit target, including initial placement, live follow, scrolling away from the live edge, loading older messages, and keyboard/history restoration. Blocked: both the repository `mobile-webkit` project and a minimal direct WebKit `newPage` smoke timed out during page setup in this environment (re-confirmed on 2026-10-15 CI-repair run: every mobile-webkit test hit the 30s page-setup timeout before any assertion ran).

## Deviations and assumptions

- The implementation was split into bounded Qwen slices after the first broad run exceeded the router deadline during WebKit setup.
- Co-located `*.test.ts(x)` files were updated because they are outside the protected `tests/` directory and their positive-coordinate assertions had to migrate with the behavior.
- No commit was created; the requested pre-commit WebKit validation remains blocked by browser startup/page creation.

## Results

- Focused viewport tests: 11/11 passed.
- Full chat scrolling suite: 49/49 passed.
- `npm run -s web:check`: passed.
- `mobile-webkit` session regression and keyboard suites: blocked during `page` setup (11 recorded 30-second setup timeouts before assertions ran).
- Minimal direct Playwright WebKit launch/new-page smoke: blocked by the same page-creation stall.
- 2026-10-15 CI repair (PR #1300): Web Unit failures were exact CSS baseline drift in `styleSources.ts` (sourceStylesCssBytes 117_194→117_387, builtAppCssBytes 99_526→99_544, builtAppCssGzipBytes 16_594→16_604; selector counts unchanged). The five Web E2E failures were legacy positive scroll-coordinate assertions/setup in `e2e/session-chat-keyboard.test.ts` and `e2e/session-chat-regression.test.ts`; ported to reversed-container equivalents (live edge = scrollTop 0, negative toward history) while retaining the existing 48px thresholds and all behavioral intent.
- Verification for the repair: focused style baseline tests (`src/styles.architecture.test.ts`, `src/shared/lib/styleSources.test.ts`) 18/18 passed; `npm run -s web:check` passed (exit 0); local mobile-webkit E2E re-blocked at page creation (exact result above) so live-edge and anchor-preservation behavior in WebKit is verified by CI after push only. Remaining risk: no local WebKit execution evidence.

## Confirmed defect follow-up: initial transcript hydration

- Tracking issue: [#1301](https://github.com/mossipcams/ajax-cli/issues/1301).
- The reversed scroll geometry is correct: at `scrollTop = 0`, the settled newest message ends at the viewport bottom.
- Existing sessions still appear to load from the top because the server sends retained history as individual chronological WebSocket envelopes. The browser paints each partial prefix while remaining correctly pinned to the end of that prefix.
- Client-side buffering would remove the visible walk by leaving the transcript blank until replay settles. That conflicts with the approved requirement of no hidden loading phase.

### Recommended architecture change

- [x] Approved by the user on 2026-10-15: extend the Web Session snapshot contract so a reset snapshot may include the retained transcript atomically, and create a new fix branch.
- [x] Add an optional chronological `transcript` field to reset snapshots in `crates/ajax-web/src/slices/web_session/protocol.rs` and populate it in `replay.rs::build_attach`.
- [x] Keep protocol version 2 and continue emitting replay envelopes so older clients remain compatible.
- [x] Parse the optional snapshot transcript in `web/src/features/chat/session/transport/parse.ts` and commit it with the synthetic `ready` events in one reducer transition.
- [x] Deduplicate the exact compatibility replay cursors; preserve the existing envelope path when the field or snapshot consumer is absent.
- [x] Omit the field for resumable reconnects (`reset: false`); use an empty array for an empty reset snapshot.
- [x] Preserve server ownership of transcript truth and chronological message order.

### Follow-up verification

- [x] Add focused protocol/replay coverage for reset, resume, retention-floor, and empty-session snapshots.
- [x] Add focused parser/reducer coverage for atomic transcript commit, legacy absence, malformed payload rejection, and replay-envelope deduplication.
- [x] Add a browser regression proving the first visible transcript paint contains the actual retained tail at the live bottom, with no partial-prefix walk or post-paint scroll correction.
- [x] Run the focused Rust Web Session tests, focused frontend transport/reducer tests, Web typecheck, lint, architecture verification, and the documented PR helper gate.
- [ ] Re-run the browser regression on iOS WebKit before commit. Blocked: the `mobile-webkit` project timed out after 30 seconds while setting up `page`, before the test body ran. The GitHub Web E2E job is required before merge.

### Documentation and compatibility

- [x] Update `docs/architecture/web-session-behavior.md` with reset-snapshot transcript presence, cursor deduplication, and fallback semantics.
- [x] Update `docs/architecture/web-cockpit.md` with the atomic initial projection contract.
- Old clients ignore the optional field and consume existing envelopes. New clients connected to old servers retain current incremental replay behavior. Resumable reconnects remain unchanged.

### Follow-up results

- Rust protocol tests: 5/5 passed; replay tests: 9/9 passed; `cargo fmt --check`: passed.
- Focused frontend transport/connection/reducer tests: 113/113 passed; full frontend unit suite: 1,546 passed and 9 skipped.
- Web typecheck, lint, and `npm run verify:arch`: passed.
- Desktop Chromium session regression: 9/9 passed; the #1301 test recorded `Earlier answer 11` as the first agent paint at `scrollTop = 0`.
- Full Rust Web Session run exposed existing test isolation: parallel execution failed seven tests after a shared forced-write-failure leaked; serial execution passed 745 tests and failed one timing-sensitive reliability assertion, which passed alone.
- Local mobile WebKit: blocked during Playwright `page` setup after 30 seconds, before assertions.

## Follow-up: bottom load (tail-first transfer)

Goal restated by the user on 2026-10-15: the transcript must *load* from the bottom (newest first), not merely paint at the bottom. PR #1302 delivers the paint only.

### Evidence

- `replay.rs::build_attach` puts the whole retained log (`from = 0` or the retention floor) into `snapshot.transcript`, so open cost grows with history and nothing renders until all of it arrives.
- The server still emits every event again as a compatibility envelope; updated clients discard them by cursor. Open payload is ~2x.
- "Load earlier" (`useHistoryWindow.ts`) is a client-only `items.slice(windowStart)` over already-loaded items (window 150 items, reveal batch 50). There is no server fetch path for older history.
- The mid-session generation-change reset (`task_session_outbound.rs::snapshot`, `reset = true`) builds a snapshot with no `transcript`, so that path still replays top-first.

### Scope

- Reset snapshot carries only the newest tail of the retained log plus the cursor where the tail starts (`transcriptFrom`).
- Older history is fetched on demand through a read-only endpoint (recommended: HTTP GET `.../session/transcript?before=<cursor>&limit=<n>`; the server stays the transcript authority).
- Client "Load earlier" fetches the older range when the loaded tail is exhausted and prepends it by re-projecting from the earliest loaded event (the reducer is append-only today, so prepend must rebuild items, not splice).
- Updated clients stop receiving duplicate compatibility envelopes (needs a client capability signal, e.g. a `snapshotTranscript=1` query flag); legacy clients keep them.
- Generation-change reset snapshots carry the tail too.

### Non-goals

- No client-side buffering or hidden loading phase; no protocol version bump; no change to transcript retention or ownership; resumable reconnects (`reset: false`) unchanged.

### Approval

- [x] Approved by the user on 2026-10-15 (HTTP GET chosen for the older-history fetch).

### Tasks

- [x] Server: tail selection (`TAIL_EVENT_LIMIT` = 300, open permission/elicitation request kept inside, snapped to a user message) in `build_attach` via `attach_inline_tail`; `transcriptFrom` and `transcriptFloor` snapshot fields; generation-change reset snapshot parity.
- [x] Server: read-only `GET /api/tasks/<handle>/session/transcript?before&limit` (`replay::read_before`, persisted log, no actor command); `snapshotTail=1` opt-in skips per-event compatibility envelopes on cold attach and generation-change resets.
- [x] Client: parse `transcriptFrom`/`transcriptFloor`; discard envelopes below the floor (cursor-0 socket errors exempt, older `prompt_accepted` still acknowledged); `history_bounds` + `prepend` reducer events; `fetchEarlierTranscript`; `useEarlierHistory` wiring with single in-flight, abort, stale-bounds discard, retry label; send `snapshotTail=1` when an `onSnapshot` consumer exists.
- [x] Docs: `web-session-behavior.md`, `web-cockpit.md`.
- [x] Tests: replay/protocol/bridge/route (Rust), transport/parse/reducer/fetch/hook/scroller (vitest), `e2e/session-bottom-load.test.ts` (tail first paint, load-earlier anchor, retry, legacy server).

### Results (2026-10-15)

- Rust: `cargo fmt --check`, workspace `cargo clippy --all-targets --all-features -- -D warnings`, `cargo check --all-targets --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo test --doc` all exit 0; `cargo test -p ajax-web --lib -- --test-threads=1`: 1,125 passed. Parallel runs still show the pre-existing `set_config_option_*` isolation flake (reproduced with the new tests skipped: 6 of 8 runs).
- Web: `web:test --run` 1,636 passed / 9 skipped; `web:check`, `web:lint`, `web:sg`, `verify:arch`, `web:build:check` exit 0; `dist/app.js` rebuilt.
- Browser: desktop Chromium regression + keyboard + bottom-load suites 30 passed. The 4 bottom-load tests also pass with `scrolling/historyScroll.ts` at HEAD, so they do not depend on unrelated uncommitted scroll edits. Local `mobile-webkit` remains blocked at Playwright page setup; the GitHub WebKit job is the WebKit evidence.

### Deviations and risks

- Delivered by bounded delegate slices; Qwen timed out on several multi-file slices and the codex/claude/cursor routes became unavailable mid-run (quota, model rejected, not logged in), so later slices ran on the remaining routes. Every delta was reviewed and re-verified in the worktree; delegate reports were schema-invalid several times and were not relied on.
- Defects caught in review and fixed before acceptance: cursor-0 socket errors were swallowed by the envelope floor; the older-range route rejected same-origin fetches with no `Origin` header; an empty older page made no forward progress; a stale page could be prepended after a reset re-based the tail; a Rust test file exceeded the 1,000-line limit.
- Compatibility risk: the host now always carries the tail (not the whole log) in a reset snapshot. A browser tab still running the pre-change bundle (it parses `transcript` but only dedupes cursors inside it) can show older envelopes after the tail until reloaded. Bundle and host ship in one binary, so this is limited to tabs left open across an upgrade.
- "Load earlier" scroll-anchor behavior for the prepend path is covered in Chromium only; WebKit is covered by CI.

## Follow-up: auto-fill, armed auto-load, anchor restore (uncommitted, 2026-10-16)

Branch commits were squash-merged as #1304; this work sits uncommitted on top and needs a fresh branch off `main` before a PR.

- [x] `TAIL_EVENT_LIMIT` 300→1000; `DEFAULT_HISTORY_WINDOW` 150→400, `HISTORY_REVEAL_BATCH` 50→100.
- [x] Auto-load armed from the start; server-fetch path arms the top restore; fill loop loads older pages while the thread does not overflow.
- [x] `restoreScrollAfterTopGrowth` keeps the negative offset unchanged (top growth does not move the live-edge distance).
- [x] Test repairs: `ChatScroller.test.tsx` page 201→200 rows (row 0 stayed hidden, button could never clear); `replay_tests.rs` cursor math now adds `log.dropped` (2×1000 events exceeds the 2000-event cap); `session-bottom-load` short tail now sends cursor envelopes (raw events were rejected → "Reconnecting"), uses 2 turns (4 already overflowed desktop), measures the gap from the last message; `session-chat-regression` anchor test scrolls midway so armed auto-load does not reveal everything before the manual click.

### Results

- Web unit 1,638 passed / 9 skipped; `web:check`, `web:lint` pass.
- `cargo test -p ajax-web --lib web_session -- --test-threads=1`: 775 passed (parallel shows the known 7-test forced-write isolation flake).
- desktop-chromium session bottom-load + regression + keyboard: 31 passed.
- [ ] mobile-webkit: still blocked locally at `page` setup (30s); CI is the WebKit evidence.
