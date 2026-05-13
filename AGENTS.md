# AGENTS.md

## Purpose

This file defines the required workflow, architecture boundaries, Rust standards, testing rules, and validation expectations for agentic coding in this repository.

These instructions are mandatory unless the user explicitly overrides them for a specific task.

---

## Required First Step

Before any architectural analysis, implementation plan, or code change:

1. Read `architecture.md`.
2. Treat it as the source of truth for current architecture boundaries and direction.
3. Keep its constraints in mind throughout planning, implementation, and validation.

Also follow the Rust guidance in:

```text
@/Users/matt/.codex/RTK.md
```

If an approved change alters architecture, update `architecture.md` in the same work so the documentation matches the finished implementation.

---

## Workflow

### Step 1: Plan First

Before changing code or documentation, create a plan.

Break the work into small tasks, roughly 5–15 minutes each.

For each code task, include:

- Failing behavior test to write
- Code to implement
- Verification command or check

For each documentation-only task, include:

- Documentation to update
- Verification command, read-through, search, formatting check, or rendered review

After showing the complete plan, stop and say exactly:

```text
Plan ready. Approve to proceed.
```

Wait for approval before making changes.

Do not skip this approval step.

### Markdown-Only Changes

Markdown-only documentation changes are exempt from TDD.

For `.md`-only changes:

- Plan first.
- Get approval.
- Do not write failing tests.
- Make the documentation change.
- Verify with an appropriate read, search, formatting check, or rendered review.

### Step 2: Execute Code Changes with TDD

For each approved code task:

1. Write a failing behavior test.
2. Run the focused test and show the failure.
3. Implement the smallest change that makes the test pass.
4. Run the focused test and show the pass.
5. Ask exactly:

```text
Task N done. Continue?
```

6. Wait for approval before moving to the next task.

### Full-Run Approval

Ask after each task by default.

If the user explicitly approves finishing all tasks, implementing all tasks, continuing through the full plan, or otherwise proceeding without stopping, continue task-by-task through the approved plan without asking between tasks.

Still follow TDD for each code task.

When finished, report the final validation results.

---

## Non-Negotiable Rules

- Never implement production code before writing a failing behavior test.
- Production code may only change to satisfy a failing behavior test.
- Make the smallest implementation needed to pass the test.
- Refactor only after tests are green.
- Never delete, weaken, or ignore test assertions.
- When tests fail, fix the implementation, not the tests.
- Do not perform unrelated refactors.
- Keep cleanup work behavior-neutral unless the task explicitly asks for runtime behavior changes.
- Preserve existing public APIs unless explicitly asked to change them.
- Replace internal legacy implementation when required, but preserve public APIs and user-visible behavior unless the approved task explicitly changes them.
- Never hide failures. Surface them with explicit errors, tests, or documented limitations.
- Do not claim a command passed unless it was actually run and passed.

---

## Test File Modification Rule

Do not modify files in `tests/` by default.

If a behavior test belongs in `tests/`, the plan must explicitly name the test file to modify or create.

Approval of that plan counts as explicit approval to modify only those listed test files.

Do not modify unrelated test files.

---

## Workspace Hygiene

The root `Cargo.toml` owns:

- Shared package metadata
- Dependency versions
- Lint policy

Member crates should use workspace dependency and lint inheritance where practical.

Preserve crate boundaries:

- `ajax-cli`: CLI parsing, dispatch, rendering, and context loading
- `ajax-core`: models, policy, live status, and registry
- `ajax-supervisor`: process supervision
- `ajax-tui`: Cockpit screen state, input, layout, and rendering

---

## Architecture

Use a restrained ports-and-adapters modular monolith.

Organize code around clear responsibilities:

- `cli/`: command-line argument parsing only
- `app/`: command and use-case orchestration
- `domain/`: core types and business rules
- `analysis/`: checking, scanning, or evaluation logic
- `ports/`: small traits only for real external boundaries
- `adapters/`: filesystem, terminal, JSON, subprocess, network, or environment access
- `tests/`: user-visible behavior verification

Do not let CLI parsing, filesystem access, terminal output, subprocess execution, networking, or environment access leak into `domain/` or `analysis/`.

Prefer concrete structs and functions over traits.

Introduce traits only for:

- I/O boundaries
- Test seams
- Genuinely swappable implementations

Do not create managers, services, processors, handlers, factories, helpers, utils, or generic abstraction layers unless there is a concrete need explained in the approved plan, code review, or PR.

Existing domain-specific registry code belongs in `ajax-core`.

Do not create new generic registries unless the approved plan explains the concrete need.

When adding a feature, implement one vertical slice at a time:

1. CLI args
2. App use case
3. Domain or analysis logic
4. Adapter changes, if needed
5. User-visible tests

---

## Rust Standards

### Core Principles

- Prefer correctness over cleverness.
- Make the smallest safe change that solves the task.
- Follow the project’s existing style, naming, architecture, and error-handling patterns.
- Surface failures clearly.

### Safety

- Do not use `unsafe` unless explicitly required and justified.
- If `unsafe` is unavoidable:
  - Keep it minimal.
  - Add a `// SAFETY:` comment explaining the invariant.
  - Add tests covering the safe wrapper.
- Avoid global mutable state.
- Prefer immutable variables by default.
- Avoid unnecessary cloning.
- Use borrowing where practical.
- Restructure ownership instead of fighting the borrow checker.

Recommended unsafe-related lint:

```rust
#![deny(unsafe_op_in_unsafe_fn)]
```

### Error Handling

- Do not use `unwrap()`, `expect()`, or `panic!()` in production code unless the invariant is impossible to violate and the reason is documented.
- Prefer `Result<T, E>` for recoverable errors.
- Prefer `Option<T>` only when absence is expected and non-exceptional.
- Do not ignore errors with `_`, `.ok()`, or silent fallbacks unless explicitly justified.
- Use the project’s existing error type and conventions.
- For applications, `anyhow` is acceptable if already used.
- For libraries, prefer structured errors such as `thiserror` if already used.
- Preserve error context.
- Do not replace meaningful errors with generic strings.
- Do not log and swallow errors unless the caller can safely continue.

### Dependencies

- Do not add crates unless necessary.
- Before adding a dependency, check whether the standard library or existing dependencies already solve the problem.
- Prefer small, well-maintained crates.
- Do not introduce heavy frameworks for small tasks.
- Do not change dependency versions unless required.
- Do not introduce unmaintained or suspicious crates without justification.
- Do not remove `Cargo.lock` for applications.
- Use `--locked` in CI-like validation when appropriate.

### Style

- Run `cargo fmt`.
- Code must pass `cargo clippy` with warnings treated as errors when feasible.
- Prefer clear names over abbreviations.
- Keep functions small and focused.
- Avoid deeply nested control flow.
- Prefer pattern matching over fragile boolean logic.
- Prefer explicit types where inference harms readability.
- Do not add comments that merely repeat the code.
- Add comments only for non-obvious reasoning, invariants, or tradeoffs.

---

## Testing Standards

- Add or update tests for every behavior change.
- Prefer unit tests for pure logic.
- Prefer integration tests for public behavior.
- Test error paths, not only success paths.
- Do not delete failing tests unless they are obsolete and the reason is clear.
- Do not weaken assertions to make tests pass.
- Avoid time-dependent or network-dependent tests unless isolated behind mocks or fixtures.
- Use `rstest` when it makes tests clearer, especially for table-driven, parameterized, or fixture-heavy tests.
- Use `cargo nextest run` instead of `cargo test`.

Use `cargo test` only for documentation checks or when `cargo nextest` is unavailable.

If using the fallback, say so explicitly.

---

## Required Validation

Before considering work complete, run the strongest applicable validation:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

If the project does not support `--all-features`, use the closest documented project-specific command.

For libraries, documentation should build successfully when applicable:

```sh
cargo doc --no-deps --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

When available and relevant, run:

```sh
cargo audit
```

Do not claim `cargo audit` passed unless it was actually run.

---

## CI Expectations

Every pull request must pass the strongest applicable checks from `Required Validation` before merge.

Recommended warning check:

```sh
RUSTFLAGS="-D warnings" cargo check --all-targets --all-features
```

Recommended feature checks:

```sh
cargo check --no-default-features
cargo check --all-features
cargo nextest run --all-features
```

Only skip feature checks if the project explicitly documents why they are unsupported.

Applications must commit `Cargo.lock`.

Libraries may omit `Cargo.lock` unless repository policy says otherwise.

If the project defines a Minimum Supported Rust Version, CI must test against it.

Do not raise the MSRV unless explicitly required and documented.

---

## CI Rules

- Do not merge failing CI.
- Do not bypass formatting failures.
- Do not bypass CI for convenience.
- Do not mark failing checks as optional without justification.
- Do not weaken CI checks to make a change pass.
- Do not modify CI configuration unless the task requires it.
- Do not allow new compiler warnings.
- Do not silence warnings with broad `allow` attributes.
- Fix flaky tests or clearly isolate and document them.

---

## Reporting Results

When reporting completion, include:

- Summary of changes
- Tests added or updated
- Validation commands run
- Any commands that failed
- Any limitations, skipped checks, or unavailable tools

For each failed command, include:

- Exact command
- Concise failure explanation
- Whether it was fixed or remains unresolved

Do not describe CI, tests, formatting, Clippy, docs, security checks, or any validation as passing unless the relevant commands were actually run and passed.
