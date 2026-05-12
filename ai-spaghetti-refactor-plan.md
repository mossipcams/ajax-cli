# AI Spaghetti Refactor Plan

This is a behavior-preserving refactor plan for moving Ajax toward a restrained
ports-and-adapters modular monolith. The goal is not a rewrite, not a framework
change, and not a microservice split. The goal is a codebase where each feature
is easy to follow as one vertical slice: CLI input, app/use-case orchestration,
domain or analysis decision logic, ports for real external boundaries, adapters
for IO, and tests around user-visible behavior.

This plan must be read with `architecture.md` and `AGENTS.md`:

- Keep the current crate boundaries unless an approved task explicitly changes
  them.
- Keep Cockpit native Rust over the `ajax-core` JSON and command contracts.
- Keep command planning separate from command execution.
- Keep lifecycle mutation centralized and visible.
- Do not create generic managers, services, processors, handlers, factories,
  registries, helpers, utils, or abstraction layers unless the concrete need is
  explained in the code review or PR.
- Prefer concrete structs and functions over traits. Introduce traits only for
  IO boundaries, test seams, or genuinely swappable implementations.

Markdown-only edits to this plan do not require TDD. Future code tasks in this
plan do require the repository TDD workflow.

## Current Shape Research

### File Size Hotspots

The current project already has useful crate boundaries, but several modules are
carrying too many responsibilities:

- `crates/ajax-cli/src/lib.rs`: about 4,941 lines. It contains public run
  entrypoints, CLI test fixtures, behavior tests, source-text policy tests,
  cockpit action tests, persistence-path tests, and execution tests.
- `crates/ajax-tui/src/lib.rs`: about 3,522 lines. It contains public TUI API,
  state, event loop, input handling, layout, rendering, and many tests.
- `crates/ajax-core/src/commands.rs`: about 3,356 lines. It is the public
  command facade, but also contains new-task planning, merge planning, cleanup
  planning, git evidence refresh, lifecycle updates, execution, and tests.
- `crates/ajax-core/src/registry.rs`: about 1,927 lines. It owns the documented
  registry boundary, but also carries SQLite schema, encoding, event parsing,
  snapshot import/export, and many persistence tests.
- `crates/ajax-core/src/adapters.rs`: about 1,083 lines. It contains command
  specs/runners plus git, tmux, and agent command construction/parsing.

These are modular-monolith refactor candidates because the problem is mostly
inside modules, not between crates.

### Existing Boundaries To Preserve

The documented crate responsibilities remain sound:

- `ajax-cli`: CLI parsing, dispatch, rendering, context loading, and process
  execution wiring.
- `ajax-core`: models, policy, live status, registry state, command planning,
  and output contracts.
- `ajax-supervisor`: process supervision and translation of live process events
  into Ajax monitor events.
- `ajax-tui`: Cockpit state, input, layout, and rendering over core responses.

The new architecture rule should be applied inside these crates before changing
crate boundaries. In particular, the existing `Registry` name is not a generic
abstraction smell in this project; it is a documented domain/persistence
boundary. Do not delete or rename it just because `AGENTS.md` discourages
inventing generic registries.

### Responsibility Leakage

Research commands found the following boundary risks:

- Direct task field mutation appears in command, CLI, cockpit backend, event,
  live, registry, and test code. Production direct mutation should shrink toward
  lifecycle and registry-level mutation functions.
- `ajax-cli` still mutates substrate evidence in execution and cockpit refresh
  paths, including `git_status`, `tmux_status`, and `worktrunk_status`.
- `ajax-core::commands` mixes use-case orchestration, domain decisions, command
  construction, git evidence interpretation, cleanup safety application, and
  execution.
- `ajax-core::adapters` mixes command runner ports with concrete git/tmux/agent
  adapter logic.
- `ajax-tui::lib` mixes app state, event processing, layout math, rendering,
  and terminal runtime.
- Source-text tests still inspect implementation strings in several places.
  Some are temporary architecture guardrails, but many should become behavior
  tests or disappear after the underlying module split is complete.

### Configuration And Execution Questions

The current config model includes `launchers` and `cleanup` fields. README only
documents managed repos and test commands in the minimal config. Before wiring
or removing config fields, inspect user-facing docs and tests and decide whether
the field is a real feature or inert shape.

`CommandMode::Spawn` exists in `architecture.md` and `ajax-core::adapters`.
Before removing it, check whether detached execution is an intended boundary for
supervision or Cockpit. If the mode stays, a production vertical slice should use
it. If it goes, update `architecture.md` in the same approved architecture work.

## Target Architecture

Ajax should become a restrained ports-and-adapters modular monolith without
throwing away the current crate layout.

### Layer Mapping

Use the new `AGENTS.md` rule as the direction of travel:

- `cli/`: command-line argument definition and parsing only. In today's repo
  this mostly maps to `ajax-cli/src/cli.rs`.
- `app/`: command/use-case orchestration. In today's repo this maps to the
  public command facade in `ajax-core::commands` plus CLI execution dispatch
  glue. Over time, command use cases should be split by vertical slice.
- `domain/`: core types and business rules. In today's repo this maps to
  `models`, `lifecycle`, `policy`, `operation`, and typed events.
- `analysis/`: checking, scanning, projection, and evaluation logic. In today's
  repo this maps to attention derivation, live-status reduction, git evidence
  interpretation, cockpit projections, and doctor checks.
- `ports/`: small traits only for external boundaries. Today these are mostly
  `CommandRunner`, `RegistryStore`, and Cockpit callback traits.
- `adapters/`: filesystem, terminal, JSON, subprocess, environment, git, tmux,
  agent CLI, and SQLite access.
- `tests/`: user-visible behavior. Because repo instructions forbid editing
  `tests/` unless explicitly asked, code tasks should first use crate-local unit
  or module tests. Add or change integration tests only when the approved task
  explicitly includes that scope.

This mapping does not require creating every directory at once. Each approved
code task should move one vertical slice or boundary at a time.

### Anti-Goals

- Do not rewrite Ajax into another application framework.
- Do not split the project into more crates just to make files smaller.
- Do not invent generic module names such as `manager`, `service`, `processor`,
  `handler`, `factory`, `helper`, or `utils`.
- Do not introduce traits where a concrete function or struct is enough.
- Do not make CLI parsing, filesystem access, terminal output, subprocess
  execution, SQLite, or JSON rendering part of domain or analysis code.
- Do not change runtime behavior during mechanical moves.
- Do not delete architecture guardrails before equivalent behavior coverage is
  in place.

## Refactor Strategy

Use small vertical slices. Each code task should be 5-15 minutes when possible
and should state:

- the failing test to write,
- the smallest implementation change,
- the focused verification command,
- the full validation command required before the phase is considered done.

Move code only when the destination responsibility is clear. If a planned move
requires broad renaming or a generic abstraction layer, stop and re-plan.

## Phase 1: Baseline And Test Shape

### Section: Baseline Validation

#### Task 1: Capture Current Health

- Test to write: none; baseline validation only.
- Code to implement: none.
- How to verify: run:
  - `cargo fmt --check`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features`
- Acceptance criteria: record failures before refactoring so later tasks do not
  hide existing breakage.

### Section: Brittle Test Inventory

#### Task 2: Catalog Source-Text Tests

- Test to write: none.
- Code to implement: none.
- How to verify: use `rg` to list tests that read source files or docs with
  `std::fs::read_to_string`, `include_str`, or string assertions against module
  names.
- Acceptance criteria: each source-text test is classified as one of:
  architectural guardrail, behavior proxy, packaging policy, or stale cleanup
  test.

#### Task 3: Replace One Behavior Proxy

- Test to write: a crate-local behavior test for the same user-visible or API
  contract currently protected by one source-text assertion.
- Code to implement: remove only the redundant source-text assertion after the
  behavior test fails and passes.
- How to verify: run the focused package test, then
  `cargo nextest run --all-features`.
- Acceptance criteria: no assertion is weakened; behavior is covered by the new
  test before source-text coverage is removed.

## Phase 2: CLI Boundary Cleanup

### Section: CLI Parsing Only

#### Task 4: Keep `cli.rs` As Argument Definition

- Test to write: focused parse test proving the selected command still parses
  with the same flags and error behavior.
- Code to implement: if any command-specific execution logic lives in parsing
  code, move it to the relevant app/use-case dispatch module.
- How to verify: `cargo nextest run -p ajax-cli cli`.
- Acceptance criteria: `ajax-cli/src/cli.rs` contains Clap command shape and
  parse behavior only.

### Section: Process Entrypoint

#### Task 5: Add Broken Pipe Output Boundary

- Test to write: unit test around a concrete output-writing function that treats
  `BrokenPipe` as successful process termination.
- Code to implement: move stdout/stderr writing out of `main` into a callable
  boundary function without moving domain logic into `main`.
- How to verify: focused `ajax-cli` test plus `cargo nextest run -p ajax-cli`.
- Acceptance criteria: normal output behavior is unchanged and piping to a
  closed reader does not print a noisy error.

### Section: Rendering Boundary

#### Task 6: Quote Rendered Plans Safely

- Test to write: render a command plan containing spaces and shell
  metacharacters and assert the human output is copy-paste safe.
- Code to implement: keep shell quoting in the render boundary. Do not push
  terminal output concerns into core domain code.
- How to verify: focused render test plus affected plan-output tests.
- Acceptance criteria: JSON command output remains structured; human rendering
  is safe to copy.

## Phase 3: App Use-Case Decomposition

The public `ajax-core::commands` module may remain the facade, but its internals
should become small use-case modules. Prefer concrete functions over traits.

### Section: New Task Vertical Slice

#### Task 7: Extract New Task Planning

- Test to write: focused test proving `new_task_plan` still validates repo,
  slugifies titles, builds branch/worktree/tmux names, and preserves duplicate
  handle behavior.
- Code to implement: move `NewTaskRequest`, `new_task_plan`,
  `task_from_new_request`, `record_new_task`, slug creation, and worktree path
  construction into `commands/new_task.rs` or a similarly concrete vertical
  slice module.
- How to verify: `cargo nextest run -p ajax-core new_task`.
- Acceptance criteria: `commands.rs` re-exports or delegates; public API remains
  stable.

#### Task 8: Move New-Task Execution State Effects Out Of CLI Dispatch

- Test to write: executing new-task provisioning records task creation,
  worktree/branch evidence, tmux evidence, and failure status after each
  successful step.
- Code to implement: add concrete core/app functions for applying provisioning
  step outcomes; call them from `ajax-cli` execution dispatch.
- How to verify: focused `new_execute_*` tests and new core tests.
- Acceptance criteria: CLI dispatch wires IO and persistence, while state
  mutation rules live in core app/domain functions.

### Section: Merge Vertical Slice

#### Task 9: Extract Merge Planning And Result Updates

- Test to write: focused merge tests for clean evidence, missing evidence,
  conflict failure, confirmation requirements, and mergeable lifecycle.
- Code to implement: move merge plan construction, preflight reasons, and merge
  result updates into a merge use-case module.
- How to verify: `cargo nextest run -p ajax-core merge`.
- Acceptance criteria: merge safety rules stay in core and execution stays in
  the runner boundary.

### Section: Cleanup And Remove Vertical Slice

#### Task 10: Extract Teardown Planning

- Test to write: cleanup, clean, remove, and sweep tests proving distinct
  confirmation, command, and lifecycle behavior.
- Code to implement: move cleanup/remove/sweep planning and cleanup-step result
  application into a teardown use-case module.
- How to verify: focused cleanup/remove/sweep tests.
- Acceptance criteria: destructive command plans still require fresh evidence
  and explicit confirmation when risky.

### Section: Open, Trunk, Check, Diff Slices

#### Task 11: Split Remaining Task Use Cases

- Test to write: one focused behavior test for the use case being moved:
  open/trunk/check/diff.
- Code to implement: move one use case at a time into a concrete module named
  for the action.
- How to verify: run focused package tests for the moved action.
- Acceptance criteria: no large shared generic layer is introduced; each module
  has one clear command/use-case responsibility.

## Phase 4: Domain And Analysis Boundaries

### Section: Lifecycle Mutation Authority

#### Task 12: Remove Production Direct Lifecycle Assignment

- Test to write: regression test for the lifecycle transition being changed,
  including the error path when the transition is invalid.
- Code to implement: replace production direct `task.lifecycle_status = ...`
  with lifecycle functions or registry-level transition functions. Test fixture
  setup may remain direct if that keeps tests readable.
- How to verify: `cargo nextest run -p ajax-core lifecycle`.
- Acceptance criteria: production lifecycle changes flow through the documented
  lifecycle boundary.

### Section: Substrate Evidence Mutation

#### Task 13: Route Git/Tmux/Worktrunk Updates Through Concrete Functions

- Test to write: substrate evidence updates record the expected registry events
  and side-flag changes.
- Code to implement: replace production direct `git_status`, `tmux_status`, and
  `worktrunk_status` mutation with concrete domain/app functions or existing
  registry functions.
- How to verify: registry event tests plus affected command execution tests.
- Acceptance criteria: evidence updates are visible, evented when required, and
  not duplicated across CLI, cockpit backend, and command modules.

### Section: Git Evidence Analysis

#### Task 14: Extract Git Evidence Interpretation

- Test to write: table-driven tests for clean, dirty, conflicted, unpushed,
  missing branch, and partial status output.
- Code to implement: move git-status-to-task interpretation out of command
  orchestration into an analysis-focused module or concrete function.
- How to verify: `cargo nextest run -p ajax-core git_evidence`.
- Acceptance criteria: command use cases ask for evidence analysis; they do not
  embed the analysis logic.

### Section: Attention And Live Projections

#### Task 15: Keep Attention And Live Status Pure

- Test to write: projection tests proving attention/live status consume task
  state and observations without mutating lifecycle.
- Code to implement: move any mutation needed by observation application into a
  concrete mutation boundary; keep projection functions pure where practical.
- How to verify: `cargo nextest run -p ajax-core attention live`.
- Acceptance criteria: analysis code evaluates state; app/domain mutation code
  changes state.

## Phase 5: Ports And Adapters Cleanup

### Section: Command Runner Port

#### Task 16: Split Runner Port From Concrete Adapters

- Test to write: existing command runner tests plus a focused test proving each
  `CommandMode` maps to the intended process behavior.
- Code to implement: separate the small `CommandRunner`/`CommandSpec` port from
  concrete process, git, tmux, and agent adapters. Avoid a generic adapter
  factory.
- How to verify: `cargo nextest run -p ajax-core adapters`.
- Acceptance criteria: domain/app code depends on the small port; subprocess
  execution stays in the adapter layer.

### Section: Git, Tmux, Agent Adapters

#### Task 17: Split Concrete Adapter Modules

- Test to write: behavior tests for the adapter being moved, such as git status
  parsing or tmux command construction.
- Code to implement: move one concrete adapter at a time into a named module:
  git, tmux, or agent. Keep command construction concrete.
- How to verify: focused adapter tests.
- Acceptance criteria: no behavior change and no new generic abstraction.

### Section: Filesystem And Environment Adapters

#### Task 18: Keep Filesystem And Environment Out Of Domain

- Test to write: context-loading tests proving config/state paths and corrupted
  state errors remain operator-facing.
- Code to implement: keep `std::fs`, env var reads, and terminal/environment
  detection in CLI or adapter modules.
- How to verify: `cargo nextest run -p ajax-cli context`.
- Acceptance criteria: core domain/analysis modules do not gain filesystem or
  environment access during refactors.

## Phase 6: Registry And Persistence Boundary

The existing `Registry` is a real domain/persistence boundary. The plan is to
sharpen it, not replace it with a generic storage layer.

### Section: Registry Facade

#### Task 19: Split Registry Encoding From Registry Behavior

- Test to write: SQLite round-trip tests for tasks, events, side flags,
  metadata, and unsupported schema versions.
- Code to implement: keep `Registry`, `InMemoryRegistry`, and `RegistryStore`
  public, but move SQLite schema/row encoding into concrete persistence-focused
  modules if that reduces `registry.rs` without changing API behavior.
- How to verify: `cargo nextest run -p ajax-core registry`.
- Acceptance criteria: registry behavior remains typed; SQLite remains an
  adapter/persistence detail.

### Section: Event Recording

#### Task 20: Centralize Evented Updates

- Test to write: lifecycle and substrate updates record expected event kinds in
  the expected order.
- Code to implement: route event-worthy mutations through registry functions or
  concrete domain functions that are called by registry functions.
- How to verify: registry event tests and affected command tests.
- Acceptance criteria: direct mutation no longer bypasses event recording in
  production code.

### Section: Snapshot Export

#### Task 21: Keep JSON Export At The Boundary

- Test to write: state export produces valid JSON and refuses to overwrite
  existing output.
- Code to implement: keep JSON serialization for operator export at the adapter
  or output boundary; do not make domain logic depend on JSON value handling.
- How to verify: focused state export tests.
- Acceptance criteria: durable SQLite remains the runtime store; JSON remains an
  export format.

## Phase 7: TUI And Cockpit Modularization

### Section: Cockpit State

#### Task 22: Extract Cockpit State Transitions

- Test to write: focused tests for changing project, task action menu, new-task
  input, help view, and confirmation state.
- Code to implement: move `AppView`, selectable construction, and state
  transition logic into a cockpit state module.
- How to verify: `cargo nextest run -p ajax-tui cockpit`.
- Acceptance criteria: state transitions are testable without terminal runtime.

### Section: Cockpit Input

#### Task 23: Extract Event-To-Action Logic

- Test to write: focused key/mouse tests proving the same events produce the
  same action outcomes.
- Code to implement: move event classification and input handling into a
  concrete input module. Do not introduce a generic event processor.
- How to verify: focused TUI input tests.
- Acceptance criteria: terminal reading remains in the runtime boundary; input
  decisions are pure enough to test.

### Section: Cockpit Rendering

#### Task 24: Extract Layout And Rendering Units

- Test to write: layout tests for constrained terminal sizes and rendering tests
  for status/action labels.
- Code to implement: move layout math and rendering functions into named modules
  that match actual responsibilities.
- How to verify: focused TUI rendering/layout tests.
- Acceptance criteria: rendering consumes core JSON/output contracts and does
  not depend on internal domain mutation details.

### Section: Cockpit Runtime Boundary

#### Task 25: Keep Terminal IO At The Edge

- Test to write: smoke-level test or focused runtime test for starting and
  exiting the TUI where practical.
- Code to implement: keep raw mode, alternate screen, event polling, and
  terminal backend wiring isolated from state and rendering logic.
- How to verify: `cargo nextest run -p ajax-tui`.
- Acceptance criteria: terminal IO is adapter/runtime code, not app or domain
  logic.

## Phase 8: Config, Paths, And Execution Semantics

### Section: Launcher Config

#### Task 26: Remove Or Wire Launchers

- Test to write: if wiring, prove the selected launcher command is used for the
  agent. If removing, prove documented config parsing no longer exposes inert
  launchers.
- Code to implement: choose the smaller behavior-preserving path after
  inspecting README, `architecture.md`, and config tests.
- How to verify: config tests and new-task execution tests.
- Acceptance criteria: config does not advertise unused features.

### Section: Cleanup Rules Config

#### Task 27: Remove Or Wire Cleanup Rules

- Test to write: if wiring, prove cleanup policy honors the configured rule. If
  removing, prove the documented config shape no longer includes inert cleanup
  fields.
- Code to implement: either pass cleanup rules into concrete cleanup policy or
  remove the unused config surface.
- How to verify: cleanup policy tests and config parsing tests.
- Acceptance criteria: cleanup behavior and documentation agree.

### Section: CommandMode::Spawn

#### Task 28: Decide Spawn Semantics

- Test to write: if keeping, prove a production command uses spawn for detached
  execution. If removing, prove no command plan can request spawn.
- Code to implement: wire `CommandMode::Spawn` into a real use case or remove
  it from code and documentation in one approved architecture change.
- How to verify: adapter tests, command plan tests, and `architecture.md`
  readback.
- Acceptance criteria: execution modes in code and architecture docs match.

### Section: Path And Argument Correctness

#### Task 29: Separate Display Paths From Execution Paths

- Test to write: path-with-space command args and cwd values survive command
  construction and execution planning.
- Code to implement: move toward `PathBuf`/`OsString` at internal execution
  boundaries where practical, while keeping JSON/rendering display strings at
  output boundaries.
- How to verify: adapter tests and command plan tests.
- Acceptance criteria: command execution does not rely on display-formatted
  paths.

## Phase 9: Architecture Sync And Final Validation

### Section: Architecture Documentation

#### Task 30: Update `architecture.md`

- Test to write: none; markdown-only change.
- Code to implement: update documented module ownership only after an approved
  architecture change is fully implemented.
- How to verify: read/search check that `architecture.md`, `AGENTS.md`, and the
  finished code agree.
- Acceptance criteria: docs reflect the actual architecture, not aspirational
  names that are not present in code.

### Section: Full Validation

#### Task 31: Run Required Validation

- Test to write: none.
- Code to implement: none.
- How to verify: run:
  - `cargo fmt --check`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features`
- Acceptance criteria: report every failed command exactly. Do not claim a check
  passed unless it was actually run.

## Stop Conditions

Stop and re-plan before implementation if a task requires any of the following:

- changing public CLI behavior without an explicit product decision,
- moving logic across crate boundaries,
- adding a new dependency,
- introducing a new trait outside an IO/test/swap boundary,
- creating a generic module name discouraged by `AGENTS.md`,
- editing files under `tests/` without explicit approval,
- removing source-text architecture guardrails before equivalent behavior tests
  exist,
- changing persistence schema or state compatibility without an explicit
  migration/no-migration decision,
- changing `architecture.md` direction before the code change is complete.

## Definition Of Done

A phase is complete only when:

- every task in the phase has a focused test or an explicit markdown-only
  exemption,
- moved code has one clear responsibility at the destination,
- public CLI and JSON contracts are unchanged unless the task explicitly changes
  them,
- domain/analysis code does not gain IO, terminal, subprocess, filesystem, or
  environment access,
- new abstractions are concrete and named after real responsibilities,
- `architecture.md` is updated if module ownership or direction changed,
- the strongest applicable validation has been run and reported.
