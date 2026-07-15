# Ghostty terminal behavioral safety net

## Approval

- Status: approved by the user (`Delegate until finished`).
- The user's blanket continuation approval supersedes the per-task pause; run
  the tasks sequentially until complete while preserving each red/green gate.

## Scope

Create an implementation-independent, iOS-Safari-focused compatibility
contract for the current default browser terminal surface, document the
Ghostty-specific remainder as removable characterization coverage, and produce
the physical-iPhone acceptance artifacts requested by the task.

The permanent contract will assert user-visible application state and
WebSocket/PTY traffic through the rendered task route, stable test IDs, public
connection APIs, and browser events. Existing Ghostty renderer tests may remain
as legacy characterization, but they will not be cited as xterm rebuild gates.

## Non-goals

- Tasks 1–11 do not add or change xterm dependencies. Task 12 unconditionally
  removes the committed Dev-only xterm Surface V2 and Ghostty implementation
  without adding a replacement.
- Do not create a Ghostty/xterm adapter or a future controller contract.
- Tasks 1–11 do not refactor `TerminalRawView.svelte`, terminal lifecycle
  ownership, DOM, event organization, or Ghostty integration. Task 12 removes
  both existing implementations and intentionally leaves the terminal surface
  absent for the later ground-up rebuild.
- Do not assert Ghostty classes/types, private buffers, generated DOM, canvas
  pixels, internal events, arbitrary dimension floors, or current resize
  algorithms in the permanent suite.
- Do not turn observed Ghostty/iOS defects into compatibility requirements.
- Do not fix unrelated terminal defects discovered by the inventory.

## Current evidence and expected classification

- Product boundary owners: `TaskDetail.svelte` / `TerminalSurfaceSelector.svelte`
  mount one terminal; `terminalConnection.ts` owns WebSocket status, UTF-8
  decoding, reconnect, input, resize, and disposal; the protected Rust route
  resolves the registered task and `terminal_pty.rs` owns PTY/tmux I/O and
  disconnect cleanup.
- Product behavior already represented in tests: connection state/recovery,
  ordered binary output delivery, exact input frames, resize validity/dedupe,
  scroll-follow, fullscreen, clipboard fallbacks, font persistence, and
  keyboard/viewport policy.
- Legacy Ghostty characterization: WASM preload, `Terminal`/`FitAddon` mocks,
  renderer metrics, private selection manager access, disabled library
  `scrollToBottom`, hidden-textarea hardening, canvas/buffer probes, and
  Ghostty-specific scaling/scroll workarounds.
- Suspected/known defects or non-contract details to keep out of permanent
  assertions: renderer garble probes, unwanted zoom/clipping/scroll conflicts,
  resize-loop mechanics, incorrect keyboard offsets, key-repeat workarounds,
  the 80-column floor, zero-lag overlay implementation, and engine-specific
  smooth-scroll suppression unless the inventory finds explicit product-level
  evidence for the resulting behavior.
- Physical iOS only: real virtual-keyboard opening/closing and repeat,
  Safari browser chrome offsets/focus zoom, touch selection and long press,
  native copy/paste menus, orientation settling, and touch-vs-page scroll/zoom.
- Current settings contract appears limited to persisted font size plus the
  existing experimental Surface V2 flag. Theme, cursor, font family, and
  scrollback are fixed implementation defaults, not user settings.

## Delegation decision

`Delegation decision: delegated via model-router`

After approval, each bounded code/test task gets its own READY
`tdd-implementation-packet` and model-router decision. Documentation-only
inventory/matrix edits remain local under the non-code exception. The parent
will review every diff and rerun validation independently.

## Task checklist

### Task 1 — Write the behavioral inventory (documentation only)

- [x] Test to write: none; this is a documentation-only inventory and a fake
  executable test would add no signal.
- [x] Implementation: added `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
  with a focused terminal compatibility inventory that
  traces mount/readiness/disposal; WebSocket and PTY I/O; resize/fit and every
  viewport source; focus/input/clipboard/selection/scroll/touch; reconnect and
  restoration; settings; Ghostty integrations/workarounds; and existing test
  infrastructure. Classify each item Product, Legacy Ghostty, Bug excluded, or
  Physical iOS.
- [x] Verification: cross-checked inventory rows against source and tests;
  both required `rg` commands passed. The review gate revised test probes and
  Ghostty-specific algorithms away from the Product classification. A final
  cleanup of any remaining cadence-oriented phrasing is assigned to Task 9.

### Task 2 — Establish engine-neutral iPhone-WebKit browser infrastructure

- [x] Test to write first: added a fixture-level contract that selects the existing
  `mobile-webkit` iPhone project and locates the terminal only by stable
  application test ID (not engine/canvas).
- [x] Expected red captured: `terminalSurface is not a function` before the
  engine-neutral helper existed; the current shared locator was constrained by
  `data-terminal-engine='ghostty'`.
- [x] Implementation: minimally extended the existing browser fixture with
  engine-neutral surface/socket/frame helpers. Reuse
  `data-testid='task-terminal-panel'`; add no production seam unless a focused
  red test proves the existing ID insufficient. Do not add desktop projects.
- [x] Verification: focused mobile-WebKit test passed (1/1); forbidden-token
  `rg` returned no matches. Confirmed the permanent helper contains no
  `ghostty`, `xterm`, `canvas`, or renderer-probe reference.
  confirm the permanent helper contains no `ghostty`, `xterm`, `canvas`, or
  renderer-probe reference.

### Task 3 — Pin session lifecycle and connection state through the task route

- [x] Test to write first: browser tests prove one surface/one socket on open,
  visible connecting/connected/reconnecting/unavailable semantics, socket
  closure on navigation, and reopen/reconnect without duplicate sockets,
  input, status, or resize effects.
- [x] Expected red: the first discarded GLM round exposed missing/broken fixture
  controls; its delta was discarded after an unauthorized stash operation.
  The successful Cursor round added deterministic controls for
  enumerating active/closed terminal sockets; implement that test-only mock
  operation after the failure.
- [x] Implementation: extended only the browser WebSocket fixture and permanent
  behavior suite; no
  terminal production changes unless current behavior violates an intentional
  contract, in which case stop and report the bug instead of fixing it.
- [x] Verification: parent ran permanent behavior plus existing smoke tests in
  mobile WebKit: 16 passed, 0 failed. Assertions use polling and event counts,
  not arbitrary sleeps.

### Task 4 — Pin PTY output and transport ordering

- [x] Test to write first: public `connectTaskTerminal` behavior tests cover split
  UTF-8 frames, emoji/combining/wide characters, ANSI/CR/LF pass-through,
  rapid ordered chunks, output during initialization/reconnect, and a bounded
  large burst with no loss/duplication/application error.
- [x] Expected red/OTHER seam gap: the browser harness lacked an operation that
  emits binary chunks and
  drains asynchronous message work deterministically; add it only after red.
- [x] Implementation: test/fixture support only. Visible-glyph rendering remains
  manual if it cannot be asserted without Ghostty DOM/private state or canvas
  pixels; record that limitation explicitly rather than adding a renderer seam.
- [x] Verification: parent focused Vitest passed 16/16 and engine-neutral
  mobile-WebKit passed 6/6 while the corpus crossed the socket. Two earlier
  parent commands failed before tests due wrong invocation/working directory;
  corrected commands are recorded in validation results.

### Task 5 — Pin user input, key ordering, modifiers, and clipboard behavior

- [x] Test to write first: browser tests for exact-once ordered printable input;
  Enter, Backspace, Tab, Escape, arrows and supported Ctrl combinations;
  browser-repeat cardinality; multiline/Unicode paste; no input on focus/blur;
  and continued input after reconnect.
- [x] Expected red: introduce the tests against a missing engine-neutral focus
  and decoded-frame fixture operation, then add the smallest fixture support.
- [x] Implementation: fixture/test changes only. Use public controls and stable
  surface interaction; do not query the renderer textarea or canvas.
- [x] Verification: focused mobile-WebKit Playwright run; compare the ordered
  decoded PTY input frames exactly, including repeat count and Unicode content.

  Delegation result: original Task 5 was discarded after two rounds because
  stable-surface Playwright WebKit never emits Ghostty's physical-iOS
  `beforeinput` Backspace path. No skip or weakened assertion was retained.
  Backspace and hold-repeat move to Task 10 physical verification. Task 5a
  implemented the remaining automatable contract and passed parent validation.

### Task 5a — Pin automatable input behavior; reserve Backspace for physical iOS

- [x] Add exact ordered printable, Enter, Tab, Escape, arrow, repeat-cardinality,
  multiline Unicode paste, focus/blur silence, and post-reconnect input tests.
- [x] Do not send or assert Backspace in Playwright. Record single Backspace and
  hold-repeat as unautomatable physical-iPhone checks, not passing expectations.
- [x] Verify focused mobile WebKit and the renderer-neutral source guard. Parent
  run passed 11/11; the forbidden-token guard returned no matches.

### Task 6 — Pin viewport, resize, orientation, keyboard, and fullscreen results

- [x] Test to write first: deterministic browser tests for a valid initial PTY
  size, eventual final resize after meaningful viewport/orientation changes,
  duplicate suppression, no resize storm during a keyboard event burst,
  correct resize after keyboard close and fullscreen enter/exit, and no
  listener/effect accumulation across reopen.
- [x] Expected red: use a missing fixture helper that snapshots resize frames
  and dispatches controlled viewport bursts; implement it after failure.
- [x] Implementation: test fixture only unless a stable test ID is demonstrably
  missing. Assert final valid dimensions and bounded event counts, not the
  current debounce timing, fit formula, or 80-column floor.
- [x] Verification: focused mobile-WebKit Playwright run plus existing
  `viewport`, `terminalRefit`, `terminalLayoutPolicy`, and
  `terminalOutputPolicy` unit suites.

  Validation: `npm run web:smoke -- --project=mobile-webkit
  crates/ajax-web/web/e2e/terminal-behavior.test.ts` passed 17/17; unit suites
  passed 63/63; forbidden-token `rg` returned no matches. Keyboard-close flush
  uses narrower evidence when restoring to the original viewport height would
  dedupe against the pre-keyboard size; the test closes into a distinct height
  (800px) to prove a settled post-keyboard resize without timing assertions.

### Task 7 — Pin scrolling, touch ownership, fullscreen continuity, and clipboard UI

- [x] Test to write first: engine-neutral browser assertions that terminal
  scrolling does not move/zoom the page, output while reading does not yank to
  bottom, `New output` restores live output, selection/long-press does not send
  PTY input, Paste remains available, and input remains usable after touch,
  scroll, and fullscreen transitions.
- [x] Expected red: the first stable interaction-surface locator failed because
  the ID was absent; the delegated RED run captured that failure before adding
  one attribute-only production seam. Permanent tests do not reuse current
  canvas/class/probe helpers.
- [x] Implementation: tests/fixtures plus only
  `data-testid="terminal-interaction-surface"` on the existing user gesture
  target. Keep exact selection painting,
  Ghostty private selection coordinates, native iOS menus, and physical
  long-press fidelity in legacy/manual coverage.
- [x] Verification: focused mobile-WebKit run with page scroll position, PTY
  frame counts, status, and visible controls as outcomes.

  Parent validation passed 23/23 after review. The suite proves `New output`,
  no-input gestures, fullscreen input continuity, and Paste continuity. Native
  touch momentum, selection handles/Copy UI, and Safari menus remain physical.

### Task 8 — Pin current terminal settings and defaults

- [x] Test to write first: because this remained tests-only, add one public
  synthetic-pinch behavior that changes the observable PTY size, preserves
  input, and persists the changed density across reload.
- [x] Expected red/revision: exact equality to a transient post-pinch resize
  failed 5/5 under repetition. The final result compares reload density with
  the original same-viewport default, not a fit-timing artifact.
- [x] Implementation: fixture/test support only; no storage key, exact font
  size/range, renderer state, theme, cursor, font-family, or scrollback
  assertion. Surface V2 is Legacy rollout scaffolding, not a rebuild setting.
- [x] Verification: focused pinch test passed 5/5 and the permanent suite
  passed 24/24; source guard returned no matches. Invalid persisted storage is
  legacy characterization because no user boundary creates it and the future
  state mechanism may change completely.

### Task 9 — Separate and label removable Ghostty characterization coverage

- [x] Test to write first: none; this is classification of existing tests, and
  duplicating implementation assertions would be counterproductive.
- [x] Implementation: added `TERMINAL_LEGACY_SURFACE_TESTS.md` and corrected
  `TERMINAL_BEHAVIOR_CONTRACT.md` so it names the
  current Ghostty-only component/probe tests, why each is removable, and which
  permanent behavior replaces it. Add only a missing characterization test if
  the inventory identifies an otherwise undocumented deletion hazard.
- [x] Verification: confirmed no permanent-suite file imports `ghostty-web`,
  references Ghostty private state/generated DOM/canvas/probe names, or asserts
  the future adapter/controller shape.

### Task 10 — Complete iPhone checklist, acceptance matrix, and excluded-bug list

- [x] Test to write: none; these are requested verification artifacts.
- [x] Implementation: added `TERMINAL_REBUILD_ACCEPTANCE.md` with a concise
  physical-iPhone checklist for real virtual
  keyboard, key repeat, touch selection, long press, native clipboard,
  orientation, Safari chrome/focus zoom, normal Safari (and explicitly note no
  standalone-PWA requirement). Add the rebuild matrix with behavior, test
  location, automated/manual, current result, and physical-iOS requirement;
  list excluded known/suspected bugs with evidence.
- [x] Verification: every required coverage bullet maps to an automated test,
  an existing intentional test, or an explicit physical/manual/unassertable
  row; no row relies only on a Ghostty characterization test.

  Matrix contains 56 rows and the exact required columns. A coverage audit
  found and closed permanent-suite gaps for supported Ctrl input and output
  during initialization/resize; the permanent file now has 27 tests.

### Task 11 — Baseline, mutation proof, final validation, and ledger closeout

- [x] Test to write: none; this task executes the requested validation protocol.
- [x] Implementation: applied one
  minimal temporary break each to input, resize, and lifecycle behavior;
  confirm the corresponding focused test fails for the intended assertion;
  restore each exact edit without `git checkout`/`reset`; verify the diff after
  every restore.
- [x] Verification: ran the pre-change baseline recorded before Task 2, all new
  focused tests, full web unit/component tests, the mobile-WebKit project,
  focused Rust terminal tests, and appropriate repo checks. Report failures,
  skips, physical-only behavior, and remaining risks; check every task complete.

  Mutation proof: duplicate input failed the Ctrl sequence test; suppressed
  resize failed initial dimensions; omitted connection disposal failed
  navigation cleanup. Each focused test passed after patch-based restoration;
  `TerminalRawView.svelte` SHA-256 returned to
  `3b82e5f58e86cba4b48b2267068b73f58beaa231838d57b1ceabb4a5f79dee49`.

### Task 12 — Remove the existing experimental xterm and Ghostty implementations

- [x] Test to write first: a focused repository-hygiene test that fails while
  specifically identified obsolete terminal components, feature flags,
  preload paths, dependencies, WASM assets, probes, and legacy-only tests remain.
  It must name exact old paths/symbols without constraining the later ground-up
  xterm architecture. (Task 12A: `legacyTerminalRemoval.test.ts`)
- [x] Expected red: the current Dev-only xterm Surface V2 and Ghostty paths are
  still present. (captured in Task 12A)
- [x] Implementation: delete the experimental xterm implementation and its
  selector/settings scaffolding; delete Ghostty renderer integration,
  workarounds, WASM/preload assets, dependency entries, and removable legacy
  characterization tests; remove the terminal mount from the task view so the
  web application still compiles without a terminal implementation. Update
  install/static-asset wiring, lockfile, built distribution, `TERMINAL.md`, and
  `architecture.md` where those deleted paths are documented. Preserve the
  permanent behavior suite and physical-iPhone checklist for the rebuild.
  (Task 12B via cursor-delegate / composer-2.5)
- [x] Verification: hygiene GREEN; web check/test/build green; ajax-web 134 and
  ajax-cli 335 nextest green; cargo check/clippy green; permanent
  mobile-WebKit behavior suite intentionally red 27/27 (`task-terminal-panel`
  absent). Hygiene test not weakened.
- [x] Final-state note: Task 11 is the last all-green validation against the
  current terminal. Task 12 intentionally makes the behavioral suite red until
  the later ground-up xterm implementation satisfies it.

## Planned validation commands

```bash
rtk npm run web:check
rtk npm run web:test -- --run
rtk npm run web:smoke -- --project=mobile-webkit
rtk cargo nextest run -p ajax-web
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
```

If `cargo nextest` is unavailable, use `cargo test -p ajax-web` and record the
substitution. Full-repository validation will be run only after focused tests
are green, and any unrelated pre-existing failure will be reported verbatim.

## Deviations and results

- Pre-Task-2 baseline attempt: `rtk npm run web:test -- --run` exited 127
  because `vitest` was not installed in this fresh worktree. Install the
  committed Node dependency set, rerun the baseline, and record the result.
- Baseline after `rtk npm install`: web unit/component tests passed 561/561;
  mobile-WebKit smoke passed 46 with 2 existing skips.
- Task 4 parent invocation corrections: running npm from
  `crates/ajax-web/web` failed with package.json `ENOENT`; running
  `rtk npx vitest` was incorrectly mapped. The corrected root commands passed
  `terminalConnection.test.ts` 16/16 and the permanent browser file 6/6.
- Task 5 original delegation was discarded after two rounds because simulated
  WebKit did not emit Ghostty's physical-iOS Backspace input path. No skip,
  weakened assertion, or production workaround was retained. Task 5a then
  passed the permanent browser file 11/11 and kept Backspace physical-only.
- One Task 10 documentation delegation command was interrupted before edits
  after shell backtick interpolation produced `zsh: command not found:
  supported`; it was rerun safely without command substitution.
- Final all-green current-surface validation before Task 12:
  `web:check` passed with 0 diagnostics; web Vitest passed 41 files / 564
  tests; all mobile-WebKit passed 73 with 2 existing skips (75 total);
  permanent behavior passed 27/27; `cargo nextest run -p ajax-web` passed
  136; `cargo fmt --check`, all-target/all-feature `cargo check`, and clippy
  with `-D warnings` all passed.
- Final post-Task-12 parent validation: removal hygiene passed 1/1;
  `web:check` passed with 0 diagnostics; web Vitest passed 27 files / 245
  tests; the production build and deterministic build check passed; retained
  non-terminal mobile-WebKit coverage passed 27 with 1 existing visual skip;
  `cargo nextest run -p ajax-web` passed 134 and `-p ajax-cli` passed 335;
  `cargo fmt --check`, all-target/all-feature `cargo check`, and clippy with
  `-D warnings` passed.
- The first parent expected-red attempt overlapped a prior Playwright web-server
  lifetime and produced 11 `Could not connect to the server` artifacts. It was
  discarded and rerun in isolation through `rtk proxy`: 27/27 permanent tests
  failed at the absent `[data-testid='task-terminal-panel']`, with zero server
  failures. This is the intended post-removal rebuild gate.
- Corrected `TERMINAL_REBUILD_ACCEPTANCE.md` so its Current result column
  distinguishes pre-removal proof from the post-Task-12 red surface state.
- Removed the delegate's redundant Task-12-only plan; this file remains the
  single persistent execution ledger.
- PR gate: Husky was installed with `npm run prepare`; the commit hook passed
  `npm run verify` (1,579 nextest tests plus doc and web tests),
  `cargo build --release -p ajax-cli`, and
  `cargo install --path crates/ajax-cli --locked --force` with exit 0.
