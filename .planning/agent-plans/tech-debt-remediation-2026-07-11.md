# Tech Debt Remediation Plan — 2026-07-11

Follow-up to `tech-debt-review-2026-07-11.md`. Verification pass corrected two of
that review's findings (see **Retractions**). This is the plan of record.

Mode: Planning-Only. No source edited. Awaiting approval before any task starts.

## Guiding rule

Only fix debt that has *caused observed pain*. Metrics (cohesion, betweenness,
LOC) locate suspects; they do not convict. Everything below is either
evidence-backed pain or an explicit decision Matt has to make. Everything that
looked bad but hasn't hurt anything is in **Do Not Do**, on purpose.

---

## Retractions from the review (I was wrong)

**R1. "Diverged duplicate planning docs" — NOT DEBT. Withdrawn.**
`.planning/agent-plans/<slug>.md` and `.planning/packets/<slug>.md` are *different
artifacts that share a slug*: the first is the plan (scope / non-goals, ~47
lines), the second is the TDD implementation packet (`PACKET_STATUS: READY`,
test-first work order, ~210 lines). That is exactly the AGENTS.md
plan → packet → delegate flow working as designed. `cmp` said "differ"; I read
the bytes and not the meaning. No action.

**R2. "Pre-commit is broken / commit with --no-verify" — STALE. Refuted.**
Memory said a non-hermetic test read the real schema-10 `~/.ajax-dev/ajax.db`.
Re-measured today under the exact condition the hook sees (Matt's shell exports
`AJAX_PROFILE=dev`, husky runs `npm run verify` which inherits it):

```
AJAX_PROFILE=dev cargo nextest run -p ajax-cli --all-features --test-threads=1
→ 336/336 passed
AJAX_PROFILE=dev cargo nextest run -p ajax-cli -E 'test(load_context_preserves_resolved_runtime_paths)'
→ 1 passed          # the exact test the memory named
```

Path resolution now takes an injected `RuntimePathRequest` and the subprocess
tests scrub the runtime env. Memory updated. **Action: stop using `--no-verify`.**
It bypasses the whole gate (fmt + clippy + nextest + web checks), not one test.

---

## Task 1 — Collapse the lockstep guard-test duplication (DO FIRST) — ✅ DONE (uncommitted)

**Status: COMPLETE, accepted at the Review Gate, left uncommitted for your review.**
Packet: `.planning/packets/web-shell-guard-dedupe.md`.
Delegation decision: **delegated via model-router** → `cursor-delegate` / `composer-2.5`.

Changed (15 insertions, 86 deletions, both `#[cfg(test)]`-only, no production code):
- `crates/ajax-cli/src/web_backend.rs` — deleted 4 duplicated content tests
  (`mobile_shell_is_responsive_and_loads_cockpit_data`,
  `mobile_shell_is_the_bundled_svelte_mount_point`, `app_script_wires_cockpit_actions`,
  `app_script_is_worker_and_push_free`) and the now-dead `render_mobile_shell` helper;
  added byte-equality wiring assertions (`browser_shell()`, `/app.js`, `/app.css`).
- `crates/ajax-web/src/slices/install.rs` — tightened one assertion from
  `!contains("serviceWorker.register")` to the stricter `!contains("serviceWorker")`
  so ajax-cli's stronger negative was not silently lost.

Validation (run by me, not taken from the delegate):
- `cargo fmt --check` → exit 0
- `cargo clippy --all-targets --all-features -- -D warnings` → clean
- `cargo nextest run -p ajax-web -p ajax-cli --all-features --test-threads=1` → **459/459 pass**
- **Coverage proof (the one that matters):** mutated `id="app"` → `id="app-MUTATED"` in
  `crates/ajax-web/web/dist/index.html`; **exactly one** test failed
  (`ajax-web slices::install::tests::shell_is_the_bundled_svelte_mount_point`), ajax-cli
  stayed green. Pre-change, that mutation reddened both crates. Mutation reverted;
  file byte-identical to HEAD; suite re-confirmed green.

### Deviations

1. **Codex packet-critique could not run** — `codex exec` hit its usage limit (resets
   00:36). It exited 1 *without rewriting* `--output-last-message`, so the report file
   still held a **stale PASS for an unrelated packet** from 11:04 that morning. Nearly
   accepted a hallucinated approval. Substituted an independent adversarial critique
   agent (read-only, same assertion-by-assertion mandate).
2. **The critique returned BLOCK, and it was right.** My first packet asserted
   "`web_install` is already imported in this module" — false. The import lives in the
   parent module; `mod tests` has an explicit `use super::{...}` list, so step D would
   have hit E0433. The obvious delegate "fix" (drop step D, delete the unused import)
   would have passed clippy and tests **while silently destroying the only assertion
   that makes the deletions safe**. Packet rebuilt with `super::web_install::…`, step D
   marked mandatory, a hard invalidation rule added, and a `grep -c 'web_install::' ≥ 3`
   tripwire added to Verification (test-count alone would not have caught it).
3. **opencode/minimax-m3 hung** with 0-byte output and no edits (10-min watchdog, exit
   143). Pre-dispatch snapshot confirmed zero worktree change, so nothing to restore.
   Rerouted to `cursor-delegate`, which succeeded first-try. Memory updated: the
   opencode lane is now 4 hangs / 0 successes — stop routing to it.

### Original analysis

**Pain: observed.** Already recorded in project memory as a thing that bites:
editing `crates/ajax-web/web/*` trips string-snapshot assertions in two crates.

**Evidence.** `crates/ajax-web/src/slices/install.rs:63` and
`crates/ajax-cli/src/web_backend.rs:443` assert the *same* retired-DOM list
(`id="inbox"`, `id="repos"`, `class="cockpit-chrome"`, `id="new-task-row"`,
`id="settings-view"`, `id="pwa-warning"`, `id="attention-summary"`) against the
*same* rendered shell, with near-identical `shell.contains(...)` /
`html.contains(...)` blocks either side.

- Test: the ajax-web guard test stays and remains the single owner of shell
  content assertions (it is the crate that owns the asset).
- Implementation: delete the duplicated content assertions from
  `ajax-cli/src/web_backend.rs`; leave one thin wiring test there proving the CLI
  serves what ajax-web renders (`render_mobile_shell` is reachable and non-empty).
- Verification: `cargo nextest run -p ajax-web -p ajax-cli --all-features`.
  Then edit a node id in `crates/ajax-web/web/app.html` and confirm exactly **one**
  test fails, not two.
- Risk: low. Deleting assertions — allowed here because they are duplicated, not
  weakened; coverage is preserved in ajax-web. Call this out in the PR body.

## Task 2 — Split `registry/sqlite.rs` — ✅ DONE (uncommitted)

**Status: COMPLETE, accepted at the Review Gate, uncommitted.**
Packet: `.planning/packets/sqlite-migrations-split.md`.
Delegation decision: **delegated via model-router** → `cursor-delegate` / `composer-2.5`.

**Scope was cut from the original plan.** The plan said extract `migrations.rs` *and*
`rows.rs`. I shipped migrations only. Moving the row/codec half would have meant relocating
the `string_codec!` macro and re-pointing six generated `parse_*` imports used by a
2,300-line test module — real risk for cosmetic gain, with no failure driving it. The
migrations were the actual pain (7 in-place schema migrations buried among row parsers).
`rows.rs` is **not** scheduled; do it only if something forces it.

Result: `sqlite.rs` 3,997 → 3,573 lines; new `sqlite/migrations.rs` at 429 lines holding
`SQLITE_SCHEMA_VERSION`, `migrate`, `create_current_schema`, all seven `migrate_v*` fns,
and the pragma/column helpers. Net +5 lines total (a `mod` decl, 3 imports, one signature)
— pure motion. Public API untouched.

Validation (run by me, not taken from the delegate):
- `cargo fmt --check` → clean; `cargo clippy --all-targets --all-features -- -D warnings` → clean
- `cargo nextest run -p ajax-core --all-features` → 769/769
- **`cargo nextest run --all-features` (whole workspace) → 1560/1560 pass**
- **SQL-identity proof → clean.** Whitespace-normalized line-multiset diff of the original
  file vs. the union of the two resulting files contains *only* the intended edits (5×
  `Self::migrate` → `migrations::migrate`, 2× `create_current_schema` call form, const/fn
  visibility, `mod migrations;`, the test path, 3 imports). **Zero SQL-bearing lines** — no
  `CREATE`, `ALTER`, `PRAGMA`, or column definition altered.

### Deviations

1. **My first packet's safety proof was broken and would have been actively dangerous.** It
   grepped single-line SQL literals, but every migration's SQL lives in multi-line `r#"..."#`
   raw strings (plus one backslash-continued literal at 295-300). It would have printed
   "SQL IDENTICAL" while a delegate silently dropped an entire `CREATE TABLE` body. Caught it
   myself pre-dispatch; the independent critique BLOCKed on the identical ground. Replaced
   with the whitespace-normalized line-multiset diff above, which catches *any* altered line.
2. Critique also caught two smaller defects: a `rusqlite::params` import hint that would have
   tripped the packet's own `clippy -D warnings` gate (no moved fn uses it), and a missing
   warning that the two `impl SqliteRegistryStore` blocks must *stay* — only the two assoc
   fns leave them. Both folded in.
3. **`cursor-agent` aborted with "Agent Looping Detected"** and produced no DELEGATE_REPORT —
   but it had already written the full, correct change before looping. Rather than trust or
   discard blindly, I evaluated the delta against the gate on its own merits (SQL proof +
   compile + full suite + scope + API). All passed → ACCEPT. A missing self-report is not
   evidence of a bad artifact, but it does mean *nothing* may be taken on the delegate's word.
4. `opencode-delegate` excluded entirely per user instruction (and its 4-hang/0-success record).

### Original analysis

**Pain: latent but real.** 1,683 production lines; graph cohesion 0.05 (weakest
community in the repo — the clustering independently says "this is several
modules"). It owns seven in-place schema migrations *plus* every row parser,
save/load path, string codec and error map. Migrations are the highest-blast-radius
code in the repo and are currently buried in the same file as `col()` helpers.

- Characterization first: existing sqlite tests already cover migration paths
  (`migrate_v2_to_v3` … `migrate_v7_to_current_schema`). Confirm green before
  touching anything; add none if coverage is real.
- Implementation, behavior-preserving, in one commit per move:
  - `registry/sqlite/migrations.rs` — `SQLITE_SCHEMA_VERSION`, `create_current_schema`,
    all `migrate_v*`, `sqlite_user_version`, `has_legacy_payload_schema`,
    `table_has_column`, `registry_tasks_has_column`.
  - `registry/sqlite/rows.rs` — `task_from_row` and the `*_from_row` family,
    `timestamp_from_row`, `col`, the `parse_*` / `*_name` codecs.
  - `registry/sqlite.rs` keeps `SqliteRegistryStore` + save/load orchestration.
- Verification: `cargo nextest run -p ajax-core --all-features`; diff must be
  pure motion (no logic edits) — reviewable by `git diff -M`.
- Skip new tests: mechanical move, compiler-verified. Record that in the PR.

## Task 3 — Remove the `interactive` / `supervisor` feature lattice — ✅ DONE (uncommitted)

**Status: COMPLETE, accepted at the Review Gate, uncommitted. Decision made by Matt on
2026-07-13: delete the lattice.**
Packet: `.planning/packets/remove-feature-lattice.md`.
Delegation decision: **delegated via model-router** → `cursor-delegate` / `composer-2.5`.

### What changed

- `crates/ajax-cli/Cargo.toml` — `[features]` block deleted; `ajax-tui`, `ajax-supervisor`,
  `nix`, `tokio` are now unconditional deps.
- 40 cfg sites across `execution_dispatch.rs`, `lib.rs`, `snapshot_dispatch.rs`,
  `web_backend.rs` — attributes removed, **all gated code kept verbatim**.
- The 8 `not(feature)` stubs deleted, including the two `Err("... support is not enabled in
  this build")` match arms, the `Ok(false)` `refresh_read_context` stub, the `None`
  `stream_command_to_writer` stub, and the `NoAgentStatusCache` path.
- `.github/workflows/ci.yml` — the `cargo check --no-default-features` step removed.
- The guard test was **re-pointed, not weakened** (see below).

### The evidence that decided it

The `--no-default-features` build was never a lean variant — it was a **degraded** one whose
`cockpit` and `supervise` commands returned *"not enabled in this build"*, and CI only ever
`cargo check`ed it (proving it compiled, never that it worked). No consumer existed: not
published to crates.io (`cargo install --path`), no feature flags in the README, no Dockerfile.

Decisively, the lattice's own premise was **half false**. It could not exclude what it claimed:

- `tokio` ships regardless via the mandatory `ajax-web` → `axum`/`hyper`.
- `nix` ships regardless via `ajax-web` → `portable-pty` → `nix v0.28`.

Only `ajax-tui` and `ajax-supervisor` (plus ratatui/crossterm) were genuinely excludable.

### Deviations

1. **I missed a guard test; Codex's critique caught it.** `lib/tests.rs:63`
   `cli_manifest_exposes_lightweight_build_without_interactive_dependencies` (from PR #79)
   codified the exact contract being deleted — asserting all four deps stay `optional = true`
   *"so lightweight builds can exclude it"*. My original packet forbade touching that file, so
   the task was **unlandable as written**. Critically, this meant the user's approval had been
   given without knowing a test codified the opposite intent — so I **stopped and re-asked**
   with the new evidence (including that the test's stated rationale is factually wrong for
   tokio and nix) rather than rewriting a design contract on my own inference. Approved.
   The test is now `cli_manifest_compiles_tui_and_supervisor_unconditionally` and asserts the
   inverse contract with **equal strength** (deps must NOT be optional; no `[features]` block;
   no `interactive =` / `supervisor =` lines; `ajax-web` still always-compiled).
2. Codex also caught that my Proof 2 (`git diff --stat` + release build) could pass while a
   delegate silently deleted live `interactive`-gated code — dead code still compiles.
   Replaced with `git diff -U0 | grep '^-'`, requiring every removed line to be a cfg attribute
   or one of the 8 named stubs.
3. **Proof 2 was contaminated at review time:** `git diff` compares against HEAD, and the
   worktree already held uncommitted work from Tasks 1/2/4. The raw diff showed Task 1's
   deletions as if they were the delegate's. I re-ran the proof against the **pre-dispatch
   snapshot** to isolate the real delegate delta. Lesson: with a dirty worktree, diff against
   the snapshot, never HEAD.
4. **`cursor-agent` looped again** ("Agent Looping Detected", exit 1, no report) — but had
   already written the complete change, same as Task 2. Verified the artifact on its own merits.
5. `cargo fmt` re-wrapped two lines in the delegate's new test; ran `cargo fmt`.

### Validation (run by me, not taken from the delegate)

- Proof 1 — all three greps find **nothing**: no `feature = "interactive"` / `"supervisor"`
  anywhere in `crates/` or `.github/`; no `"not enabled in this build"`; no
  `no-default-features` in CI.
- Proof 2 (snapshot-isolated) — every non-cfg deleted line is one of the 8 named stubs. The
  live logic survived: `web_backend.rs:196-199` still builds `TmuxAgentStatusSnapshot` and
  calls `refresh_runtime_context_with_tier` with the real cache.
- `cargo fmt --check` clean · `clippy -D warnings` clean · **`cargo nextest run --all-features`
  → 1560/1560 pass** · `cargo build --release -p ajax-cli` OK.

### Original analysis

**This one needs Matt, not an agent.** 33 `cfg(feature = "interactive")` sites
(4 files) + 8 `supervisor` sites (2 files). Default is `["interactive","supervisor"]`.
CI runs `cargo check --no-default-features` — it is **compiled, never tested**. I
found no Dockerfile, README, or workflow that actually *builds* headless.

Two honest options:

- **(a) Somebody really ships headless** → add one CI job:
  `cargo nextest run --no-default-features` (and the `supervisor`-only combo).
  Cost: one job, permanent proof.
- **(b) Nobody does** → the lattice is dead flexibility across 41 cfg sites; delete
  the features and fold the deps in. Per AGENTS.md this removes a public build
  surface = **Architecture Change → needs your explicit approval.**

Default recommendation: **(b)**, unless you know of a consumer. Do not let it
keep sitting in the "checked but unproven" middle — that is the worst of both.

## Task 4 — Move the mega inline test modules out of `src/` — ⚠️ HALF DONE, half DELIBERATELY ABANDONED

**Status: ajax-tui half COMPLETE and accepted (uncommitted). ajax-core half REJECTED and
reverted — on purpose. Do not retry it.**
Packet: `.planning/packets/move-inline-test-modules.md`.
Delegation decision: **delegated via model-router** → `cursor-delegate` / `composer-2.5`.

### Done: `crates/ajax-tui/src/lib.rs`

Was 11 production lines carrying 3,956 test lines. Now 149 lines, with the tests in
`crates/ajax-tui/src/lib/tests.rs` (3,814 lines), declared as
`#[cfg(test)] #[path = "lib/tests.rs"] mod tests;` — matching the pattern already used at
`crates/ajax-cli/src/lib.rs:452`.

Validation (run by me): test count **974 → 974** (unchanged), `cargo fmt --check` clean,
clippy clean, **full workspace 1560/1560 pass**. Pure-motion diff shows only the
`mod tests {` → `mod tests;` wrapper change plus rustfmt re-wrapping of lines that got
de-indented one level (e.g. `assert!(` + `matches!(…)` + `);` collapsing to one line). No
test name or assertion content changed.

### Abandoned: `crates/ajax-core/src/task_operations.rs` — and WHY (read this before retrying)

The delegate correctly **BLOCKED** on this half, surfacing a guard I did not know existed:

`crates/ajax-core/src/lifecycle.rs:393`
`lifecycle_status_assignments_are_not_in_production_submodules` is an **architecture test**
enforcing an AGENTS.md non-negotiable — lifecycle writes must go through
`ajax_core::lifecycle`, i.e. *core owns task truth*. It walks every `.rs` file under
`ajax-core/src` and separates production from test code by splitting each file at its first
**inline** `\n#[cfg(test)]` marker (lifecycle.rs:402-405).

A standalone `tests.rs` contains no `#[cfg(test)]` marker inside it — the attribute lives on
the `mod tests;` declaration in the *parent* file. So the guard reads the **entire**
`task_operations/tests.rs` as production code, and every `task.lifecycle_status = …` in the
tests becomes a violation.

**That guard is structurally coupled to the inline-test convention.** Landing this half would
require making it skip files named `tests.rs` — punching a hole in an architecture check that
protects task-truth ownership, in exchange for *file-navigation cosmetics*. Not a trade worth
making. The ajax-core half was reverted to its exact pre-dispatch hash and
`task_operations/tests.rs` deleted.

Note the guard scans **only ajax-core** (`env!("CARGO_MANIFEST_DIR")`), which is why the
ajax-tui half is unaffected and safe.

**If you ever revisit this:** the honest fix is to teach the guard about sibling test modules
(e.g. skip any file whose module is declared `#[cfg(test)]`), *not* to skip by filename — and
that is an architecture change needing its own approval. Until something actually hurts,
leave `task_operations.rs`, `commands.rs`, `runtime.rs`, and `sqlite.rs` tests inline.

### Original analysis

**Pain: real but diffuse.** The most-connected nodes in the whole 5,183-node graph
are test fixtures, not production code: `sample_context()` (degree 114),
`sample_repos()` (111), `sample_tasks()` (103), `context_with_tasks()` (100).
Concentrations: `ajax-cli/src/lib/tests.rs` 7,934 lines / 186 tests;
`ajax-core/src/commands.rs` 469 prod : 4,166 test; `ajax-tui/src/lib.rs`
147 prod : 3,820 test; `ajax-core/src/task_operations.rs` 7 prod : ~2,000 test.

**Deliberately scoped down.** Do the mechanical part, skip the ambitious part:

- DO: move each oversized inline `mod tests` into a sibling `<module>/tests.rs`
  along the seams the production code already has. Pure motion, compiler-checked,
  no assertion touched (global test rules: never weaken/delete assertions).
- DO NOT: redesign the fixtures, split `sample_context()`, or rewrite the 186
  tests. That is a large refactor with no observed failure driving it, and it
  would churn the highest-traffic test surface in the repo for a metric.
- Verification: test *count* must be identical before and after
  (`cargo nextest list | wc -l`), and green.
- Sequence: after Task 2, so the sqlite move lands in a quiet tree first.

## Task 5 — Tidy: personal absolute paths as test fixture literals

`/Users/matt` is hardcoded as a fixture literal in `crates/ajax-cli/src/context.rs`
(≈545, 562, 579) and `crates/ajax-cli/src/cockpit_backend.rs:314`
(`/Users/matt/.ajax-dev/worktrees`). These are *injected* fake homes, so they are
hermetic and pass — it is a smell, not a bug. One-line-each swap to a neutral
`/home/ajax-test`. Trivial, zero risk, do it while touching those files anyway.
Not worth a PR of its own.

---

## Do Not Do (and why — this is the point of the plan)

- **Unify the crypto backends.** Yes, `ring` *and* `aws-lc-rs` both compile in.
  But `jsonwebtoken` 10.4 has no `ring` feature (only `aws_lc_rs` / `rust_crypto`),
  so "unifying" means pushing `rustls` + `rcgen` onto `aws-lc-rs` — trading build
  seconds for `aws-lc-sys` C/asm toolchain risk on `cargo install`, which is
  already this repo's most fragile surface (see the `--locked` / rustc-1.88 pins).
  Zero user-visible benefit. **Revisit only if clean-build time actually hurts.**
- **Refactor `refresh_runtime_context_with_tier()`** (betweenness 0.217, 9
  communities). High betweenness is what a runtime-reconciliation hub *is*. There
  is no bug here. Don't refactor a metric. Just don't let it grow: new status
  behavior goes in `live.rs`/`attention.rs`, not into the bridge.
- **Refactor `TerminalRawView.svelte`** (1,484 lines). Post-#432 it is already an
  orchestrator over extracted policy modules — the trajectory is correct. Leave it;
  enforce only that new behavior lands in `terminal*.ts`, not back in the component.
- **Anything to `.planning/` layout.** See R1.

## Suggested order

1. Task 1 (small, real, unblocks confidence) — one PR.
2. Task 3 (your decision; blocks nothing but rots).
3. Task 2 (mechanical, medium diff) — one PR, commit-per-move.
4. Task 4 (mechanical, large diff) — separate PR, after 2.
5. Task 5 — folded into whichever PR touches those files.

## Delegation decision

Per AGENTS.md, Tasks 1, 2, 4 and 5 are bounded code changes → **delegated via
`model-router`**, each with a `tdd-implementation-packet` in `.planning/packets/`.
Task 3 is a decision and is not delegated. This planning pass itself is
review-only and was not delegated.

## Validation commands (every task)

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --test-threads=1
```

Run them yourself. Do not pass `--no-verify` (see R2).
