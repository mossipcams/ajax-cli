# D. ajax-core tech-debt audit (non-frozen)

Read-only audit of `crates/ajax-core` (baseline 30,694 prod / 21,879 test lines).
Scope excludes tmux/TUI-only code per the slice brief. Method: scripted pub-item
sweep (`rg -lw` word-boundary reference counts across all repo `.rs` files) plus
manual confirmation of every item below. ajax-core tests are inline (lifecycle
guard), so test line counts are `#[cfg(test)]` sections inside the named files.

Ranked by lines removed.

### D1 agent_prompt.rs — dead TUI prompt parser (whole file)
- Paths: `crates/ajax-core/src/agent_prompt.rs`, plus one `mod` line in `lib.rs`
- Prod lines removed: 352 (lines 1–352, before `#[cfg(test)]`)
- Test lines removed: 134 (inline tests, lines 353–486)
- Evidence:
  - `rg -n "parse_prompt|answer_keys|AgentPrompt|SendKeys" crates -g '*.rs' | grep -v 'agent_prompt.rs'` → **zero results** (exit 1).
  - Every pub item (`AgentPrompt`, `AnswerError`, `ChoiceRole`, `OperatorAnswer`,
    `PromptChoice`, `PromptKind`, `SendKeys`, `Confidence`, `answer_keys`,
    `parse_prompt`) shows exactly 1 referencing file in the sweep = its own file.
- What replaces it: nothing — this parsed Codex numbered/yes-no approval prompts
  out of a tmux pane and mapped operator answers to keystrokes (`SendKeys`). No
  caller exists anywhere (not even ajax-tui). It predates the chat remediation /
  operator-answer flow (architecture.md Phase 4: "Chat verbs are remediation IDs,
  not `OperatorActions`").
- Behavior or public contract change: none proven. Note it lives in frozen TUI
  territory, but it is unreachable, so removal breaks no frozen behavior.
- Risk: low — zero references; only the `mod agent_prompt;` line in lib.rs must go.
- Depends on: none
- Confidence: proven

### D2 validity.rs — dead task-validity module (whole file)
- Paths: `crates/ajax-core/src/validity.rs`, plus one `mod` line in `lib.rs`
- Prod lines removed: 58 (lines 1–58)
- Test lines removed: 95 (inline tests, lines 59–153)
- Evidence:
  - Per-item sweep: `TaskValidity ext:0`, `TaskValidityIssue ext:0`,
    `is_valid ext:0`, `task_validity ext:0` (reference counts computed with
    `rg -lw <name> -g '*.rs'` over the whole repo, excluding only the module's own dir).
  - `rg -l "crate::validity|ajax_core::validity" crates --type rust` → zero files.
- What replaces it: task invalidity is already represented by lifecycle status +
  annotations (`recommended::task_is_known_invalid`, attention annotations); this
  module is a parallel validity layer nothing consumes.
- Behavior or public contract change: none.
- Risk: low — whole self-contained module, zero external references.
- Depends on: none
- Confidence: proven

### D3 runtime.rs — dead RuntimeEvidence<T> abstraction layer
- Paths: `crates/ajax-core/src/runtime.rs` (481 lines)
- Prod lines removed: ~165 (lines 10–165: `ObservationConfidence`,
  `RuntimeEvidenceSource`, `ResourcePresence`, `RuntimeEvidence<T>` + impl,
  `RuntimeEvidenceStatus`, `ReducedRuntimeEvidence`, `reduce_resource_evidence`;
  plus dead `reconcile_runtime` fn, lines 172–180)
- Test lines removed: ~185 (inline tests from line 241 on; the test functions
  `runtime_reconciliation_distinguishes_checkout_mismatch_from_missing_or_unobserved`,
  `runtime_reconciliation_collapses_substrate_evidence_into_one_health_verdict`, and
  `runtime_evidence_keeps_missing_probe_failed_not_observed_and_stale_distinct` plus
  their setup helpers exist solely for the dead API)
- Evidence:
  - Sweep (refs outside module dir): `RuntimeEvidence ext:0`, `RuntimeEvidenceSource
    ext:0`, `ReducedRuntimeEvidence ext:0`, `ResourcePresence ext:0`,
    `ObservationConfidence ext:0`, `not_observed ext:0`, `probe_failed ext:0`,
    `reduce_resource_evidence ext:0`, `reconcile_runtime ext:0`.
  - Live in the same file (keep): `RUNTIME_PROJECTION_FRESH_FOR` (1 external ref),
    `ObservedTaskRuntime`, `reconcile_runtime_for_task` (1 external ref, used by
    runtime_refresh/commands).
- What replaces it: the newer `runtime_refresh` evidence pipeline consumes raw
  probes directly (`ObservedTaskRuntime`); the generic `RuntimeEvidence<T>`
  wrapper is a superseded abstraction.
- Behavior or public contract change: none (nothing outside the file uses it).
- Risk: low-medium — file is shared with live items, so removal is surgical; must
  not touch `reconcile_runtime_for_task`.
- Depends on: none
- Confidence: proven for dead items; line totals approximate (item-boundary counted)

### D4 ghost_task.rs — dead persistence-disposition API
- Paths: `crates/ajax-core/src/ghost_task.rs` (319 lines)
- Prod lines removed: ~44 (lines 4–47: `RegistryGhostReason`,
  `RegistryPersistenceDisposition`, `registry_persistence_disposition`)
- Test lines removed: to be counted in the inline test module for those items only
  (tests from line 95 on; only functions exercising
  `registry_persistence_disposition`/the two enums qualify — estimated ~80–120).
- Evidence: sweep shows `RegistryGhostReason ext:0`,
  `RegistryPersistenceDisposition ext:0`, `registry_persistence_disposition ext:0`;
  while `is_registry_ghost_task ext:4` and `is_cockpit_visible_task ext:1` are live.
- What replaces it: callers use the boolean ghost/visibility predicates directly.
- Behavior or public contract change: none. Touches registry truth adjacent code —
  **requires approval per AGENTS.md stop conditions** even though the removed part is unreachable.
- Risk: medium — file is load-bearing for live ghost detection; surgical removal only.
- Depends on: none
- Confidence: proven

### D5 output.rs — dead registry export snapshot path (minor)
- Paths: `crates/ajax-core/src/output.rs` (477 lines)
- Prod lines removed: ~20 (lines 175–180 `RegistryExportSnapshot`, 196–202
  `registry_export_snapshot`, 227–232ish `registry_export_json_snapshot`)
- Test lines removed: ~35 (`registry_export_snapshot_serializes_state_as_json_contract`
  and its setup)
- Evidence: sweep `RegistryExportSnapshot ext:0`, `registry_export_snapshot ext:0`,
  `registry_export_json_snapshot ext:0`. The *state* export family is live via
  `crates/ajax-cli/src/snapshot_dispatch.rs:269` (`state_export_json_snapshot`), so
  only the registry half of the dual snapshot API dies.
- What replaces it: the state export path subsumes it (it snapshots the same registry).
- Behavior or public contract change: none proven — no CLI command references the
  registry-only exporter. **Flag: verify** `ajax snapshot`/`export` CLI verbs don't
  route through `registry_export_json_snapshot` before removal.
- Risk: low. Below the 150-line floor on its own; included because it is a
  duplicate-in-pair (two snapshot APIs, one dead) and totals ~55 with tests.
- Depends on: none
- Confidence: proven for zero references; contract check outstanding

### D6 config.rs + agent_capability.rs + pane_fallback.rs — small dead items in frozen-support files
- Paths: `crates/ajax-core/src/config.rs` (703), `agent_capability.rs` (179),
  `pane_fallback.rs` (317)
- Prod lines removed: ~95 total (`ConfigParseError`, `ConfigPaths`, `for_home` in
  config.rs; `CapabilitySupport`, `AgentCapabilityProfile` fields/impl parts,
  `profile_for_hook_client`, `support_for`, `supports_native` in agent_capability.rs;
  `PaneWaitHint` + `recognize_wait_hint` lines 10–26 in pane_fallback.rs)
- Test lines removed: ~30 (inline tests for the dead items only)
- Evidence: per-item sweep shows `ext:0` for each named item. Live neighbors kept:
  `Config` (51 refs), `WorktreePlacement` (8), `CapabilityFact` +
  `profile_for_agent_client` + `allows_pane_fallback` (used only by pane_fallback,
  the frozen TUI path).
- What replaces it: nothing; these are half-removed abstraction shells.
- Behavior or public contract change: none. **Note:** `SttConfig` was investigated
  and is LIVE — `crates/ajax-web/src/runtime/state.rs:68` reads
  `config.stt.provider_command` (speech-input feature, documented in
  docs/speech-input.md). Do not remove.
- Risk: low; pane_fallback/agent_capability sit under the frozen TUI surface, but
  only unreachable items are proposed.
- Depends on: none
- Confidence: proven for ext:0 items; line totals approximate

### D7 registry/sqlite/migrations.rs — ancient-schema migration ladder (v2→v7)
- Paths: `crates/ajax-core/src/registry/sqlite/migrations.rs` (566 lines)
- Prod lines removed: ~240 if collapsed (`migrate_v2_to_v3` 461–496, `migrate_v3_to_v4`
  439–460, `migrate_v4_to_v5` 426–438, `migrate_v5_to_v6` 391–~425,
  `migrate_v6_to_v7` 269–295, `migrate_v7_to_current_schema` 297–389)
- Test lines removed: ~100–200 (inline `mod tests` from line 526 on plus old-schema
  fixtures in `registry/sqlite/tests/suite_2.rs`, `suite_3.rs` that set
  `user_version`; exact split not measured — say so plainly: **not precisely counted**)
- Evidence:
  - Ladder is reachable only from databases at schema versions 2–7 (migrations.rs
    lines 15–36); fresh installs and v8+/v9/v10 DBs never touch v2–v7 steps.
  - `rg -ln "user_version" crates/ajax-core/src/registry/sqlite/tests/` → suite_2,
    suite_3, execution_store, mod (the tests that pin the ladder).
  - I could NOT prove no v<10 database exists in any operator's field (no registry
    of deployed DB versions exists in-repo). That is the entire risk question.
- What replaces it: a single "user_version < 8 → create_current_schema + additive
  column ensures" path, matching what `migrate_v7_to_current_schema` already does
  for v7. For v2–v6 this changes data preservation semantics (the ladder backfills
  columns like `live_status_observed_at_*`, `attention_acknowledged_at_*`).
- Behavior or public contract change: **YES for ancient databases** — v2–v6 DBs would
  lose the column-backfill guarantees. This is registry-truth territory and an
  AGENTS.md stop condition.
- Risk: high — data loss on old databases; requires explicit approval with a
  decision on minimum supported schema version (e.g. declare v8+ only, update docs).
- Depends on: none
- Confidence: speculative (dead-in-practice for any recent install, unproven in field)

### Family total (D1–D7)
| ID | Prod | Test | Confidence |
|----|-----:|-----:|-----------|
| D1 agent_prompt.rs | 352 | 134 | proven |
| D2 validity.rs | 58 | 95 | proven |
| D3 runtime evidence layer | ~165 | ~185 | proven (approx counts) |
| D4 ghost_task disposition API | ~44 | ~100 (est.) | proven, approval required |
| D5 output.rs registry export path | ~20 | ~35 | proven, contract check outstanding |
| D6 small dead items (config/capability/pane) | ~95 | ~30 | proven (approx counts) |
| D7 ancient migration ladder | ~240 (opt.) | ~150 (est., not measured) | speculative, approval + high risk |
| **Total (proven)** | **~734** | **~579** | ≈ 1,313 lines (~4.3% of ajax-core) |
| **Total (incl. D7 if approved)** | **~974** | **~729** | ≈ 1,703 lines (~5.6% of ajax-core) |

## Not worth it (examined, keep)
- `recommended.rs` (811): `OperatorActionPlan`/`operator_action()` flagged by the
  sweep, but `operator_action(task)` is live via `commands/projection.rs:92`,
  shuttle_tests, lifecycle_state_machine_tests, and web. Keep.
- `config.rs::SttConfig`: consumed by ajax-web speech-input (`state.rs:68`). Keep.
- `output.rs` state export family + Cockpit/TaskCard view types: live via CLI
  render/snapshot paths and TUI. Keep.
- `ghost_task.rs` ghost predicates, `pane_fallback.rs` wait-hint reconciliation,
  `runtime.rs::ObservedTaskRuntime`: live in runtime_refresh/commands/web. Keep.
- `commands/` vs `task_operations/`: examined — not a duplicate pair.
  `task_operations/*` is the dispatched-operation layer (used by
  `ajax-web/src/slices/operate/mod.rs` and `ajax-cli/src/dispatch.rs`, e.g.
  `plan_drop_task_operation` at dispatch.rs:145); `commands/*` is the CLI/read-
  projection layer. The sweep found no dead items in either, and no operation is
  implemented twice under different names that I could prove. Keep both; a finer
  per-operation mapping was not attempted (budget).
- `migrations.rs` current-schema DDL (`create_current_schema`, 75–268): live on
  every fresh install. Keep (the v7→current copy is the only near-duplicate, see D7).

## Not reached (budget)
- ui_state.rs / attention.rs internal redundancy (attention has an 874-line test
  file; module itself not item-swept beyond top-level names).
- Precise test-line split for D3/D4/D6 (approximated from item boundaries).
- Per-operation duplication mapping inside commands/ vs task_operations/.
