# Separate worktree presence from checkout state

## Scope

Model a task worktree's physical presence independently from its checked-out
branch. A registered Git worktree path remains present when it switches to a
different branch or becomes detached. Ajax surfaces checkout mismatch as its
own recoverable state, keeps terminal access available, blocks branch-sensitive
or destructive operations until reconciliation, and lets an operator explicitly
adopt the observed branch through Repair.

This is an architecture change because it changes substrate interpretation,
runtime health, operation safety, and task intent reconciliation. Update
`architecture.md` in the implementation.

Non-goals:

- Automatically switch Git branches.
- Automatically adopt an observed branch during refresh.
- Rename/rekey the task, worktree directory, or tmux session from branch names.
- Delete or move an occupied worktree.
- Add a database schema migration or a second source of task truth.
- Add separate Adopt/Restore UI actions; the existing Repair action is the
  explicit, confirmed adoption path for this slice.

## Locked behavior

- `GitStatus.worktree_exists` means the registered path appears in `git
  worktree list`, regardless of branch.
- `GitStatus.branch_exists` means the task's expected branch exists in the repo,
  independently of the worktree path.
- `GitStatus.current_branch` is the branch observed at that path; `None` means
  detached or not observed.
- Path present plus a different/detached checkout is checkout mismatch, never
  `WorktreeMissing`.
- Task handle, title, path, tmux session, lifecycle, history, and receipts remain
  unchanged when a branch is adopted.
- Resume/open and check remain available on a present mismatched worktree.
- Review/diff compares the current checkout (`HEAD`) against the task base.
- Ship, drop/cleanup, and other branch-sensitive destructive work are blocked
  while checkout mismatch remains.
- Repair on mismatch requires confirmation and adopts the named observed branch
  into task intent without running `git switch`; detached checkout cannot be
  adopted.
- Manually switching back to the expected branch resolves mismatch on refresh
  without mutating task intent.

Delegation decision: delegated via model-router after approval as sequential,
bounded TDD packets. The parent reviews each diff and runs validation.

Approval status: approved by user on 2026-07-20 via “delegate until finished.”

## Task checklist

- [x] Task 1 — Record physical worktree presence independently of checkout.
  - Test: add focused adapter/evidence tests proving porcelain entries for a
    different branch and detached HEAD retain the registered path, set
    `worktree_exists = true`, preserve the actual `current_branch` when named,
    and clear `WorktreeMissing` without claiming the expected branch is checked
    out. Run the new evidence test first and capture the current false failure.
  - Implementation: make `GitAdapter::parse_worktrees` retain path-only detached
    entries, and make `refresh_git_substrate_evidence` derive presence from the
    exact path while deriving expected-branch availability separately.
  - Verification: run the new adapter and Git-evidence tests, then the existing
    `refresh_git_substrate_evidence*` tests.

- [x] Task 2 — Derive checkout mismatch in the core runtime model.
  - Test: add table-driven model/runtime tests for aligned, other-branch,
    detached, missing-path, and unobserved cases; prove only missing-path maps to
    `MissingWorktree` and mismatch persists through SQLite runtime-health
    encoding/decoding.
  - Implementation: add one derived task helper for checkout mismatch and one
    `RuntimeHealth::CheckoutMismatch` variant. Pass the expected branch into
    runtime reconciliation; do not add a side flag or database column.
  - Verification: run focused `models`, `runtime`, and SQLite codec tests.

- [x] Task 3A — Project checkout mismatch and core operator actions.
  - Test: add focused status, annotation, recommendation, and Cockpit projection
    tests that show `Worktree on <observed>; expected <expected>` (or
    `Worktree detached; expected <expected>`), never `Worktree missing`, and
    offer Repair plus Resume while keeping mismatch outside `SubstrateGap` and
    `has_missing_substrate`.
  - Implementation: add typed checkout-mismatch annotation evidence, derive the
    canonical dynamic explanation in core, and project Repair/Resume from core
    without adding a side flag or UI policy.
  - Verification: run focused `models`, `ui_state`, `attention`, `recommended`,
    and `commands::projection` tests plus `cargo check -p ajax-core`.

- [x] Task 3B — Render the core mismatch projection through CLI and Web JSON.
  - Test: add output-contract, CLI human/JSON, and Web Cockpit card/detail tests
    proving the canonical mismatch explanation and Repair/Resume actions pass
    through unchanged.
  - Implementation: serialize the existing `TaskSummary` status, explanation,
    and action fields instead of hiding them; adapters perform no mismatch
    detection or policy.
  - Verification: run focused `ajax-core` output, `ajax-cli` render, and
    `ajax-web` Cockpit tests plus focused crate checks.

- [x] Task 4 — Make ordinary and dangerous operations branch-correct.
  - Test: add behavior tests proving Resume/Open and Check work on a present
    mismatched path; Review uses `base...HEAD`; Ship and Drop/Cleanup are blocked
    with expected/observed branch details; genuinely missing worktrees retain
    existing repair behavior.
  - Implementation: remove branch availability from worktree/open presence
    checks, use `HEAD` for worktree-local diff, and add the shared mismatch guard
    to merge and teardown safety/planning. Do not duplicate guards in UI code.
  - Verification: run focused command, operation eligibility, merge safety,
    teardown, and task-operation tests.

- [x] Task 5 — Adopt the observed branch explicitly through Repair.
  - Test: add core TDD cases proving mismatched Repair is confirmation-required,
    performs no Git/tmux command, rejects detached/no-longer-current evidence,
    updates only `task.branch`, clears mismatch after reconciliation, preserves
    identity/history/session/path/lifecycle, and records a substrate-change
    event. Keep existing missing-worktree Repair tests green.
  - Implementation: add a registry branch-intent update operation and a
    mismatch-specific Repair plan/execution path. Re-observe or validate the
    named current branch at execution; never infer adoption from a stale plan.
  - Verification: run focused registry and `task_operations::task_command`
    tests, including existing Repair red/green coverage.

- [x] Task 6A — Carry adoption through CLI plan/execute/persistence.
  - Test: add CLI plan and execute tests proving Repair renders the exact typed
    adoption, requires `--yes`, persists the updated task branch, and invokes no
    Git branch switch/tmux/check command. Prove decline leaves intent unchanged.
  - Implementation: render the existing typed core plan and pass CLI
    confirmation through the existing executor; add no CLI branch policy.
  - Verification: run focused `ajax-cli` Repair/render/persistence tests.

- [x] Task 6B — Carry confirmed adoption through native Cockpit deferral.
  - Test: add native Cockpit tests proving first activation requests
    confirmation for the exact typed core plan, second activation defers that
    same confirmed plan, execution adopts without external commands, and
    stale/declined actions leave intent unchanged.
  - Implementation: retain the optional exact confirmed core plan in the
    existing interactive handler across the two activations, then pass it to
    the existing deferred executor; query core instead of deriving mismatch in
    TUI code.
  - Verification: run focused `ajax-tui` input/action tests and `ajax-cli`
    pending-Cockpit Repair tests.

- [x] Task 6C1 — Carry exact confirmed adoption through Web Cockpit's Rust boundary.
  - Test: add focused Rust tests proving core Repair-plan metadata marks a named
    mismatch confirmation-required, the typed HTTP request reaches the operate
    slice unchanged, confirmed execution persists the exact branch pair without
    external mutation commands, and unconfirmed/stale requests leave intent
    unchanged.
  - Implementation: project the typed core `BranchAdoptionPlan` on the existing
    Web action DTO and carry `confirmed` plus that exact pair through runtime and
    operate. Reuse core execution and its stale-pair guard; add no Web-owned
    mismatch policy.
  - Verification: run focused `ajax-web` action/cockpit/operate/runtime tests,
    the focused CLI web-bridge persistence test, crate checks, formatting,
    diff check, and warning-free clippy.

- [x] Task 6C2 — Retain and submit exact Web Cockpit confirmation in the browser.
  - Test: add browser tests proving first tap makes no request, second tap sends
    `confirmed: true` with the exact projected adoption pair, a projection
    refresh between taps cannot replace that retained pair, and ordinary
    actions remain immediate.
  - Implementation: type and validate the optional branch-adoption payload,
    retain the pending `WebAction` rather than only its label, and submit the
    retained pair on confirmation through the existing API helper.
  - Verification: run focused ActionBar and contract Vitest files, TypeScript,
    lint, and the Web production build.

- [x] Task 7A — Remove the stale occupied-path inference from task-window repair.
  - Test: add a focused regression where cached evidence says the registered
    path is absent but retains another `current_branch`; prove Repair ignores
    that stale checkout value and recreates the missing worktree from the
    expected branch instead of emitting the superseded occupied-path blocker.
  - Implementation: when `worktree_exists` is false, decide only from physical
    path absence and expected-branch existence. Do not interpret
    `current_branch`, which is meaningful only for a present registered path.
  - Verification: run the focused stale-evidence test, existing missing-
    worktree task-window tests, grouped task-window tests, focused check,
    formatting, diff check, and warning-free clippy.

- [x] Task 7B — Document and validate the completed architecture change.
  - Test: no new executable test is meaningful for prose; verify the documented
    claims against the accepted source/tests and run formatting/diff checks.
  - Implementation: update `architecture.md` to define path presence, expected
    branch, observed checkout, mismatch precedence, operation safety, explicit
    adoption, and exact-plan confirmation across adapters.
  - Verification: run `cargo fmt --check`, `cargo check --all-targets
    --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`,
    `cargo nextest run --all-features`, `cargo test --doc`, and relevant web
    checks. If preparing a PR, also run the full Husky gate required by
    `AGENTS.md`.

- [x] Task 7C — Clear the cumulative warning-as-error gate without changing behavior.
  - Test: use the already-reproduced failing focused Clippy command as RED and
    keep the Task 5 stale/declined adoption behavior test green.
  - Implementation: factor the long stale-case closure tuple into one local test
    type alias; no production or assertion change.
  - Verification: rerun the focused Task 5 test, ajax-core all-targets Clippy,
    formatting, and diff check.

- [x] Task 7D — Preserve the established JSON shape for ordinary running tasks.
  - Test: keep `crates/ajax-cli/tests/live_cli.rs` unchanged and use
    `live_new_execute_records_task_and_persists_it_to_sqlite_state` as RED; it
    proves a normal running task must not gain `status`,
    `status_explanation`, or `actions` fields.
  - Implementation: limit the new `TaskSummary` operator-state serialization
    to non-running states while retaining the fields internally for human
    rendering. Keep mismatch JSON status, explanation, and actions intact.
  - Verification: rerun the live integration test, core read-contract test,
    mismatch human/JSON regression, output tests, formatting, focused checks,
    diff check, and warning-free Clippy.

## Execution notes

- Implementation worktree: `/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-worktree-checkout-state`
  on `fix/worktree-checkout-state`, created from `origin/main` at
  `d735dfb2252cf7be38b9e5f972d17f84129428b5`.
- The prior planning checkout remains on merged branch
  `ajax/tmux-worktree-missing`; do not rewrite or reset it.
- Create one READY TDD implementation packet per checklist task. Material scope
  changes must update this plan before execution continues.
- Existing assertions may change only when the newly separated semantics make
  them stale; never delete or weaken safety assertions.

## Risks

- Treating mismatch as healthy would allow Ship/Drop to target the wrong branch;
  the explicit mismatch guard is mandatory.
- Automatically adopting on refresh would let external Git activity rewrite
  Ajax intent; adoption must remain confirmed and evented.
- Detached worktrees have no branch to adopt and must remain recoverable only by
  an explicit Git switch followed by refresh.
- Changing `RuntimeHealth` requires backwards-compatible SQLite label handling,
  but no schema migration.
- Repair currently also runs task-window/test recovery; the mismatch path must
  exit through a distinct reducer so it does not falsely mark checks successful.

## Deviations

- The latest instruction “delegate until finished” is treated as explicit
  approval to continue through the approved task sequence without pausing for a
  fresh confirmation after every completed task.
- Task 1's Pi round timed out after leaving the first two test edits. Cursor's
  cross-tool revision completed the bounded patch and emitted every required
  report field, but wrapped the YAML in a Markdown fence, so the adapter marked
  the envelope invalid. The parent reviewed the raw report/diff and independently
  reran every packet verification command before accepting the code gate.
- Task 2's Cursor implementation stayed in scope and supplied red/green details,
  but used a custom fenced report instead of `DELEGATE_REPORT`; the adapter
  rejected it. The parent accepted only after reviewing the deterministic diff
  and independently passing all seven packet commands.
- Task 3 was split into bounded 3A core-truth and 3B adapter-rendering packets;
  approved behavior and scope are unchanged.
- Task 3A required one revision after parent review found stale mismatch health
  could outrank missing substrate. The revision added status, annotation, and
  action precedence regressions. Cursor's final report had the right root and
  evidence but used a multiline `FILES_CHANGED` value rejected by the checker;
  parent validation, not that report, supplied the acceptance evidence.
- Task 3B returned a schema-valid report; its focused post-implementation pass
  was labeled VERIFY instead of GREEN, so the parent treated the TDD claim as
  incomplete metadata and independently ran the full packet gate.
- Task 4 round 1 was rejected and deterministically restored: its shared
  explanation suppressed a fresh Git mismatch whenever an unrelated tmux gap
  made `has_missing_substrate()` true, which could let Merge bypass the branch
  guard. Its custom report also omitted auditable RED/GREEN evidence. The Task 4
  revision adds the simultaneous mismatch-plus-tmux-gap regression and keeps UI
  display precedence separate from operation safety.
- Task 4 round 2 returned every required report field and four exact RED/GREEN
  pairs, but fenced the marker envelope, so the adapter emitted
  `MISSING_STRUCTURED_REPORT`. No third round was sent. The parent accepted the
  source only after deterministic scope review and independently passing all ten
  packet commands; generated run logs were not retained in the worktree.
- Task 5 round 1 was rejected and deterministically restored because adoption
  confirmation depended on the public mutable `requires_confirmation` plan bit.
  The revision makes confirmation an unconditional execution invariant and adds
  a tampered-plan regression, plus fresh-mismatch validation in the executor and
  registry operation.
- Task 5 round 2 returned a complete plain report with four exact RED/GREEN
  pairs, but the adapter again emitted `MISSING_STRUCTURED_REPORT`. No third
  round was sent. The parent accepted the four-file source delta only after
  reviewing the confirmation invariant and independently passing all ten packet
  commands; generated run artifacts were removed.
- Task 6 was split into bounded CLI (6A), native Cockpit (6B), and Web Cockpit
  (6C) packets. The approved behavior and adapter-only scope are unchanged.
- Task 6A returned a complete plain report with honest characterization passes
  for already-working execution transport, but the delegate adapter again
  emitted `MISSING_STRUCTURED_REPORT`. The first deterministic snapshot was
  labeled incorrectly and its in-repo artifact directory polluted the raw
  delta with generated files; filtering those artifacts showed exactly the two
  allowed source paths and no source scope violation. The parent accepted only
  after reviewing that two-file delta and independently passing all eleven
  packet commands.
- Task 6B's initial one-bit transport design was tightened before editing. A
  boolean alone would let the second activation re-plan and silently confirm a
  different observed branch if checkout evidence changed after the prompt. The
  native boundary will retain the exact typed core plan shown on first
  activation in its already-persistent event handler and execute that plan,
  allowing core's existing stale-pair guard to reject changed evidence without
  changing TUI public types.
- Task 6B round 1 was rejected and deterministically restored because Cursor
  edited the packet outside allowed scope and left production dead-code
  warnings, plus avoidable retained-plan wrappers/redundant control flow. The
  sole revision keeps the same three-file behavior scope and adds a blocking
  `-D warnings` check.
- Task 6B round 2 changed exactly the three allowed source files and supplied
  the requested raw evidence, but the delegate adapter again emitted
  `MISSING_STRUCTURED_REPORT`. No further round was sent. The parent accepted
  only after reviewing the exact-plan retention and independently passing all
  eleven packet commands.
- Task 6C was split into sequential Rust-boundary (6C1) and browser-retention
  (6C2) packets after the native review exposed the same stale-confirmation
  hazard in Web Cockpit. A bare confirmation boolean is insufficient: the
  browser must retain and submit the typed pair that was displayed, and core
  remains responsible for rejecting changed evidence.
- Task 6C1 round 1 was rejected and deterministically restored despite a clean
  five-file scope because it duplicated `BranchAdoptionPlan` in ajax-web and
  round-tripped `CommandPlan` through JSON. The sole revision adds a one-line
  ajax-core re-export so both adapters can use the existing typed core payload
  directly, and requires actual RED evidence for all six focused tests.
- Task 6C1 round 2 used exactly the six allowed files and the re-exported core
  type, but the adapter again emitted `MISSING_STRUCTURED_REPORT` despite a
  complete raw report. The parent accepted only after reviewing the typed
  transport and independently passing all thirteen commands. A one-token local
  test strengthening then changed the pairless case to `confirmed: true`,
  directly proving a bare boolean cannot adopt; its focused regression gate
  also passed.
- Task 6C2 round 1 was rejected and deterministically restored because the
  required Web production build updated tracked `dist/app.js` outside the
  packet's five-file scope, even though the source delta and raw test evidence
  were otherwise sound. The sole revision explicitly allows that generated
  embedded asset, requires the build-layout check, and forbids any other dist
  churn.
- Task 6C2 round 2 changed exactly the five allowed source/test files plus
  generated `dist/app.js`; the adapter again emitted
  `MISSING_STRUCTURED_REPORT` despite complete raw evidence. Parent validation
  passed all eleven commands. A parent-added Drop confirmation assertion first
  tripped the Vitest lint preference, was consolidated to the required exact
  matcher, and then passed the focused test, TypeScript, lint, and diff gates.
- Task 7's closeout scan found the exact superseded occupied-path error still
  reachable through the public task-window planner when stale cached evidence
  combined `worktree_exists: false` with an old different `current_branch`.
  Correct refresh no longer creates that state, but physical absence must not be
  reinterpreted through stale checkout evidence. Task 7 was split into a
  bounded deletion/regression (7A) and architecture documentation (7B).
- Task 7A's seven focused parent commands passed and the exact legacy production
  text is gone. Its final ajax-core Clippy command exposed a cumulative Task 5
  test-only `type_complexity` warning outside the packet. The source gate was
  accepted, while the warning-as-error cleanup was split into Task 7C and
  remained blocking for the PR until Task 7C passed.
- Task 7C changed only the test-local stale-case type annotation. Its complete
  raw report was rejected by the adapter as `MISSING_STRUCTURED_REPORT`; parent
  review confirmed the one-alias delta and independently passed the focused
  behavior, ajax-core Clippy, formatting, and diff gates. Task 7A's prior
  unrelated warning is therefore resolved.
- Task 7B changed only `architecture.md` and documented the three independent
  Git facts, mismatch precedence, safe operation matrix, typed adoption, and
  adapter transport contract. Its complete raw report was again rejected by
  the adapter as `MISSING_STRUCTURED_REPORT`; deterministic scope review and
  all three parent documentation checks passed before acceptance.
- PR-base alignment fast-forwarded the otherwise-uncommitted branch from
  `d735dfb` to current `origin/main` (`962e382`) through a named stash. Source
  restoration merged cleanly; only tracked generated `web/dist/app.js`
  conflicted. Stage 1 was the old generated bundle, stage 2 contained the newer
  mainline Web bundle changes, and stage 3 contained the branch-adoption Web
  bundle changes. Discarding either side would lose shipped Web behavior, while
  combining minified output manually would be unsafe. The resolution is to
  regenerate the bundle once from the already-merged source tree, then run the
  Web build-layout and full repository gates. The retained stash and
  `backup/conflict-resolution-20260720-144817` branch are safety checkpoints
  until validation succeeds.
- The first full `npm run verify` on current `origin/main` exited 100 after
  328 passing tests because the unchanged live CLI integration test detected
  added `status`, `status_explanation`, and `actions` keys for a normal running
  task. The test remains unchanged; the production-only compatibility fix is
  isolated as Task 7D and blocked the PR gate until its fix passed.
- Task 7D's first mechanical packet check exited 1 because two required section
  headings were singular/missing; the packet was corrected and passed before
  dispatch. Cursor then changed exactly `output.rs`, returned a schema-valid
  RED/GREEN report, and used typed `TaskStatus::Running` conditional
  serialization. Parent review found no scope violation and accepted only after
  independently passing all nine packet commands.

## Validation results

- Task 1 RED: parser entry-count test exited 101 (actual 2, expected 3); refresh
  presence test exited 101 (`worktree_exists` remained false); repair-plan test
  exited 101 on the stale occupied-path blocker.
- Task 1 GREEN: all three focused commands exited 0.
- Task 1 VERIFY: `cargo test -p ajax-core refresh_git_substrate_evidence --
  --nocapture` exited 0 (3 passed); `cargo fmt --check` exited 0.
- Task 2 RED: the helper test exited 101 for missing
  `has_checkout_mismatch`; the reducer test exited 101 for missing
  `CheckoutMismatch` and the expected-branch argument.
- Task 2 GREEN/VERIFY: the helper, reducer, label, SQLite round-trip, and grouped
  runtime tests each exited 0; `cargo check -p ajax-core --all-targets` and
  `cargo fmt --check` exited 0.
- Task 3A RED: canonical status was Idle instead of Error; mismatch evidence was
  absent; recommendation selected Ship instead of Repair; inbox reasoning was
  not canonical; stale mismatch then incorrectly outranked missing-worktree
  status/actions. Each focused failure exited 101 before its implementation.
- Task 3A GREEN/VERIFY: four focused behavior tests, 12 grouped mismatch tests,
  three stale-precedence tests, `cargo check -p ajax-core --all-targets`,
  `cargo fmt --check`, and `git diff --check` all exited 0.
- Task 3B RED: CLI JSON returned null for the expected `status` before the serde
  change (exit 101). GREEN/VERIFY: core JSON contract, CLI human/JSON, Web
  card/detail, three-crate check, and formatting all exited 0.
- Task 4 RED (delegate evidence): Diff used `main...ajax/fix-login`, operation
  eligibility returned Allowed, safety returned Safe, and Drop returned no
  blocker; each focused test exited 101 before production edits. GREEN/VERIFY
  (parent rerun): the four focused behavior tests, two Diff tests, 12 grouped
  missing-worktree tests, 16 grouped checkout-mismatch tests,
  `cargo check -p ajax-core --all-targets`, `cargo fmt --check`, and
  `git diff --check` all exited 0.
- Task 5 RED (delegate evidence): the registry adoption method and typed plan
  field were absent, so all four focused commands exited 101 before production
  edits. GREEN/VERIFY (parent rerun): all four focused adoption tests, two
  existing Repair-operation tests, seven task-window Repair tests, 19 grouped
  checkout-mismatch tests, `cargo check -p ajax-core --all-targets`,
  `cargo fmt --check`, and `git diff --check` all exited 0.
- Task 6A RED (delegate evidence): both human rendering tests exited 101 because
  the typed adoption line was absent; JSON, decline, and persistence tests
  honestly passed as pre-existing Task 5 transport behavior. GREEN/VERIFY
  (parent rerun): four focused adoption tests, JSON rendering, two Review/HEAD
  regressions, 12 grouped Repair tests, `cargo check -p ajax-cli --all-targets`,
  `cargo fmt --check`, and `git diff --check` all exited 0.
- Task 6B RED (delegate evidence): the four focused native-Cockpit commands
  exited 101 before production edits because Repair neither prompted nor
  retained an exact plan. GREEN/VERIFY (parent rerun): all four focused tests,
  two pending-Repair tests, 95 grouped Cockpit tests, five ajax-tui confirmation
  tests, focused check, formatting, diff check, and warning-free clippy all
  exited 0.
- Task 6C1 RED (delegate evidence): all six focused Rust commands exited 101
  before production edits because Web actions and requests lacked the typed
  confirmation fields. GREEN/VERIFY (parent rerun): all six focused tests,
  seven grouped operate tests, 23 grouped Cockpit tests, the operation
  idempotency regression, focused two-crate check, formatting, diff check, and
  warning-free clippy all exited 0. The strengthened bare-boolean test and
  ordinary missing-worktree Repair regression also exited 0 afterward.
- Task 6C2 RED (delegate evidence): malformed adoption metadata was accepted,
  confirmed Repair omitted its pair/flag, refreshed props replaced pending
  data, and ordinary actions omitted false; each focused browser command exited
  1 before production edits. GREEN/VERIFY (parent rerun): four focused tests,
  both full test files (22 tests), TypeScript, lint, ast-grep, production build,
  build-layout check, and diff check all exited 0. The strengthened ActionBar
  file (11 tests) and corrected lint gate also exited 0.
- Task 7A RED (delegate evidence): stale `current_branch` produced the exact
  occupied-path blocker and no worktree-add plan (exit 101). GREEN/VERIFY
  (parent rerun): the focused regression, two existing missing-worktree tests,
  eight grouped task-window tests, focused check, formatting, and diff check all
  exited 0. Focused ajax-core Clippy exited 101 on the unrelated cumulative
  Task 5 test type at `task_operations.rs:1098`; assigned to Task 7C.
- Task 7C RED: ajax-core all-targets Clippy exited 101 on the cumulative
  `type_complexity` warning. GREEN/VERIFY (parent rerun): the focused stale/
  declined adoption test, warning-free ajax-core Clippy, formatting, and diff
  check all exited 0.
- Task 7B VERIFY (parent rerun): the architecture terminology search,
  `git diff --check -- architecture.md`, and `cargo fmt --check` all exited 0.
- Full gate attempt 1: `npm run verify` passed formatting, all-target checks,
  warning-free Clippy, and the first 328 Nextest cases, then exited 100 at
  `ajax-cli::live_cli live_new_execute_records_task_and_persists_it_to_sqlite_state`.
  The failure showed only the three newly serialized operator-state keys; Task
  7D owns the production fix and rerun.
- Task 7D RED: the unchanged focused live CLI test exited 101 because ordinary
  Running JSON included the new operator-state trio. GREEN/VERIFY (delegate and
  parent reruns): that integration test, the Waiting read-contract test, the
  mismatch human/JSON regression, four core output tests, formatting, two
  focused checks, diff check, and warning-free core Clippy all exited 0.
- Base-alignment validation: `npm run web:build`, `npm run web:build:check`, the
  two focused browser files (22 tests), and the focused ajax-web branch-adoption
  test all exited 0 after regenerating the conflicted bundle. Conflict-marker
  search exited 1 because it correctly found no markers; combined diff check
  exited 0.
- Full gate attempt 2: `npm run verify` exited 0. Formatting, all-target/all-
  feature check, warning-free Clippy, 1,702 Nextest cases, doc tests,
  TypeScript, ESLint, ast-grep, and 408 Vitest cases all passed. JSDOM printed
  its known unimplemented canvas diagnostic during Vitest, but the suite exited
  0 with every test passing.
- PR local gate: `npm prepare`, `cargo build --release -p ajax-cli`, and
  `cargo install --path crates/ajax-cli --locked --force` all exited 0. The
  locked install warned that `num-bigint v0.4.7` is yanked, then successfully
  replaced the local `ajax-cli` binary; no dependency or lockfile change was
  made.
- Post-PR base alignment: the Start target-branch collision regression, Web
  typed-adoption regression, Web stale-adoption regression, focused ajax-web
  all-target check, formatting, and diff check all exited 0 after resolving the
  single import conflict against `582d4ae`.
- Full gate attempt 3 on `582d4ae`: `npm run verify` exited 0. Formatting,
  all-target/all-feature check, warning-free Clippy, 1,706 Nextest cases, doc
  tests, TypeScript, ESLint, ast-grep, and 408 Vitest cases passed. JSDOM again
  printed only its non-fatal canvas diagnostic.

## Base alignment impact report

- PR branch: `fix/worktree-checkout-state`; base: `origin/main` at `962e382`;
  strategy: named stash, fast-forward, then stash restoration.
- Conflicted file: only generated `crates/ajax-web/web/dist/app.js`.
- Branch-side functionality: typed branch-adoption payload validation,
  retention, and submission in the Web Cockpit bundle.
- Base-side functionality: intervening Web fixes including asset-version/cache
  behavior and newer browser source bundled by main.
- Discard risk: choosing the branch bundle would lose mainline Web fixes;
  choosing the base bundle would lose branch adoption; naively merging minified
  output could create invalid or stale generated code.
- Resolution: source files merged without conflicts, then `npm run web:build`
  regenerated `app.js` from the combined source. `runtime.rs` and `operate.rs`
  retain both mainline and branch symbols; no additional source, schema,
  migration, lockfile, route, or configuration resolution was needed.
- Validation: deterministic Web build check, focused Rust/browser tests, full
  repository gate, marker search, and diff check passed. No remaining conflict
  risk or file requiring separate human conflict review was identified.
- After PR creation, `main` advanced once more to `582d4ae` with Start collision
  guards from PR #611. Rebase produced one import-block conflict in
  `ajax-web::slices::operate`: base added typed `local_branch_exists`, while
  this branch added typed `BranchAdoptionPlan`. Dropping base would weaken Start
  safety; dropping the branch would break explicit checkout adoption; a naive
  side choice would not compile one feature. The resolution retains both
  imports. Their separate function bodies/tests auto-merged, with no schema,
  lockfile, migration, generated-file, route, or configuration conflict.
