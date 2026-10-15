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
- [ ] Exercise the session transcript in an iOS WebKit target, including initial placement, live follow, scrolling away from the live edge, loading older messages, and keyboard/history restoration. Blocked: both the repository `mobile-webkit` project and a minimal direct WebKit `newPage` smoke timed out during page setup in this environment.

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
