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
- [ ] Add an optional chronological `transcript` field to reset snapshots in `crates/ajax-web/src/slices/web_session/protocol.rs` and populate it in `replay.rs::build_snapshot_and_replay`.
- [ ] Keep protocol version 2 and continue emitting replay envelopes so older clients remain compatible.
- [ ] Parse the optional snapshot transcript in `web/src/features/chat/session/transport/parse.ts` and commit it with the synthetic `ready` event in one reducer transition.
- [ ] Deduplicate following replay envelopes through the committed cursor; preserve the existing envelope path when the field is absent.
- [ ] Omit the field for resumable reconnects (`reset: false`); use an empty array for an empty reset snapshot.
- [ ] Preserve server ownership of transcript truth and chronological message order.

### Follow-up verification

- [ ] Add focused protocol/replay coverage for reset, resume, retention-floor, and empty-session snapshots.
- [ ] Add focused parser/reducer coverage for atomic transcript commit, legacy absence, malformed payload rejection, and replay-envelope deduplication.
- [ ] Add a browser regression proving the first visible transcript paint contains the actual retained tail at the live bottom, with no partial-prefix walk or post-paint scroll correction.
- [ ] Run the focused Rust Web Session tests, focused frontend transport/reducer tests, Web typecheck, and the documented local verification gate.
- [ ] Re-run the browser regression on iOS WebKit before commit; if local WebKit remains blocked, report the exact failure and require the GitHub WebKit job before merge.

### Documentation and compatibility

- [ ] Update `docs/architecture/web-session-behavior.md` with reset-snapshot transcript presence, cursor deduplication, and fallback semantics.
- [ ] Update `docs/architecture/web-cockpit.md` with the atomic initial projection contract.
- Old clients ignore the optional field and consume existing envelopes. New clients connected to old servers retain current incremental replay behavior. Resumable reconnects remain unchanged.
