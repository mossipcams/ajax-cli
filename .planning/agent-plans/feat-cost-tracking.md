# Plan: Per-task token/cost tracking (read-time session-log scan)

Mode: Behavior Change. Status: **draft v2, unapproved**.
Delegation decision: TBD at execution — delegate via model-router (bounded, TDD-able).

v2 rethink: v1 draft wired usage through the supervisor's JSONL event stream —
**wrong data source**. Tasks launch agents interactively via tmux `send-keys`
(`ajax-core/src/adapters/tmux.rs:70`); the supervisor JSONL parsers only run
under the separate `ajax-cli supervise --prompt` path, so the daily loop would
have measured nothing. Verified replacement: **agent CLIs already persist
per-session JSONL on disk with `cwd` and token usage.** Read those at query
time. No AgentEvent, no reducer, no migration, no supervisor changes.

## Verified data sources (probed on this machine, 2026-07-09)
- Claude Code: `~/.claude/projects/<path-slug>/*.jsonl` — messages carry
  `"cwd":"<worktree>"` and `usage` with `input_tokens` / `output_tokens`.
  Directory name is the slugged worktree path (`/`→`-`), so task worktree →
  project dir is a direct mapping.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` — carries
  `"cwd":"<worktree>"`, `token_count`, `total_token_usage`, and
  `input_tokens`. `total_token_usage` naming implies **cumulative** —
  T0-lite must confirm cumulative-vs-incremental (take last/max, never SUM,
  if cumulative; summing cumulative double-counts).

## Problem
No visibility into which of many parallel tasks burned how many tokens.

## Scope (v1 = tokens only)
- `ajax cost` (human + `--json`): per-task input/output/total tokens, computed
  by scanning session logs whose `cwd`/slug matches each registered task's
  worktree path. Cross-agent (claude + codex) rolled up per task.
- Include in `inspect` output for a single task.

## Non-goals (v1)
- **No dollars.** Token counts are exact; prices drift. v1.1 adds optional
  `[pricing]` config (per-model $/Mtok — config, never code) and multiplies.
- No DB persistence/caching of totals — cost is derived data, recomputable
  from files the agent CLIs already own. Cache only if scanning proves slow.
  `// ponytail: full rescan per invocation; add mtime-based cache if slow`
- No budgets/alerts, no web chart (add after CLI proves the data).

## Anchors (verified)
- CLI verb surface: `ajax-cli/src/cli.rs::build_cli()` — add `cost` beside
  `inbox`/`ready` via `json_command`.
- Read-only routing: `ajax-cli/src/snapshot_dispatch.rs`.
- Task worktree paths come from the registry (tasks list).
- New core module suggestion: `ajax-core/src/commands/cost.rs` (fits existing
  `commands/` split); filesystem access behind an adapter fn so tests feed
  fixture JSONL, per the adapter architecture.

## Design
1. `commands/cost.rs`: given tasks (worktree paths) + session roots, scan
   matching JSONL files, extract usage lines (serde_json per line, tolerate
   parse failures by skipping), aggregate per task.
   - Claude: slug the worktree path → project dir; SUM per-message usage
     (confirm per-message semantics in T0-lite; Claude usage lines are
     per-API-call — sum).
   - Codex: filter session files by `cwd` field; apply cumulative-vs-sum rule
     from T0-lite.
2. Session roots default to `~/.claude/projects` and `~/.codex/sessions`,
   overridable for tests (and for weird homes) — parameters, not config keys.
3. Render: table sorted by total desc; `--json` typed output like the other
   snapshot commands.

## Tasks (test → impl → verify)
- [x] **T0-lite — DONE 2026-07-09, findings:**
  - Claude usage object (per API call, SUM across lines & files):
    `"usage":{"input_tokens":N,"cache_creation_input_tokens":N,
    "cache_read_input_tokens":N,"output_tokens":N,...}`. 65 usage lines in one
    session file; `cwd` constant within a file. Track all four fields in JSON;
    human table shows in/out/total (cache fields in `--json` only).
  - Codex `total_token_usage` is **CUMULATIVE** (verified monotonic
    21,648 → 3,323,589 within one file) → take LAST per file, never SUM.
    Shape: `{"input_tokens":N,"cached_input_tokens":N,"output_tokens":N,
    "reasoning_output_tokens":N,"total_tokens":N}`. `last_token_usage`
    (per-turn) also exists. `cwd` lives on the session-header line
    (`id`/`timestamp`/`cwd`).
  - Multi-file: this project dir had 1 file but N is normal — aggregate =
    SUM(claude lines across files) + SUM(last-of-each codex file).
- [ ] **T1 extraction.** Fixture JSONL (one claude, one codex, one malformed
  line) → expected per-task totals. Impl `commands/cost.rs`. Verify:
  `cargo test -p ajax-core`.
- [ ] **T2 task matching.** Test: slug mapping for claude dirs; codex cwd
  filter; task with no sessions reads zero (not error); unrelated sessions in
  the same roots are excluded. Verify: `cargo test -p ajax-core`.
- [ ] **T3 `ajax cost` command.** Snapshot tests human + `--json` over seeded
  registry + fixture roots; wire into `snapshot_dispatch`. Add to `inspect`.
  Verify: `cargo test -p ajax-cli`.

## Risks
- **Session-file format is another CLI's private format** — it can change on
  agent upgrade. Extractor must skip unparseable lines and report per-task
  `sources_scanned`/`lines_skipped` in `--json` so silent undercount is
  detectable. `// ponytail:` comment names the fragility.
- Worktree path reuse: a recycled handle/worktree path attributes old sessions
  to the new task. v1 accepts this (note in help text); filtering by task
  `started_at` vs session timestamps is the v1.1 fix if it bites.
- Big session dirs → slow scan. Accepted for v1 (read-only command, operator
  invoked); mtime cache is the named upgrade path.

## Validation
`cargo test -p ajax-core -p ajax-cli` · `cargo clippy --all-targets -- -D warnings`
· `cargo fmt --check`. Manual: `AJAX_PROFILE=dev` with a task that actually ran,
`ajax cost --json` shows nonzero tokens matching a hand-check of one file.

## v1.1 (deferred, note only)
`[pricing]` config → dollars; `started_at` session filtering; mtime cache;
web cockpit column.
