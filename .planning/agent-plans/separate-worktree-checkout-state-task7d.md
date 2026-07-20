ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 1
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/output.rs]
  REASON: One bounded serialization compatibility fix with an exact existing RED integration test; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, another source file is required, the mismatch JSON regression must weaken, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Restore the established `tasks --json` shape for an ordinary running task while
preserving the new canonical `status`, `status_explanation`, and `actions`
fields for checkout mismatch and other non-running operator states.

## Allowed files

- `crates/ajax-core/src/output.rs`

## Forbidden changes

- Do not edit any file under a `tests/` directory.
- Do not edit projection, UI-state, CLI rendering, architecture, Web, generated,
  lock, manifest, or planning files.
- Do not delete, weaken, skip, or rewrite an assertion.
- Do not hide mismatch status, explanation, or actions from JSON.
- Do not remove the three fields from `TaskSummary`; human rendering still uses
  them.
- Do not commit, push, merge, rebase, create branches, or change branches.

## Context evidence

- Current `origin/main` kept `TaskSummary.status`,
  `TaskSummary.status_explanation`, and `TaskSummary.actions` internal with
  `skip_serializing`.
- This branch exposed all three so a checkout mismatch appears identically in
  human and JSON output.
- The full gate now fails only because a newly-created ordinary running task
  serializes `status: running`, `status_explanation: Agent working`, and
  `actions: [resume, drop]`, expanding a stable live CLI JSON contract.
- `TaskSummary` already has the canonical `TaskStatus`, so serialization can
  distinguish running from non-running without deriving policy in an adapter.
- The inline core read-contract test uses `Waiting` and must continue to
  serialize the three fields. The inline CLI mismatch regression uses `Error`
  and must continue to serialize its exact explanation and Repair/Resume pair.

## Code anchors

- `crates/ajax-core/src/output.rs` → `TaskSummary` derive/serialization near the
  top of the file. Implement the smallest local conditional serialization that
  omits all three operator-state fields only when `status == TaskStatus::Running`.
  Preserve derived `Deserialize` and every other field exactly.

## Edit instructions

1. Keep `TaskSummary`'s current fields and derived `Deserialize` behavior.
2. Serialize the legacy fields for every task.
3. Serialize `status`, `status_explanation`, and `actions` together only when
   typed `status` is not `TaskStatus::Running`.
4. Use local typed status logic; do not inspect explanation text or action
   strings to decide the JSON shape.

## Test-first instructions

1. Run the existing focused live integration test before editing and record its
   nonzero exit plus the unexpected three fields. Do not modify that test.
2. Make the minimal production-only change in `output.rs`.
3. Rerun the same command and record exit 0.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-cli --test live_cli live_new_execute_records_task_and_persists_it_to_sqlite_state -- --nocapture`
2. `cargo test -p ajax-core read_commands_serialize_as_json_contracts -- --nocapture`
3. `cargo test -p ajax-cli checkout_mismatch_renders_identically_in_human_and_json -- --nocapture`
4. `cargo test -p ajax-core output::tests -- --nocapture`
5. `cargo fmt --check`
6. `cargo check -p ajax-core --all-targets`
7. `cargo check -p ajax-cli --all-targets`
8. `git diff --check -- crates/ajax-core/src/output.rs`
9. `cargo clippy -p ajax-core --all-targets -- -D warnings`

## Acceptance criteria

- The unchanged live integration test passes and ordinary Running task JSON has
  exactly the established fields, with no operator-state trio.
- Waiting output retains its existing inline JSON contract.
- Checkout mismatch JSON still contains `status: error`, the exact mismatch
  explanation, and its actions.
- Human rendering retains access to all three in-memory fields.
- Only `crates/ajax-core/src/output.rs` changes and all nine commands pass.

## Stop conditions

- Stop if satisfying the live contract requires changing a test or another
  production file.
- Stop if the solution depends on matching explanation text or action strings
  instead of the typed status.
- Stop if any non-running operator state loses its three JSON fields.
- Return the exact report below as the entire response, without Markdown fences
  or prose outside the markers. Include separate RED and GREEN evidence for the
  focused live test and an evidence item for every verification command.

ROUTER_REPORT_BEGIN
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [crates/ajax-core/src/output.rs]
  TEST_FIRST: PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED
      COMMAND: <exact focused command>
      EXIT_CODE: <nonzero>
      OUTPUT_EXCERPT: <unexpected running-task operator-state keys>
    - PHASE: GREEN
      COMMAND: <same exact focused command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
    - PHASE: VERIFY
      COMMAND: <exact command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
ROUTER_REPORT_END

## Parent review result

- ACCEPTED. Deterministic inspection found exactly one modified allowed path,
  `crates/ajax-core/src/output.rs`, and no scope violations.
- The delegate report was schema-valid and proved the unchanged live integration
  test failed before the production edit and passed afterward.
- Parent verification independently passed all nine packet commands. The custom
  serializer keys only on typed `TaskStatus::Running`; non-running JSON and all
  in-memory fields remain intact.
