# Ajax Critical and High Defects Remediation Plan

## Objective

Fix the confirmed Critical and High defects from the repository-wide review,
in risk order:

1. Prevent concurrent native/Web saves from silently losing durable Ajax facts.
2. Make Web task creation idempotent and coordinated.
3. Keep slow external commands outside the global Web Cockpit state lock.
4. Bound browser pane tmux commands.
5. Persist start intent and successful provisioning receipts at step boundaries.
6. Make supervisor cancellation wait for the child process to exit.

Preserve public CLI/API vocabulary and existing crate boundaries. Update
`architecture.md` when the finished implementation changes persistence,
operation-coordination, or runtime-lock contracts.

## Explicit Test File Approval

Approval of this plan explicitly permits modifying tests only in these existing
source-module test locations:

- `crates/ajax-cli/src/context.rs`
- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-core/src/registry/sqlite.rs`
- `crates/ajax-core/src/task_operations.rs`
- `crates/ajax-core/src/slices/pane.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/pane.rs`
- `crates/ajax-supervisor/src/runtime.rs`
- `crates/ajax-supervisor/src/process_observer.rs`

Do not modify files under any crate's `tests/` directory, including
`crates/ajax-cli/tests/smoke_user_flows.rs`.

## Task 1: Add Durable SQLite Revision Compare-And-Swap

**Failing behavior test**

- In `crates/ajax-core/src/registry/sqlite.rs`, open two independently loaded
  snapshots of one database, save the first, then assert saving the second with
  its stale revision returns an explicit conflict instead of replacing the
  first save.
- Assert a successful save advances the durable revision.

**Code to implement**

- Add a small Ajax-owned registry revision value in SQLite.
- Load the revision with registry snapshots.
- Save inside one SQLite transaction using a compare-and-swap revision check.
- Return a typed snapshot conflict when the expected revision is stale.
- Keep the existing schema migration path compatible with current databases.

**Verification**

```sh
rtk cargo nextest run -p ajax-core registry::sqlite
```

## Task 2: Preserve Concurrent Task Facts During CLI Save

**Failing behavior test**

- In `crates/ajax-cli/src/context.rs`, create concurrent native/Web changes to
  the same task while lifecycle remains equal.
- Assert the final save preserves disk-side runtime evidence, side flags,
  metadata, agent attempts, events, and step receipts rather than silently
  replacing them.
- Add a regression where concurrent writes are detected by revision even when
  file mtime is unchanged.

**Code to implement**

- Replace mtime as the correctness mechanism with the SQLite revision contract.
- Retain mtime only as an optional reload optimization.
- Expand registry merge behavior to preserve independently added durable facts.
- Surface explicit conflict for incompatible same-task field changes.
- Avoid whole-snapshot overwrite after a stale load.

**Verification**

```sh
rtk cargo nextest run -p ajax-cli context
rtk cargo nextest run -p ajax-core registry::sqlite
```

## Task 3: Coordinate and Deduplicate Web Task Starts

**Failing behavior test**

- In `crates/ajax-web/src/runtime.rs`, submit the same `/api/tasks` request ID
  twice and assert the bridge executes start once and returns the cached first
  response.
- Submit concurrent start requests for the same intended task and assert the
  second receives a typed conflict.
- Assert empty request IDs are rejected.

**Code to implement**

- Route `/api/tasks` through `OperationCoordinator`.
- Use `StartTaskRequest.request_id` for in-flight and completed-response
  deduplication.
- Coordinate starts by a stable intended-task key derived before provisioning.
- Return request IDs in start responses consistently with other operations.

**Verification**

```sh
rtk cargo nextest run -p ajax-web runtime::tests::axum_start
```

## Task 4: Release the Global Web Lock Around Start and Task Operations

**Failing behavior test**

- In `crates/ajax-web/src/runtime.rs`, run a deliberately slow start and assert
  `/api/cockpit` and task detail remain responsive.
- Run a deliberately slow operation on one task and assert task detail and an
  operation on another task remain responsive.
- Assert concurrent result merging does not overwrite a newer revision.

**Code to implement**

- Snapshot the context, runner, bridge, and revision under a short lock.
- Run external operation work outside the shared-state mutex.
- Merge the result under the lock only when its revision/base state remains
  valid.
- Keep per-task operation serialization in `OperationCoordinator`.
- Return an explicit conflict when a result cannot safely merge.

**Verification**

```sh
rtk cargo nextest run -p ajax-web runtime::tests::axum
```

## Task 5: Bound Pane Commands and Release the Lock During Answers

**Failing behavior test**

- In `crates/ajax-core/src/slices/pane.rs`, assert capture and send-key command
  specs carry a bounded timeout.
- In `crates/ajax-web/src/runtime.rs`, run a slow prompt answer and assert
  `/api/cockpit` and task detail remain responsive.
- In `crates/ajax-web/src/slices/pane.rs`, assert an answer commit is rejected
  when the task/prompt generation changed during unlocked external work.

**Code to implement**

- Apply the existing bounded tmux probe timeout contract to pane capture,
  prompt recapture, and send-keys.
- Split prompt answering into prepare, external capture/send, and commit
  phases.
- Run tmux work outside the global shared-state lock.
- Preserve fingerprint validation, request deduplication, and rate limiting.

**Verification**

```sh
rtk cargo nextest run -p ajax-core slices::pane
rtk cargo nextest run -p ajax-web slices::pane
rtk cargo nextest run -p ajax-web runtime::tests::axum_task_answer
```

## Task 6: Reject Out-of-Order Pane Snapshot Commits

**Failing behavior test**

- In `crates/ajax-web/src/slices/pane.rs`, prepare two captures from one
  sequence, commit the newer capture first, then assert the older capture
  cannot replace newer lines, state, or prompt fingerprint.

**Code to implement**

- Carry the prepared pane generation in `PaneCaptureWork`.
- Commit only when the stored generation still matches.
- Return the current snapshot or a typed stale-capture result without
  incrementing sequence for discarded work.

**Verification**

```sh
rtk cargo nextest run -p ajax-web slices::pane
```

## Task 7: Persist Start Intent and Receipts Incrementally

**Failing behavior test**

- In `crates/ajax-core/src/task_operations.rs`, verify start execution exposes a
  persistence checkpoint after recording provisional intent and after every
  successful provisioning receipt.
- In `crates/ajax-cli/src/web_backend.rs`, simulate failure after one successful
  provisioning effect and assert SQLite contains the provisional task and that
  step's receipt.

**Code to implement**

- Add a narrowly scoped persistence callback/port to start execution rather
  than coupling core to SQLite.
- Persist provisional task intent before the first external side effect.
- Persist after each successful named step and receipt.
- Preserve existing final success/error persistence behavior.

**Verification**

```sh
rtk cargo nextest run -p ajax-core task_operations::tests::start
rtk cargo nextest run -p ajax-cli web_backend
```

## Task 8: Wait for Supervisor Child Exit on Cancellation

**Failing behavior test**

- In `crates/ajax-supervisor/src/process_observer.rs` or
  `crates/ajax-supervisor/src/runtime.rs`, start a process that records shutdown,
  cancel the monitor, await the handle, and assert the child is no longer
  running before cancellation completes.
- Assert cancellation remains bounded when the process ignores graceful
  termination.

**Code to implement**

- Explicitly terminate and await the child during cancellation.
- Add bounded escalation if graceful termination does not complete.
- Keep stdout/stderr reader shutdown and monitor receiver closure ordered.

**Verification**

```sh
rtk cargo nextest run -p ajax-supervisor runtime_cancel
rtk cargo nextest run -p ajax-supervisor process_observer
```

## Task 9: Document the Finished Architecture Contracts

**Documentation to update**

- Update `architecture.md` with:
  - SQLite revision-based optimistic concurrency.
  - Durable incremental start receipts.
  - Per-task Web operation coordination and unlocked external work.
  - Bounded pane control commands.
  - Supervisor cancellation completion semantics.

**Verification**

```sh
rtk rg -n "revision|receipt|operation coordinator|pane|cancellation" architecture.md
```

## Task 10: Full Validation

**Verification**

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
RUSTDOCFLAGS="-D warnings" rtk cargo doc --no-deps --all-features
rtk cargo audit -D warnings
```

Report every failed or unavailable command explicitly.

## Execution Order and Stop Points

Execute Tasks 1–10 in order. Tasks 1 and 2 establish the persistence contract
required before increasing Web Cockpit concurrency. After each approved task,
follow TDD and ask exactly:

```text
Task N done. Continue?
```

unless the user explicitly approves completing the full plan without stopping.
