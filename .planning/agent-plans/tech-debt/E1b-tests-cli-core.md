# E1b: Rust test-bulk audit (ajax-cli, ajax-core)

Read-only sample audit. Scratch scripts: `.planning/router-runs/tech-debt/e1b_sweep.py`, `e1b_near.py`.
Scope: `crates/ajax-cli` and `crates/ajax-core` tests (inline `#[test]` and `tests/` dirs). `ajax-tui` ignored.
Counting: `git ls-files <path> | xargs wc -l` per file; no summed `find` output.

## Method and what was swept

- Test fns found by regex on `#[test]`/`#[tokio::test]`: 931 tests in 67 files (ajax-cli + ajax-core, excl. ajax-tui).
- Exact-normalized duplicate bodies (identifiers kept, strings/numbers masked): 5 groups.
- Near-duplicate bodies (token SequenceMatcher ratio >= 0.85, both >= 8 lines): 224 pairs. Pairs per file are led by `commands/tests/suite_3.rs` (35), `runtime_refresh/tests/suite_2.rs` (17), `ui_state/tests.rs` (17), `tmux_task_session/tests.rs` (16).
- Duplicate test names across files: 0.
- Duplicate helper fn names: `run` (30 files define one, mostly fake `CommandRunner`s), `new`, `run_task_session`, `drop`, `sample_task`, `temp_path`, `new_run`, `store`, `prompt`.
- Fake `CommandRunner` impls in cli+core: 50 impls, 865 lines (`rg -c 'impl CommandRunner for'`).
- Source-text tests (`read_to_string`/`include_str!` of src or Cargo/ci files): listed in the candidates below and in "Not worth it".
- `suite_N` files: 36 files, 19,388 lines (`git ls-files ... | grep -E 'suite_[0-9]+\.rs$' | xargs wc -l`). Splitting is a file split, so no line counts are credited to it.

Overlap with already-reported work (not re-counted here):
- `A-frozen-surfaces.md` A1 already credits `crates/ajax-cli/src/tmux_task_session/tests.rs` (632), `cockpit_backend/` tests dir (1,248), and the cockpit-keyword tests in `lib/tests/suite_*.rs` (about 2,348 lines in suites 1/2/3/4/9-13). Cockpit and pending-cockpit tests such as `pending_cockpit_open_action_runs_task_without_lifecycle_change` (suite_13) and `cockpit_merge_task_action_stays_inside_ajax` (suite_10) are inside A1.
- `B2-web-backend.md` B-04 credits process-kill and dispatch-prompt tests. `D-core.md` D1-D7 credits `registry/sqlite/tests/suite_2/3` ladder fixtures and `agent_prompt`/`validity`/`runtime` tests. `B2` also credits legacy-import tests in `web_session_store`.

---

### E1b-01 Parameterize the three cursor permission-attention tests in agent_event
- Paths: `crates/ajax-cli/src/agent_event/tests.rs` lines 192-232 (`cursor_before_shell_execution_requests_permission_attention`, `cursor_before_mcp_execution_requests_permission_attention`, `cursor_notification_permission_prompt_requests_permission_attention`)
- Prod lines removed: 0
- Test lines removed: about 26 (3 tests of 12-14 lines each, about 40 lines, collapsed to one table test of about 14 lines). Count = lines in the three fns (`sed -n 192,232p`) minus the replacement, counted by hand.
- Evidence: `grep -n 'fn cursor_' crates/ajax-cli/src/agent_event/tests.rs` shows the three fns share an identical body shape: `translate_native_event("cursor", <event>, &payload)` then `assert_eq!(kind, AttentionRequested)` and `assert_eq!(detail, Attention{Permission})`. Sweep: these bodies normalize to the same string (ratio 0.9+). The same family appears across `agent_event/tests.rs` (sweep: 11 near-dup pairs in this file).
- What replaces it: one table-driven test `cursor_permission_events_request_permission_attention` looping over `[("beforeShellExecution", ...), ("beforeMCPExecution", ...), ("Notification:permission_prompt", ...)]`. Each row keeps the same assertions, so each event is still checked. Failure output names the event when an assert message includes it.
- Behavior or public contract change: none. Same events and assertions.
- Risk: low. Pure test structure, no production code.
- Depends on: none.
- Confidence: proven that the bodies are the same shape (sample read in full); the line saving is small on its own, so it only counts as part of a family.

Sample before/after (real file, lines 193-204 shown; the other two tests are the same shape):

Before (3 fns, about 40 lines):
```rust
#[test]
fn cursor_before_shell_execution_requests_permission_attention() {
    let payload = serde_json::json!({});
    let canonical = translate_native_event("cursor", "beforeShellExecution", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(canonical.detail, Some(CanonicalEventDetail::Attention { attention: AttentionReason::Permission }));
}
// + 2 more with identical bodies
```
After (about 14 lines):
```rust
#[test]
fn cursor_permission_events_request_permission_attention() {
    for event in ["beforeShellExecution", "beforeMCPExecution", "Notification:permission_prompt"] {
        let canonical = translate_native_event("cursor", event, &serde_json::json!({})).unwrap();
        assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested, "{event}");
        assert_eq!(canonical.detail, Some(CanonicalEventDetail::Attention { attention: AttentionReason::Permission }), "{event}");
    }
}
```

---

### E1b-02 Duplicate fake `CommandRunner` helpers across cli and core tests (family)
- Paths: 50 `impl CommandRunner for ...` blocks in tests across `crates/ajax-cli/src/{web_backend,cockpit_backend,lib,tmux_task_session,web_service}/tests*`, `crates/ajax-core/src/{runtime_refresh,task_operations,commands,diff_review}/tests*`.
- Prod lines removed: 0
- Test lines removed: not yet proven. Upper bound is the 865 lines in the 50 impls. Realistic removal is one shared `RecordingRunner` / `ScriptedRunner` in a `test_support` module plus per-file configuration. Estimate 150-250 lines, but this is speculative until the impls are diffed.
- Evidence: `git ls-files crates/ajax-cli crates/ajax-core | xargs grep -c 'impl CommandRunner for'` gives the 50-impl list. Same-name, same-size pair confirmed by the measurement script: `LiveRefreshRunner` is 35 lines in both `crates/ajax-cli/src/web_backend/tests/suite_2.rs` and `crates/ajax-cli/src/cockpit_backend/tests.rs`. Other fakes repeat the name `run` in 30 files. Each fake has its own `run` match arm.
- What replaces it: a shared scripted runner (command-prefix -> output table). Behavior is asserted on the command log and output, not on which struct ran it, so every behavior assertion stays.
- Behavior or public contract change: none (test-only).
- Risk: medium. Some fakes model failure sequences that are part of the test's point (e.g. `OrphanRecoveryRunner`, `AbsentDropRunner`). Merging them can hide the scenario. Needs per-fake review.
- Depends on: E1b-03 and A1 (cockpit fakes in `cockpit_backend/tests.rs` are credited there).
- Confidence: likely for the duplicate pair; speculative for the total. Not diffed yet.

---

### E1b-03 Near-duplicate scenario tests within one file (table-driven) in ajax-core
- Paths:
  - `crates/ajax-core/src/commands/tests/suite_3.rs`: `mark_task_opened_preserves_claude_evidence_and_lifecycle` vs `mark_task_opened_preserves_codex_evidence_and_lifecycle` (sweep ratio 0.97, 29 lines)
  - `crates/ajax-core/src/commands/tests/suite_3.rs`: `open_task_plan_allows_old_healthy_complete_runtime_projection` vs `open_task_plan_allows_stale_runtime_when_live_status_requests_resume` (0.96, 42 lines)
  - `crates/ajax-core/src/commands/tests/suite_6.rs`: `task_window_repair_plan_repairs_task_when_tmux_session_exists` vs `task_window_repair_plan_creates_missing_task_when_tmux_session_exists` (0.95, 40 lines)
  - `crates/ajax-core/src/runtime_refresh/tests/suite_2.rs` vs `suite_3.rs` (0.96, 75 lines), not a clean parameterization; scenario differs in state setup.
- Prod lines removed: 0
- Test lines removed: about 30-60 across the pairs above, not yet counted precisely.
- Evidence: sweep output above, plus `rg -n 'fn mark_task_opened_preserves' crates/ajax-core/src/commands/tests/suite_3.rs`. The Claude/Codex pair differs only in the harness string and evidence fixture.
- What replaces it: one parameterized fn per pair (loop over harness / state table). Assertions unchanged.
- Behavior or public contract change: none.
- Risk: low for the harness pair; medium for the runtime_refresh pair (setup differs).
- Depends on: none.
- Confidence: proven that the bodies are near-identical (sweep ratio); likely for the line count. Parameterization category.

---

### E1b-04 Test-only duplicate context save scenarios (ajax-cli context) — NOT a candidate
Recorded so it is not re-proposed: `crates/ajax-cli/src/context/tests.rs` has four `save_context_*` tests with 0.89-0.96 similarity (`save_context_accepts_concurrent_task_deletion_without_conflict` vs `save_context_keeps_memory_deletion_when_disk_revision_moved_issue_1398`, 51-52 lines each). Each is a different race (issue-numbered). Collapsing them drops the regression intent. Keep. See "Not worth it".

---

## Not worth it

- `crates/ajax-core/src/architecture.rs` (261 lines, all source-scan guard tests reading `src/*.rs`) and `crates/ajax-core/src/lifecycle.rs` guard tests. These enforce module boundaries. Removing them is coverage loss, and the repo memory says these guards are intentional. Keep.
- Source-reading tests in `crates/ajax-cli/src/lib/tests/suite_1.rs` (reads manifest/workflow) and `suite_5.rs` (reads workspace `Cargo.toml`). They are guard tests tied to release/CI configuration, which memory says to keep.
- `crates/ajax-cli/tests/smoke_user_flows/suite_1.rs` (717 lines, 13 tests) and `main.rs` (787). Binary-level smoke flows. Non-cockpit flows (`smoke_new_plan_has_no_side_effects`, `smoke_merge_and_clean_completed_task`, `smoke_state_export_writes_json_and_refuses_overwrite`, `smoke_multi_repo_attention_routing`) overlap in intent with `lib/tests` suites, but the binary/PTY path is the only end-to-end proof. Cockpit flows are A1's.
- `crates/ajax-cli/tests/live_cli.rs` (875) and `crates/ajax-core/tests/wave0_run_state_characterization.rs` (289). Characterization tests; they pin the CLI contract. Not reviewed in detail (time).
- `suite_N` file splits in general (36 files, 19,388 lines). Moving tests between files does not reduce lines.
- Mock-heavy task-operation tests (`task_operations/tests/suite_1..4`, 40+ fake references). They test behavior through fakes at the adapter boundary; the fakes are the seam, not a mock-only test.
- `commands/new_task/tests.rs` (972 lines, 34 tests). Largest single file in the sample, but the only near-dup pairs are small (`start_task_identity_uses_core_slug_rules` vs `punctuation_only_title_uses_deterministic_fallback_id`, 14-line bodies). Not a family worth reporting.

## Not reached

- Oversized per-file builders (`fn sample_task` in 4 files, `temp_path` in 4 sqlite test files, `run_task_session` in 2): not diffed, so no line counts.
- `ui_state/tests.rs` (836) and `attention/tests.rs` (874): largest near-dup concentration in sweep, not read.
- Full `CommandRunner` fake diff (E1b-02): not done.
- `wave0_run_state_characterization.rs` vs registry tests overlap: not checked.

## Totals by confidence

| ID | Category | Prod removed | Test removed | Confidence |
|---|---|---:|---:|---|
| E1b-01 | parameterize (cursor perm tests) | 0 | ~26 | proven shape, small on its own |
| E1b-02 | shared fake CommandRunner | 0 | 150-250 est. | speculative (not diffed) |
| E1b-03 | parameterize (ajax-core pairs) | 0 | ~30-60 est. | likely |
| Subtotal (credited, not double-counted) | | 0 | ~200-330 est. | |

A1 (cockpit/tmux test surface) and B2/D-core test tails are already credited elsewhere and are excluded from the subtotal.

## Verification note

Read-only audit. No file other than this report was edited. Scratch scripts live in `.planning/router-runs/tech-debt/`. No cargo commands, no git writes, no `/tmp` use.
