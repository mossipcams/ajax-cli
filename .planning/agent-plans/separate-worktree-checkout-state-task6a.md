ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-cli/src/render.rs, crates/ajax-cli/src/lib/tests.rs]
  REASON: This is a bounded adapter behavior change; the user explicitly requested Cursor delegation and the architecture-sensitive alternate lane was unavailable earlier in this goal.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Carry core's typed checkout-mismatch branch-adoption plan through the CLI. Human
Repair plans must name the exact observed and expected branches, JSON must retain
the typed payload, execution without `--yes` must decline without changing task
branch intent, and confirmed execution must persist the adopted branch without
running a Git switch/checkout, tmux command, or check command.

## Allowed files

- `crates/ajax-cli/src/render.rs`
- `crates/ajax-cli/src/lib/tests.rs`

## Forbidden changes

- Do not edit any other source, plan, generated run artifact, or file under a
  `tests/` directory.
- Do not change core adoption, registry, mismatch, refresh, or operation logic.
- Do not derive mismatch or branch policy in CLI code; render/pass the existing
  `CommandPlan.branch_adoption` payload only.
- Do not run or plan `git switch`, `git checkout`, tmux, shell/check, worktree,
  branch mutation, or any external command beyond the two existing Git
  observation commands used by CLI Repair refresh.
- Do not weaken confirmation, bypass `--yes`, rename/rekey tasks, or alter
  non-adoption Repair behavior.
- Do not add a dependency, abstraction, snapshot, or unrelated cleanup.
- Do not delete or weaken assertions. The two stale CLI Review assertions may
  be updated from `main...ajax/fix-login` to `main...HEAD` because Task 4 already
  changed the accepted core behavior.

## Context evidence

- `crates/ajax-core/src/commands/context.rs` already defines the serializable
  optional `CommandPlan.branch_adoption` with exact `expected_branch` and
  `observed_branch` values plus `CommandPlan::set_branch_adoption`.
- `crates/ajax-core/src/task_operations/task_command.rs` already returns a
  zero-command, confirmation-required Repair plan for a named fresh mismatch;
  execution unconditionally requires confirmation, revalidates the exact pair,
  adopts through Registry, and returns `state_changed = true`.
- `crates/ajax-cli/src/dispatch.rs::render_task_command` already refreshes Git
  evidence for Repair, passes the CLI `--yes` flag as `confirmed`, and returns
  core's state-changed result. Do not edit it unless the specified tests prove
  this evidence false; stop and report instead.
- `run_with_context_paths_and_runner` saves the registry snapshot when the
  rendered command reports a state change. Registry persistence serializes the
  whole Task, so no schema change is needed.
- `render_plan_human` currently prints title, confirmation, blockers, and
  external commands but omits the typed adoption payload.
- `QueuedRunner` and `output` already exist in `crates/ajax-cli/src/lib/tests.rs`.
  Git refresh for the one sample repo consumes exactly two outputs: porcelain
  worktree listing, then branch listing.
- A mismatch refresh fixture should report the exact task path on the observed
  branch, for example:
  `worktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/fix/pane-stuck\n\n`
  followed by `main\najax/fix-login\nfix/pane-stuck\n`.

## Code anchors

- `crates/ajax-cli/src/render.rs`: `render_plan_human` and its inline tests.
- `crates/ajax-cli/src/lib/tests.rs`: `sample_context`, `QueuedRunner`, `output`,
  `repair_command_renders_configured_test_plan`,
  `review_command_renders_diff_summary_plan`, `diff_execute_uses_injected_runner`,
  and existing SQLite persistence examples near Drop tests.
- `crates/ajax-cli/src/lib.rs`: read-only context, mutable runner, and
  paths-plus-runner entry points are context only, not allowed edits.

## Test-first instructions

Make all test edits before production edits. Run each named RED command and
capture the intended failure; a command that runs zero tests is not evidence.

1. In `render.rs`, add
   `render_plan_human_surfaces_typed_branch_adoption`. Build a plan titled
   `repair task: web/fix-login`, set adoption from `ajax/fix-login` to
   `fix/pane-stuck`, mark it confirmation-required, and assert the exact full
   output:
   `repair task: web/fix-login\nrequires confirmation\nadopt branch: fix/pane-stuck (expected ajax/fix-login)`.
   This must fail RED because the third line is currently absent.
2. Also prove `render_plan(..., true)` keeps structured JSON fields
   `branch_adoption.expected_branch` and `branch_adoption.observed_branch`.
   This is a contract assertion and may already pass before the human renderer
   implementation; report it honestly rather than manufacturing a failure.
3. In `lib/tests.rs`, add a small local fixture/helper only if it meaningfully
   avoids repetition. Add
   `repair_mismatch_cli_plan_renders_typed_adoption_and_requires_confirmation`.
   Seed task-1 with present Git evidence whose expected branch remains
   `ajax/fix-login` and current branch is `fix/pane-stuck`; render both human and
   JSON read-only Repair plans. Assert the exact human three-line output above,
   zero command lines, `requires_confirmation == true`, and exact JSON typed
   fields. This must fail RED on the missing human adoption line.
4. Add `repair_mismatch_cli_decline_preserves_branch_intent`. Use mutable
   dispatch with the named mismatch and the two queued Git refresh outputs.
   Execute without `--yes`; assert exact CLI confirmation-required error, task
   branch remains `ajax/fix-login`, and runner recorded exactly the two Git
   observation commands with no switch/checkout/tmux/sh command.
5. Add `repair_mismatch_cli_yes_persists_adopted_branch_without_switching`.
   Create a unique temp config/state pair using existing file-backed test
   patterns, save a context whose task has named mismatch evidence, call
   `run_with_context_paths_and_runner` with
   `ajax repair web/fix-login --execute --yes` and the same two refresh outputs,
   reload SQLite, clean up the temp directory, and assert: empty execution
   output; stored task branch is `fix/pane-stuck`; stored task ID/handle/path/
   tmux session are unchanged; stored task no longer has checkout mismatch;
   runner commands are exactly the two observation commands and contain no
   switch/checkout/tmux/sh command.
6. Before production edits run:
   - `cargo test -p ajax-cli render_plan_human_surfaces_typed_branch_adoption -- --nocapture`
   - `cargo test -p ajax-cli repair_mismatch_cli_plan_renders_typed_adoption_and_requires_confirmation -- --nocapture`
   - `cargo test -p ajax-cli repair_mismatch_cli_decline_preserves_branch_intent -- --nocapture`
   - `cargo test -p ajax-cli repair_mismatch_cli_yes_persists_adopted_branch_without_switching -- --nocapture`
7. The first two commands must fail for the missing adoption line before the
   renderer edit. The execution assertions may already pass because the core
   and dispatch transport were implemented in Task 5; preserve and report
   those characterization results honestly.

## Edit instructions

1. In `render_plan_human`, after the existing confirmation line and before
   blockers/commands, render `plan.branch_adoption` as exactly
   `adopt branch: <observed> (expected <expected>)`.
2. Add no new CLI planning or execution branch. The typed payload, core
   executor, existing `--yes`, and existing persistence gate are the source of
   truth.
3. Update the two stale CLI Review expectations in `lib/tests.rs` from
   `main...ajax/fix-login` to `main...HEAD`; preserve every surrounding assertion.
4. Keep the source delta minimal. If any requested execution/persistence test
   fails after the renderer change because existing dispatch evidence is
   inaccurate, stop and report the exact failure rather than expanding scope.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-cli render_plan_human_surfaces_typed_branch_adoption -- --nocapture`
2. `cargo test -p ajax-cli repair_mismatch_cli_plan_renders_typed_adoption_and_requires_confirmation -- --nocapture`
3. `cargo test -p ajax-cli repair_mismatch_cli_decline_preserves_branch_intent -- --nocapture`
4. `cargo test -p ajax-cli repair_mismatch_cli_yes_persists_adopted_branch_without_switching -- --nocapture`
5. `cargo test -p ajax-cli render_plan_json_remains_structured -- --nocapture`
6. `cargo test -p ajax-cli review_command_renders_diff_summary_plan -- --nocapture`
7. `cargo test -p ajax-cli diff_execute_uses_injected_runner -- --nocapture`
8. `cargo test -p ajax-cli repair_ -- --nocapture`
9. `cargo check -p ajax-cli --all-targets`
10. `cargo fmt --check`
11. `git diff --check`

## Acceptance criteria

- Human Repair plan names exact observed and expected branches after the
  existing confirmation line; JSON retains the typed core payload.
- Named mismatch execution without `--yes` leaves branch intent unchanged and
  returns the normal CLI confirmation error.
- Named mismatch execution with `--yes` persists the adopted branch through
  SQLite and clears derived mismatch without changing task identity/path/session.
- Both execution paths invoke only existing Git observation commands and never
  switch/checkout a branch or invoke tmux/check commands.
- Existing ordinary Repair and updated `base...HEAD` Review tests pass.
- Only the two allowed files change and every packet verification command passes.

## Stop conditions

- Stop if CLI needs to inspect branch names or implement adoption policy itself.
- Stop if confirmed adoption needs an external mutation command, schema change,
  task rename/rekey, or edit outside the allowed files.
- Stop if a baseline failure is unrelated to this packet; do not change
  unrelated code or tests.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or prose before or
  after it. Every verification command needs its own evidence item.

---DELEGATE_REPORT_START---
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [<allowed source paths>]
  TEST_FIRST: PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED
      COMMAND: <exact focused command>
      EXIT_CODE: <nonzero or honest zero for characterization>
      OUTPUT_EXCERPT: <intended failure or pre-existing pass>
    - PHASE: GREEN
      COMMAND: <same focused command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
    - PHASE: VERIFY
      COMMAND: <remaining command; add one item per command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---

## Parent gate result

- Status: accepted without a revision round.
- Delegate TDD evidence: the two human-rendering tests exited 101 before the
  renderer edit and 0 afterward; JSON, decline, and persistence tests honestly
  characterized already-working Task 5 transport.
- Parent verification: all eleven packet commands exited 0.
- Safety review: CLI reads only the typed core payload, `--yes` reaches the core
  executor, SQLite saves the state-changed result, and recorded commands are
  exactly the two read-only Git observation commands.
- Scope: exactly the two allowed source files changed. The first snapshot label
  was corrected after dispatch; generated in-repo run artifacts were excluded
  from source scope review and removed after acceptance.
- Report caveat: raw output contained the complete requested marker envelope,
  but the adapter classified it as `MISSING_STRUCTURED_REPORT`; parent-run
  validation supplied acceptance evidence.
