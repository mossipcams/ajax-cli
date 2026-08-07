# AGENTS.md

Repo-level operating contract for coding agents working in Ajax.

Keep this file short, durable, and Ajax-specific. It should guide agent behavior,
not replace `architecture.md`, `CONTRIBUTING.md`, or task-specific plans.

## Instruction Priority

Follow instructions in this order:

1. Explicit user instruction
2. This `AGENTS.md`
3. Root `architecture.md`, then the focused doc under `docs/architecture/` for
   the subsystem being changed
4. Existing code and tests
5. Generated summaries, code maps, Graphify output, or prior plans

When instructions conflict, preserve the safest behavior and identify the
conflict. Ask only when the next step would be destructive, architectural,
security-sensitive, or user-visible in a way the request did not clearly
authorize.

## Read First

Before editing, inspect the relevant source files and tests. Do not rely only on
summaries.

Read root `architecture.md` before work involving:

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
- operator-slice layout, shared-kernel admission, or dependency direction

Then open only the focused doc under `docs/architecture/` for the subsystem you
touch (see the navigation map in `architecture.md`). Prefer the owning operator
slice and its tests over loading sibling slices.

Root `architecture.md` plus those focused docs are the source of truth for
system design. Do not duplicate large architecture explanations here.

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

## Work Strategy

Choose the smallest workflow that fits the request. The active agent owns the
task, including investigation, implementation decisions, review, and
verification.

Harness-specific workflows such as pstack may provide additional playbooks or
native delegation. They are optional local capabilities. Do not fail, block, or
reproduce their rules when they are unavailable.

### Planning-Only

Use when the user asks for a plan, review, critique, investigation, or design.

* Inspect the relevant source and tests.
* Produce a concrete, evidence-backed result.
* Do not edit code unless the user also requested implementation.

### Code Change

Use for fixes, features, behavior changes, refactors, and cleanup.

* Make the smallest safe change that satisfies the request.
* Preserve existing behavior unless the task explicitly changes it.
* Verify with the strongest practical evidence.
* Tests are one verification method, not a required red-green workflow.
* Direct implementation and native delegation are both valid. Choose based on
  what helps the task rather than a repository-wide delegation requirement.

### Architecture Change

Use when changing ownership, boundaries, task truth, registry semantics,
terminal behavior, runtime authority, public contracts, or security assumptions.

* Read architecture.md and the focused subsystem documentation.
* Create a written plan.
* Wait for approval unless the user explicitly requested immediate
  implementation.
* Update the relevant architecture documentation in the same change.

### Persistent Plans

Create `.planning/agent-plans/<short-slug>.md` when:

* the user asks for a persistent plan,
* the work spans multiple dependent implementation steps,
* the change affects architecture or security,
* the task needs a durable handoff across sessions or agents.

Do not create a persistent plan for trivial, localized, or mechanical work
merely to satisfy process.

When a persistent plan is used, keep it current and include:

* scope and non-goals,
* implementation and verification tasks,
* material deviations or changed assumptions,
* validation commands and results.

### Model Routing and Delegation

The active harness owns same-harness work.

* Cursor to Cursor uses Cursor-native delegation.
* Codex to Codex uses Codex-native delegation.
* Pi to Pi uses Pi-native execution when available.
* Do not launch a second instance of the same harness through Ajax Model Router.

Use the model-router skill only when intentionally delegating to a model in a
different harness or provider subscription. Examples include Codex delegating
to Cursor, Cursor delegating to Codex, or either harness delegating to Pi.

Ajax Model Router owns:

* target-harness and model validation,
* exact provider model IDs,
* cross-harness transport,
* timeouts and cancellation,
* pre-dispatch snapshots and post-dispatch deltas,
* write-scope enforcement,
* structured delegate reports,
* verification artifacts,
* parent review bundles,
* safe restoration of rejected delegate changes.

Ajax Model Router does not own:

* engineering playbook selection,
* architecture or implementation strategy,
* whether the parent implements directly,
* same-harness delegation,
* risk-based or file-type-based model selection.

Do not duplicate provider model rankings or exact model IDs in this file. The
Ajax Model Router registry is their source of truth.

### Cross-Harness Work Orders

Every cross-harness delegation must specify:

* the target harness,
* the requested model,
* one bounded task,
* allowed files or write scope,
* observable acceptance criteria,
* relevant verification,
* explicit stop conditions.

Do not delegate a vague request. The active agent must gather enough context to
produce a bounded work order before invoking Ajax Model Router.

If the requested target or model is unavailable, stop and report that
constraint. Do not silently substitute another provider or model.

### Review Ownership

The active agent remains responsible for delegated work.

Before accepting a delegate result:

1. Inspect the actual delta.
2. Confirm the change stayed within allowed scope.
3. Check the implementation against the acceptance criteria.
4. Confirm the verification is relevant and passed.
5. Run additional focused validation when needed.
6. Reject or safely restore unrelated, incomplete, or unsupported changes.

An empty diff with a success claim is a failure. A delegate report is evidence,
not approval.

External delegates must not commit, push, merge, rebase, create branches, or
switch branches unless the user explicitly authorizes that behavior.

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
- normal iOS Safari as the target browser mode (full Cockpit without install)
- no classic PWA packaging (`manifest.webmanifest`, app icons, service worker)
- Home Screen install remains optional and is required only for Declarative Web
  Push phone pings — not for core Cockpit use
- no service worker/offline mutation model
- no browser-owned task records
- no Live/snapshot/composer terminal model as the default path
- no public-internet product path unless the security model is explicitly changed

Web Cockpit should feel immediate and mobile-friendly, but correctness comes from
backend/core contracts.

## Testing and Verification

Evidence that the change works is required. Test-first / TDD is not.

- Prefer the strongest practical verification for the change: focused tests,
  existing coverage, `cargo check` / clippy, browser/manual checks, or other
  appropriate commands.
- Add or update tests when they are the best way to lock behavior or prevent
  regressions. Do not add tests only to satisfy process.
- Do not add meaningless tests that assert implementation details.
- For mechanical changes (formatting, comments, pure renames, proven dead-code
  deletion), compiler/lint coverage is usually enough.

## Validation Commands

Prefer focused validation first, then broader checks. For routine slice work,
prefer slice-local commands before full-workspace verify (see
`architecture.md` → Validation):

```bash
npm run verify:arch
npm run verify:core
npm run verify:slice -- repair
npm run verify:slice -- operate
```

Common full-suite commands:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Use narrower crate commands when appropriate:

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
- Keep Rust source files near **~600 LOC**; hard max **1000 LOC** per `.rs`
  file on disk (including inline tests). Split by cohesive responsibility only
  — never arbitrary LOC chunks or generic util/helper modules. When a file
  grows past the limit, peel `#[cfg(test)] mod tests` into a sibling first,
  then split production code by ownership. Large operator slices may become
  directories with focused modules (`plan`, `execute`, `reduce`, `validation`).
  Do not land new features into an already over-max file. Shared-kernel
  admission and layer rules live in `architecture.md`; do not dump
  verb-specific or one-off code into the kernel.

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
| durable architecture, layers, invariants | `architecture.md` |
| subsystem architecture detail | `docs/architecture/*` |
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

### CI trigger matrix

Guarded by `scripts/verify-ci-workflows.mjs` (runs in `npm run verify`). Change
the workflows and that script together.

| Event | Runs |
| --- | --- |
| Normal PR opened/updated | full CI suite, once per head; superseded runs cancelled. CodeQL. |
| Normal PR merged to main | Release Please only. **No CI run** — see below. |
| Release Please updates its PR | `Release Candidate` job only; superseded runs cancelled. CodeQL. |
| Release Please PR merged | tag + GitHub release. No test run. |

There is no `push: main` CI run. Its absence is only safe because the `CI`
repository ruleset sets `strict_required_status_checks_policy: true`: a PR
cannot merge unless its head is already up to date with main, so the tree that
passed `CI` is the tree that lands. **If that rule is ever relaxed, restore the
`push: main` trigger in the same change.**

The generated Release Please PR skips the expensive jobs because every commit it
releases already passed the full suite on its own PR. It is not unchecked — the
`Release Candidate` job checks out the exact head SHA and verifies it merges
cleanly into current main (`git merge-tree --write-tree`), that
`.release-please-manifest.json`, `version.txt`, `crates/ajax-cli/Cargo.toml` and
the `ajax-cli` entry in `Cargo.lock` all carry one version, and that
`cargo check --locked -p ajax-cli` passes.

`release-please-config.json` bumps `Cargo.lock` in place via an `extra-files`
entry, so the release PR reaches its final SHA in one update. The jsonpath is
`$.package[?(@.name.value=="ajax-cli")].version` — `.value` is required because
release-please's TOML reader wraps each scalar in a `{start, end, value}` span.
`release-type` stays `simple`: this workspace runs ajax-cli, its sibling crates
and `workspace.package` on deliberately different versions, and the `rust`
strategy would unify them.

CodeQL uses GitHub's **default setup**, which cannot exclude a branch, so it also
scans the release PR. That is a known, accepted duplicate; excluding it would
mean hand-maintaining an advanced-setup workflow.

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
- verification used (tests, commands, or other evidence) and results
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
