# AGENTS.md

Shared repository contract for coding agents working in Ajax. Keep this file
concise, durable, Ajax-specific, and independent of any one harness.

## Scope and instruction priority

Follow instructions in this order:

1. Explicit user instruction.
2. This `AGENTS.md`.
3. Root `architecture.md`, then the focused document for the subsystem being
   changed.
4. Existing source and tests.
5. Generated summaries, code maps, or prior plans.

When instructions conflict, preserve the safest behavior and identify the
conflict. The active agent remains responsible for investigation, engineering
decisions, review, and verification, including delegated work.

Before editing, inspect the relevant source files and tests. Treat source and
tests as authoritative over generated summaries.

## Conditional reading

Do not load every linked document for every task. Read the applicable document
before editing:

- For task lifecycle, registry truth, runtime reconciliation, substrate
  evidence, terminal/session behavior, command execution, Cockpit behavior,
  security assumptions, cross-crate boundaries, public CLI/API behavior,
  operator slices, shared-kernel admission, or dependency direction, read
  `architecture.md`, then the focused document selected by its navigation map.
- For Web Cockpit work, also read `docs/architecture/web-cockpit.md`; for its
  optional orchestration session, also read
  `docs/architecture/web-session-behavior.md`.
- For Rust changes, read `docs/agent/rust.md`.
- For confirmed product defects, read `docs/defect-process.md`.
- Before delegation, read `docs/agent/routing.md`.
- For explicitly requested plans, multiple dependent implementation steps,
  durable handoffs, architecture changes, or security-sensitive changes, read
  `docs/agent/plans.md` and create the required persistent plan.
- Before creating or retitling a PR or changing CI/release behavior, read
  `docs/agent/pull-requests.md`.

For architecture or security changes, create a written plan and wait for
approval unless the user explicitly requested immediate implementation. Update
the owning architecture documentation in the same change.

## RTK

When RTK and its guidance are available through the repository, active harness,
or local environment, read that guidance and use RTK for the shell commands it
covers. Do not assume a Codex-only location or invent missing RTK behavior.

RTK must not be required for CI, remote agents, or environments where it is
unavailable. No local-machine-only file may be required for repository
correctness or remote execution.

## Ajax ownership boundaries

- Core owns task truth.
- UI presents task truth.
- CLI dispatches commands.
- Supervisor observes and reports execution.
- Browser code must not become an alternate registry, policy engine, lifecycle
  owner, or task source of truth.
- Runtime state must reconcile through core/backend contracts.

Do not bypass lifecycle, registry, command, task-operation, or
runtime-reconciliation boundaries. Git, tmux, and supervised processes remain
authoritative for their own reality; browser and SQLite projections must not
reinterpret that evidence as a second source of truth.

If a change blurs these boundaries, treat it as an architecture change.

## Universal safety

- Make the smallest safe change that satisfies the request and preserve existing
  behavior unless the task explicitly changes it.
- Do not weaken, delete, skip, or rewrite tests merely to make a change pass.
  Fix implementation failures rather than weakening assertions.
- Do not claim validation passed unless the command actually ran and passed.
  Never hide failed commands; report failures and skipped checks with reasons.
- Do not accidentally change public behavior or preserve a removed public
  contract without an explicit compatibility requirement.
- Do not add generated code, large snapshots, or lockstep rewrites unless the
  task requires them.
- Update the owning documentation when behavior, commands, architecture, or
  workflows change.

A confirmed Ajax product defect must have an existing or newly opened
GitHub issue on `mossipcams/ajax-cli` and a focused regression test that would have
failed on the defect. Use only the documented untestable exception in
`docs/defect-process.md`, including its required issue and PR explanation. Do
not silently fix a confirmed defect or treat chat, plans, or local TODOs as the
tracking system.

## Rust file-size limit

Keep handwritten Rust source files near 600 lines. The hard maximum is 1,000
lines per `.rs` file on disk, including inline tests, and new features must not
land in an already over-limit file. Split only by cohesive responsibility; see
`architecture.md` and `docs/agent/rust.md` for the focused rules.

## Delegation

Always use the `model-router` skill for implementation writes. Do not spawn
native harness subagents (Cursor Task, best-of-n, Claude/Codex task children,
or pstack explorers) as a substitute. The orchestrator writes plans when
required, emits one `EXECUTION` decision, and reviews delegate work. It does
not explore the tree, implement, commit, push, or open pull requests.

Call `model-router` for every implementation write. Dispatch through acpx
(`scripts/run-delegate`). A Cursor delegate must implement in-process.
Missing `acpx` is stop, not a license to Task or parent-local writes. Do not
duplicate model rankings or exact model IDs in this file.

If the user explicitly approved bypassing delegation for this request, the
active agent may implement, commit, push, and open pull requests in-process.
That approval is per-request; it does not change the default.

When the user asks to create a PR, the selected delegate runs the repository's
local verification gate, commits, pushes, and runs `scripts/gh-pr-create`; the
orchestrator reports the PR URL after reviewing the delta. After an explicit
bypass, the active agent does that same PR path in-process. Delegates must not
merge, rebase, force-push, or switch branches unless the user explicitly
authorizes that behavior.

Every delegated task must be bounded by scope, acceptance criteria,
verification, and stop conditions. The active agent must inspect the actual
delta, confirm scope, and independently accept or reject the result. A delegate
report is evidence, not approval.

Harness-specific workflows are optional. They cannot override repository
requirements or become dependencies for other harnesses. Detailed routing,
work-order, and review rules live in `docs/agent/routing.md`.

## Verification and reporting

Evidence that a change works is required; test-first development is not a
repository-wide mandate. Prefer focused verification first, then the strongest
practical broader checks for the risk: existing tests, focused regression
coverage, compiler/lint checks, browser/manual checks, or the slice and full
commands documented in `architecture.md` and `README.md`.

Add or update tests when they best lock behavior or prevent regression. Do not
add meaningless tests that only assert implementation details. Compiler or lint
coverage is normally enough for mechanical documentation, formatting, comment,
pure-rename, or proven dead-code changes.

If validation cannot run because of missing tools, environment limits, time, or
unrelated failures, report the exact command and result. Final reports must say
what changed, identify the persistent plan and checklist status (or state that
none was used), list verification and its results, disclose failed or skipped
commands, and state remaining risks or follow-up work. Do not claim the
repository is clean unless status was checked.

## Stop conditions

Stop and request direction before an unapproved change that would:

- delete user data or perform another materially destructive action;
- change task lifecycle semantics, registry truth, ownership boundaries, or
  runtime authority;
- replace the terminal model;
- add public network exposure or change authentication/security assumptions;
- remove a public command or documented behavior;
- perform a large rewrite not explicitly requested.

Do not stop for routine, bounded work unless the user requested an approval
gate. Harness-specific playbooks may add safer pauses but may not weaken these
conditions.

## Maintaining this contract

Keep one root `AGENTS.md`. Retain here only Ajax-specific rules needed for nearly
every task; put conditional runbooks in the owning architecture, defect,
verification, or shared `docs/agent/` document. Do not place shared requirements
only in harness-specific configuration.
