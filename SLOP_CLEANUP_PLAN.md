# Slop Cleanup Runbook

This is an explicit step-by-step runbook for addressing the "clean but
vibe-coded slop" review findings. It is written for a non-frontier agent. Do not
skip steps. Do not infer broad refactors from the task names. Each task is a
small red/green change.

## Non-Negotiable Workflow

1. Read `architecture.md`.
2. Read `/Users/matt/.codex/RTK.md`.
3. Prefix every shell command with `rtk`.
4. Use `rg` for text search.
5. Use `ast-grep` when looking for Rust code shape.
6. Do not edit `crates/ajax-cli/tests/smoke_user_flows.rs`.
7. Do not edit files under a `tests/` directory unless a later approved plan
   explicitly names that exact file.
8. For every code task:
   - Write the named failing test first.
   - Run the named focused test.
   - Confirm and report the failure.
   - Implement only the smallest code change needed.
   - Run the same focused test.
   - Confirm and report the pass.
   - Ask exactly `Task N done. Continue?`
   - Wait unless the user has explicitly approved completing the whole runbook.
9. For documentation or CI-only edits, run the listed verification commands and
   report exact results.
10. Never claim a check passed unless the command was run and passed.

## Required Preflight

1. Confirm the worktree state.

   ```sh
   rtk git status --short
   ```

2. Read architecture and RTK.

   ```sh
   rtk sed -n '1,260p' architecture.md
   rtk sed -n '1,260p' /Users/matt/.codex/RTK.md
   ```

3. Run the structural inventory below. Save notable paths and line numbers in
   your work notes.

   ```sh
   rtk ast-grep -p '$STATE.run_optimistic($$$ARGS)' --lang rust crates/ajax-web/src/runtime.rs
   rtk ast-grep -p '$REGISTRY.get_task_mut($ID)' --lang rust crates/ajax-core/src crates/ajax-cli/src crates/ajax-web/src
   rtk ast-grep -p '$TASK.lifecycle_status = $STATUS' --lang rust crates/ajax-core/src crates/ajax-cli/src crates/ajax-web/src
   rtk ast-grep -p 'DefaultHasher::new()' --lang rust crates
   rtk ast-grep -p 'include_bytes!($PATH)' --lang rust crates/ajax-web/src
   rtk ast-grep -p 'include_str!($PATH)' --lang rust crates/ajax-web/src
   rtk ast-grep -p 'pub fn execute_plan($$$ARGS) -> Result<Vec<CommandOutput>, CommandError> { $$$BODY }' --lang rust crates/ajax-core/src/commands.rs
   rtk ast-grep -p 'for $CMD in &$PLAN.commands { $$$BODY }' --lang rust crates/ajax-core/src
   rtk rg 'pub fn route|pub fn route_with_bridge|split_path_and_query|percent_decode' crates/ajax-web/src/runtime.rs crates/ajax-cli/src/web_backend.rs
   ```

4. Do not change code during preflight.

## Task 1: Block Concurrent Web Mutations Before Side Effects

Goal: a second mutable web action must be rejected before bridge side effects
when another mutable action is already in flight.

1. Inspect the current implementation.

   ```sh
   rtk sed -n '70,130p' crates/ajax-web/src/runtime.rs
   rtk sed -n '190,235p' crates/ajax-web/src/runtime.rs
   rtk sed -n '785,865p' crates/ajax-web/src/runtime.rs
   rtk ast-grep -p '$STATE.run_optimistic($$$ARGS)' --lang rust crates/ajax-web/src/runtime.rs
   ```

2. In `crates/ajax-web/src/runtime.rs`, add a failing async test named
   `axum_rejects_concurrent_different_task_operations_before_bridge_side_effects`.

3. The test must:
   - Build an Axum app with two existing tasks, for example `web/fix-login` and
     `api/fix-auth`.
   - Use a fake bridge/runner that counts operation execution.
   - Make the first action block after entering the bridge.
   - Send a second action for the other task while the first is blocked.
   - Assert the second response is `409 Conflict`.
   - Assert the bridge operation count is `1`, proving the second request did
     not run external side effects.

4. Run the focused test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-web axum_rejects_concurrent_different_task_operations_before_bridge_side_effects
   ```

5. Implement the smallest fix:
   - Add a helper on `OperationCoordinator` that answers whether any mutable
     action is in flight.
   - In the Axum action route, check that helper before calling
     `run_optimistic`.
   - If any mutable action is in flight, return the existing conflict/error
     response shape with the request id preserved.
   - Preserve duplicate-request idempotency. A repeated request id should still
     receive the cached response when appropriate.

6. Run the same focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-web axum_rejects_concurrent_different_task_operations_before_bridge_side_effects
   ```

7. Run the nearby concurrency tests.

   ```sh
   rtk cargo nextest run -p ajax-web axum_operation
   ```

8. Stop and report: failure output summary, implementation summary, pass output
   summary.

Risk: medium. This intentionally serializes mutable web actions to protect
state and side-effect consistency.

## Task 2: Normalize Start-Task Identity Once

Goal: web start-task in-flight keys must use the same identity rules as core
task creation.

1. Inspect current identity code.

   ```sh
   rtk sed -n '380,525p' crates/ajax-core/src/commands/new_task.rs
   rtk sed -n '725,780p' crates/ajax-web/src/runtime.rs
   rtk rg 'format!\\("start:' crates/ajax-web/src/runtime.rs
   ```

2. In `crates/ajax-core/src/commands/new_task.rs`, add a failing test named
   `start_task_identity_uses_core_slug_rules`.

3. The core test must assert:
   - A request for title `"Fix login"` and repo `"web"` produces identity
     `web/fix-login`.
   - A request for title `"Fix login!"` and repo `"web"` produces the same
     identity.
   - The helper returns a typed `TaskId` or a new narrow identity type, not a
     raw ad hoc formatted string.

4. Run the core focused test and confirm it fails because the helper does not
   exist yet.

   ```sh
   rtk cargo nextest run -p ajax-core start_task_identity_uses_core_slug_rules
   ```

5. Implement the smallest core helper:
   - Reuse the existing slugification path.
   - Do not copy slug logic into a new function with different behavior.
   - Expose only what web needs.

6. Run the same core focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-core start_task_identity_uses_core_slug_rules
   ```

7. In `crates/ajax-web/src/runtime.rs`, add a failing async test named
   `axum_start_task_rejects_concurrent_colliding_normalized_identity`.

8. The web test must:
   - Send concurrent start requests for `"Fix login"` and `"Fix login!"` in the
     same repo.
   - Block the first bridge start.
   - Assert the second response is a conflict before bridge execution.
   - Assert the bridge start count is `1`.

9. Run the web focused test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-web axum_start_task_rejects_concurrent_colliding_normalized_identity
   ```

10. Update `axum_start_task`:
    - Replace `format!("start:{}:{}", request.repo, request.title.trim())`.
    - Use the core helper from step 5.
    - Keep `request_id` behavior unchanged.

11. Run the web focused test and confirm it passes.

    ```sh
    rtk cargo nextest run -p ajax-web axum_start_task_rejects_concurrent_colliding_normalized_identity
    ```

12. Run an AST/text check to ensure the old ad hoc key is gone.

    ```sh
    rtk rg 'format!\\("start:' crates/ajax-web/src/runtime.rs
    ```

Risk: medium. This changes only concurrent duplicate detection, not task
creation behavior.

## Task 3: Bound Completed Operation Idempotency Cache

Goal: completed request-id cache must not grow forever.

1. Inspect the coordinator fields and completion path.

   ```sh
   rtk sed -n '195,230p' crates/ajax-web/src/runtime.rs
   rtk rg 'completed' crates/ajax-web/src/runtime.rs
   ```

2. In `crates/ajax-web/src/runtime.rs`, add a failing unit test named
   `operation_coordinator_prunes_completed_request_ids`.

3. The test must:
   - Create a coordinator.
   - Insert more completed request ids than the chosen cap.
   - Assert the oldest completed request id is no longer returned.
   - Assert the newest completed request id is still returned.
   - Assert in-flight request behavior still works.

4. Run the focused test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-web operation_coordinator_prunes_completed_request_ids
   ```

5. Implement the smallest fix:
   - Add `const MAX_COMPLETED_OPERATIONS: usize = 128`.
   - Add a queue, for example `VecDeque<String>`, beside the completed map.
   - When storing a completed response, push the request id and prune oldest
     ids until the map is within the cap.
   - Do not add TTLs or background cleanup.

6. Run the focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-web operation_coordinator_prunes_completed_request_ids
   ```

Risk: low.

## Task 4: Make Stale Web Dist Impossible in CI

Goal: CI must fail when embedded web assets are stale.

1. Inspect scripts and CI.

   ```sh
   rtk sed -n '1,180p' package.json
   rtk sed -n '1,140p' .github/workflows/ci.yml
   rtk sed -n '1,220p' scripts/web-build-check.mjs
   rtk rg 'web:build:check|web:test|web:check|web:smoke' package.json .github/workflows scripts
   ```

2. In `crates/ajax-cli/src/lib/tests.rs`, add a failing test named
   `ci_web_job_runs_web_build_check`.

3. The test must:
   - Read `.github/workflows/ci.yml`.
   - Assert the CI workflow contains `npm run web:build:check`.
   - Prefer a simple text assertion over YAML parsing unless the file already
     has a YAML helper.

4. Run the focused test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-cli ci_web_job_runs_web_build_check
   ```

5. Implement the CI change:
   - Add `npm run web:build:check` to the existing web CI job after web source
     tests and before any smoke check.
   - Do not weaken existing web checks.
   - Only add it to `package.json` aggregate verify scripts if that aggregate is
     intended to mirror CI.

6. Run the focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-cli ci_web_job_runs_web_build_check
   ```

7. Run the actual asset freshness command.

   ```sh
   rtk npm run web:build:check
   ```

Risk: low.

## Task 5: Replace Persistent DefaultHasher Use

Goal: persisted or user-visible names must not depend on `DefaultHasher`.

1. Inspect current hashing.

   ```sh
   rtk sed -n '390,420p' crates/ajax-core/src/commands/new_task.rs
   rtk ast-grep -p 'DefaultHasher::new()' --lang rust crates
   rtk rg 'fnv|hash' crates/ajax-web/src crates/ajax-core/src
   ```

2. In `crates/ajax-core/src/commands/new_task.rs`, add a failing test named
   `rooted_repo_dir_hash_is_stable_for_known_path`.

3. The test must:
   - Use one fixed absolute path string.
   - Assert the hash suffix equals a hard-coded value produced by the new stable
     algorithm.
   - Assert the same input returns the same suffix on repeated calls.

4. Run the focused test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-core rooted_repo_dir_hash_is_stable_for_known_path
   ```

5. Implement the smallest production change:
   - Replace production `DefaultHasher::new()` in
     `crates/ajax-core/src/commands/new_task.rs`.
   - Use a stable local FNV-1a style helper.
   - Keep the directory name format otherwise unchanged.
   - Do not edit `crates/ajax-cli/tests/smoke_user_flows.rs`.

6. Run the focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-core rooted_repo_dir_hash_is_stable_for_known_path
   ```

7. Confirm production `DefaultHasher` is gone from core.

   ```sh
   rtk ast-grep -p 'DefaultHasher::new()' --lang rust crates/ajax-core/src
   ```

8. If smoke tests later fail because they compute the old hash, stop and ask for
   explicit approval before editing `crates/ajax-cli/tests/smoke_user_flows.rs`.

Risk: high. Existing rooted worktree paths may use old hashes. If compatibility
is required, add a separate approved task for legacy path fallback.

## Task 6: Delete the Duplicate Command Execution Kernel

Goal: `commands.rs` must not own a second external command execution loop.

1. Inspect both execution paths.

   ```sh
   rtk sed -n '430,500p' crates/ajax-core/src/commands.rs
   rtk sed -n '1,80p' crates/ajax-core/src/task_operations.rs
   rtk ast-grep -p 'pub fn execute_plan($$$ARGS) -> Result<Vec<CommandOutput>, CommandError> { $$$BODY }' --lang rust crates/ajax-core/src/commands.rs
   rtk ast-grep -p 'for $CMD in &$PLAN.commands { $$$BODY }' --lang rust crates/ajax-core/src
   rtk rg 'execute_plan\\(' crates/ajax-core/src crates/ajax-cli/src crates/ajax-web/src
   ```

2. In `crates/ajax-core/src/architecture.rs`, add a failing static test named
   `commands_module_does_not_own_external_command_execution_loop`.

3. The static test must fail if `crates/ajax-core/src/commands.rs` contains:
   - `pub fn execute_plan`
   - or a direct loop over `plan.commands`
   - unless the remaining function is a one-line delegating compatibility
     wrapper. Prefer no wrapper if callers can be updated cleanly.

4. Run the focused architecture test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-core commands_module_does_not_own_external_command_execution_loop
   ```

5. Implement the smallest refactor:
   - Move all direct callers to the task-operation execution kernel.
   - If public API compatibility requires keeping `commands::execute_plan`,
     make it delegate immediately to the task-operation kernel.
   - Do not redesign `CommandOutput`, `CommandError`, or runner traits.
   - Do not rewrite ship/drop behavior beyond removing duplicate execution
     loops.

6. Run the focused architecture test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-core commands_module_does_not_own_external_command_execution_loop
   ```

7. Run behavior tests around execution.

   ```sh
   rtk cargo nextest run -p ajax-core execute_plan
   rtk cargo nextest run -p ajax-core task_operations
   ```

8. Run the AST check again. It should show no direct command loop in
   `commands.rs`.

   ```sh
   rtk ast-grep -p 'for $CMD in &$PLAN.commands { $$$BODY }' --lang rust crates/ajax-core/src/commands.rs
   ```

Risk: medium.

## Task 7: Move Check/Merge Lifecycle Mutations Behind Typed Helpers

Goal: check and merge commands must not mutate task state through raw registry
access.

1. Inspect current mutations.

   ```sh
   rtk sed -n '1,120p' crates/ajax-core/src/commands/check.rs
   rtk sed -n '1,110p' crates/ajax-core/src/commands/merge.rs
   rtk ast-grep -p '$REGISTRY.get_task_mut($ID)' --lang rust crates/ajax-core/src/commands/check.rs crates/ajax-core/src/commands/merge.rs
   rtk ast-grep -p '$TASK.lifecycle_status = $STATUS' --lang rust crates/ajax-core/src/commands/check.rs crates/ajax-core/src/commands/merge.rs
   ```

2. In `crates/ajax-core/src/architecture.rs`, add a failing static test named
   `check_and_merge_do_not_mutate_tasks_through_raw_registry_access`.

3. The static test must:
   - Read `crates/ajax-core/src/commands/check.rs`.
   - Read `crates/ajax-core/src/commands/merge.rs`.
   - Fail if either file contains `.get_task_mut(`.

4. Run the focused architecture test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-core check_and_merge_do_not_mutate_tasks_through_raw_registry_access
   ```

5. Implement the smallest code change:
   - Add narrow helper functions near the lifecycle/task-operation boundary for
     exactly the check/merge state updates.
   - Replace `.get_task_mut(` in `check.rs` and `merge.rs`.
   - Do not change every `get_task_mut` call in the repo.
   - Do not introduce a generic manager/service/factory.

6. Run the focused architecture test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-core check_and_merge_do_not_mutate_tasks_through_raw_registry_access
   ```

7. Run behavior tests for check and merge.

   ```sh
   rtk cargo nextest run -p ajax-core check
   rtk cargo nextest run -p ajax-core merge
   ```

8. Confirm AST no longer finds raw access in those files.

   ```sh
   rtk ast-grep -p '$REGISTRY.get_task_mut($ID)' --lang rust crates/ajax-core/src/commands/check.rs crates/ajax-core/src/commands/merge.rs
   ```

Risk: medium.

## Task 8: Make Lifecycle Mutation Guard Recursive

Goal: lifecycle assignment guard must scan production submodules, not only
top-level files.

1. Inspect existing guard.

   ```sh
   rtk sed -n '360,430p' crates/ajax-core/src/lifecycle.rs
   rtk ast-grep -p '$TASK.lifecycle_status = $STATUS' --lang rust crates/ajax-core/src
   ```

2. In `crates/ajax-core/src/lifecycle.rs`, add or replace with a failing test
   named `lifecycle_status_assignments_are_not_in_production_submodules`.

3. The test must:
   - Recursively walk `crates/ajax-core/src`.
   - Read `.rs` files.
   - Ignore content inside clearly test-only modules only if the existing helper
     already supports that safely.
   - Fail on production `lifecycle_status =` assignments outside lifecycle
     helpers.

4. Run the focused test and confirm it fails if production bypasses exist.

   ```sh
   rtk cargo nextest run -p ajax-core lifecycle_status_assignments_are_not_in_production_submodules
   ```

5. Implement the smallest fixes:
   - Route exposed production assignments through lifecycle helper functions.
   - Leave test fixture setup alone if it is truly inside `#[cfg(test)]`.
   - Do not create a broad registry abstraction.

6. Run the focused test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-core lifecycle_status_assignments_are_not_in_production_submodules
   ```

7. Run the AST check again and manually compare remaining hits with the allow
   list.

   ```sh
   rtk ast-grep -p '$TASK.lifecycle_status = $STATUS' --lang rust crates/ajax-core/src
   ```

Risk: medium.

## Task 9: Replace Legacy Web Router Tests With Axum Coverage

Goal: remove the parallel non-Axum router and manual path parsing helpers.

1. Inspect old router and callers.

   ```sh
   rtk rg 'pub fn route|pub fn route_with_bridge|split_path_and_query|percent_decode' crates/ajax-web/src/runtime.rs crates/ajax-cli/src/web_backend.rs
   rtk sed -n '920,1160p' crates/ajax-web/src/runtime.rs
   rtk rg 'route_with_bridge|route\\(' crates/ajax-cli/src/web_backend.rs crates/ajax-web/src/runtime.rs
   ```

2. In `crates/ajax-web/src/runtime.rs`, add a failing static test named
   `runtime_exposes_only_axum_router`.

3. The static test must fail if `runtime.rs` contains any of:
   - `pub fn route`
   - `pub fn route_with_bridge`
   - `split_path_and_query`
   - `percent_decode`

4. Run the focused static test and confirm it fails.

   ```sh
   rtk cargo nextest run -p ajax-web runtime_exposes_only_axum_router
   ```

5. Before deleting code, find each old-router test expectation and map it to an
   Axum request test.

   ```sh
   rtk rg 'route_with_bridge|route\\(' crates/ajax-web/src/runtime.rs crates/ajax-cli/src/web_backend.rs
   ```

6. Add or update Axum tests for any behavior that would otherwise lose coverage.
   Keep the tests in source files, not under `tests/`.

7. Delete:
   - `route`
   - `route_with_bridge`
   - `split_path_and_query`
   - `percent_decode`
   - obsolete tests that only exercise the deleted manual router

8. Run the focused static test and confirm it passes.

   ```sh
   rtk cargo nextest run -p ajax-web runtime_exposes_only_axum_router
   ```

9. Run web and CLI backend focused tests.

   ```sh
   rtk cargo nextest run -p ajax-web axum
   rtk cargo nextest run -p ajax-cli web_backend
   ```

10. Confirm text search finds no old router symbols.

    ```sh
    rtk rg 'pub fn route|pub fn route_with_bridge|split_path_and_query|percent_decode' crates/ajax-web/src/runtime.rs crates/ajax-cli/src/web_backend.rs
    ```

Risk: medium.

## Task 10: Harden Web Fixture and API Outcome Tests

Goal: web tests should prove user outcomes, not just fixture plumbing.

1. Inspect existing frontend tests.

   ```sh
   rtk sed -n '1,220p' crates/ajax-web/web/src/lib/api.test.ts
   rtk sed -n '1,220p' crates/ajax-web/web/src/lib/fixtures.test.ts
   rtk rg 'snapshot|toMatch|fixture|requestId|status' crates/ajax-web/web/src
   ```

2. In `crates/ajax-web/web/src/lib/api.test.ts`, add failing tests for
   non-2xx action responses.

3. The API tests must assert:
   - The parsed error preserves the server `requestId`.
   - The parsed error preserves a useful message/detail.
   - The test fails if the client only checks `response.ok` and discards the
     body.

4. In `crates/ajax-web/web/src/lib/fixtures.test.ts`, add a failing test that
   deleted or ghost task records are absent from the cockpit projection fixture.

5. Run frontend tests and confirm the new tests fail.

   ```sh
   rtk npm run web:test -- --run
   ```

6. Implement the smallest frontend/API fixture changes:
   - Tighten API error parsing only where the failing test requires it.
   - Avoid broad snapshots.
   - Assert named fields.

7. Run frontend tests and confirm they pass.

   ```sh
   rtk npm run web:test -- --run
   ```

8. Run type/check validation.

   ```sh
   rtk npm run web:check
   ```

Risk: low.

## Task 11: Add Narrow Agent and Task String Invariants

Goal: permissive string inputs must have explicit invariants.

1. Inspect new-task parsing and identity handling.

   ```sh
   rtk sed -n '1,240p' crates/ajax-core/src/commands/new_task.rs
   rtk sed -n '460,540p' crates/ajax-core/src/commands/new_task.rs
   rtk rg 'AgentClient::Other|slugify|TaskId::new|repo' crates/ajax-core/src/commands/new_task.rs crates/ajax-core/src/models.rs
   ```

2. In `crates/ajax-core/src/commands/new_task.rs`, add failing tests for the
   exact invariants below.

3. Add test `unknown_agent_is_preserved_for_execution_but_classified_other`.
   It must assert:
   - The raw agent command string remains available to the adapter.
   - The classified client is `AgentClient::Other`.

4. Add test `punctuation_only_title_uses_deterministic_fallback_id`.
   It must assert:
   - A title like `"!!!"` does not create an empty id.
   - The fallback is deterministic and documented by the test expectation.

5. Add test `repo_name_cannot_escape_managed_namespace`.
   It must assert:
   - Repo names containing `/`, `..`, or path separators are rejected or safely
     normalized.
   - The resulting task/worktree identity cannot escape the managed namespace.

6. Run the focused tests and confirm failures.

   ```sh
   rtk cargo nextest run -p ajax-core unknown_agent_is_preserved_for_execution_but_classified_other punctuation_only_title_uses_deterministic_fallback_id repo_name_cannot_escape_managed_namespace
   ```

7. Implement the smallest fixes:
   - Preserve intentional `AgentClient::Other` behavior if it is the contract.
   - Add validation only where unsafe inputs are accepted today.
   - Do not introduce a generic parser framework.

8. Run the focused tests and confirm they pass.

   ```sh
   rtk cargo nextest run -p ajax-core unknown_agent_is_preserved_for_execution_but_classified_other punctuation_only_title_uses_deterministic_fallback_id repo_name_cannot_escape_managed_namespace
   ```

9. Run the broader new-task tests.

   ```sh
   rtk cargo nextest run -p ajax-core new_task
   ```

Risk: low to medium.

## Task 12: Update Architecture Documentation Only If Boundaries Changed

Goal: keep `architecture.md` accurate without adding aspirational prose.

1. Decide whether any completed task changed architecture facts:
   - Web mutation concurrency policy changed.
   - Task-operation boundary changed.
   - Legacy router deletion changed adapter shape.
   - Lifecycle mutation boundary changed.

2. If none changed, do not edit `architecture.md`.

3. If one changed, update only the relevant paragraph.

4. Verify the edit by reading the changed section.

   ```sh
   rtk sed -n '1,260p' architecture.md
   ```

Risk: low.

## Task 13: Final Validation

Goal: prove the cleanup did not merely pass focused tests.

1. Run formatting.

   ```sh
   rtk cargo fmt --check
   ```

2. Run Rust build checks.

   ```sh
   rtk cargo check --all-targets --all-features
   ```

3. Run Clippy with warnings denied.

   ```sh
   rtk cargo clippy --all-targets --all-features -- -D warnings
   ```

4. Run the Rust suite.

   ```sh
   rtk cargo nextest run --all-features
   ```

5. Run web checks.

   ```sh
   rtk npm run web:check
   rtk npm run web:test -- --run
   rtk npm run web:build:check
   ```

6. If any command fails:
   - Report the exact command.
   - Summarize the failure.
   - Fix implementation when it is in scope.
   - Do not weaken tests.
   - Do not mark validation as passed.

7. Final report must include:
   - Summary of changes.
   - Tests added or updated.
   - Validation commands run.
   - Failed commands and whether they were fixed.
   - Known risks or deferred items.

## Deferred Until After This Runbook

Do not start these without a new approved plan:

1. Split `crates/ajax-core/src/commands.rs` into vertical capability modules.
2. Reduce public mutable fields on `Task` across the whole codebase.
3. Move all task operations into slices.
4. Rewrite broad command-vector tests into outcome-only tests.
5. Add legacy rooted-worktree hash fallback if Task 5 breaks existing task
   discovery.
