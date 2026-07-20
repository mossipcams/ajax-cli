ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/task_operations.rs]
  REASON: This is a one-file test-only mechanical fix for a reproduced warning-as-error gate; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, the Clippy RED is not reproduced, the delta leaves allowed scope, behavior/assertions change, or verification fails]

PACKET_STATUS: READY
TASK_KIND: mechanical
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Clear the cumulative `clippy::type_complexity` warning in the Task 5
stale/declined branch-adoption test without changing behavior or assertions.

## Allowed files

- `crates/ajax-core/src/task_operations.rs`

## Forbidden changes

- Do not edit production logic, another file, or any file under a `tests/`
  directory.
- Do not add `#[allow]`, weaken/delete/reorder assertions, change stale cases,
  change closures, or split/merge behavior tests.
- Do not add a generic helper, trait, struct, enum, dependency, or unrelated
  cleanup.

## Context evidence

- Parent validation reproduced
  `cargo clippy -p ajax-core --all-targets -- -D warnings` exit 101 at
  `crates/ajax-core/src/task_operations.rs:1098`.
- The offending test-local declaration is
  `Vec<(&str, Box<dyn Fn(&mut CommandContext<InMemoryRegistry>)>)>` inside
  `checkout_mismatch_repair_rejects_stale_or_declined_adoption`.
- The behavior test itself already passes; this task is compiler/lint coverage,
  so the failing warning-as-error command is the meaningful RED gate.

## Code anchors

- `crates/ajax-core/src/task_operations.rs:1035`:
  `checkout_mismatch_repair_rejects_stale_or_declined_adoption`.
- `crates/ajax-core/src/task_operations.rs:1098`: the exact `stale_cases` vector
  annotation; expected mechanical match count is one.

## Test-first instructions

NOT_APPLICABLE: this is a test-only type-annotation cleanup with no behavior
change. The parent already captured the Clippy failure; before editing, reproduce
that exact exit 101 and do not add a runtime test.

## Edit instructions

1. Immediately before `stale_cases`, add one local type alias for the exact tuple
   type and change the vector annotation to `Vec<AliasName>`.
2. Keep the alias concrete and test-local. Change no closure body or assertion.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core checkout_mismatch_repair_rejects_stale_or_declined_adoption -- --nocapture`
2. `cargo clippy -p ajax-core --all-targets -- -D warnings`
3. `cargo fmt --check`
4. `git diff --check`

## Acceptance criteria

- The exact behavior test remains green.
- Focused ajax-core all-targets Clippy is warning-free.
- The delta is only a local alias plus the shorter vector annotation in the one
  allowed file.

## Stop conditions

- Stop if the RED is absent, another warning remains, or behavior code/assertions
  would need to change.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or prose before or
  after it. Every command needs its own evidence item.

---DELEGATE_REPORT_START---
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [crates/ajax-core/src/task_operations.rs]
  TEST_FIRST: PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED
      COMMAND: cargo clippy -p ajax-core --all-targets -- -D warnings
      EXIT_CODE: 101
      OUTPUT_EXCERPT: <type_complexity failure>
    - PHASE: GREEN
      COMMAND: <exact verification command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---

## Parent gate result

- Round 1 accepted on 2026-07-20 after deterministic scope review showed only
  the allowed test source changed, with one local alias and one shortened vector
  annotation. The raw report was complete, but the adapter again emitted
  `MISSING_STRUCTURED_REPORT`.
- The focused Task 5 behavior test, ajax-core all-targets Clippy, formatting,
  and diff check all exited 0 in the parent rerun.
