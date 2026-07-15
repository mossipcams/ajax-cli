# Experimental Wterm stabilization — built-in core

## Scope

Make experimental Surface V2 usable on normal iOS Safari by using Wterm's
built-in core with stable logical geometry, then repair history, viewport,
interaction, and background behavior.

## Non-goals and boundaries

- Keep the default `ghostty-web` terminal unchanged and selected whenever
  `ajax.terminal.surfaceV2` is off.
- Keep `@wterm/ghostty`, its loader, WASM asset, and existing tests installed;
  this pass stops using it for Surface V2 but does not remove it.
- Keep all changed behavior behind `ajax.terminal.surfaceV2`, off by default.
- Keep the existing authenticated WebSocket and backend PTY/tmux ownership.
- Do not add an Ajax history buffer, dependency fork, private Wterm API cast,
  second resize observer, replay path, or timer-based scroll restoration.
- Do not edit files under a `tests/` directory or weaken unrelated assertions.

## Decision and approval

- Core decision: Surface V2 uses the already-installed `@wterm/core`
  `WasmBridge`; normal Ghostty remains the product default.
- Geometry decision: Wterm uses a stable 80x24 grid with `autoResize: false`.
  CSS may crop/scroll that grid as the visual viewport changes, but viewport
  jitter must not rebuild terminal history.
- Delegation decision: delegated via model-router after plan approval.
- Previous Task 2 packet: superseded; its Ajax scroll-anchor approach targeted
  the wrong layer because `@wterm/ghostty@0.3.0` cannot read scrollback rows.
- Approval status: approved by user (`delegate and implement`).

## Task 1 — Select the built-in core and remove wasted preload (5–15 min) ✅

- Test: make the focused preload/component tests fail unless Surface V2 avoids
  the 400 KB `@wterm/ghostty` preload and constructs Wterm with an explicit
  built-in `WasmBridge`, `cols: 80`, `rows: 24`, and `autoResize: false`.
- Implementation: add `@wterm/core@0.3.0` as the direct dependency already
  required transitively, load `WasmBridge` in `WtermTerminalView.svelte`, and
  stop `terminalPreload.ts` from warming `@wterm/ghostty`. Leave its package,
  loader, asset, and loader tests intact.
- Verify: focused preload and component Vitest files, then `npm run web:check`.

## Task 2 — Prove real history works without terminal rebuilds (5–15 min) ✅

- Test: add a real built-in-core contract that writes numbered output and
  asserts old scrollback cells contain text, then extend the mobile-WebKit
  Surface V2 case to scroll upward, append output, change viewport height, and
  prove old/new markers occur once and the view does not jump to the bottom.
- Implementation: no Ajax history code. Keep the stable grid through output
  and viewport changes; only fix public host scroll-follow state if the real
  browser test exposes an Ajax-owned defect.
- Verify: the new real-core Vitest contract and the named mobile-WebKit history
  case red then green; run all Surface V2 mobile-WebKit cases.

## Task 3 — Make the fixed grid fit viewport and keyboard changes (5–15 min) ✅

- Test: add focused component/mobile-WebKit cases for keyboard open/close,
  rotation, and expand enter/exit. Require the host to follow available height,
  retain history, avoid local `resize()`, and reannounce exactly 80x24 after
  keyboard close/reconnect.
- Implementation: override Wterm's `autoResize: false` inline locked height
  with the existing flex host CSS; reuse `viewport.ts`, layout policy, and
  refit scheduling only for crop/focus/server reannouncement. No new sizing
  abstraction or observer.
- Verify: viewport/layout/refit/component focused tests plus mobile-WebKit.

## Task 4 — Preserve input, modes, and reconnect behavior (5–15 min) ✅

- Test: run the existing input, control-key, bracketed-paste, application
  cursor, alternate-screen, UTF-8, reconnect, and PTY-size contracts against
  the built-in core; add only a focused failing assertion for a confirmed gap.
- Implementation: use the explicit `WasmBridge` for mode queries already made
  by the component; change only behavior proven broken.
- Verify: component, connection, built-in-core integration, and mobile-WebKit
  Surface V2 suites.

## Task 5 — Remove the background wash while preserving cell colour (5–15 min) ✅

- Test: render several ANSI cell backgrounds plus a coloured status row and
  require the host/grid/row canvas to remain opaque `#1e1e1e` while coloured
  cell spans remain coloured.
- Implementation: the minimum Surface V2 CSS override that neutralizes
  Wterm's renderer-written grid/row background and box shadow; do not suppress
  span-level ANSI colours.
- Verify: named mobile-WebKit colour case and focused component test.

## Task 6 — Experimental gate and full validation (5–15 min) ✅

- Test: add no new framework; add an assertion only if integration reveals a
  missing Surface-V2-off guard.
- Implementation: remove redundant new code only. Do not touch default Ghostty
  behavior, routes, backend protocol, or terminal ownership. Update
  `crates/ajax-web/web/TERMINAL.md` so the experiment accurately records its
  built-in core, fixed geometry, and retained inactive Wterm Ghostty package.
- Verify:
  - `npm run web:check`
  - focused Wterm/preload/connection/viewport Vitest files
  - mobile-WebKit `terminal-surface-v2.test.ts`
  - `npm run web:test -- --run`
  - `npm run web:build:check`
  - Surface-V2-off selector/preload coverage proving Ghostty is unchanged

## Validation ledger

- Task 1 RED: focused preload/component suite failed on the two intended
  assertions: Ghostty-core preload still called once and `WasmBridge.load`
  not called (exit 1).
- Task 1 GREEN (delegate): focused suite passed 51 tests with 3 existing todos;
  `npm run web:check` passed with 0 errors and 0 warnings.
- Task 1 parent verification: same 51 tests/3 todos passed and `web:check`
  passed. Review gate ACCEPT; six allowed files changed, no scope violations.
- Task 2 characterization: real built-in core history test passed; mobile
  WebKit history passed through output/viewport but exposed a final 16 px
  row-aligned bottom gap after typing (focused smoke exit 1).
- Task 2 follow-up RED/GREEN: reused `snapToNewest()` in the shared accepted
  input path (one production line). Focused history smoke then passed.
- Task 2 parent verification: all 4 mobile-WebKit Surface V2 cases passed;
  component/core suites passed 48 tests with 3 existing todos; `web:check`
  passed. Review gate ACCEPT; no scope violations.
- Task 3 RED: source contract and expanded mobile-WebKit host-height checks
  failed while Wterm's inline locked height won.
- Task 3 GREEN: one CSS declaration lets existing flex layout own height while
  keeping the 80x24 grid fixed. Delegate process was terminated at the ceiling
  after writing its complete report; snapshot showed a coherent scoped delta.
- Task 3 parent verification: all 5 mobile-WebKit cases passed; component/core
  suites passed 49 tests with 3 existing todos; `web:check` passed. Review gate
  ACCEPT; no scope violations.
- Task 4 characterization: the real built-in core passed new contracts for
  application cursor keys, bracketed paste, alternate-screen restoration, and
  UTF-8 viewport cells. No production change was required.
- Task 4 parent verification: the real-core suite passed 5 tests; component and
  connection suites passed 57 tests with 3 existing todos; `web:check` passed.
  Review gate ACCEPT; only the allowed integration-test file changed.
- Task 5 RED: the named mobile-WebKit mount case found renderer-owned row canvas
  paint was not neutral (`rgba(0, 0, 0, 0)` observed where the dark terminal
  background was required); the existing grid override alone was insufficient.
- Task 5 GREEN: one scoped CSS rule overrides row background/shadow only; all
  rows stay dark and at least one ANSI child span remains coloured.
- Task 5 parent verification: all 5 mobile-WebKit Surface V2 cases passed;
  component tests passed 49 tests with 3 existing todos; `web:check` passed.
  Review gate ACCEPT; the raw diff matched the three allowed files.
- Task 6 documentation: `TERMINAL.md` now records built-in `@wterm/core`, fixed
  80x24 geometry, experimental gating, default Ghostty preservation, and the
  retained inactive Wterm Ghostty adapter. Diff check passed; gate ACCEPT.
- Task 6 initial full gate: mobile-WebKit passed 5/5 and `web:check` passed, but
  the all-web suite failed 1 of 634 tests because the selector failure-path test
  still mocked the no-longer-called Wterm Ghostty loader. `web:build:check`
  built successfully but failed its stale requirement that `terminal.js`
  reference the now-inactive Wterm Ghostty WASM path. Both failures are being
  repaired as separate bounded delegations before the gate is rerun.
- Task 6a RED/GREEN: the focused selector suite reproduced the stale-mock
  failure, then passed all 5 tests after moving only the test seam to
  `WasmBridge.load()`. Component/preload tests passed 53 tests with 3 existing
  todos and `web:check` passed. Parent verification matched; gate ACCEPT.
- Task 6b RED/GREEN: the build completed but the old guard required the inactive
  adapter path; the guard now keeps both WASM assets present/distinct, requires
  normal Ghostty in the terminal chunk, and forbids the inactive Wterm adapter
  path. Parent `web:build:check` passed; gate ACCEPT.
- Task 6 final parent gate: all 45 web unit files passed (631 tests, 3 existing
  todos); the focused terminal/gating set passed 96 tests with the same 3 todos;
  all 5 mobile-WebKit Surface V2 cases passed; `web:check`,
  `web:build:check`, and `git diff --check` passed. The successful versioned
  `dist` bundle was regenerated; `terminal.js` is 5,566 bytes smaller than HEAD.
- The commit hook subsequently ran the complete repository gate: formatting,
  check, clippy, 1,576 nextest tests, doc tests, web checks/tests, and a release
  build all passed.
- Reassessment probe: `@wterm/ghostty@0.3.0` reported 91 scrollback rows but
  returned length 0 and blank cells for every row.
- Reassessment probe: built-in `WasmBridge@0.3.0` returned readable numbered
  history before resize.
- Reassessment probe: built-in vertical shrink from 10 rows to 5 corrupted the
  five rows entering scrollback; horizontal-only resize retained sampled
  history. Stable 80x24 geometry therefore avoids the known failure path.
- No production or test files changed during reassessment/replanning.

## Deviations

- Task 1's accepted `@wterm/ghostty` preload optimization is now intentionally
  bypassed by Surface V2 because the selected built-in core is embedded. Its
  loader implementation/tests remain rather than deleting assertions during
  this behavior change.
- The old Task 2 delegation delta remains discarded.
- Final validation exposed two Option-2 guard updates omitted from the original
  task list: the selector init-failure mock must target built-in `WasmBridge`,
  and the build check must reject rather than require the inactive adapter path.
