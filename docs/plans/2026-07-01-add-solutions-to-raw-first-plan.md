# Plan: Add Solutions To Raw-First Mobile Terminal Packet

## Scope

Documentation-only update to `docs/plans/2026-07-01-raw-first-mobile-terminal.md`.
No Rust, TypeScript, Svelte, generated assets, tests, README, architecture, or
AGENTS changes in this task.

## Task 1: Add A Review Findings And Solutions Section

- Documentation to update:
  - Add a concise section after the architecture/code-anchor context that records
    the review findings and the approved solution for each.
  - Cover product-roadmap alignment, Codeman alignment, unsupported subroute
    semantics, test deletion/replacement policy, pane approval boundaries,
    docs-search precision, and slash-command quick actions as follow-up scope.
- Verification:
  - Read the inserted section in context with `sed`.
  - Confirm the section is present with `rg -n "Review Findings|Solution" docs/plans/2026-07-01-raw-first-mobile-terminal.md`.

## Task 2: Tighten Backend Route Removal Instructions

- Documentation to update:
  - Update Task D so `/keys` and `/snapshot` removal tests assert exact
    unsupported-subroute behavior rather than accepting a broad 404.
  - Require the implementation to distinguish removed terminal subroutes from
    task-detail lookup for handles such as `web/fix-login/snapshot`.
  - Preserve `/api/tasks/{handle}/terminal` and guarded pane approval routes.
- Verification:
  - Search for `/snapshot`, `/keys`, `unsupported`, and `task not found` in the
    packet to confirm the route semantics are explicit.

## Task 3: Replace Test-Deletion Ambiguity With A Safe Test Policy

- Documentation to update:
  - Replace language that suggests deleting obsolete snapshot/composer tests with
    instructions to replace them with absence/removal coverage unless the user
    explicitly approves deletion.
  - Clarify that backend `send_task_keys` and `task_pane_snapshot` tests are real
    behavior coverage and must be removed only as part of the approved capability
    removal, with replacement route-level absence coverage.
- Verification:
  - `rg -n "delete|deleting|weaken|absence|removal coverage" docs/plans/2026-07-01-raw-first-mobile-terminal.md`

## Task 4: Tighten Docs Verification Search

- Documentation to update:
  - Replace broad `Live`/`snapshot` grep checks with targeted searches that do
    not flag legitimate architecture terms such as `Live Status`, Cockpit
    projection snapshots, or guarded pane snapshots.
  - Keep checks focused on `TerminalSnapshotView`, `sendTaskKeys`,
    `fetchTaskSnapshot`, `Terminal mode`, `snapshot/composer`, and explicit
    terminal-mode labels.
- Verification:
  - Inspect all verification command blocks in the packet.

## Task 5: Final Read-Through

- Documentation to update:
  - None unless the read-through catches an internal contradiction.
- Verification:
  - `sed -n '1,390p' docs/plans/2026-07-01-raw-first-mobile-terminal.md`
  - Confirm no code files changed with `git status --short`.
