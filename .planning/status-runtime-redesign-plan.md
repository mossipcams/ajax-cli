# Ajax Status, Lifecycle, State, and Agent Runtime Redesign Plan

Date: 2026-06-08

## Goal

Make Ajax task state truthful and actionable by separating durable task lifecycle
from observed runtime facts. The UI should stop collapsing observable failures
and stale evidence into `Unknown`, and should never mutate durable lifecycle from
low-confidence terminal text or probe failures.

## Current Failure Modes

- `LiveStatusKind` mixes unrelated dimensions: agent activity, command/test
  activity, prompts, missing substrate, CI/Git blockers, terminal idleness, and
  unknown pane text.
- `AgentRuntimeStatus::Unknown` is assigned when Ajax actually knows something
  concrete, such as missing worktree, tmux session, or task window.
- Tmux probe failures can be interpreted as resource absence:
  - `tmux list-sessions` errors can cascade into missing-session evidence.
  - `tmux list-windows` errors become `WorktrunkMissing`.
  - `tmux capture-pane` errors become `CommandFailed`.
- Terminal pane scrollback can drive durable lifecycle transitions to
  `Reviewable`, `Waiting`, or `Error`.
- Agent status cache values have no freshness metadata, so stale `working`,
  `wait`, `ask`, or `done` files can dominate fresh reality.
- Normal task start launches the agent directly in tmux. The supervisor-backed
  event stream exists, but is only used by explicit `ajax supervise`, so normal
  Cockpit status is reconstructed from cache strings and terminal text.
- `RuntimeProjection::requires_refresh` treats `RuntimeObservationSource::Unknown`
  as not needing refresh, which lets default or incomplete projections look
  fresh in some call paths.

## Target Model

### Durable Lifecycle

`LifecycleStatus` represents Ajax-owned workflow state only:

- Creation/provisioning/removal operation states.
- Active/review/merge/cleanup states caused by Ajax task operations or trusted
  terminal agent completion evidence.
- Operation failure states caused by Ajax operations.

Runtime observations must not directly write workflow lifecycle except through a
small, explicit transition boundary for trusted terminal events. In particular:

- Waiting-for-input is runtime/attention, not lifecycle.
- Auth/rate/context/CI/merge blockers are runtime conditions, not generic
  lifecycle `Error`.
- Pane classifier text is low-confidence and cannot mark a task `Reviewable` or
  `Error`.

Keep existing enum variants for compatibility during the first implementation
pass, but stop producing `Waiting` from runtime evidence and treat existing
loaded `Waiting` rows as active tasks with a waiting runtime condition.

### Runtime Evidence

Introduce an explicit runtime evidence model in `ajax-core`, keeping adapters at
the boundary:

- `Observed<T>` or equivalent records:
  - `Observed(value)`
  - `NotObserved(reason)`
  - `ProbeFailed(reason)`
  - `Stale(value, observed_at)`
- Substrate evidence is tracked per resource instead of collapsed immediately:
  - worktree
  - branch
  - tmux session
  - task window/path
- Agent runtime evidence is tracked separately:
  - `NotStarted`
  - `Starting`
  - `Running`
  - `WaitingForInput`
  - `WaitingForApproval`
  - `Blocked`
  - `ExitedSuccess`
  - `ExitedFailure`
  - `Stopped`
  - `NotObserved(reason)`
- Activity evidence is separate from agent state:
  - shell idle
  - thinking
  - command running
  - tests running
- Blocking conditions are separate facts:
  - auth required
  - rate limited
  - context limit
  - merge conflict
  - CI failed
  - command failed
  - process hung

Every evidence item should carry:

- observation source: operation result, tmux probe, git probe, agent wrapper,
  agent status cache, pane classifier, supervisor event, or filesystem event;
- observed timestamp;
- confidence: authoritative, high, or low.

### Derived Operator Status

Expose one derived presentation status for CLI/Cockpit/TUI/Web. It is computed
from durable lifecycle plus runtime evidence using deterministic priority:

1. Removed/archived lifecycle.
2. Destructive operation in progress or incomplete teardown.
3. Observed substrate gaps.
4. Probe failures or stale evidence.
5. Trusted blockers and prompts.
6. Trusted agent activity.
7. Review/merge/cleanup lifecycle.
8. Idle or not started.

The derived status should use explicit labels such as:

- `runtime probe failed: tmux list-windows`
- `agent status stale`
- `tmux session missing`
- `waiting for approval`
- `agent exited successfully`
- `review ready`

It should not render `unknown` for expected states. `Unknown` can remain as a
legacy deserialization value, but new projection code should render explicit
not-observed or stale reasons.

### Runtime Authority

Normal task launch should create an Ajax-owned runtime evidence source, not rely
on pane scraping as the primary source. Preferred approach:

- Add a small host-native agent runtime wrapper command in `ajax-cli`.
- The wrapper runs inside the task tmux pane, launches the selected agent with
  inherited stdio, writes atomic runtime snapshot files under the selected Ajax
  runtime cache/log path, records start/heartbeat/exit, and preserves the normal
  interactive terminal experience.
- `runtime_refresh` reads wrapper snapshots as high-confidence evidence.
- Existing `tmux-agent-status` cache remains useful for prompt-level state, but
  it must be freshness checked and lower priority than wrapper process evidence.
- Pane classifier remains a low-confidence fallback only.

## TDD Task Plan

### Task 1: Define the New Runtime Evidence Contract

- Failing test to write:
  - Add focused unit tests in `crates/ajax-core/src/runtime.rs` proving that
    runtime reduction distinguishes:
    - observed missing resource;
    - probe failure;
    - not observed;
    - stale previous evidence.
  - Add a test that no reduced status label is `unknown` when a reason is known.
- Code to implement:
  - Add runtime evidence types and reduction helpers in `ajax-core::runtime`.
  - Keep the existing `RuntimeProjection` public shape available while adding
    the richer internal evidence model.
- Verification:
  - `rtk cargo nextest run -p ajax-core runtime`

### Task 2: Stop Runtime Evidence from Mutating Durable Lifecycle

- Failing test to write:
  - Add tests in `crates/ajax-core/src/live_application.rs` showing:
    - pane-classified `Done` does not mark an active task `Reviewable`;
    - pane-classified `CommandFailed`, auth, rate limit, and context limit do
      not mark lifecycle `Error`;
    - waiting evidence does not mark lifecycle `Waiting`.
- Code to implement:
  - Split live evidence application into runtime/attention updates and trusted
    workflow transitions.
  - Keep task annotations and UI state showing attention without changing
    durable lifecycle.
- Verification:
  - `rtk cargo nextest run -p ajax-core live_application`

### Task 3: Add a Trusted Terminal Event Transition Boundary

- Failing test to write:
  - Add tests in `crates/ajax-core/src/events.rs` proving that trusted agent
    wrapper or supervisor completion can mark `Active -> Reviewable`, while
    low-confidence pane text cannot.
  - Add tests that trusted process failure records an agent failure condition
    without using generic lifecycle `Error` unless an Ajax operation failed.
- Code to implement:
  - Introduce explicit trusted event application APIs.
  - Update monitor-event application to use the trusted path.
- Verification:
  - `rtk cargo nextest run -p ajax-core events`

### Task 4: Make Probe Failures First-Class

- Failing test to write:
  - Add tests in `crates/ajax-core/src/runtime_refresh.rs` showing:
    - `tmux list-sessions` command failure becomes probe-failed evidence, not
      missing session;
    - `tmux list-windows` command failure becomes probe-failed evidence, not
      missing task window;
    - `capture-pane` failure becomes pane-unavailable evidence, not
      `CommandFailed`.
  - Add coverage for `tmux` no-server output as true missing-session evidence
    only when the command outcome is known and parsed as no server.
- Code to implement:
  - Add typed probe outcomes around tmux/git runner calls.
  - Refresh runtime evidence from `ProbeFailed` without clearing the last
    credible observed resource value.
- Verification:
  - `rtk cargo nextest run -p ajax-core runtime_refresh`

### Task 5: Add Freshness to Agent Status Cache

- Failing test to write:
  - Add tests in `crates/ajax-cli/src/agent_status_cache.rs` showing:
    - stale status files are ignored or marked stale;
    - newest pane/session status wins when multiple files exist;
    - stale `working` cannot override fresh `done` or fresh waiting evidence.
- Code to implement:
  - Include file modification time in cache snapshot entries.
  - Add a freshness window constant and deterministic newest-value reduction.
  - Surface stale cache evidence as stale/not-observed rather than `Unknown`.
- Verification:
  - `rtk cargo nextest run -p ajax-cli agent_status_cache`

### Task 6: Add the Agent Runtime Wrapper

- Failing test to write:
  - Add unit tests in `crates/ajax-cli/src/agent_runtime.rs` or the chosen new
    module proving the wrapper:
    - writes an atomic `starting` snapshot before spawning;
    - writes heartbeat/running evidence while the child is active;
    - writes exited-success and exited-failure snapshots with exit code;
    - preserves the selected runtime cache/log path.
- Code to implement:
  - Add the small wrapper module/command using existing process facilities where
    practical.
  - Avoid SQLite writes from the wrapper; write append-only JSONL plus latest
    atomic snapshot under the runtime cache/log path.
- Verification:
  - `rtk cargo nextest run -p ajax-cli agent_runtime`

### Task 7: Launch Normal Tasks Through the Wrapper

- Failing test to write:
  - Add tests in `crates/ajax-core/src/commands/new_task.rs` proving the start
    plan sends the wrapper command to tmux and preserves the selected agent
    command, worktree path, and prompt behavior.
  - Add CLI-level test coverage in `crates/ajax-cli/src/lib/tests.rs` for runtime
    paths being passed to the wrapper command.
- Code to implement:
  - Extend command planning so the tmux `send-keys` launches the Ajax wrapper
    with enough task/runtime context to emit runtime evidence.
  - Preserve public task start behavior and existing tmux session shape.
- Verification:
  - `rtk cargo nextest run -p ajax-core new_task`
  - `rtk cargo nextest run -p ajax-cli start`

### Task 8: Merge Wrapper, Cache, Pane, Git, and Tmux Evidence

- Failing test to write:
  - Add runtime refresh tests in `crates/ajax-core/src/runtime_refresh.rs`
    proving evidence priority:
    - authoritative wrapper exit beats stale cache `working`;
    - fresh cache prompt beats low-confidence pane text;
    - pane text can fill display summary only when no stronger evidence exists;
    - missing substrate outranks agent activity.
- Code to implement:
  - Hydrate wrapper snapshots in the CLI adapter and pass them into core refresh
    through a small port.
  - Update core refresh reduction to use confidence/source/timestamp priority.
- Verification:
  - `rtk cargo nextest run -p ajax-core runtime_refresh`
  - `rtk cargo nextest run -p ajax-cli cockpit`

### Task 9: Persist Rich Runtime Evidence Safely

- Failing test to write:
  - Add SQLite tests in `crates/ajax-core/src/registry/sqlite.rs` showing:
    - rich runtime evidence survives save/load;
    - old rows with `Unknown` load as explicit not-observed/stale evidence;
    - old `Waiting` lifecycle rows load into an active workflow with waiting
      runtime condition;
    - schema migration preserves existing task intent and substrate fields.
- Code to implement:
  - Add schema columns or JSON evidence payloads for runtime evidence.
  - Add migration and backward-compatible parsing.
  - Preserve existing public JSON fields during the migration window.
- Verification:
  - `rtk cargo nextest run -p ajax-core sqlite`

### Task 10: Rebuild Operator Projection and Actions from Derived Status

- Failing test to write:
  - Add tests in:
    - `crates/ajax-core/src/ui_state.rs`
    - `crates/ajax-core/src/attention.rs`
    - `crates/ajax-core/src/recommended.rs`
    - `crates/ajax-core/src/commands/projection.rs`
  - Cover:
    - no `unknown` label for concrete evidence;
    - probe failure recommends refresh/repair instead of drop;
    - observed missing worktree recommends repair/drop according to policy;
    - waiting and blocked statuses show attention without lifecycle `Error`.
- Code to implement:
  - Introduce one derived operator-facing status function.
  - Update annotation, UI state, recommended actions, and task cards to consume
    it instead of reinterpreting raw fields independently.
- Verification:
  - `rtk cargo nextest run -p ajax-core ui_state attention recommended projection`

### Task 11: Update CLI, TUI, and Web Rendering

- Failing test to write:
  - Add rendering/projection tests in:
    - `crates/ajax-cli/src/render.rs`
    - `crates/ajax-web/src/slices/cockpit.rs`
    - `crates/ajax-tui/src/lib.rs` or the narrowest TUI module that owns status
      labels
  - Cover explicit labels for stale/probe-failed/not-observed runtime states.
- Code to implement:
  - Render derived status and evidence reason consistently across CLI JSON,
    human CLI, Native Cockpit, and Web Cockpit.
  - Keep old fields present where API compatibility currently requires them.
- Verification:
  - `rtk cargo nextest run -p ajax-cli render`
  - `rtk cargo nextest run -p ajax-web cockpit`
  - `rtk cargo nextest run -p ajax-tui status`

### Task 12: End-to-End Reliability Regression

- Failing test to write:
  - Add focused integration-style tests in `crates/ajax-cli/src/lib/tests.rs`
    using fake git/tmux/agent commands, not `crates/ajax-cli/tests/smoke_user_flows.rs`.
  - Cover:
    - newly started task becomes running from wrapper evidence;
    - killed agent becomes exited/stopped, not unknown;
    - tmux command failure shows probe failed, not missing session;
    - agent completion becomes reviewable only from trusted wrapper/supervisor
      event;
    - stale cache cannot keep a dead task running.
- Code to implement:
  - Wire all adapters and compatibility surfaces together.
  - Remove any remaining production path that writes `Unknown` for known
    observed conditions.
- Verification:
  - Focused nextest command for the new cases in `ajax-cli`.
  - Then final validation:
    - `rtk cargo fmt --check`
    - `rtk cargo check --all-targets --all-features`
    - `rtk cargo clippy --all-targets --all-features -- -D warnings`
    - `rtk cargo nextest run --all-features`

## Files Expected to Change

- `architecture.md`
- `crates/ajax-core/src/models.rs`
- `crates/ajax-core/src/runtime.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `crates/ajax-core/src/live.rs`
- `crates/ajax-core/src/live_application.rs`
- `crates/ajax-core/src/events.rs`
- `crates/ajax-core/src/attention.rs`
- `crates/ajax-core/src/ui_state.rs`
- `crates/ajax-core/src/recommended.rs`
- `crates/ajax-core/src/commands/projection.rs`
- `crates/ajax-core/src/commands/new_task.rs`
- `crates/ajax-core/src/registry/sqlite.rs`
- `crates/ajax-cli/src/agent_status_cache.rs`
- `crates/ajax-cli/src/agent_runtime.rs` or equivalent new module
- `crates/ajax-cli/src/cli.rs`
- `crates/ajax-cli/src/execution_dispatch.rs`
- `crates/ajax-cli/src/render.rs`
- `crates/ajax-cli/src/lib.rs`
- `crates/ajax-cli/src/lib/tests.rs`
- `crates/ajax-web/src/slices/cockpit.rs`
- `crates/ajax-tui/src/lib.rs` or a narrower TUI status/rendering module

No files under `crates/ajax-cli/tests/` are planned for modification. In
particular, `crates/ajax-cli/tests/smoke_user_flows.rs` is intentionally left
untouched.

## Compatibility and Migration Notes

- Keep existing public JSON fields during the first pass:
  - `lifecycle_status`
  - `agent_status`
  - `live_status`
  - `runtime_projection`
- Add richer status fields alongside the legacy fields, then migrate callers.
- Continue to parse legacy `Unknown` values from SQLite, but normalize them into
  explicit not-observed or stale evidence for new projections.
- Keep legacy enum variants until downstream output and storage compatibility is
  intentionally removed in a separate approved cleanup.
- Update `architecture.md` in the same work because this changes status,
  runtime authority, and lifecycle boundaries.

## Final Validation

Run the strongest applicable validation after all approved tasks are complete:

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
```

Run documentation validation if the architecture/doc changes warrant it:

```sh
rtk cargo doc --no-deps --all-features
RUSTDOCFLAGS="-D warnings" rtk cargo doc --no-deps --all-features
```
