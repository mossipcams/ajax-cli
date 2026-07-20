ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [architecture.md]
  REASON: This is one bounded authoritative documentation update after behavior is accepted; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, the delta leaves architecture.md, source behavior is ambiguous, or verification fails]

PACKET_STATUS: READY
TASK_KIND: docs-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Document the completed separation between physical worktree presence, expected
branch intent, and observed checkout, including canonical mismatch behavior,
operation safety, explicit branch adoption, and exact confirmation transport
across operator adapters.

## Allowed files

- `architecture.md`

## Forbidden changes

- Do not edit source, tests, README, changelog, another doc, or planning files.
- Do not describe implementation history, packet/task numbers, temporary names,
  or rejected approaches.
- Do not claim Ajax owns Git truth, that mismatch is healthy, that branches are
  switched automatically, or that browser/TUI owns branch policy.
- Do not duplicate a long architecture section or reorganize unrelated content.

## Context evidence

- `Task::has_checkout_mismatch` derives mismatch only for a present worktree
  whose observed named branch differs from expected, or whose checkout is
  detached.
- Git refresh derives `worktree_exists` from exact registered-path presence,
  `branch_exists` from expected-branch existence in the repo, and
  `current_branch` from the checkout at that path.
- Task-window repair no longer consults stale `current_branch` when the path is
  absent; it blocks on missing expected branch or recreates from an existing
  expected branch.
- Core status/action projections render mismatch distinctly from missing
  substrate. Open/Resume and Check remain usable, Review diffs `base...HEAD`,
  while Ship and Drop/Cleanup are blocked until reconciliation.
- Mismatch Repair creates a zero-command, confirmation-required typed
  `BranchAdoptionPlan`. Core revalidates the exact expected/observed pair,
  updates only task branch intent, records the substrate-change event, and
  preserves identity/path/session/lifecycle/history. Detached checkout cannot
  be adopted; switching back manually and refreshing clears mismatch without
  changing intent.
- CLI, native Cockpit, and Web Cockpit display/retain the exact core pair. A
  bare boolean is insufficient; core rejects changed evidence as stale. Browser
  state is transient and never becomes task truth.

## Code anchors

- `architecture.md` → `## Task Authority Model`, after the paragraph about
  cached substrate evidence.
- `architecture.md` → `## Task Operations`, the single-task command-operation
  bullet.
- `architecture.md` → `### ajax-web::slices::cockpit` and
  `### ajax-web::slices::operate` for concise adapter confirmation boundaries.

## Test-first instructions

NOT_APPLICABLE: this is an authoritative prose update describing behavior
already covered by accepted executable tests. Do not invent a documentation
snapshot test.

## Edit instructions

1. Add one compact subsection under Task Authority Model defining:
   - registered-path presence (`worktree_exists`),
   - expected branch intent (`Task.branch`) and independent expected-branch
     existence (`branch_exists`),
   - observed checkout (`current_branch`; named or detached),
   - present different/detached checkout as mismatch, never missing.
2. State reconciliation/precedence: true physical absence remains missing
   substrate; present mismatch has its own error explanation/action path; an
   aligned refresh clears mismatch; missing-path repair ignores stale observed
   checkout and uses expected-branch existence.
3. Extend the task-operation description concisely with the allowed/blocked
   operation matrix and explicit Repair adoption invariants. Make clear Review
   targets `base...HEAD`, adoption runs no branch-switch command, and detached
   checkout requires an external explicit switch plus refresh.
4. Add a short adapter contract: confirmation-required actions carry the exact
   typed expected/observed pair; native and browser retain that plan/payload
   between activations; core alone revalidates and mutates. Do not add browser
   branch comparisons or another policy description.

## Verification commands

Run in this order and report every exit code:

1. `rg -n "Worktree presence|checkout mismatch|base\.\.\.HEAD|BranchAdoptionPlan|stale" architecture.md`
2. `git diff --check -- architecture.md`
3. `cargo fmt --check`

## Acceptance criteria

- The three facts and their ownership are unambiguous.
- Missing path and present mismatch cannot be conflated by the prose.
- Operation safety, explicit adoption, identity preservation, detached handling,
  and exact stale confirmation are all documented once in their owning sections.
- Only `architecture.md` changes and all three commands pass.

## Stop conditions

- Stop if any claim cannot be verified from current source/tests or requires a
  second document.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or prose before or
  after it. Every command needs its own evidence item.

---DELEGATE_REPORT_START---
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [architecture.md]
  TEST_FIRST: NOT_APPLICABLE
  COMMAND_EVIDENCE:
    - PHASE: VERIFY
      COMMAND: <exact command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---

## Parent review result

- ACCEPTED. Deterministic delta inspection showed only `architecture.md` was
  modified, with no scope violations.
- The delegate raw log contained the complete requested report and three
  passing commands, but the adapter emitted `MISSING_STRUCTURED_REPORT`.
- Parent verification independently passed the terminology search,
  `git diff --check -- architecture.md`, and `cargo fmt --check`.
