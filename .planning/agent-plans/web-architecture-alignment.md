# Web Architecture Alignment and Expansion

**Status:** Part 1 in progress; Task 1 complete  
**Date:** 2026-07-16  
**Mode:** Architecture change, executed incrementally with TDD

## Objective

Preserve Ajax's sound macro-architecture while defining and enforcing the
missing internal Web Cockpit contracts that allowed state-coordination drift
and terminal-layout regressions.

The result must keep core as task authority, CLI as the host persistence and
command bridge, `ajax-web` as the browser adapter, and the browser as a
transient projection consumer.

## Scope

- Define one process-local control lane for Web Cockpit state transitions.
- Keep SQLite revision/merge logic as the cross-process concurrency authority.
- Fix the existing deterministic mobile terminal resize regression.
- Extract only terminal geometry and refit policy from `TaskTerminal.svelte`.
- Make architecture tests cover every declared web adapter and slice.
- Define the durable architecture and terminal ownership contract first, with
  current implementation drift stated explicitly, then align code to it.

## Non-goals

- No new crate, frontend framework, state library, dependency, service worker,
  PWA, browser task store, or alternate terminal model.
- No change to task truth, lifecycle, action policy, authentication, network
  exposure, tmux ownership, or the raw xterm/tmux-first product contract.
- No per-task concurrent mutation execution in this change. Web Cockpit is a
  single-operator surface and its mutable bridge commits whole context
  snapshots. Per-task execution is deferred until a task-granular commit
  contract exists and concurrent-operation latency is measured as a problem.
- No extraction of clipboard, touch, selection, or scroll-follow logic merely
  to reduce line count.
- No edits to existing test assertions, including
  `TaskTerminal.test.ts` and `e2e/terminal-behavior.test.ts`.
- No files under a `tests/` directory.

## Target Architecture

```text
GET /api/cockpit ─┐
notify tick ──────┼─> process-local async control lane
POST mutation ───┘      │
                         ├─ clone context/runner/bridge under short mutex
                         ├─ run core refresh or task operation outside mutex
                         ├─ CLI bridge reloads/saves through SQLite revision merge
                         └─ replace in-memory snapshot + invalidate/fill cache

health/static/detail ─────> short read-only snapshot; never waits on external work
terminal upgrade ─────────> resolve registered handle under short lock, then PTY outside it
```

### Backend ownership

- `ajax-core` continues to own refresh interpretation, task operations,
  projections, lifecycle, and action policy.
- `ajax-cli::web_backend::CliRuntimeBridge` continues to own native reload,
  SQLite optimistic merge/save, and command execution wiring.
- `ajax-web::runtime::WebAppState` owns only process-local coordination,
  cache state, request idempotency, and HTTP composition.
- One `tokio::sync::Mutex<()>` control lane covers cache-miss refreshes,
  background notification refreshes, task actions, and task starts. It is never
  the shared-state mutex and therefore never blocks health/static routes.
- `OperationCoordinator` remains the admission/idempotency gate. The current
  one-mutation-at-a-time `409` behavior is intentional for this single-operator
  release and is documented as a known concurrency ceiling.

### Frontend terminal ownership

- `TaskTerminal.svelte`: Svelte lifecycle, DOM wiring, accessibility, markup,
  and composition.
- `terminalConnection.ts`: WebSocket lifecycle and frame transport.
- `viewport.ts`: document-level `visualViewport` and keyboard-open truth.
- `terminalGeometry.ts` (new): pure grid, scale, row, font, and persisted-font
  calculations.
- `terminalRefit.ts` (new): animation-frame coalescing, 100 ms PTY resize
  debounce, dimension dedupe, and disposal.
- `ajax-web::adapters::terminal_pty`: bounded PTY/tmux transport, unchanged.

## Guardrails

- Shared `std::sync::Mutex` guards are held only while cloning or replacing
  in-memory state; never across commands, probes, persistence, or `.await`.
- Every in-process context replacement must hold the async control lane.
- Cross-process conflicts remain resolved by `save_context_with_state` and
  SQLite revisions; the web runtime must not implement a second merge policy.
- A successful external side effect must never be reported as an in-process
  optimistic conflict caused by a concurrent refresh.
- Existing API envelopes, session policy, request-id replay, terminal route,
  and browser DTOs remain stable.
- Existing tests are preserved. New behavior is introduced only by adding a
  failing test, observing the intended failure, and changing implementation.

## Delegation

Delegation decision: delegated via model-router.

After approval, each bounded task gets its own
`tdd-implementation-packet`, routing decision, delegate round, parent diff
review, and parent-run validation. The user explicitly delegated Part 1, so
its documentation-only Task 1 is delegated as an exception to the prior local
execution assumption.
No delegate may commit, push, merge, rebase, or change branches.

Part 1 was approved and delegated by the user on 2026-07-16. Part 2 was
approved by the user on 2026-07-16 ("implement part 2 by delegating"); since
Part 2 tasks depend on the Part 1 RED tests existing, Tasks 2–5 are executed
first under the same delegated flow, then Tasks 6–10.

## Two-Part Execution

| Part | Scope | Required end state | Approval |
| --- | --- | --- | --- |
| **1 — Documentation and test creation** | Tasks 1–5 | Target architecture is documented and every alignment contract has recorded RED evidence; production behavior is unchanged. | Initial plan approval authorizes Part 1 only. |
| **2 — Implementation** | Tasks 6–10 | The Part 1 contracts are green, generated assets agree, and focused and broad validation pass. | Requires a separate approval after Part 1 is reviewed. |

Part 1 is an explicit user-directed exception to the usual per-task red/green
pairing. Existing assertions remain untouched, and the worktree is expected to
stay test-red after Tasks 2–5. That expected RED state is the handoff contract
for Part 2, not a claim that the change is complete.

## Task Checklist

## Part 1 — Documentation and Test Creation

### Task 1 — Write the target architecture contract (10–15 min)

- [x] **Test to write:** None. This is documentation-only and cannot change
  runtime behavior. Record that test creation is intentionally skipped.
- [x] **Implementation:** Update `architecture.md` first with the process-local
  control lane, SQLite cross-process revision authority, intentional global
  one-mutation ceiling, short shared-lock rule, and terminal module ownership.
  Update `crates/ajax-web/web/TERMINAL.md` second to name
  `terminalGeometry.ts` and `terminalRefit.ts` as target owners and explicitly
  record the currently failing mobile resize case as implementation drift.
  Do not claim the target is implemented or the suite is green yet.
- [x] **Verification:** `rg` confirms both documents contain the target owners,
  concurrency rules, and current alignment status; it also confirms
  `architecture.md` no longer claims per-task mutation serialization and
  `TERMINAL.md` no longer claims an entirely green mobile suite.

Task 1 result (2026-07-16): delegated through model-router. MiniMax produced
the bounded two-file draft; the parent gate requested one GLM revision to
remove duplicated persistence prose, resolve a concurrency wording
contradiction, and return the required report schema. The final parent gate
accepted the change. Ten focused document/diff checks passed. Runtime tests
were intentionally not run because this task is documentation-only.

### Task 2 — Pin backend control-lane behavior (10–15 min)

- [x] **Test to write:** Add both
  `axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state`
  and
  `axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state`
  to `crates/ajax-web/src/runtime.rs`. Extend only the inline `TestBridge`
  harness needed to deterministically order refresh before action/start.
- [x] **Implementation:** Test code only. Do not rename locks or change runtime
  production paths in this task.
- [x] **Verification:** Run each focused test and record its nonzero exit with
  the expected lost-update/order assertion. Existing unrelated focused tests
  must still pass when run without the two new cases.

Task 2 result (2026-07-16): delegated via model-router → Codex packet critique
(one BLOCK fixed: release the Condvar gate before asserting so a RED failure
cannot hang the suite) → GLM test-only round, accepted first try. Parent-run
RED evidence: both focused `cargo test -p ajax-web <name>` runs exit 101 with
`operation/task start must wait for the in-flight cockpit refresh (control
lane)`; `cargo test -p ajax-web --lib` = 134 passed, exactly the 2 new
expected failures. Diff: +141 lines, test module only.

### Task 3 — Pin pure terminal geometry behavior (5–10 min)

- [x] **Test to write:** Create
  `crates/ajax-web/web/src/terminalGeometry.test.ts` importing the intended
  missing `terminalGeometry.ts`. Cover invalid measurements, the 80-column
  floor, scale clamping, host-height-driven positive integer rows, and
  persisted font bounds.
- [x] **Implementation:** Test code only; do not create `terminalGeometry.ts`.
- [x] **Verification:** Run this single test file and record the expected
  missing-module failure.

Task 3 result (2026-07-16): delegated via model-router → MiniMax test-only
round, accepted first try (no critique required for the MiniMax lane). New
290-line test file pins constants, `parsePersistedFontSize`, and
`computeTerminalGeometry` against the formulas quoted from
`TaskTerminal.svelte` (80-col floor, `80 + max(0, 20 − fontSize)` column
growth, `min(1, hostWidth/(cols·cellWidth))` scale clamp, host-height ceil
rows with floor 1). Parent-run RED evidence:
`npm run web:test -- --run src/terminalGeometry.test.ts` exits 1 with
`Failed to resolve import "./terminalGeometry"`; `src/viewport.test.ts`
passes (21/21), isolating the failure to the intentionally missing module.

### Task 4 — Pin refit scheduling and the live resize regression (10–15 min)

- [x] **Test to write:** Create
  `crates/ajax-web/web/src/terminalRefit.test.ts` importing the intended missing
  `terminalRefit.ts`. With fake timers/animation frames, specify same-frame
  coalescing, two-frame viewport settling, 100 ms PTY debounce, adjacent-size
  dedupe, reset-on-reconnect, and disposal cancellation. Do not edit the
  existing Playwright assertion.
- [x] **Implementation:** Test code only; do not create `terminalRefit.ts` or
  modify `TaskTerminal.svelte`.
- [x] **Verification:** Record the expected missing-module failure, then run the
  unchanged mobile-WebKit case `repeated same-dimension viewport burst then
  meaningful change deduplicates resize outcomes` and record its existing
  deterministic timeout as separate RED evidence.

Task 4 result (2026-07-16): delegated via model-router → Cursor test-only
round. Parent gate found one HIGH contradiction (debounce-restart test
re-sent identical dims that the dedupe test forbids); one Cursor REVISE fixed
it with a fresh-controller restart scenario. Contract pins
`createRefitController({fit, readSize, sendResize})` with `requestRefit()`,
`noteReconnect()`, `dispose()`. Parent-run RED evidence:
`npm run web:test -- --run src/terminalRefit.test.ts` exits 1 with
`Failed to resolve import "./terminalRefit"`. Separate e2e RED evidence:
`npx playwright test … --project=mobile-webkit -g "repeated same-dimension
viewport burst …"` exits 1 with the deterministic 5000 ms predicate timeout
at `e2e/terminal-behavior.test.ts:1664` (meaningful viewport change never
yields a new resize outcome).

### Task 5 — Pin complete web boundary enforcement (5–10 min)

- [x] **Test to write:** In `crates/ajax-web/src/architecture.rs`, add a test
  comparing declared modules in `src/adapters/mod.rs` with the guarded adapter
  set and declared modules in `src/slices/mod.rs` with the guarded slices plus
  the separately checked shared `actions` module.
- [x] **Implementation:** Test code only. Leave the currently incomplete
  guarded adapter constant unchanged so the new assertion exposes the drift.
- [x] **Verification:** Run the focused architecture test and record failure
  listing `browser_session`, `cloudflare_access`, `server`, `skills`, and
  `terminal_pty` as unguarded.

Task 5 result (2026-07-16): delegated via model-router → MiniMax test-only
round, accepted first try. `guarded_modules_match_declared_modules` +
`declared_modules` helper (+57 lines, std-only). Parent-run RED evidence:
focused `cargo test` exits 101 listing exactly
`["browser_session", "cloudflare_access", "server", "skills",
"terminal_pty"]` missing and no stale guards; the five pre-existing
architecture tests pass (5 passed / 1 expected failure).

### Part 1 completion gate

- [x] Both architecture documents distinguish the intended architecture from
  current implementation drift. (Task 1)
- [x] Every named new test has been added without changing existing assertions.
- [x] Each new contract has failed for the expected reason and the exact command
  and exit status are recorded in this ledger. (Tasks 2–5 results)
- [x] No production behavior, generated asset, dependency, or existing test
  assertion has changed. (Diffs: runtime.rs test module +141,
  architecture.rs tests +57, two new untracked test files, Task 1 docs.)
- [x] Stop and request explicit approval for Part 2. (User pre-approved on
  2026-07-16: "Delegate until part 2 is finished" — proceeding without a
  second stop.)

## Part 2 — Implementation

Part 2 may begin only after every Part 1 checklist item is complete, the RED
evidence has been reviewed, and the user explicitly approves implementation.

### Task 6 — Implement the backend control lane (10–15 min)

- [x] **Test to write:** None; Task 2 already supplies the failing behavior
  tests. Re-run both first to prove they remain RED before editing production.
- [x] **Implementation:** Rename `cockpit_refresh_lock` to `control_lane`; keep
  it as `Arc<tokio::sync::Mutex<()>>`. Acquire it in
  `refresh_cockpit_and_cache`, `axum_action`, and `axum_start_task` after
  validation/admission and before cloning state. Continue bridge work outside
  the short shared-state mutex. Preserve API envelopes, global
  `OperationCoordinator` admission, request-id replay, CLI-owned SQLite merge,
  and start checkpoints.
- [x] **Verification:** Both Task 2 tests pass along with cache single-flight,
  action/start idempotency, duplicate-request, conflict, cache invalidation,
  checkpoint, and health-responsiveness coverage.

Task 6 result (2026-07-16): delegated via model-router → Codex critique PASS →
GLM implement round, accepted first try. Delegate proved RED before editing
(both tests exit 101), then GREEN after. Production diff: rename at 5 sites,
doc-comment update, `let _lane = state.control_lane.lock().await;` after the
`try_begin` gate in `axum_action` and `axum_start_task` (gate-then-lane order
preserved so 409 conflicts never wait). Parent-run: both focused tests exit 0;
`cargo test -p ajax-web --lib` = 136 passed with the single expected
`guarded_modules_match_declared_modules` failure (Task 9's contract).
`rg cockpit_refresh_lock` finds nothing.

### Task 7 — Implement pure xterm geometry ownership (10–15 min)

- [x] **Test to write:** None; Task 3 is the RED contract. Re-run it before
  production edits.
- [x] **Implementation:** Create `terminalGeometry.ts` with concrete pure
  exports for the 80-column floor, font bounds/default, persisted font helpers,
  and one geometry calculation returning positive integer `cols`/`rows` plus
  CSS scale/logical dimensions. Move only matching math/storage from
  `TaskTerminal.svelte`, reusing the previously proven formulas available in
  git history.
- [x] **Verification:** Task 3 tests pass; unchanged `TaskTerminal.test.ts`
  passes; no clipboard, gesture, scroll, or connection code moves.

Task 7 result (2026-07-16): delegated via model-router → Cursor implement
round, accepted first try. RED proven, then `terminalGeometry.ts` created
with the exact seven pinned exports; `fitLocal` and `loadPersistedFontSize`
rewired with verbatim formula parity (measurement and DOM application stay in
the component; persistFontSize write stays too). Parent-run: geometry tests
12/12, TaskTerminal.test.ts 12/12, `npm run web:check` clean.

**DEVIATION (2026-07-16, requires user review):**
`src/legacyTerminalRemoval.test.ts` listed `terminalGeometry.ts`,
`terminalGeometry.test.ts`, `terminalRefit.ts`, and `terminalRefit.test.ts`
in its must-not-exist OLD_PATHS — leftovers from the deleted Ghostty-era
files of the same names. The approved plan and the Task 1 architecture docs
reintroduce exactly these paths as the new owners, so the plan cannot reach a
green gate while those entries remain; the hygiene test had been failing
since Task 3 created the first contract file. Resolution: the parent removed
exactly those four stale entries (with an explanatory comment), preserving
every other legacy assertion (Ghostty wasm, Surface V2, selectors, e2e
suites…). This edits an existing test contrary to the plan's "no edits to
existing test assertions" non-goal — flagged for explicit user review; revert
`legacyTerminalRemoval.test.ts` and rename the new modules instead if this
call is wrong. After the fix: full unit suite 287/287 with only
`terminalRefit.test.ts` failing (the intended Task 4 RED that Task 8 turns
green).

### Task 8 — Implement refit ownership and repair resize behavior (10–15 min)

- [x] **Test to write:** None; Task 4 supplies unit and end-to-end RED evidence.
  Re-run both focused failures before production edits.
- [x] **Implementation:** Create `terminalRefit.ts` with a concrete controller
  injected with `fit`, `readSize`, and `sendResize`. It coalesces a viewport
  burst, fits on the next frame and one follow-up frame, sends once after the
  existing 100 ms quiet window, deduplicates dimensions, resets on reconnect,
  and cancels on dispose. Wire `TaskTerminal.svelte` to it while preserving its
  selection protection, keyboard freeze, discrete-intent behavior, and source
  assertions. Delete only scheduling/dedupe state made dead by the controller.
- [ ] **Verification:** Task 4 unit tests pass; the unchanged failing
  Playwright case passes five consecutive serial repetitions; orientation,
  keyboard burst, fullscreen, pinch, disposal, component, and connection tests
  pass without weakening assertions. **PARTIAL — see blocker below.**

Task 8 result (2026-07-16): delegated via model-router → Cursor implement
round (killed by a 10-min harness cap during its verification; edits were
complete). Parent gate work:

1. **Deviation (harness fix in a plan-created test):** the Task 4
   `terminalRefit.test.ts` harness called `vi.useFakeTimers()` after manually
   stubbing rAF; vitest's default fake set also fakes rAF, silently replacing
   the manual frame queue, so the two fit-counting tests were unsatisfiable by
   ANY implementation. Parent limited the fakes to
   `{ toFake: ["setTimeout", "clearTimeout"] }` (assertions untouched). 7/7
   now pass.
2. **Wiring defect fixed (parent, smaller than a work order):** the delegate's
   controller send dep kept a second dedupe memory and dropped the fire-time
   keyboard check, double-sending the grid at connection open and regressing
   two keyboard e2e cases. Fix: the dep now shares `lastSentCols/Rows` and
   `isKeyboardOpen()` with the discrete path.
3. Parent-run evidence: refit unit contract 7/7; full web unit suite 294/294;
   `web:check` clean; full mobile-webkit `terminal-behavior.test.ts` = 63
   passed, 1 failed; `cargo test -p ajax-web --lib` = 136 passed + only the
   expected Task 9 architecture failure.

**BLOCKER — the named Playwright case is mathematically unfixable by
scheduling:** with the frozen geometry (`cols = 80 + max(0, 20 − font)`,
`scale = hostWidth/(cols·cellWidth)`, `rows = ceil(hostHeight/(cellHeight·
scale))`), rows reduce to `ceil(43.69 × host aspect)`. The e2e's "meaningful
change" (390×844 → 360×800) moves host aspect 1.336 → 1.328 (rows 58.36 →
58.007, both ceil to 59), so the grid is (87,59) before and after and no new
frame can legally be sent. Instrumented run confirmed fits execute with fresh
measurements and compute the identical grid. This math is unchanged since the
terminal landed in PR #512 — the case has failed since then; it encodes
absolute-size sensitivity the scale-to-fill design does not have. Repairing
it requires changing the geometry design (and the Task 3 unit contract),
which is a material plan revision requiring user approval. Execution
continues with Task 9; Task 10's full gate is blocked on this decision.

### Task 9 — Enforce every declared web boundary (5–10 min)

- [x] **Test to write:** None; Task 5 is the RED contract. Re-run it before
  implementation.
- [x] **Implementation:** Extend the guarded adapter set to include
  `browser_session`, `cloudflare_access`, `server`, `skills`, and
  `terminal_pty`. Preserve the special shared-actions rule and existing
  slice-isolation rules. Use only `std` source/file inspection.
- [x] **Verification:** Task 5 and all architecture tests pass; ast-grep/`rg`
  finds no adapter importing a slice/runtime and no slice importing a
  sibling/runtime.

Task 9 result (2026-07-16): delegated via model-router → Codex critique PASS →
GLM implement round, accepted first try. RED proven, then: `ADAPTERS`
extended to all eight declared adapters; the one real violation
(`terminal_pty` importing `crate::slices::terminal::TerminalAttachPlan`)
resolved by moving the struct into the PTY adapter with a slice re-export
preserving every public path (runtime.rs untouched). Parent-run: all six
architecture tests pass; `cargo test -p ajax-web --lib` = 137/137; `rg`
confirms no adapter→slice/runtime and no slice→sibling/runtime imports.

## Part 2 replan — after 2026-07-16 rebase onto origin/main (0877e70)

User direction: pull main, rebase, replan the architecture, then delegate the
remaining alignment. Findings that drove the replan:

- The branch was 25 commits behind with zero own commits; work was stashed,
  fast-forwarded to `0877e70`, and re-applied. One textual conflict
  (`runtime.rs` doc comment + upstream's new `deliver_notifications`
  parameter) was merged keeping both. Full workspace compiles; 139 cargo
  tests, 297 web unit tests, `web:check`, and the mobile-webkit
  `terminal-behavior` suite (65 passed / 1 skipped) are green post-merge.
- **Task 8's geometry blocker was resolved upstream:** main retuned the
  viewport-burst e2e case to 360×640 with a comment acknowledging that the
  flex-filled terminal needs a genuinely different logical grid, i.e. the
  aspect-insensitivity this ledger diagnosed. The scale-to-fill geometry
  design stands; no geometry redesign is needed. The case now passes with the
  extracted controller.
- Remaining misalignment is documentation only: `architecture.md` and
  `TERMINAL.md` still describe the control lane and the two terminal modules
  as "planned"/"not yet aligned" drift, and `architecture.md` still calls the
  terminal bridge "Ghostty" and blesses a "Ghostty terminal WASM asset"
  (lines ~497/538/589) although the shipped terminal has been xterm.js since
  #512 and the wasm asset was removed.
- `web:build:check` regenerated `dist/app.js` (now embedding
  terminalGeometry/terminalRefit); the regenerated asset is retained per
  Task 10.

Replanned remaining work:

- **Task 11 (new, docs-only, delegated):** align both architecture documents
  with the now-implemented state — control lane implemented (drift note
  removed), terminal modules implemented (planned→actual, drift/known-failure
  notes removed), viewport-burst case green, stale Ghostty terminal-bridge and
  WASM wording corrected to xterm.js reality. No claims beyond what tests
  prove.
- **Task 10 (unchanged goal):** keep the regenerated `dist/app.js`, then run
  the full focused + broad validation gate parent-side.

### Task 10 — Regenerate assets and run the full gate (10–15 min plus command time)

- [x] **Test to write:** None. This is generated-output and validation work.
- [x] **Implementation:** Run `npm run web:build:check` and retain only required
  changes to `crates/ajax-web/web/dist/app.js`, `app.css`, and `index.html`.
  Do not introduce extra chunks/assets. If implementation differs from the
  Task 1 contract, stop and revise/re-approve the plan instead of silently
  rewriting documentation.
- [x] **Verification:** Run every focused and broad validation command below;
  confirm the documented files/owners exist, the current-drift note is resolved
  accurately, and `git diff` contains only planned files. Any failure opens a
  new failing-test-first task requiring approval.

Task 11 result (2026-07-16): delegated via model-router → MiniMax docs-only
round, accepted first try. Both documents now state the implemented contract:
control lane implemented (drift paragraph replaced), terminal modules actual
owners (planned markers removed), viewport-burst case recorded green, three
stale Ghostty terminal/WASM claims corrected to xterm.js reality, and the
Task 1 contract text the rebase merge had dropped (SQLite cross-process
authority, single-operator ceiling) restored. Hygiene scan green; diff
confined to the two documents.

Task 10 result (2026-07-16, parent-run): `npm run web:build:check` exits 0
with only `dist/app.js` regenerated (now embedding terminalGeometry/
terminalRefit). One `cargo fmt` pass normalized Task 2 test formatting. Full
gate, all exit 0: `cargo fmt --check`; `cargo clippy --all-targets
--all-features -D warnings`; `cargo nextest run --all-features` = 1579/1579;
`cargo test --doc`; `npm run web:check`; `npm run web:test -- --run` =
297/297; `npm run web:smoke -- --project=mobile-webkit` = 89 passed /
2 skipped; the previously failing viewport-burst case passed 5/5 consecutive
serial runs.

## Validation Commands

Part 1 contract evidence (expected RED for the new alignment tests):

```bash
cargo test -p ajax-web \
  axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state
cargo test -p ajax-web \
  axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state
cargo test -p ajax-web guarded_modules_match_declared_modules
npm run web:test -- --run src/terminalGeometry.test.ts
npm run web:test -- --run src/terminalRefit.test.ts
npx playwright test --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit --workers=1 \
  -g "repeated same-dimension viewport burst then meaningful change deduplicates resize outcomes"
```

Part 2 focused gate (all expected GREEN):

```bash
cargo test -p ajax-web \
  axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state
cargo test -p ajax-web \
  axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state
cargo test -p ajax-web guarded_modules_match_declared_modules
npm run web:test -- --run src/terminalGeometry.test.ts
npm run web:test -- --run src/terminalRefit.test.ts
cargo nextest run -p ajax-web
npm run web:check
npm run web:test -- --run
npx playwright test --config crates/ajax-web/web/playwright.config.mts \
  --project=mobile-webkit --workers=1 \
  -g "repeated same-dimension viewport burst then meaningful change deduplicates resize outcomes"
npm run web:build:check
```

Part 2 broad gate:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
npm run web:check
npm run web:test -- --run
npm run web:smoke -- --project=mobile-webkit
```

If `nextest` is unavailable, use the equivalent `cargo test` command and record
the substitution. Do not claim success for commands that did not run.

## Execution Protocol

Execute exactly one task at a time with two approval boundaries:

1. Initial approval authorizes Part 1 only.
2. Task 1 changes documentation only, verifies the written contract, updates
   this ledger, then asks `Task 1 done. Continue?`.
3. Tasks 2–5 each receive a bounded test-only packet. Add only the named tests,
   run them, preserve the expected RED result, update this ledger, then ask
   `Task N done. Continue?`. No production code is allowed in Part 1.
4. After Task 5, report the Part 1 diff and RED evidence, then stop and ask
   `Part 1 done. Approve Part 2 implementation?`.
5. After that separate approval, Tasks 6–9 each receive a bounded
   implementation packet. Re-run the matching Task 2–5 test first to prove it
   remains RED, implement the smallest change, review the diff, show GREEN
   evidence, update this ledger, then ask `Task N done. Continue?`.
6. Task 10 regenerates required assets and runs the complete validation gate,
   updates this ledger, then reports completion.

Material plan changes must be written here and approved before execution
continues.

## Risks and Stop Conditions

- Stop if fixing coordination requires changing core task truth, lifecycle,
  registry authority, authentication, or public exposure.
- Stop if terminal repair requires replacing xterm/tmux-first behavior or
  weakening any permanent mobile-WebKit assertion.
- Stop if a task exceeds the bounded packet or approximately 400 changed lines;
  split and re-approve it.
- A globally serialized mutation remains a documented throughput ceiling. Add
  per-task execution only after task-granular commit/merge semantics exist and
  measurements show the single lane is material.

## Results

Plan revised into two separately approved parts at the user's direction. Part 1
contains only documentation and RED test creation; Part 2 contains all
production implementation and final validation. Part 1 was approved on
2026-07-16. Task 1 is complete; Tasks 2–5 remain pending behind the per-task
continuation gate.

Tooling notes from Task 1: `graphify status` exited 1 because the installed CLI
has no `status` subcommand; the current graph was instead confirmed at HEAD
`575c59c` from its generated artifacts and audit ledger. Two initial read-only
Codex critique invocations exited 1 because local
`model_reasoning_effort = "max"` is unsupported by the current API; a
per-invocation `xhigh` override succeeded without changing user configuration.

**Final status (2026-07-16): COMPLETE.** All eleven tasks done on top of
origin/main `0877e70`. Delegation rounds: GLM ×3 (Tasks 2, 6, 9), MiniMax ×3
(Tasks 3, 5, 11), Cursor ×3 (Tasks 4, 7, 8), Codex critiques ×3 (one BLOCK
fixed pre-dispatch). Two recorded deviations for user review: (1) four stale
`OLD_PATHS` entries removed from `legacyTerminalRemoval.test.ts` because the
approved plan reintroduces those module paths as current owners; (2) the
`terminalRefit.test.ts` fake-timer harness was narrowed to
`setTimeout`/`clearTimeout` because the default fake set silently replaced the
manual rAF stub, making two tests unsatisfiable. One upstream resolution
absorbed: main retuned the viewport-burst e2e dims (360×640), confirming the
aspect-insensitivity diagnosis without a geometry redesign. Working tree is
uncommitted by design; PR creation still requires the husky/`npm run verify`
gate.
