# Web Task Architecture Cleanup: Lazy Tmux, Lifecycle & Status Alignment, and Drop-First Actions

## Goal

Fix the architectural mismatch between Ajax Web (`skip_interactive_agent` / orchestration-chat tasks) and the core task/runtime model:

1. **No automatic tmux on web task creation:** `skip_interactive_agent` tasks only provision their git worktree (and router symlinks seed if configured) on creation—never a detached tmux session. Base tmux sessions are created lazily only when the user actually attaches a terminal (`#/t/<handle>` or `ajax open`).
2. **Fix broken web task lifecycle and status derivation:**
   - Complete start provisioning (`Provisioning -> Active`) for **all** `skip_interactive_agent` harnesses (`claude`, `codex`, `cursor`, `pi`), removing the legacy `"cursor"`-only gate in `crates/ajax-core/src/task_operations/start.rs`.
   - Treat tmux as optional for `skip_interactive_agent` tasks in `runtime_health`, `runtime_refresh`, `Task::apply_tmux_status`, and `ui_state` so missing tmux (or no tmux server running) never forces web tasks into `Error ("Tmux session missing")` / `Error ("Status unavailable")` / `AgentRuntimeStatus::Dead`, and never runs CLI `capture-pane` wait-fallbacks over ACP-owned tasks.
   - Project fresh `Active` web tasks awaiting their first prompt (`AgentRuntimeStatus::NotStarted`, `live_status: None`) as `TaskStatus::Idle ("Ready")` instead of `TaskStatus::Unknown`.
   - Automatically reconcile task lifecycle (`Active <-> Reviewable <-> Mergeable -> Merged`) from authoritative turn starts and Git/GitHub PR/CI evidence so Web Cockpit sees and uses `Reviewable`, `Mergeable`, and `Merged` (`Ship`, `"Ready to ship"`, `"Merged — ready to drop"`, `Review` attention band) without manual CLI ceremony.
3. **Remove `Repair` from Ajax Web and make dashboard swipe-left exclusively `Drop`:**
   - Remove `OperatorAction::Repair` from Web Cockpit (`supported_web_action` and web `operate`), retiring the legacy CLI/tmux repair verb from browser cards, chat headers, and details sheets.
   - Bind the dashboard task row left-swipe reveal slot (`TaskList.tsx`) specifically to the `drop` action so swiping a task row on Web Cockpit always reveals `Drop`.

## Scope

- `crates/ajax-core/src/commands/new_task.rs`
- `crates/ajax-core/src/commands/open.rs`
- `crates/ajax-core/src/task_operations/start.rs`
- `crates/ajax-core/src/models/task.rs`
- `crates/ajax-core/src/runtime.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `crates/ajax-core/src/runtime_refresh/ci_monitor.rs`
- `crates/ajax-core/src/lifecycle.rs`
- `crates/ajax-core/src/live_application.rs`
- `crates/ajax-core/src/ui_state.rs`
- `crates/ajax-web/src/slices/terminal.rs`
- `crates/ajax-web/src/adapters/terminal_pty/attach.rs`
- `crates/ajax-web/src/adapters/terminal_pty/bridge.rs`
- `crates/ajax-web/src/slices/actions.rs`
- `crates/ajax-web/src/slices/operate/mod.rs`
- `crates/ajax-web/web/src/features/task/TaskList.tsx`
- Focused unit/integration tests in `crates/ajax-core` and `crates/ajax-web` (Rust and Vitest)
- Owning architecture docs: `docs/architecture/web-cockpit.md`, `docs/architecture/web-session-behavior.md`, `docs/architecture/task-operations.md`, `docs/architecture/core-subsystems.md`

## Non-goals

- Removing `ajax repair` from the CLI (`ajax-cli`).
- Removing `Reviewable`, `Mergeable`, or `Merged` from `LifecycleStatus`.
- Changing ACP WebSocket session protocol or transcript storage schema.

## Architecture and boundary check

- **Core owns task truth:** Lifecycle transitions (`Provisioning -> Active`, `Active <-> Reviewable <-> Mergeable -> Merged`), optional-tmux runtime health for `skip_interactive_agent` tasks, and operator status derivation remain in `ajax-core`.
- **UI presents task truth:** `ajax-web` maps supported web actions (excluding `Repair`) and lazily ensures the base tmux session when the browser terminal WebSocket attaches. `TaskList.tsx` binds swipe-reveal to `drop` without inventing client-side lifecycle rules.
- **No second source of truth:** Git worktrees, GitHub PRs/checks, ACP session events, and tmux remain authoritative for their own reality.

## File-size budget

All touched Rust files must remain under the 1,000-line hard limit and near the 600-line target:

- `crates/ajax-core/src/commands/new_task.rs`: ~594 lines -> ~600 lines
- `crates/ajax-core/src/task_operations/start.rs`: ~163 lines -> ~165 lines
- `crates/ajax-core/src/commands/open.rs`: ~81 lines -> ~100 lines
- `crates/ajax-core/src/models/task.rs`: ~490 lines -> ~505 lines
- `crates/ajax-core/src/runtime.rs`: ~506 lines -> ~525 lines
- `crates/ajax-core/src/runtime_refresh.rs`: ~815 lines -> ~835 lines
- `crates/ajax-core/src/runtime_refresh/ci_monitor.rs`: ~383 lines -> ~420 lines
- `crates/ajax-core/src/lifecycle.rs`: ~520 lines -> ~550 lines
- `crates/ajax-core/src/live_application.rs`: ~710 lines -> ~725 lines
- `crates/ajax-core/src/ui_state.rs`: ~330 lines -> ~345 lines
- `crates/ajax-web/src/slices/terminal.rs`: ~80 lines -> ~95 lines
- `crates/ajax-web/src/adapters/terminal_pty/attach.rs`: ~322 lines -> ~350 lines
- `crates/ajax-web/src/adapters/terminal_pty/bridge.rs`: ~493 lines -> ~515 lines
- `crates/ajax-web/src/slices/actions.rs`: ~215 lines -> ~195 lines
- `crates/ajax-web/src/slices/operate/mod.rs`: ~611 lines -> ~590 lines

## Steps

- [x] **Step 1 (`ajax-core` start & optional tmux substrate):**
  - Update `new_task_plan_with_observation` (`crates/ajax-core/src/commands/new_task.rs`) so `skip_interactive_agent` tasks do not push `tmux.new_detached_task_session` or `tmux.send_agent_command`.
  - Update `execute_start_task_operation_with_checkpoint` (`crates/ajax-core/src/task_operations/start.rs`) to synthesize `StartProvisioningStep::AgentCommandSent` (transitioning `Provisioning -> Active`) for **all** `skip_interactive_agent` tasks regardless of harness (`claude`, `codex`, `cursor`, `pi`).
  - Update `open_task_plan` (`crates/ajax-core/src/commands/open.rs`) so `OpenMode::NoAttach` succeeds with an empty plan for `skip_interactive_agent` tasks without tmux, and `OpenMode::Attach` lazily includes `tmux.new_detached_task_session` when `tmux_status` is absent/false.
  - Update `Task::apply_tmux_status` and `Task::apply_task_window_status` (`crates/ajax-core/src/models/task.rs`), `runtime_health` (`crates/ajax-core/src/runtime.rs`), and `refresh_runtime_state` (`crates/ajax-core/src/runtime_refresh.rs`) so:
    - `skip_interactive_agent` tasks treat tmux as optional (missing tmux never sets `SideFlag::TmuxMissing`/`TaskWindowMissing`, `LiveStatusKind::TmuxMissing`, `AgentRuntimeStatus::Dead`, or `RuntimeHealth::MissingSession`/`MissingTaskWindow`),
    - `tmux list-sessions` exiting non-zero because no tmux server is running is treated as an empty session list rather than a `runtime_probe_failure`,
    - `skip_interactive_agent` tasks skip the CLI `agent_status_source` / `tmux capture-pane` loop in `refresh_runtime_state`.
- [x] **Step 2 (`ajax-core` lifecycle & operator status alignment):**
  - Update `derive_task_status` and `canonical_missing_substrate_explanation` (`crates/ajax-core/src/ui_state.rs`) so a `skip_interactive_agent` task in `Active` with `has_no_status_evidence` projects `TaskStatus::Idle ("Ready")` instead of `Unknown`, and ignores tmux missing flags/health.
  - Update `apply_authoritative_observation_at` (`crates/ajax-core/src/live_application.rs`) so `LiveStatusKind::AgentRunning | CommandRunning | TestsRunning` transitions `Provisioning | Waiting | Reviewable | Mergeable -> Active`.
  - Update `ci_monitor.rs` (`crates/ajax-core/src/runtime_refresh/ci_monitor.rs`) and `lifecycle.rs` (`crates/ajax-core/src/lifecycle.rs`) so:
    - Discovering a merged PR (`PullRequestState::Merged`) or `git_status.merged` transitions `Active | Reviewable | Mergeable -> Merged`,
    - Discovering an open PR (`PullRequestState::Open`) when the agent is not running/waiting (`AgentRuntimeStatus::Done | NotStarted`) transitions `Active -> Reviewable`,
    - Passing CI checks (`CiChecksState::Healthy`) on a `Reviewable` task (without `Dirty`/`Conflicted`/`TestsFailed`) transitions `Reviewable -> Mergeable`.
- [x] **Step 3 (`ajax-web` lazy terminal tmux, remove `Repair`, and `Drop`-first swipe):**
  - Include `worktree_path` in `TerminalAttachPlan` (`crates/ajax-web/src/slices/terminal.rs`, `crates/ajax-web/src/adapters/terminal_pty/attach.rs`) and lazily create the base task tmux session (`tmux new-session -d -s <tmux_session> -n <task_window> -c <worktree_path>`) in `bridge_task_terminal_socket` (`crates/ajax-web/src/adapters/terminal_pty/bridge.rs`) when the base session does not yet exist and `worktree_path` exists on disk.
  - Remove `OperatorAction::Repair` from `supported_web_action` (`crates/ajax-web/src/slices/actions.rs`) and reject `"repair"` as an unsupported web action in `crates/ajax-web/src/slices/operate/mod.rs`.
  - Update `TaskList.tsx` (`crates/ajax-web/web/src/features/task/TaskList.tsx`) so `revealAction` selects the `drop` action (`visibleTaskActions(card.actions).find((a) => a.action === "drop")`), ensuring swipe-left on a task row always reveals `Drop`.
  - Update affected Rust and frontend unit tests and architecture docs (`docs/architecture/web-cockpit.md`, `docs/architecture/web-session-behavior.md`, `docs/architecture/task-operations.md`, `docs/architecture/core-subsystems.md`).
- [x] **Step 4 (Independent review & verification):**
  - Run `cargo test -p ajax-core`, `cargo test -p ajax-web`, and frontend Vitest suite (`npm test` in `crates/ajax-web/web`).
  - Route independent review to a non-implementer delegate and verify `git diff` and `git status`.

## Verification

- `cargo fmt --check` (exit 0)
- `cargo check --all-targets --all-features` (exit 0)
- `cargo clippy --all-targets --all-features -- -D warnings` (exit 0)
- `cargo nextest run -p ajax-core -p ajax-web` (exit 0 — 2,361 passed, 1 skipped)
- `npm run verify:arch` (exit 0)
- `npm run web:check` (exit 0)
- `npm run web:lint` (exit 0)
- `npm run web:sg` (exit 0)
- `npm run web:test -- --run` (exit 0 — 166 test files passed, 1,659 tests passed)
- `npm run web:build` (exit 0)

## Risks and open questions

- Existing tests in `ajax-core` and `ajax-web` that asserted `tmux new-session` on `orchestration_chat: true` start or `"repair"` in `browser_cockpit_view` actions must be updated to reflect the new architectural contract (no tmux on web task start; no `repair` action in Web Cockpit).
- Resolved with user:
  1. Remove `Repair` from Ajax Web completely.
  2. Keep `Reviewable`, `Mergeable`, and `Merged` lifecycle transitions driven automatically so Web Cockpit sees and uses them.
