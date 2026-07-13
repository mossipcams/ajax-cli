# Tech Debt Review — 2026-07-11

Mode: Planning-Only (review). No code edited. Evidence: graphify knowledge graph
(5,183 nodes / 13,704 edges / 199 communities from AST + doc extraction), serena
symbol analysis, ast-grep structural scans.

## Scope and non-goals

- Scope: whole-repo debt audit (Rust crates + web frontend + .planning corpus).
- Non-goals: fixes, refactors, dependency changes. Each finding below is a
  candidate task, not a plan.

## What is NOT debt (verified clean)

- Production `unwrap()`: 0. Production `expect()`: 3 (context.rs ×2, task_operations/start.rs ×1).
- `#[allow]` attributes: 1 in the entire workspace (`clippy::too_many_arguments`).
- Dead exports: 0 of 275 `pub` symbols in ajax-core prod code are unreferenced.
- Longest function: 144 lines (`save_task`). No god functions.
- Crate topology: clean star — core at center; cli is the only composition root.
- Legacy markers are guard tests asserting removed things stay removed. Good.
- Workspace deps: lean, all load-bearing.

## Findings (ranked)

### 1. Test architecture is the biggest coupling hub in the repo
Graph god nodes — the most-connected nodes in 5,183 — are test fixtures:
`sample_context()` (deg 114), `sample_repos()` (111), `sample_tasks()` (103),
`context_with_tasks()` (100), `sample_inbox()` (68), `run_with_context_and_runner()` (62).
Every fixture change ripples through 100+ tests. Concentrations:
- `crates/ajax-cli/src/lib/tests.rs`: 7,934 lines, 186 tests, one file, inside `src/`.
- `crates/ajax-core/src/commands.rs`: 469 prod / 4,166 inline test lines.
- `crates/ajax-tui/src/lib.rs`: 147 prod / 3,820 inline test lines.
- `crates/ajax-core/src/task_operations.rs`: 7 prod lines / ~2,000 test lines.
- `crates/ajax-web/src/runtime.rs`: 1,017 prod / 1,894 test.
Overall test:prod = 39.4k:29.3k lines — ratio fine, distribution terrible.
Candidate: split mega test modules along the same seams as the prod splits that
already happened; break shared fixtures into narrower builders.

### 2. `registry/sqlite.rs` — 1,683 prod lines, cohesion 0.05
Largest genuine prod file. Owns: 7 in-place schema migrations (v2→v9), all row
parsers, all save/load, string codecs, error mapping. Graph cohesion for its
community is 0.05 (weakly interconnected — the graph itself says split).
Candidate: `migrations.rs` + `rows.rs` split; zero behavior change.

### 3. `refresh_runtime_context_with_tier()` is a 9-community bridge
Betweenness 0.217, degree 51 — the single hottest cross-cutting prod symbol
(crates/ajax-core/src/runtime_refresh.rs). Combined with live.rs (1,162 prod)
this is the status-classification knot; history shows it is also the most
regression-prone area. Any change here touches 9+ graph communities.
Candidate: characterization tests already exist; consider narrowing its
parameter surface before the next status feature, not after.

### 4. Diverged duplicate planning docs
5 packets exist as DIVERGED copies in both `.planning/agent-plans/` and
`.planning/packets/`: terminal-clipboard-slice4, terminal-layout-policy-slice1,
terminal-scroll-follow-slice2, terminal-zero-lag-slice3,
web-ui-color-intuitiveness. Two directories claim to be the packet home
(43 + 20 files). Nobody knows which copy is truth.
Candidate: pick one directory, delete the other copies, note the rule in AGENTS.md.

### 5. Dual crypto backends compiled into every build
`jsonwebtoken` pulls **aws-lc-rs** (slow C/asm build via aws-lc-sys) while
rustls/rcgen pin **ring**. Both stacks compile into ajax-cli. Pure build-time
and binary-size waste. Candidate: unify on one backend (rustls can run
aws-lc-rs, or jsonwebtoken can avoid it) — measure clean-build delta first.

### 6. Lockstep guard-test duplication across crates
`crates/ajax-cli/src/web_backend.rs` and `crates/ajax-web/src/slices/install.rs`
carry near-identical static-shell guard/snapshot assertions. Editing
`ajax-web/web/*` requires touching both (already bit us — recorded in project
memory). Candidate: one owner (ajax-web), ajax-cli keeps a thin wiring test.

### 7. `interactive`/`supervisor` feature lattice: checked, never tested
33 `cfg(feature = "interactive")` sites + 8 supervisor. CI runs only
`cargo check --no-default-features` — the non-default configs are never
*tested*. Either a real consumer builds headless (then test it) or nobody does
(then the lattice is dead flexibility). Decide which.

### 8. TerminalRawView.svelte — 1,484 lines, 17 default-noop callback props
Post-#432 it is an orchestrator over extracted policy modules, so trajectory is
right, but it remains the largest frontend file with a 2,677-line test file.
Watch that new behavior lands in policy modules, not back in the component.

## Validation commands run

- ast-grep unwrap/expect/fn-length scans (results above)
- python LOC/test-boundary measurements over crates/
- `cargo tree -p ajax-cli` (crypto backend duplication)
- graphify full pipeline (graph.json, GRAPH_REPORT.md, graph.html)
- cross-crate pub-symbol reference scan (0 dead)

## Deviations

- 2 of 4 graphify semantic subagents hit the session usage limit mid-run, but
  all 4 chunk files were written before termination — full doc coverage.
