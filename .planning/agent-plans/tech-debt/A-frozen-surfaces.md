# A — Frozen Surfaces (native TUI, tmux verbs, terminal-agent tasks, supervisor)

Audit slice A of the 15% tech-debt effort. Read-only; evidence via grep/wc -l from
the worktree at audit time. Goal question: full cost of keeping the frozen surfaces,
and how much code goes if each is removed or reduced to a minimum.

Method notes:

- "Prod lines" excludes `#[cfg(test)]` blocks and dedicated test files; "Test lines"
  counts only tests that exist solely for this code.
- Inline test line counts computed by counting from the first `#[cfg(test)]` in each
  file to EOF (all such blocks are terminal in these crates); cross-checked with
  `wc -l` per file.
- Shared-file dispatch arms and CLI test-suite portions are estimated by keyword
  attribution (heuristic script in `.planning/router-runs/tech-debt/count_tests.py`);
  those numbers carry a ±10% margin and are flagged as such.

Reachability summary (proven below per candidate):

- `ajax cockpit` / `ajax stable` / `ajax dev` → native TUI (`crates/ajax-tui`) via
  `cockpit_backend.rs`. `PtyTaskSessionRunner` (CLI tmux attach,
  `tmux_task_session/`) is instantiated ONLY in `cockpit_backend.rs:86`; the only
  production `impl TaskSessionRunner` outside tests is at
  `crates/ajax-cli/src/tmux_task_session/mod.rs:481`. So all CLI tmux verbs are
  reachable only through the frozen TUI.
- `ajax supervise` → `crates/ajax-supervisor`, via `supervise.rs` + one arm in
  `execution_dispatch.rs:163`. No reference to `ajax_supervisor` anywhere in
  `crates/ajax-web/src` (`grep -rln ajax_supervisor crates/ajax-web/src` → no matches).
- The Web cockpit terminal tab (the "lazy tmux" chat path) uses
  `crates/ajax-core::adapters::{tmux_session_target, tmux_window_target,
  tmux_session_slot_target}` (`crates/ajax-web/src/adapters/terminal_pty/attach.rs:9`)
  — so the **core** TmuxAdapter and its runtime_refresh status classification stay;
  they are not part of any removal below.

---

### A1 Remove native TUI / Cockpit CLI surface (incl. CLI tmux verbs)

- Paths:
  - `crates/ajax-tui/` (entire crate: lib.rs, actions.rs, cockpit_state.rs, input.rs,
    layout.rs, navigation.rs, palette.rs, rendering.rs, runtime.rs, architecture.rs,
    lib/tests.rs)
  - `crates/ajax-cli/src/cockpit_backend.rs`, `crates/ajax-cli/src/cockpit_actions.rs`,
    `crates/ajax-cli/src/cockpit_snapshot.rs`, `crates/ajax-cli/src/cockpit_backend/`
    (tests dir), `crates/ajax-cli/src/tmux_task_session/` (mod.rs, attach.rs, tests.rs)
  - Dispatch arms: `execution_dispatch.rs` (`render_cockpit_entry_command`,
    `mobile_web_port_for_cockpit_entry`, `ExecuteNewTaskWithSession`,
    `execute_new_task_plan_with_task_session_and_checkpoint`, imports),
    `snapshot_dispatch.rs` arm, `cli.rs` cockpit/stable/dev subcommand builders,
    `context.rs:66-70` dev/stable profile inference, `lib.rs:472` watch-flag handling
- Prod lines removed: **~4,630** (how counted)
  - ajax-tui crate prod: 2,138 (= 6,657 total − 4,519 tests; inline tests computed
    per-file from `#[cfg(test)]` to EOF plus lib/tests.rs 4,247 and the 99-line
    architecture guard)
  - cockpit_backend.rs 593 + cockpit_actions.rs 550 + cockpit_snapshot.rs 16 = 1,159
  - tmux_task_session mod.rs 580 + attach.rs 604 = 1,184 (tests excluded)
  - dispatch/cli glue ≈150 (execution_dispatch fns at lines 377-473 ≈135, cli.rs
    builders ≈40, context.rs ≈6, lib.rs arm/flag ≈4) — estimate, keyword-attributed
- Test lines removed: **~8,750** (how counted)
  - ajax-tui tests: 4,519
  - tmux_task_session/tests.rs: 632
  - `cockpit_backend/` tests dir: tests.rs 728 + cockpit_persistence_tests.rs 477 +
    mobile_web_companion_tests.rs 43 = 1,248 (dir exists solely for this code)
  - ajax-cli lib/tests suites: ~2,348 keyword-attributed lines in suite_1/2/3/4/9-13
    (heuristic; shared test files also cover non-cockpit commands, so actual removal
    is a slice of these files, not the whole file)
- Evidence:
  - `wc -l` crate totals: ajax-tui 6,657; per-file listing run at audit time
  - Entry points: `crates/ajax-cli/src/cli.rs:289-298` (`cockpit_command`,
    `cockpit_alias_command("stable"/"dev")`); dispatch arms at
    `execution_dispatch.rs:187` and `snapshot_dispatch.rs:140`
  - TUI reachability: `grep -rln ajax_tui crates/ajax-cli/src crates/ajax-web/src` →
    only ajax-cli cockpit files + their tests; zero matches in ajax-web
  - tmux verbs reachability: `grep -rn 'PtyTaskSessionRunner' crates/ajax-cli/src
    --include='*.rs' | grep -v 'tests/'` → single production caller,
    `cockpit_backend.rs:86`; `impl TaskSessionRunner` (production) only at
    tmux_task_session/mod.rs:481; the `start`/`resume` command paths do NOT route
    through tmux_task_session (`execution_dispatch.rs:62-99` uses
    `execute_start_task_operation`, no TaskSessionRunner involvement)
  - The interactive "open new task session in terminal" loop (TaskSessionEnd::OpenNewTask)
    is driven from cockpit_actions.rs:410/483 and cockpit_backend.rs:157 only
- What replaces it, or why nothing needs to: The Web Cockpit (`ajax web`) already
  provides the operator console, task actions (Fix CI / Resolve conflicts), and a
  live terminal tab via `adapters/terminal_pty` + core tmux targets. Nothing in
  ajax-web imports ajax-tui or cockpit_backend, so no web-side change is needed.
- Behavior or public contract change: **PUBLIC** — CLI commands `ajax cockpit`,
  `ajax stable`, `ajax dev` (with `--no-web`, `--watch`, `--json`, `--interval-ms`)
  are removed; the auto-start-mobile-web-companion behavior of those verbs is lost
  (`ajax web --host --port` remains as the equivalent). Docs referencing cockpit
  (`docs/architecture/cockpit.md`, README) must be updated.
- Risk: low — self-contained crate + CLI-only glue; no core/web dependency on it;
  frozen (no new features) so no in-flight work depends on it. Main residual risk is
  the keyword-attributed test slices inside shared lib/tests files.
- Depends on: none
- Confidence: proven (reachability + counts); dispatch-arm line totals are estimates

### A2 Remove live supervisor (`ajax supervise` + crates/ajax-supervisor)

- Paths:
  - `crates/ajax-supervisor/` (entire crate: runtime.rs, process_observer.rs,
    repo_observer.rs, lib.rs, event_log.rs, renderer.rs, status.rs, agent/codex.rs,
    agent/cursor.rs, architecture.rs)
  - `crates/ajax-cli/src/supervise.rs`
  - `crates/ajax-cli/src/execution_dispatch.rs:163-185` (supervise arm), 496-579
    (`validate_supervised_task`, `supervisor_agent_for_task`)
  - `crates/ajax-cli/src/cli.rs:119-148` (`supervise_command`)
- Prod lines removed: **~2,190** (how counted)
  - ajax-supervisor prod: ~1,861 (= 4,059 total − ~2,198 tests; inline test blocks
    counted per file from `#[cfg(test)]` to EOF: lib 494, runtime 564,
    process_observer 351, codex 285, repo_observer 152, cursor 123, status 82,
    event_log 32, renderer 16, plus the 99-line architecture guard file)
  - supervise.rs: 190
  - execution_dispatch supervise arm + helper fns ≈107 (lines 163-185 and 496-579,
    minus the cockpit test mod which belongs to A1)
  - cli.rs supervise_command ≈30
- Test lines removed: **~2,520** (how counted)
  - ajax-supervisor inline/guard tests: ~2,198 (see above)
  - ajax-cli lib/tests suites: ~321 keyword-attributed supervisor lines (heuristic,
    shared files)
- Evidence:
  - `wc -l` crate total: 4,059; per-file cfg(test) positions listed at audit time
  - Entry point: `cli.rs:120` (`json_command("supervise")`); dispatch arm
    `execution_dispatch.rs:163`; sole production user of the crate is
    `supervise.rs` (`grep -rn 'spawn_monitor\|MonitorConfig' crates/ajax-cli/src` →
    only supervise.rs)
  - No web usage: `grep -rln ajax_supervisor crates/ajax-web/src` → no matches
  - Core-side MonitorEvent type stays: `crates/ajax-core/src/acp_execution_state.rs`
    and `events.rs` also reference it (shared with chat ACP execution), so only the
    CLI + crate go away
- What replaces it, or why nothing needs to: Chat path already runs agents behind the
  session shim (`ajax-cli session-shim`) with status hooks (`agent-hooks`,
  `__agent-event`) feeding attention; the supervisor's job (observe codex/cursor CLI
  process) is the terminal-agent-era equivalent and is frozen. Nothing in web/Chat
  calls it.
- Behavior or public contract change: **PUBLIC** — CLI command `ajax supervise`
  (incl. `--task`, `--prompt`, `--agent codex|cursor`, hidden `--codex-bin` /
  `--cursor-bin`) and the opt-in env var `AJAX_SUPERVISE_EVENT_LOG` go away;
  monitor event logging into the registry via that path stops (chat-path ACP events
  are unaffected).
- Risk: low — no in-crate dependents, CLI-only entry, frozen. Slightly higher than A1
  only because `execution_dispatch.rs` is a shared file with intertwined arms.
- Depends on: none (independent of A1; both touch execution_dispatch.rs, so land as
  one change or serialize)
- Confidence: proven

### A3 Terminal-agent-task-specific core paths (not separable in this slice)

- Paths: `crates/ajax-core/src/models/task.rs` (`TaskKind::TerminalAgent`,
  lines 51, 154-160), plus interleaved branches inside `commands/new_task/`,
  `task_operations/`, `runtime_refresh.rs`, `attention/` that treat terminal-agent
  tasks as a special case.
- Prod lines removed: **not counted** — could not prove a clean boundary. The
  terminal-agent branches are interleaved with chat-task branches in the same
  functions, and the Web terminal tab reuses the same tmux window/session machinery
  for chat tasks (`task_pane_agent.rs`, `terminal_pty/attach.rs`), so a removal
  would require rewriting shared lifecycle code, not deleting it.
- Test lines removed: not counted (same reason).
- Evidence:
  - `grep -n 'TaskKind' crates/ajax-core/src/models/task.rs` → enum at line 51,
    `kind()` maps agent presence to TerminalAgent vs Chat (lines 154-160)
  - `runtime_refresh.rs:362-372` classifies per-window status generically for all
    tasks; no kind-gated branch isolatable by grep in this slice
- What replaces it, or why nothing needs to: a dedicated follow-up audit would need
  to gate every TerminalAgent branch and prove chat paths never hit them. Left out of
  the totals deliberately.
- Behavior or public contract change: **PUBLIC** if pursued (task kind becomes
  Chat-only; `--agent` on `ajax start` semantics change).
- Risk: high — core lifecycle/registry truth; touches architecture.md invariants.
- Depends on: A1, A2 (do those first so the TUI drawer's terminal-task actions are
  gone and no frozen surface references TaskKind::TerminalAgent)
- Confidence: speculative

---

## Totals by confidence (this slice)

| Candidate | Prod lines | Test lines | Total | Confidence |
|---|---:|---:|---:|---|
| A1 native TUI + CLI tmux verbs | ~4,630 | ~8,750 | ~13,380 | proven (dispatch-arm glue ±10%) |
| A2 supervisor | ~2,190 | ~2,520 | ~4,710 | proven (shared-file arms ±10%) |
| A3 terminal-agent core paths | 0 (not counted) | 0 (not counted) | — | speculative |
| **Total** | **~6,820** | **~11,270** | **~18,090** | |

~18,090 of the 38,650-line target (≈47%), from two whole-structure removals. No
overlap between A1 and A2 was assumed; keyword-attributed test slices in shared
`lib/tests/suite_*.rs` files are the main source of uncertainty.

## Tiered options (user decides)

- **Minimum (~4,710 lines):** A2 only. Self-contained crate + one CLI arm; loses
  `ajax supervise`. Lowest risk, smallest public surface change.
- **Medium (~18,090 lines):** A1 + A2. Native operator console and supervisor gone;
  Web Cockpit becomes the single console (it already does everything both do,
  including the terminal tab). Public loss: `ajax cockpit|stable|dev`,
  `ajax supervise`. Recommended tier.
- **Maximum:** Medium + A3 follow-up (terminal-agent core trim, speculative, needs
  its own audit) + deleting now-dead docs (`docs/architecture/cockpit.md`,
  supervisor sections of `docs/architecture/cli-supervisor.md` and README).
  Docs deletions are real maintenance removal but were not counted toward the 15%
  target here.

## Not worth it (looked, should stay)

- **`crates/ajax-core/src/adapters/tmux.rs` (295 lines) + tmux usage in
  `runtime_refresh.rs`, `sweep_cleanup.rs`, `drop_task.rs`, `remediation.rs`,
  `new_task/`:** the Web cockpit terminal tab attaches to task tmux windows
  (`ajax-web/src/adapters/terminal_pty/attach.rs:9,97`), and session-existence checks
  drive status reconciliation for ALL tasks including chat. Removing this would break
  the chat terminal tab.
- **`crates/ajax-cli/src/agent_runtime.rs`, `session_shim.rs`, `agent_hooks.rs`
  (+ `__agent-runtime`, `session-shim`, `agent-hooks` verbs):** the session shim is
  the authoritative substrate for chat turn state (architecture.md, Chat-first), and
  `__agent-runtime` is referenced from the web operate slice
  (`grep -rn 'agent_runtime' crates/ajax-web/src/slices/operate/tests.rs` → hit).
- **`crates/ajax-web/src/adapters/terminal_pty/` (1,798) + `slices/terminal.rs`
  (78) + `web_session/task_pane_agent.rs` (150):** the "lazy tmux" terminal tab —
  that IS the chat path, not a frozen surface. Reference only per slice brief.
- **`crates/ajax-core/src/events.rs` MonitorEvent plumbing:** shared with
  `acp_execution_state.rs` (chat ACP execution); only the supervisor's feed into it
  goes away with A2.
- **`render.rs`, `dispatch.rs`, `snapshot_dispatch.rs` bulk:** shared CLI plumbing
  for live commands (start/resume/ship/web/state); only small cockpit/supervise arms
  are attributed above.

## Not reached / remaining work

- A3 line-level quantification of TerminalAgent-gated branches in ajax-core (needs a
  dedicated branch-by-branch audit; flagged speculative, excluded from totals).
- Per-suite test-file boundary verification: keyword attribution is a heuristic;
  actual deletion will prune individual `#[test]` fns, not whole files.
- Docs inventory (README/PRODUCT/docs/ references to cockpit + supervise) was only
  spot-checked (`grep -rln 'supervise' docs README.md PRODUCT.md` → README.md).
- Did not verify whether `cockpit_backend`'s mobile-web companion startup logic
  duplicates `web_backend` boot code line-for-line (potential extra ~50-100 lines if
  A1 lands and the companion dies with it).
