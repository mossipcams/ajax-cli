# ACP review corrections

## Status

- Approval: approved (`delegate until finished`)
- Delegation decision: delegated via model-router to Cursor (`grok-4.5-high`) because the user selected Cursor and these are bounded, multi-file correctness fixes.
- Execution: complete

## Scope

Correct the four validated ACP review findings:

1. Make `--state-root` consistently mean the actual `agent-acp` snapshot directory.
2. Serialize each task's generation check and snapshot replacement so an old publisher cannot overwrite a newer claim.
3. Keep ACP prompt failure details free of peer/SDK-provided text.
4. Remove the dead `agent_process_alive_at` fallback; ACP snapshots are the only agent-status evidence.

## Non-goals

- No legacy status compatibility or migration shim.
- No public ACP schema expansion.
- No dependency additions.
- No edits under `tests/`.
- No unrelated refactor, formatting sweep, commit, push, or branch change.

## Tasks

- [x] **Task 1 — Repair the snapshot-directory contract (5–15 min)**
  - Test: add a focused inline snapshot test proving a publisher given `<state-root>` writes `<state-root>/<task>.json`, then run it and record the expected current failure caused by the extra `agent-acp` nesting.
  - Implement: make snapshot write/read helpers accept the actual state directory and update the Cockpit/Web readers to pass `cache_dir/agent-acp`; preserve the existing launch command and `--state-root` value.
  - Verify: rerun the focused snapshot test plus focused Cockpit/Web status-refresh tests.
  - Result: Cursor observed RED (`expected_path.is_file()` failed), then GREEN. Parent accepted after one scope-correction resume.
  - Validation: focused root test (1 passed), snapshot module (5 passed), Cockpit refresh (1 passed), Web API refresh (1 passed), and `cargo fmt --check` passed.
  - Deviation: `crates/ajax-cli/src/lib/tests.rs::write_acp_snapshot` was added to the packet because its crate-local fixture also receives the cache parent; no file under a `tests/` directory changed.

- [x] **Task 2 — Make generation ownership atomic (10–15 min)**
  - Test: add a channel-coordinated inline concurrency test showing both replacement claims and stale publishes participate in the same per-task lock; confirm the unlocked implementation fails the test.
  - Implement: reuse the existing `nix` dependency for one advisory sidecar lock per task and hold it across generation verification and atomic rename in both `claim` and `publish`.
  - Verify: rerun the concurrency test and the existing generation-mismatch/heartbeat snapshot tests.
  - Result: Cursor observed RED because an unlocked replacement claim returned while the test held the expected sidecar lock, then GREEN after locking both claim and publish.
  - Validation: focused concurrency test (1 passed), generation mismatch (1 passed), snapshot module (6 passed), `cargo fmt --check`, and focused all-target/all-feature clippy passed.
  - Review: parent confirmed the lock spans `ensure_current_generation` through `write_snapshot`; one resume removed unauthorized planning-file edits.

- [x] **Task 3 — Sanitize prompt-failure status detail (5–10 min)**
  - Test: add a focused inline console-event test whose injected prompt error contains a marker and assert the published operator detail is fixed text without that marker; run it and record the expected leak failure.
  - Implement: publish the fixed message `ACP prompt failed.` while leaving existing diagnostics behavior unchanged.
  - Verify: rerun the focused console-event/ACP tests.
  - Result: Cursor observed RED with the peer marker embedded in `ACP prompt failed: …`, then GREEN after routing the callback through fixed safe detail.
  - Validation: focused safety test (1 passed), `agent_acp::tests` (20 passed), and `cargo fmt --check` passed.
  - Review: parent confirmed the peer error is discarded before the operator channel; one resume removed unauthorized planning-file edits.

- [x] **Task 4 — Delete the obsolete process-liveness tier (5–10 min)**
  - Test: add an inline projection test inserting literal legacy `agent_process_alive_at` metadata and expecting `Unknown`; run it and record the current `Idle` failure.
  - Implement: delete the unused metadata constant/helper and its projection branch so missing or stale ACP evidence resolves to `Unknown`.
  - Verify: rerun focused `ui_state` projection tests.
  - Result: Cursor observed RED (`Idle` instead of `Unknown`), then GREEN after deleting the legacy constant/helper and metadata clause.
  - Validation: focused regression (1 passed), `ui_state::tests` (48 passed), and `cargo fmt --check` passed.
  - Review: repository search leaves the literal metadata key only in the regression test; one resume removed an unauthorized duplicate plan file.

- [x] **Task 5 — Parent review and validation (10–15 min)**
  - Test: no new test; review every Cursor diff against its packet and reject out-of-scope edits.
  - Implement: only focused `resume` corrections if review finds a defect.
  - Verify:
    - `rtk cargo fmt --check`
    - `rtk cargo check --all-targets --all-features`
    - `rtk cargo clippy --all-targets --all-features -- -D warnings`
    - `rtk cargo nextest run --all-features`
    - `rtk npm run verify`
  - Result: all commands exited 0. Nextest passed 1,671 tests; Web Vitest passed 485 tests; CI/release invariant checks passed.
  - Audit: `git diff --check` passed; unsafe prompt formatting and legacy liveness symbols are absent; the legacy metadata literal remains only in its regression test.
  - Cleanup: removed eight untracked `acp-review-task-*-r*` router-run directories created outside the Cursor packets.

## Execution protocol

For each implementation task:

1. Create a complete TDD implementation packet with exact files, anchors, forbidden changes, and commands.
2. Delegate only that task to Cursor; Cursor must write and run the failing test before production changes.
3. Review the diff and personally rerun the focused validation.
4. Record RED/GREEN evidence and any deviation here before advancing.

## Risks

- Advisory locking must cover both the generation read and final rename; locking either operation alone leaves the race.
- Snapshot-directory semantics have several callers, so parent review must confirm writer and both readers use the same root.
- Existing unrelated worktree changes must remain untouched.

## Results

- Task 1 complete. First Cursor invocation lost its connection before editing; the retry completed. One resume corrected its planning-file scope violation and explicitly narrowed the required `src/lib/tests.rs` fixture edit.
- Tasks 2–4 completed by separate Cursor chats with parent diff review and focused reruns. Each chat required one scope-cleanup resume for an unauthorized planning artifact; no such artifact remains.
- Full validation passed. `npm run verify` emitted non-fatal jsdom `HTMLCanvasElement.getContext` warnings and exited 0.
