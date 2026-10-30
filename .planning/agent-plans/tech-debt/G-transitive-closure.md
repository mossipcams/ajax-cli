# G — Transitive closure after A1/A2 removal (read-only audit)

Started 2026-10-30 07:43 CDT. Hard limit 15 min. Only this file is written.
Counting: tracked files only, `git ls-files <path> | xargs cat | wc -l`.
Reference set: production code only (excludes `#[cfg(test)]` blocks and `*_tests.rs`/`tests.rs`).
Treated as deleted: A1 (crates/ajax-tui, ajax-cli cockpit_backend/cockpit_actions/cockpit_snapshot,
tmux_task_session, dispatch arms) and A2 (crates/ajax-supervisor, ajax-cli supervise.rs).

Status: complete (partial sweep; see Limits).

## Candidates (ranked by lines removed)


### G-00 ajax-core module-level sweep after A1/A2 removal: no new whole-module deletions
- Paths: every top-level `pub mod` in `crates/ajax-core/src/lib.rs` (33 modules).
- Prod lines removed: **0 new** beyond D1/D2/D4/D6 (already counted by Slice D).
- Test lines removed: 0 new.
- Evidence: for each module `m`, `git grep -lwE "$m" -- 'crates/*.rs'` minus A-paths (`crates/ajax-tui`, `crates/ajax-supervisor`, `cockpit_*`, `tmux_task_session`, `supervise.rs`) and test files. Results: every module has at least one production referrer outside A **and** outside `lib.rs`, with the exception of the Slice D set (`agent_prompt`, `validity`, `ghost_task`, `pane_fallback`/`agent_capability`). Examples: `task_operations` referenced by `ajax-web/src/slices/operate/mod.rs`; `ui_state` by `ajax-web/src/slices/cockpit/mod.rs`, `slices/actions.rs`, `slices/push.rs`, `web_session/session_activity.rs`; `remediation` by `ajax-web/src/runtime/task_routes/live.rs`, `slices/ci_agent_delivery.rs`, `slices/operate/mod.rs`; `live` by `ajax-web/src/adapters/web_session_acp/*`; `execution` by `ajax-web/src/slices/web_session/execution_ownership.rs`; `runtime_refresh` by `ajax-web/src/runtime/bridge.rs`, `runtime/mod.rs`.
- Method note: the sweep counts a file as a referrer whether it uses the module for one line or many. So "referenced" means "not provably dead at file granularity", which is a weaker claim than "used". It can only prove a module is kept, not that it is needed in full.
- What replaces it: nothing. A1/A2 removal does not orphan a whole ajax-core module that Slice D did not already report.
- Behavior or public contract change: none.
- Risk: low.
- Depends on: A1, A2, D1, D2, D4, D6.
- Confidence: proven (for the negative: no module is orphaned by A1/A2 alone).

### G-01 ajax-core `pane_fallback.rs` + `agent_capability.rs`: disputed with D6
- Paths: `crates/ajax-core/src/pane_fallback.rs` (317), `crates/ajax-core/src/agent_capability.rs` (179).
- Prod lines removed: 0 (not claimed here).
- Evidence: `git grep -nw pane_fallback` shows production call sites at `crates/ajax-core/src/runtime_refresh.rs:475`, `:483`, `:485` (`crate::pane_fallback::profile_allows_any_pane_wait_fallback`, `maybe_pane_wait`, `reconcile_wait_from_pane`). `runtime_refresh` is reached from ajax-web (`runtime/bridge.rs`, `runtime/mod.rs`). D6 states these are dead; this sweep does not confirm that.
- Why this is not a removal candidate here: the call sites are in production code outside A1/A2, so A1/A2 removal does not make them unreachable. Whether they are dead is a D6 question, and it must be settled by checking whether the `profile_allows_any_pane_wait_fallback` branch can ever be true for a Chat-first agent profile. Not measured.
- Behavior or public contract change: none claimed.
- Risk: medium: it sits in the tmux-wait path that runtime_refresh uses for Codex tmux-only tasks.
- Depends on: D6.
- Confidence: speculative.

### G-02 TaskKind::TerminalAgent family: terminal-only branches (answer to second question)
- Paths: the TerminalAgent path is the default for any task without `metadata["skip_interactive_agent"] == "1"`: `crates/ajax-core/src/models/task.rs:148-172` (`skip_interactive_agent`, `kind`, `set_skip_interactive_agent`), creation sites `crates/ajax-core/src/commands/new_task.rs:176,236-237`, `crates/ajax-web/src/slices/operate/mod.rs:218` (`skip_interactive_agent: request.orchestration_chat`), and the frozen-path hardcoded `false` at `crates/ajax-cli/src/cockpit_actions.rs:242,379`, `execution_dispatch.rs:567`, `lib.rs:516` (A1 paths, not double counted).
- Terminal-only check sites (where a `!skip_interactive_agent()` arm is the tmux path):
  - ajax-core: `models/task.rs:209,350,370,451`; `recommended.rs:261`; `runtime_refresh.rs:159,172,257,397,411`; `ui_state.rs:220,330`; `task_operations/start.rs:104`; `commands/open.rs:24`; `commands/set_session_model.rs:20`; `commands/swap_agent.rs:24`.
  - ajax-web: `runtime/headless.rs:27`; `runtime/state.rs:605,616`; `runtime/task_routes/live.rs:588`; `runtime/task_routes/cockpit.rs:327`; `slices/cockpit/mod.rs:74,156`; `slices/web_session/prepare_session.rs:28`; `session_activity.rs:67`; `startup_recovery.rs:159`; `task_session_directory.rs:135`.
- Prod lines removed: **not measured**. Each site's terminal arm is a few lines; a total would be guesswork, so I give no number. Upper bound from the site count alone: about 30 sites in ajax-core and ajax-web.
- Test lines removed: not measured. Tests that exist only for the tmux path: `models/task.rs:494-500`, `remediation.rs` tests that construct `TerminalAgent` tasks.
- Evidence: `git grep -nE "TerminalAgent|terminal_agent" -- 'crates/*.rs'` (excluding tests) → 4 hits, all in `models/task.rs`. `git grep -n "TerminalAgent|terminal-agent|terminal_agent" -- crates/ajax-web/web/src` → 0 hits. So the frontend does not name the kind; the split lives entirely in the metadata flag.
- What replaces it: if new terminal-agent tasks stop being creatable, every task is Chat, and the `skip_interactive_agent` flag, `TaskKind`, and the terminal arms become dead for new tasks. But **existing** tasks have no flag and resolve to TerminalAgent through `kind()`. Removing the terminal arms breaks them unless a migration sets the flag on every existing task. That is the approval gate.
- Behavior or public contract change: PUBLIC. `ajax task create` / the new-task route can no longer produce a terminal-agent task; the web terminal tab for existing tmux-backed tasks depends on the tmux arms that would go. The existing registry metadata key `skip_interactive_agent` must keep its meaning for old rows.
- Risk: high: this is lifecycle and registry truth (`models/task.rs`, sqlite metadata). Requires Matt's approval before any change.
- Depends on: A1 (cockpit, `tmux_task_session`, dispatch arms that set `false`), D5 if it covers `runtime_refresh` tmux checks.
- Confidence: likely for the reference list (grep-backed); speculative for line counts.
- FLAG: lifecycle/registry truth: needs approval.

## Totals (new beyond A1, A2, D1-D6)

| Confidence | Prod lines | Test lines |
|---|---:|---:|
| proven | 0 | 0 |
| likely | 0 | 0 |
| speculative | not measured (G-01, G-02) | not measured |

A1/A2 gross lines (ajax-tui 10,765 + ajax-supervisor 1,816 + the cockpit and supervise files) are already counted by Slice A and are not repeated here. Their transitive support in ajax-core was measured (G-00) and found to add nothing beyond D.

## Not worth it

- `crates/ajax-core/src/ui_state.rs` (1,186 lines): referenced by ajax-web `slices/cockpit/mod.rs`, `slices/actions.rs`, `slices/push.rs`. Stays.
- `crates/ajax-core/src/runtime_refresh.rs` (4,840 lines): referenced by ajax-web `runtime/bridge.rs` and `runtime/mod.rs`. Stays.
- `crates/ajax-core/src/task_operations.rs` (4,374 lines): referenced by ajax-web `slices/operate/mod.rs`. Stays.
- `crates/ajax-core/src/remediation.rs` (712 lines): referenced by ajax-web `runtime/task_routes/live.rs`, `slices/ci_agent_delivery.rs`, `slices/operate/mod.rs`. Stays.
- `crates/ajax-core/src/live.rs` family and `execution.rs`: referenced by ajax-web adapters and slices. Stays.
- `crates/ajax-core/src/recommended.rs` (811 lines): used by `commands/projection.rs` in production. Stays.
- `crates/ajax-core/src/analysis/` (327 lines): used by `commands.rs` and `commands/teardown/drop_observation.rs`. Stays.

## Limits

- Measurement budget was about 16 tool calls, well under the 70-call and 15-minute caps.
- Not done: the per-module pub-item sweep (script listed in the brief), hand-confirmation of the largest module, and line counts for the terminal-branch family. These are not reached.
- The module sweep works at file granularity and does not prove usage at item granularity.
