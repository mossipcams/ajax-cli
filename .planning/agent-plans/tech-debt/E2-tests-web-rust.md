# E2 — ajax-web Rust test bulk (crates/ajax-web/src, 42,668 baseline test lines)

Status: complete for the named families. READ-ONLY audit. Baseline (tech-debt-15pct.md):
ajax-web Rust prod 39,279 / tests 42,668. Pure `*_tests.rs` + `tests/` files under
`crates/ajax-web/src`: **41,500 lines** (`git ls-files | grep -E '(_tests\.rs$|/tests/|tests\.rs$)' | xargs cat | wc -l`).
The remaining ~1,200 test lines are scattered inline `#[cfg(test)]` modules in prod files.

Already credited elsewhere (not re-counted here): A1/A2 (frozen TUI/supervisor),
B-01/B-02/B-04, B3-01, C1/C2, D1–D7. Test lines tied to those prod candidates are
counted in their own slices.

**Headline result:** the named families do **not** contain large behavioral
duplicates. Test names were extracted per file and compared; the four
adopt/restart/restore/startup files have zero name collisions and drive four
distinct production entry points, phase1 vs phase2 are disjoint, runtime suites
vs refresh_lane are disjoint (HTTP semantics vs lock ordering), and the ACP
adapter tests cover two genuinely different transports. The bankable test debt is
duplicated micro-harness scaffolding and dead mock surface, ~240 lines total.

---

### E-1 Duplicated micro-harness helpers + dead TestBridge surface (family)
- Paths: `crates/ajax-web/src/slices/web_session/test_support.rs` (canonical, 225 lines, already used by 27 files); local copies in `adapters/web_session_acp/{client_tests,client_restore_tests,spawn_tests,client_spawn_model_tests}.rs`, `slices/web_session/{ws_bridge_tests,session_cleanup_tests,startup_recovery_tests,task_session_adopt_tests,task_session_restore_tests}.rs`, `adapters/web_session_store/{tests.rs,prompt_ledger/tests.rs,prompt_ledger.rs}`, `adapters/{tls.rs,program.rs,browser_session.rs}`, `slices/push.rs`, `runtime/tests/mod.rs`
- Prod lines removed: 0
- Test lines removed: ~240 (estimated from definition sizes; see evidence)
  - `scratch_dir`: 1 local copy per file in 14 files (~8–15 lines each ≈ 150), duplicating `test_support::scratch_dir` (lines 152–164) and the two `adapters` variants (`tls.rs:101`, `program.rs:245`)
  - `fake_acp_fixture`: 6 local copies (~3–4 lines each ≈ 24), duplicating `test_support::fake_acp_fixture` (lines 166–168); e.g. `client_tests.rs:22`, `spawn_tests.rs:10`, `ws_bridge_tests.rs:23`
  - `task_session_restore_tests.rs:30 pump_until_pong_or_turn_end` (~25 lines) is a specialization of the generic `test_support::pump_until` (lines 190–212)
  - `task_session_adopt_tests.rs` local `agent_text_count` (line 211) / `event_text_count` (line 655) ≈ 20 lines, near-duplicates of each other and of `test_support::has_message` (lines 180–188)
  - Dead `TestBridge` mock surface in `runtime/tests/mod.rs`: fields with zero uses in any suite — `drop_task_on_refresh` (:64), `acknowledge_probe` + `AcknowledgeProbe` type (:63,:70), `persist_result` builder half (:62, :178), `clear_registry_on_operate` (:61) ≈ 15–20 lines
- Evidence:
  - `grep -nE 'fn (scratch_dir|fake_acp_fixture|pump_until_pong_or_turn_end|agent_text_count|event_text_count|interruption_note_count|has_message)\(' crates/ajax-web/src` → 23 definitions in 17 files (list above)
  - `grep -l <field> suite_1..4.rs` per TestBridge field → `drop_task_on_refresh: 0`, `acknowledge_probe: 0`, `persist_result: 0`, `clear_registry_on_operate: 0`
  - Shared harness already exists and is adopted: 27 files reference `test_support::scratch_dir|fake_acp_fixture` (grep, e.g. `execution_ownership_tests.rs` 18 hits, `session_activity_directory_tests.rs` 8)
- What replaces it: the existing `test_support.rs` for the web_session slice; a small `adapters/tests_common` (or moving the two adapter variants into one shared fn) for the adapters; delete dead fields.
- Behavior or public contract change: none. Test-only code; identical assertions keep running.
- Risk: low — pure consolidation of test scaffolding; each move is mechanical and compiler-checked.
- Depends on: none
- Confidence: proven (definitions and zero-use sites are quoted); the ~240 figure is a sum of measured definition sizes, so treat ±30 as noise

### E-2 Parameterizing / table-driving: already applied — no bankable remainder
- Paths: sample file `crates/ajax-web/src/slices/web_session/task_session_phase2_tests.rs`
- Prod lines removed: 0; Test lines removed: ~0 remaining
- Evidence: the issue_1324 group is **already table-driven**: one shared body `assert_failed_context_reset_preserves_live_queue(operation: &str)` (lines 506–580, 75 lines) plus three one-call wrapper tests (`issue_1324_harness_switch_ledger_failure_...` :583, `issue_1324_clear_context_...` :588, `issue_1324_warm_acquire_propagates_reset_failure` :593). Sample before/after from this real file: the un-parameterized shape would be 3 × ~65 lines (the full acquire→prompt→force-fail→assert block is repeated verbatim per scenario elsewhere in the same file — cf. `issue_1327_*` at :741 and :788, ~47 lines each with full setup); as written it is 75 + 3×5 ≈ 90 lines. So the pattern is already saving ~100+ lines here, and no further table-driving candidate of ≥150 lines was found in the audited families.
- Behavior change: none. Risk: n/a. Confidence: proven

---

## Overlap measurement for the named families (evidence of NON-duplication)

Test names extracted per file (`grep -E '^(async )?fn [a-z0-9_]+\('`) and compared:

| Family | Files (lines) | Entry points / module driven | Name collisions |
|---|---|---|---|
| adopt cluster | task_session_adopt_tests 990, task_session_restore_tests 530, task_session_restart_tests 315, startup_recovery_tests 313 (Σ 2,148) | live-shim `acquire_typed`+Scratch/SleepChild; restore/load (`g1_*`, issues 1031/1151/1179); restart/respawn; `recover_interrupted_prompts` boot-ownership only | **0** — disjoint names, 4 distinct entry points |
| phase1 vs phase2 | task_session_tests 790 vs task_session_phase2_tests 830 | queue basics (submit/dedupe/cancel/permissions) vs harness-switch/context-reset ledger failures (issues 1086/1189/1324/1327) | **0** |
| reliability cluster | task_session_reliability_tests 849, task_session_durability_tests 237, host_crash_tests 347 | unexpected-exit reconciliation vs ledger durability vs host-process crash — disjoint names | **0** |
| replay | replay_tests 865 (+ restore-side `issue_1031_load_replay_not_in_transcript_after_attach_pump`, `g1_successful_load_drains_replay_from_transcript`) | pure `super::replay` snapshot/cursor logic + TranscriptLog; the two restore tests hit the *same issue* at the integration layer (ACP load path) — two layers of one behavior, both keep | no duplicate |
| ws_bridge | ws_bridge_tests 988 + ws_bridge_tail_tests 226 | `apply_client_message`/set_config_option persistence (issues 942/952/962/979/989) vs tail-snapshot envelope framing only; tail client is a live prod path (`ws_bridge.rs:38,41,155`) | no duplicate |
| runtime suites + lane | tests/mod 612, suite_1 627, suite_2 669, suite_3 944, suite_4 950 (Σ 3,802) vs refresh_lane_tests 878 | s1 auth/TLS/static; s2 refresh-cache/push-tick/idempotency; s3 coordinator/conflicts/diff-review/dev-deploy; s4 task endpoints/swap/stt — vs control-lane lock ordering (ProbeGate, attach race, start external effects) | **0**; suites assert HTTP semantics, lane asserts concurrency |
| ACP adapters | client_tests 845, claude_sdk_client_tests 492 (+ pi_rpc_map 727, claude_sdk_map 830, config_options 694, spawn 168, cursor_login 473) | `AcpStdioClient` (generic ACP stdio) vs `ClaudeSdkClient` (sidecar protocol) — two real transports (`session_client.rs:145-157` constructs all four) | not duplicates |
| operate/tests 999 | web operate slice command construction + merge/fix-ci dispatch | `grep -rl 'start_task_cursor_agent_command\|agent_subcommand_without_cd' crates/ajax-cli/` → **no matches**: no cross-crate overlap with ajax-cli's own tests | none |

Scaffolding measured (lines before first test): adopt_tests 328 (unique Scratch+SleepChild live-shim simulation — no equivalent anywhere else; not removable), refresh_lane_tests 344 (unique ProbeGate/app builder), all others < 78. The shared harness `test_support.rs` already exists and covers the common case — E-1's remainder is only the stragglers above.

---

## Totals by confidence (this slice)

| ID | Prod | Test | Confidence |
|---|---:|---:|---|
| E-1 helper family + dead mock surface | 0 | ~240 (est ±30) | proven (definitions/zero-use quoted) |
| E-2 parameterizing remainder | 0 | ~0 | proven (already applied) |
| **Total bankable** | **0** | **~240** | |

This slice does not approach a large share of the 38,650-line target: the ajax-web
test bulk is issue-pinned regression coverage with distinct behaviors, not a
redundant structure. (Context for the parent: cross-slice prod removals in A/B/D
will carry their own test lines out with them.)

## Not worth it (looked, should stay)

- **`test_support.rs` (225)** — already the shared harness; 27 consumers.
- **adopt_tests Scratch/SleepChild scaffolding (~300)** — the only thing that can simulate a *live* session-shim child process for adoption tests; unique, load-bearing.
- **`runtime/tests/mod.rs` TestBridge (612)** — shared mock used by all four suites; 57 fields cover distinct trait behaviors (only ~4 dead fields, in E-1).
- **issue-pinned regression tests** (issues 942/952/962/979/989/1018/1031/1086/1090/1151/1179/1275/1298/1324/1326/1327/1334/1356/1432/1439) — each name maps to a distinct failure mode; removing any is weakening coverage, which this audit may not propose.
- **`terminal_pty/tests.rs` (846) + `slices/web_session/tests.rs` map_* suite** — Terminal is frozen-but-supported and the ACP→browser event mapper has no other test home.

## Not reached (budget)

Name-level checks only, no per-test overlap measurement: `acp_drain_tests.rs`
(711), `session_activity_directory_tests.rs` (664), `slices/cockpit/tests.rs`
(565), `slices/training/tests.rs` (676), `background_jobs_{,live_}tests`,
`live_harness_tests.rs`/`harness_contract_tests.rs`, `claude_sidecar_tests.rs`
(893), inline test modules in `task_session_directory.rs` (largest inline block)
and `slices/operate/tests.rs` cross-check vs core. If the parent wants more out
of this slice, those are the remaining unmeasured ~12k lines; early signal from
names is that they too are behavior-partitioned.
