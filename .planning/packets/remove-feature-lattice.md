# TDD Implementation Packet — Remove the `interactive` / `supervisor` feature lattice

```yaml
PACKET_STATUS: READY
TASK_KIND: refactor
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
```

## Task contract

`ajax-cli` has two Cargo features, `interactive` and `supervisor`, both on by default. They
are **compiled** in CI (`cargo check --no-default-features`) but **never tested**, and there
is no consumer: `ajax-cli` is not published to crates.io (install is `cargo install --path`),
the README never mentions feature flags, and no Dockerfile or workflow builds headless.

The non-default build is not a lean variant — it is a **degraded** one. The
`#[cfg(not(feature = ...))]` arms are stubs that disable functionality: they return
`Err("cockpit support is not enabled in this build")`, `Err("supervise support is not
enabled in this build")`, `Ok(false)`, `None`, and `let _ = save_state;`.

Delete the lattice. Make `ajax-tui`, `nix`, `ajax-supervisor`, and `tokio` unconditional
dependencies, keep every `feature`-enabled code path, and delete every `not(feature)` stub.

**This is behavior-preserving for the default build** (which is the only build anyone runs).
Nothing about the shipped binary changes.

Persistent plan (already created and approved by the parent, do not edit):
`.planning/agent-plans/tech-debt-remediation-2026-07-11.md` (Task 3). The contract change
below was explicitly approved by the user on 2026-07-13.

## Allowed files

- `crates/ajax-cli/Cargo.toml`
- `crates/ajax-cli/src/execution_dispatch.rs`
- `crates/ajax-cli/src/lib.rs`
- `crates/ajax-cli/src/snapshot_dispatch.rs`
- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-cli/src/lib/tests.rs` — **only** the one test named in Rule 5. Every other
  test in this 7,900-line file is off-limits.
- `.github/workflows/ci.yml`
- `Cargo.lock` (only if cargo regenerates it — do not hand-edit)

## Forbidden changes

- Any other file. Do NOT touch `crates/ajax-tui/`, `crates/ajax-supervisor/`,
  `crates/ajax-core/`, or `crates/ajax-web/`.
- Do NOT change any behavior of the **default** build. Every code path currently compiled
  under `feature = "interactive"` or `feature = "supervisor"` must survive **verbatim** —
  you are removing the `#[cfg(...)]` attribute, not the code under it.
- Do NOT touch the pre-existing uncommitted work in `crates/ajax-cli/src/web_backend.rs`'s
  `mod tests` (the guard-test dedupe), `crates/ajax-core/src/registry/sqlite.rs`,
  `crates/ajax-core/src/registry/sqlite/migrations.rs`, or `crates/ajax-tui/src/lib.rs` +
  `crates/ajax-tui/src/lib/tests.rs`.
- Do NOT add any `#[allow(...)]` or `#![allow(...)]` to make it compile.

## Code anchors

Apply these four mechanical rules to all 40 cfg sites in the four `src/` files.

### Rule 1 — `#[cfg(feature = "interactive")]` and `#[cfg(feature = "supervisor")]`
**Delete the attribute line. KEEP the item it gates, unchanged.**

Sites: `execution_dispatch.rs` 5, 7, 21, 27, 30, 32, 157, 182, 225, 251, 261, 333, 381, 410,
417, 426, 504, 528 · `lib.rs` 2, 5, 7, 15, 17, 24, 30, 45, 352 · `snapshot_dispatch.rs` 13,
129 · `web_backend.rs` 198.

### Rule 2 — `#[cfg(not(feature = ...))]`
**Delete the attribute AND the entire item/expression it gates.** These are the disabled-build
stubs. All 8 sites:

| Site | What to delete |
|---|---|
| `execution_dispatch.rs:178` | the `Some(("supervise", _)) => Err(... "supervise support is not enabled in this build" ...)` match arm |
| `execution_dispatch.rs:186` | the `Some(("cockpit" \| "stable" \| "dev", _)) => Err(... "cockpit support is not enabled in this build" ...)` match arm |
| `execution_dispatch.rs:241` | the whole stub `fn refresh_read_context(...) -> Result<bool, CliError> { Ok(false) }` |
| `execution_dispatch.rs:339` | the statement `let _ = save_state;` |
| `lib.rs:381` | the whole stub `fn stream_command_to_writer(...) -> Option<Result<bool, CliError>>` (returns `None`) |
| `snapshot_dispatch.rs:131` | the `Some(("cockpit", _)) => Err(... "cockpit support is not enabled in this build" ...)` match arm |
| `web_backend.rs:1` | the `use ajax_core::runtime_refresh::{refresh_runtime_context_with_tier, NoAgentStatusCache};` import |
| `web_backend.rs:206` | the `{ refresh_runtime_context_with_tier(context, runner, &NoAgentStatusCache, tier) }` block |

For `web_backend.rs` 198-210: after deleting the `not` block and the Rule-1 attribute, the
function body is just the (previously `interactive`-gated) block that builds
`TmuxAgentStatusSnapshot` and calls `refresh_runtime_context_with_tier`. Keep that logic
byte-for-byte; you may unwrap the now-redundant bare `{ }` block or leave it — both compile.

### Rule 3 — `#[cfg(any(feature = "interactive", feature = "supervisor"))]`
`execution_dispatch.rs:19` — **delete the attribute, keep the `use`.**

### Rule 4 — `#[cfg(all(test, feature = "interactive"))]`
`execution_dispatch.rs:483` — **rewrite to `#[cfg(test)]`.** Do not delete the test module.

### `crates/ajax-cli/Cargo.toml`

- Delete the entire `[features]` block (`default`, `interactive`, `supervisor`).
- Remove `optional = true` from `ajax-supervisor`, `ajax-tui`, `nix`, and `tokio`. Keep every
  other attribute (path, version, workspace, and tokio's
  `features = ["rt-multi-thread", "net", "time"]`) exactly as-is.

### Rule 5 — re-point the guard test (`crates/ajax-cli/src/lib/tests.rs:63-86`)

`cli_manifest_exposes_lightweight_build_without_interactive_dependencies` currently asserts the
very contract we are deleting: that all four deps carry `optional = true`, and that the
`interactive`/`supervisor` feature lines exist. It will fail after this change.

**This is an intentional contract change, explicitly approved.** Re-point the test to assert the
NEW contract with equal strength. Do **not** delete it, do **not** weaken it, do **not**
`#[ignore]` it. Rename it to `cli_manifest_compiles_tui_and_supervisor_unconditionally` and
assert:

- each of `ajax-supervisor`, `ajax-tui`, `nix`, `tokio` is still declared, and its line does
  **NOT** contain `optional = true`;
- the manifest contains **no** `[features]` block, and no `interactive =` / `supervisor =` lines;
- `ajax-web` is still the always-compiled browser boundary (keep that existing assertion and its
  message verbatim).

Keep the same file-reading approach (`env!("CARGO_MANIFEST_DIR")` + `Cargo.toml`). Touch no other
test in this file.

### `.github/workflows/ci.yml`

Delete the step at lines 107-108:

```yaml
      - name: Check no default features
        run: cargo check --no-default-features
```

(Verified: no test asserts the existence of this CI step. The test that reads `ci.yml` is
`ci_web_job_runs_web_build_check`, which checks an unrelated job — leave it alone.)

## Verification commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --test-threads=1
```

All must exit 0. Test count must be **unchanged** — this removes zero tests (the module at
`execution_dispatch.rs:483` is kept, just re-gated to `#[cfg(test)]`).

### Proof 1 — the lattice is gone (required)

```bash
grep -rn 'feature = "interactive"\|feature = "supervisor"' crates/ .github/ ; echo "exit=$?"
grep -rn 'not enabled in this build' crates/ ; echo "exit=$?"
grep -rn 'no-default-features' .github/ ; echo "exit=$?"
```

All three greps must find **nothing** (`exit=1`). Any hit means a stub or gate survived.

### Proof 2 — no live code was deleted (required, paste the FULL output)

The shipped (default) binary must not change. A release build alone would NOT catch it if you
silently deleted a chunk of live `interactive`-gated logic — dead code still compiles. So show
every removed line:

```bash
cargo build --release -p ajax-cli && echo "RELEASE BUILD OK"
git diff -U0 -- crates/ajax-cli/src/execution_dispatch.rs crates/ajax-cli/src/lib.rs \
  crates/ajax-cli/src/snapshot_dispatch.rs crates/ajax-cli/src/web_backend.rs \
  | grep '^-' | grep -v '^---'
```

**Every single line in that output must be one of:**

1. a `#[cfg(...)]` attribute line (Rule 1 / 3 / 4), or
2. a line inside one of the exactly-8 Rule-2 stubs (the two `"...not enabled in this build"`
   match arms in `execution_dispatch.rs`, the `refresh_read_context` stub, `let _ = save_state;`,
   the `stream_command_to_writer` stub, the `snapshot_dispatch.rs` cockpit `Err` arm, the
   `web_backend.rs` `NoAgentStatusCache` import, and the `web_backend.rs` `not` block), or
3. brace/whitespace churn from unwrapping the `web_backend.rs` bare block.

**If a single line of live logic appears there, you deleted code you should have kept — STOP.**
Remember: for positive `feature = "..."` gates, only the attribute goes; the code stays.

## Stop conditions

- Any verification command fails and the fix requires editing a Forbidden file.
- Any Proof 1 grep still finds a hit.
- You need an `#[allow(...)]` to compile (e.g. an unused import after a stub deletion — the
  correct fix is to remove that specific now-unused import, not to silence the lint).
- You are about to delete code that was gated by a **positive** `feature = "..."` cfg. Only
  the attribute goes; the code stays.
- The release build fails.
