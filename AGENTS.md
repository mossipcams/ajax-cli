# Workflow (STRICT)

For ANY code change, follow this exact sequence:

## Step 1: Plan
- Break work into small tasks (5-15 min each)
- For each task, specify:
  * What test to write
  * What code to implement
  * How to verify it works
- Show the complete plan
- STOP and say: "Plan ready. Approve to proceed."
- WAIT for approval

## Step 2: Execute with TDD
For each task after approval:
1. Write failing test -> run it -> show failure
2. Write minimal implementation -> run test -> show pass
3. Ask: "Task N done. Continue?" -> wait for yes

## Rules
- NEVER implement without failing test first
- NEVER skip approval step
- NEVER move to next task without asking
- Never modify files in the `tests/` directory unless explicitly asked to.
- Never delete or weaken test assertions.
- When tests fail, fix the implementation, not the tests.

@/Users/matt/.codex/RTK.md

# Strict Rust Rules for Agentic Coding

## Core Principles

1. Prefer correctness over cleverness.
2. Make the smallest safe change that solves the task.
3. Do not perform unrelated refactors.
4. Preserve existing public APIs unless explicitly asked to change them.
5. Follow the project's existing style, architecture, naming, and error-handling patterns.
6. Never hide failures. Surface them with explicit errors, tests, or documented limitations.

## Rust Safety Rules

1. Do not use `unsafe` unless explicitly required and justified.
2. If `unsafe` is unavoidable:
   - Keep it minimal.
   - Add a `// SAFETY:` comment explaining the invariant.
   - Add tests covering the safe wrapper.
3. Do not use `unwrap()`, `expect()`, or `panic!()` in production code unless:
   - The invariant is truly impossible to violate, and
   - The reason is documented.
4. Prefer `Result<T, E>` for recoverable errors.
5. Prefer `Option<T>` only when absence is expected and non-exceptional.
6. Do not ignore errors with `_`, `.ok()`, or silent fallbacks unless explicitly justified.
7. Avoid global mutable state.
8. Avoid unnecessary cloning. Use borrowing where practical.
9. Do not fight the borrow checker with poor design. Restructure ownership instead.
10. Prefer immutable variables by default.

## Error Handling

1. Use the project's existing error type and conventions.
2. For applications, `anyhow` is acceptable if already used.
3. For libraries, prefer structured errors such as `thiserror` if already used.
4. Preserve error context.
5. Do not replace meaningful errors with generic strings.
6. Do not log and swallow errors unless the caller can safely continue.

## Dependencies

1. Do not add new crates unless necessary.
2. Before adding a dependency, check whether the standard library or existing dependencies already solve the problem.
3. Prefer small, well-maintained crates.
4. Do not introduce heavy frameworks for small tasks.
5. Do not change dependency versions unless required.
6. Do not remove `Cargo.lock` for applications.
7. Use `--locked` in CI-like validation when appropriate.

## Code Style

1. Run `cargo fmt`.
2. Code must pass `cargo clippy` with warnings treated as errors when feasible.
3. Prefer clear names over abbreviations.
4. Keep functions small and focused.
5. Avoid deeply nested control flow.
6. Prefer pattern matching over fragile boolean logic.
7. Prefer explicit types where inference harms readability.
8. Do not add comments that merely repeat the code.
9. Add comments only for non-obvious reasoning, invariants, or tradeoffs.

## Testing Rules

1. Add or update tests for every behavior change.
2. Prefer unit tests for pure logic.
3. Prefer integration tests for public behavior.
4. Test error paths, not only success paths.
5. Do not delete failing tests unless they are obsolete and the reason is clear.
6. Do not weaken assertions to make tests pass.
7. Avoid time-dependent or network-dependent tests unless isolated behind mocks or fixtures.

## Required Validation

Before considering work complete, run the strongest applicable set:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## CI Rules

### Required Checks

Every pull request must pass the strongest applicable validation before merge.

Required Rust checks:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

If the project does not support `--all-features`, use the closest documented project-specific commands.

### Formatting

- Code must be formatted with `cargo fmt`.
- CI must fail if formatting changes are required.
- Do not bypass formatting failures.

### Type Checking

- CI must run `cargo check`.
- CI should check all targets where practical:
  - libraries
  - binaries
  - tests
  - examples
  - benches

### Clippy

- CI must run `cargo clippy`.
- Clippy warnings must be treated as errors.
- Do not suppress Clippy lints unless the reason is documented.

Required command:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```

### Tests

- CI must run the test suite.
- New behavior must include tests.
- Bug fixes should include regression tests when practical.
- Do not delete, weaken, or ignore tests just to make CI pass.

Required command:

```sh
cargo test --all-features
```

### Warnings

- CI should treat warnings as failures where feasible.
- Do not allow new compiler warnings.
- Do not silence warnings with broad `allow` attributes.

Recommended command:

```sh
RUSTFLAGS="-D warnings" cargo check --all-targets --all-features
```

### Lockfile Rules

- Applications must commit `Cargo.lock`.
- Libraries may omit `Cargo.lock` unless the repository policy says otherwise.
- CI should use locked dependencies when appropriate.

Recommended commands:

```sh
cargo check --locked
cargo test --locked
```

### Dependencies

- Do not add new dependencies unless necessary.
- Dependency changes must be intentional and reviewable.
- Do not change dependency versions unless required.
- Do not introduce unmaintained or suspicious crates without justification.

### Feature Flags

CI should validate supported feature combinations.

Recommended minimum:

```sh
cargo check --no-default-features
cargo check --all-features
cargo test --all-features
```

Only skip feature checks if the project explicitly documents why they are unsupported.

### Documentation

For libraries, CI should verify that documentation builds successfully.

Recommended commands:

```sh
cargo doc --no-deps --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

### Unsafe Code

- Unsafe code must be avoided unless necessary.
- Any unsafe block must include a `SAFETY:` explanation.
- CI should deny unsafe-related issues where practical.

Recommended crate-level lint:

```rust
#![deny(unsafe_op_in_unsafe_fn)]
```

### Security Checks

When available, CI should run security checks.

Recommended command:

```sh
cargo audit
```

Do not claim `cargo audit` passed unless it was actually run.

### Minimum Supported Rust Version

If the project defines a Minimum Supported Rust Version, CI must test against it.

Example:

```text
MSRV: 1.75.0
```

Do not raise the MSRV unless explicitly required and documented.

### CI Failure Policy

- Do not merge failing CI.
- Do not bypass CI for convenience.
- Do not mark failing checks as optional without justification.
- Fix flaky tests or clearly isolate and document them.
- Do not weaken CI checks to make a change pass.

## Code search and structural refactoring

Use `rg` as the default text search tool and `rg --files` as the default file discovery tool. Use ast-grep when the task depends on Rust/TypeScript syntax or code shape rather than exact text.

Prefer:

```bash
rg "search text"
rg "TODO|FIXME"
rg "Result<|anyhow|thiserror"
rg --files
rg --files | rg '(^|/)(Cargo.toml|package.json|AGENTS.md|CLAUDE.md)$'
rg --files | rg '(^|/)(crates|scripts|tests)/'
```

Do not use `grep`, `find`, `ls -R`, or broad shell commands for repo search unless `rg` is unavailable or the task specifically requires another tool. Use raw `rg` when exact search results matter, especially for debugging, security-sensitive work, or migration planning.

Use ast-grep for syntax-aware structural searches, migration inventories, and safer refactor planning. ast-grep uses `--globs` for include/exclude patterns, not ripgrep-style `-g`.

Useful Ajax searches:

```bash
rg 'unwrap\(|expect\(|panic!' crates
rg 'Command::new|std::process|tmux|git ' crates
rg 'ports|adapters|domain|analysis|use_case|registry' crates
rg 'cargo check|cargo clippy|nextest|cargo test' AGENTS.md README.md .github scripts
rg 'fetch\(|WebSocket|EventSource|serviceWorker|manifest' crates/ajax-web web scripts
```

Useful ast-grep searches:

```bash
ast-grep -p '$EXPR.unwrap()' --lang rust crates
ast-grep -p '$EXPR.expect($MSG)' --lang rust crates
ast-grep -p 'panic!($$$ARGS)' --lang rust crates
ast-grep -p 'Command::new($CMD)' --lang rust crates
ast-grep -p 'fn $NAME($$$ARGS) -> Result<$OK, $ERR> { $$$BODY }' --lang rust crates
```

Before broad refactors:

1. Use `rg` to inventory text references.
2. Use ast-grep to inventory structural matches.
3. Inspect representative matches manually.
4. Make the smallest safe change.
5. Run the required focused and repo-level checks.

Agents must read the actual source before editing and must not rely only on semantic search, generated summaries, or memory.

### Agent CI Rules

When working as a coding agent:

- Run the strongest applicable validation before completion.
- Do not claim CI, tests, formatting, or Clippy passed unless the commands were actually run.
- Report any command that failed.
- Include the exact failing command and a concise explanation.
- Do not hide failures behind vague wording.
- Do not modify CI configuration unless the task requires it.
- Do not weaken CI checks to make a change pass.

## Delegation Routing

Start with the `model-router` skill for any single bounded code behavior change
in this worktree that you would otherwise implement yourself. It decides whether
to create a `tdd-implementation-packet` (the source-of-truth handoff), delegate
implementation, delegate review, or stop. Skip it for trivial one-liners,
non-code work, or pure Q&A/exploration.

Lanes it routes to (reach for one directly only when the user names it):

- `cursor-delegate` (Composer 2.5) — default implementer: frontend, TypeScript,
  Svelte, PWA, repo-aware Rust, normal bounded bug fixes.
- `opencode-delegate` — MiniMax-M3 for cheap mechanical/boilerplate/docs work,
  GLM 5.2 for reasoning-heavy backend, architecture, bug isolation, refactors.
- `codex-delegate` (GPT-5.5) — default reviewer and packet critique; delegator
  mode when Codex should itself dispatch to a sub-lane and review the result;
  implementation only when the user explicitly asks.

The parent always reviews the git diff before accepting. Delegates never commit,
push, merge, rebase, or change branches.

## Pull Request Expectations

A completed change should be easy to review.

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
