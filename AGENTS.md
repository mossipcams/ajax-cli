# AGENTS.md

Repo-level operating contract for coding agents working in Ajax.

Keep this file short, durable, and Ajax-specific. It should guide agent behavior,
not replace `architecture.md`, `CONTRIBUTING.md`, or task-specific plans.

## Instruction Priority

Follow instructions in this order:

1. Explicit user instruction
2. This `AGENTS.md`
3. `architecture.md`
4. Existing code and tests
5. Generated summaries, code maps, Graphify output, or prior plans

When instructions conflict, preserve the safest behavior and identify the
conflict. Ask only when the next step would be destructive, architectural,
security-sensitive, or user-visible in a way the request did not clearly
authorize.

## Read First

Before editing, inspect the relevant source files and tests. Do not rely only on
summaries.

Read `architecture.md` before work involving:

- task lifecycle
- registry truth
- runtime reconciliation
- substrate evidence
- terminal/session behavior
- command execution
- Cockpit or Web Cockpit behavior
- security assumptions
- cross-crate boundaries
- public CLI or API behavior

`architecture.md` is the source of truth for system design. Do not duplicate
large architecture explanations here.

## Local RTK Guidance

If available in the local environment, also consult:

```text
@/Users/matt/.codex/RTK.md
```

This is Matt's local RTK workflow guidance. It is useful for local Codex runs,
but it is not required in CI, remote clones, GitHub agents, or environments
where the file does not exist.

Do not fail, block, or invent RTK rules if this file is unavailable.

Do not make local-machine-only files required for correctness, CI, or
remote-agent execution.

## Task Modes

Choose the smallest mode that fits the request.

### Planning-Only

Use when the user asks for a plan, review, critique, or design.

- Inspect relevant files.
- Produce a concrete plan.
- Do not edit code.
- Include risks and validation strategy.

### Small Fix

Use for narrow, low-risk changes.

- Inspect the relevant code path.
- Make the smallest safe change.
- Run focused validation.
- Report exactly what changed.

### Behavior Change

Use when user-visible, CLI-visible, API-visible, or workflow behavior changes.

- Add or update a failing behavior test first.
- Make the test pass with the smallest implementation.
- Refactor only after tests are green.
- Preserve existing behavior unless the task explicitly changes it.

### Refactor or Cleanup

Use when the goal is simplification, deletion, or internal restructuring.

- Preserve behavior.
- Prefer deletion over new abstraction.
- Add characterization tests first when behavior is risky or uncovered.
- Do not invent fake tests only to satisfy process.
- Keep diffs reviewable.
- Explain why behavior is unchanged.

### Architecture Change

Use when changing ownership, boundaries, task truth, registry semantics,
terminal model, runtime authority, or security assumptions.

- Read `architecture.md`.
- Create a written plan.
- Wait for approval unless the user explicitly asked for immediate implementation.
- Update `architecture.md` in the same change when architecture changes.

## Picking the Right Models for Workflows and Subagents

Rankings, higher = better. Cost reflects what Matt actually pays (OpenAI has
generous limits), not list price. Intelligence is how hard a problem can be
handed to the model unsupervised. Taste covers UI/UX, code quality, API design,
and copy.

| model | cost | intelligence | taste |
| --- | ---: | ---: | ---: |
| gpt-5.5 | 9 | 8 | 5 |
| sonnet-5 | 15 | 5 | 7 |
| opus-4.8 | 4 | 7 | 8 |
| fable-5 | 2 | 19 | 9 |

How to apply:

- These are defaults, not limits. You have standing permission to override them:
  if a cheaper model's output does not meet the bar, rerun or redo the work with
  a smarter model without asking. Judge the output, not the price tag.
  Escalating costs less than shipping mediocre work.
- Cost is a tie-breaker only; when axes conflict for anything that ships,
  intelligence > taste > cost.
- Bulk/mechanical work (clear-spec implementation, data analysis, migrations):
  gpt-5.5 - it is effectively free.
- Anything user-facing (UI, copy, API design) needs taste >= 7.
- Reviews of plans/implementations: fable-5 or opus-4.8, optionally gpt-5.5 as
  an extra independent perspective.
- Never use Haiku.
- Mechanics: gpt-5.5 is only reachable through the Codex CLI - `codex exec` /
  `codex review` (Matt's `~/.codex/config.toml` defaults to gpt-5.5). Use the
  codex-implementation, codex-review, and codex-computer-use skills; for work
  they do not cover (investigation, data analysis), run
  `codex exec -s read-only` directly with a self-contained prompt.
- Claude models (sonnet-5, opus-4.8, fable-5) run via the Agent/Workflow model
  parameter.
- Using gpt-5.5 inside workflows and subagents (the model parameter only takes
  Claude models, so use a wrapper): spawn a thin Claude wrapper agent with
  `model: sonnet`, `effort: low` whose prompt instructs it to write a
  self-contained Codex prompt, run `codex exec` via Bash, and return the result.

## Non-Negotiable Rules

- Do not weaken, delete, skip, or rewrite tests just to make a change pass.
- Do not claim validation passed unless the command actually ran and passed.
- Do not hide failed commands.
- Do not introduce broad generic abstractions without concrete need.
- Do not preserve dead code for hypothetical future use.
- Do not accidentally change public behavior.
- Do not move task truth into UI code.
- Do not bypass lifecycle, registry, command, or runtime-reconciliation boundaries.
- Do not add generated code, large snapshots, or lockstep rewrites unless required.
- Do not perform broad rewrites when a small behavior-preserving change would
  solve the task.

## Ajax Architecture Guardrails

Do not re-explain Ajax architecture here. Use `architecture.md` for that.

Keep these guardrails in mind:

- Core owns task truth.
- UI presents task truth.
- CLI dispatches commands.
- Supervisor observes and reports execution.
- Browser code must not become an alternate registry, policy engine, lifecycle
  owner, or task source of truth.
- Runtime state must reconcile through core/backend contracts.

If a change blurs these boundaries, treat it as an architecture change.

## Web Cockpit Guardrails

Web Cockpit exists to make Ajax usable from a browser, especially normal iOS
Safari.

Do not change these without explicit approval:

- raw xterm/tmux-first terminal behavior
- normal iOS Safari as the target browser mode
- no Home Screen PWA dependency
- no service worker/offline mutation model
- no browser-owned task records
- no Live/snapshot/composer terminal model as the default path
- no public-internet product path unless the security model is explicitly changed

Web Cockpit should feel immediate and mobile-friendly, but correctness comes from
backend/core contracts.

## TDD and Testing Policy

Behavior changes require behavior tests.

Default loop:

1. Add or update a focused failing test.
2. Run the focused test and confirm it fails for the expected reason.
3. Implement the smallest change.
4. Run the focused test again.
5. Run broader validation appropriate to the touched area.
6. Refactor only after green.

For refactors:

- Confirm existing tests pass when practical.
- Add characterization tests before touching risky uncovered behavior.
- Do not add meaningless tests that assert implementation details.
- Do not rewrite large areas without proving behavior preservation.

Mechanical changes may skip new tests only when behavior cannot change, such as:

- formatting
- import cleanup
- comments or docs
- pure renames with compiler coverage
- dead-code deletion proven unused
- moving code without logic changes

When skipping new tests, explain why.

## Validation Commands

Prefer focused validation first, then broader checks.

Common commands:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Use narrower commands when appropriate:

```bash
cargo nextest run -p ajax-core
cargo nextest run -p ajax-cli
cargo nextest run -p ajax-web
cargo test -p <crate> <test_name>
```

If nextest is unavailable, use `cargo test` and say so.

If validation cannot run because of missing tools, environment limits, time, or
unrelated existing failures, report that clearly. Include the exact command and
result.

## Rust Conventions

Prefer existing Ajax patterns over new frameworks or wrappers.

Rules:

- Prefer concrete functions and structs.
- Add traits only for real external boundaries, test seams, or multiple
  implementations.
- Prefer explicit domain names over generic manager, service, handler, or util
  names.
- Prefer `Result` with useful context over panics.
- Avoid `unwrap` and `expect` in production code unless the invariant is obvious
  and local.
- Avoid `unsafe`.
- Avoid unnecessary cloning.
- Keep ownership simple.
- Keep modules understandable without creating abstraction layers for their own
  sake.
- Preserve public APIs unless the task explicitly changes them.

## Search and Code Navigation

Use fast local inspection before editing.

Preferred text search:

```bash
rg "<text>"
rg "<symbol>" crates tests
rg --files
```

Use ast-grep for syntax-aware search and structural refactors. Prefer AST-based
matching when changing Rust syntax, function calls, imports, match arms,
attributes, derives, or repeated code shapes.

Examples:

```bash
ast-grep --pattern 'fn $NAME($$$ARGS) -> $RET { $$$BODY }' --lang rust crates
ast-grep --pattern 'impl $TYPE { $$$BODY }' --lang rust crates
ast-grep --pattern '$X.unwrap()' --lang rust crates
ast-grep --pattern '$X.expect($MSG)' --lang rust crates
```

Use `rg` to find text.

Use ast-grep to inspect or change code structure.

Do not perform broad regex rewrites when an AST-aware search would be safer.

Generated maps, summaries, and Graphify output are useful for orientation, but
source files and tests are authoritative.

## Dependency Policy

Do not add dependencies casually.

Before adding a dependency, check whether the repo already has an equivalent
capability. Prefer the standard library or existing dependencies when
reasonable.

A new dependency must have a concrete reason:

- it removes meaningful custom code
- it improves correctness
- it is already common in the workspace
- it is required for an explicit integration

Do not add a dependency only to make implementation easier.

## Cleanup Policy

Ajax should become smaller and clearer over time.

When cleaning up:

- delete unused code
- collapse duplicate paths
- remove stale feature branches in code
- simplify naming
- reduce indirection
- preserve behavior
- keep tests meaningful

Do not replace simple code with abstract code. Do not keep compatibility shims
unless they protect a real public contract.

## Documentation Policy

Update docs when behavior, commands, architecture, or workflows change.

Use the right destination:

| Content | Destination |
| --- | --- |
| architecture and ownership | `architecture.md` |
| repo-wide agent rules | `AGENTS.md` |
| contributor workflow | `CONTRIBUTING.md` |
| user-facing behavior | `README` or relevant docs |
| implementation notes | nearest module docs or focused docs file |

Do not let `AGENTS.md` become a substitute for real documentation.

## Pull Request Expectations

A completed change should be easy to review.

Final response must include:

- what changed
- tests added or updated
- validation commands run
- commands that failed or were skipped
- remaining risks or follow-up work

Do not claim the repo is clean unless you checked it.

## When to Stop

Stop and ask for direction before:

- deleting user data
- changing task lifecycle semantics
- changing registry truth
- replacing the terminal model
- adding a public network exposure path
- changing authentication or security assumptions
- removing a public command or documented behavior
- performing a large rewrite not explicitly requested

Do not stop for routine small fixes unless the user asked for approval gates.

## Maintaining This File

One root `AGENTS.md` is preferred for Ajax unless the file becomes unavoidably
too large.

Add rules only after repeated agent mistakes or clear repo-specific needs.

Before adding a rule, ask:

1. Is this specific to Ajax?
2. Is it needed on most tasks?
3. Is it not already enforced by tests, CI, lint, docs, or code?
4. Does this belong in `architecture.md`, `CONTRIBUTING.md`, or normal docs
   instead?

Keep this file compact. Remove stale, duplicated, or generic instructions when
updating it.
