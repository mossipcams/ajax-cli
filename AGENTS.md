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

Choose the smallest mode that fits the request. For code-changing Small Fix and
Behavior Change work, delegation is the default execution path. Apply the
Delegation rules before editing source.

## Persistent Plans

For any code change, create a repo-local Markdown plan before editing source.
Use `.planning/agent-plans/<short-slug>.md` unless a task-specific planning
directory already exists. Keep the file small, concrete, and current.

Each plan must include:

- scope and non-goals
- a task checklist with the test, implementation, and verification for each task
- approval status when approval is required
- deviations discovered during execution
- validation commands and results

Use the plan as the execution ledger. Check off each task as it completes, note
failed commands or changed assumptions where they happen, and keep the checklist
aligned with the actual work. If the plan changes materially, update the file
before continuing and call out the change to the user.

For trivial mechanical changes where new tests are not meaningful, still create
the plan file and explicitly record why tests are skipped.

### Planning-Only

Use when the user asks for a plan, review, critique, or design.

- Inspect relevant files.
- Produce a concrete plan and save it in the persistent plan file.
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

## Model Routing

Use the `model-router` skill for model, lane, and delegate-tool decisions. Do
not duplicate model rankings, model preferences, or lane-selection rules in this
file.

## Delegation

Default rule: bounded code changes are delegated. A user request like “fix,”
“implement,” “change,” “add,” or “update” is authorization to delegate unless
the user explicitly says not to delegate.

For any code change, create or update the persistent plan and record one
delegation decision before editing source:

- `Delegation decision: delegated via model-router`
- `Delegation decision: not delegated because <specific allowed exception>`

You stay the planner, reviewer, and final approver. Strict workflow:

1. Create or update the persistent plan.
2. Make and record the delegation decision.
3. When delegating implementation, create a complete
   `tdd-implementation-packet` as the source of truth.
4. Delegate via `model-router`; let it choose the model, lane, and tool.
5. Review the diff.
6. Run validation personally; do not trust the delegate's claim alone.
7. Accept, reject, or send a focused `resume` order.

Never delegate implementation from a vague prompt.

Delegation quality lives in the prompt. Give the delegate a work order, not a
wish:

- name the files and code paths to touch
- state the expected behavior and the tests to add or update
- state what must not change (public behavior, unrelated files, architecture)
- include the validation commands from this file that the delegate should run

One bounded task per delegation. Split larger work into sequential `implement` →
`resume` rounds rather than one broad prompt.

Review before accepting, for every delegation:

1. Read the diff and check it against the requested scope.
2. Confirm tests were added or updated when applicable.
3. Run validation yourself. An empty diff plus a success claim is a failure.
4. Send unrelated or overly broad edits back via `resume` instead of quietly
   fixing them.
5. Never commit, push, or report done solely because the delegate finished.
   Delegates never commit, push, merge, rebase, or change branches.

Do not implement directly unless one of these exceptions applies:

- The user explicitly says not to delegate.
- The change is truly smaller than the work order needed to describe it, such as
  a one-line typo, formatting-only edit, or comment-only correction.
- The work is non-code, pure Q&A, planning-only, or review-only.
- The `model-router` skill or its selected delegation tool is unavailable. In
  that case, report the unavailable tool instead of silently taking over.
- The task is on the do-not-delegate list below.

Do-not-delegate list:

- vague discovery or broad architecture planning
- large refactors without a written plan
- security-sensitive changes without human review
- tasks requiring credentials or private external access
- changes outside the current worktree

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

### Naming conventions (commits and PR titles)

Ajax uses Conventional Commits. **PR titles** are enforced by CI; commit
messages should use the same vocabulary so Release Please can build
`CHANGELOG.md`.

Sources of truth (keep this section aligned when either changes):

- Allowed PR types: `.github/workflows/ci.yml` → `pr-title` job `types`
- Changelog types: `release-please-config.json` → `changelog-sections`
- Release PR title pattern: `release-please-config.json` →
  `pull-request-title-pattern` (`chore: release ajax-cli <version>`)

Allowed types:

| Type | PR title | Release Please changelog | Use for |
| --- | --- | --- | --- |
| `feat` | yes | Features | user-visible feature |
| `fix` | yes | Bug Fixes | bug fix |
| `perf` | yes | Performance Improvements | performance improvement |
| `refactor` | yes | Code Refactoring | behavior-preserving restructure |
| `revert` | yes | Reverts | revert of a prior change |
| `chore` | yes | no (intentional) | tooling, tests-only cleanup, docs/agent hygiene; does **not** bump a release |

Format: `type(optional-scope): summary` — e.g. `fix(web): …`, `chore(test): …`.

Hard rules:

- Do **not** use `test:`, `docs:`, `ci:`, `build:`, `style:`, or any type
  outside the table. The `PR Title` check fails with `Unknown release type`
  and skips the rest of CI.
- Tests-only or local-suite cleanup → `chore:` / `chore(test):`, never `test:`.
- `chore:` passes the PR Title check but does **not** bump a version or open a
  Release Please release PR. Use `feat:` / `fix:` / `perf:` / `revert:` when the
  change should cut a product release. (`chore: release ajax-cli <version>` is
  only the title pattern Release Please writes on its own release PRs.)
- Prefer a scope when it helps (`web`, `cli`, `core`, `test`).
- Before `gh pr create` or retitling, confirm the type is in the table above.

### Local verify gate (blocking)

Do not create a pull request until local tests have passed in this worktree.

Required before `gh pr create` / opening a PR:

1. Husky must be installed (`npm prepare` / `npx husky` so `.husky/pre-commit` runs).
2. The commits on the PR branch must have gone through the husky pre-commit hook
   successfully, **or** you must run the same local suite yourself and it must
   pass: `npm run verify` (what husky runs), plus the rest of `.husky/pre-commit`
   (`cargo build --release -p ajax-cli` and
   `cargo install --path crates/ajax-cli --locked --force`) when those steps did
   not already run via the hook.
3. If `prek` is available and configured for this repo, it may satisfy the same
   gate when it runs the equivalent local verify suite to success.

Hard stops:

- Do not use `--no-verify`, `--no-gpg-sign` to skip hooks, or otherwise bypass
  husky/prek just to open a PR.
- Do not open a PR after a failed verify. Fix failures first, then re-run until
  green.
- Focused crate tests alone are not enough for PR creation; the full local
  verify gate above is required.

Record the verify command(s) and exit status in the persistent plan and in the
final response.

Final response must include:

- what changed
- persistent plan file path and whether all checklist items are complete
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
