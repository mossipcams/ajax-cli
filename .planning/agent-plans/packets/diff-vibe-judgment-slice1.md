PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Add deterministic Diff Review judgment to ajax-core: after parsing a unified diff into `DiffFile`s, compute `DiffJudgment` (totals, reading_order, flags) and attach it to `TaskDiffProjection`.

## Scope

### Allowed
- `crates/ajax-core/src/diff_review.rs` only

### Forbidden
- ajax-web / TypeScript / CSS / architecture.md
- LLM or freeform prose summaries
- Reordering or filtering `TaskDiffProjection.files`
- Persisting judgment into task metadata
- AST / language-aware export analysis
- CI observation
- Unrelated refactors

## Acceptance

1. New types (serde rename_all lowercase where enums are serialized later by web DTO; core structs may use same naming):
   - `DiffTotals { files, signal, noise, additions, deletions }` (all u32)
   - `DiffFlagKind`: `unexpected_path | deleted_test | secret_pattern | permission_widen | dependency_manifest | deleted_check_path`
   - `DiffFlagSeverity`: `info | warn | critical`
   - `DiffFlag { kind, severity, path: Option<String>, detail: String }`
   - `DiffJudgment { totals, reading_order: Vec<String>, flags: Vec<DiffFlag> }`
2. `pub fn assess_diff_judgment(files: &[DiffFile]) -> DiffJudgment` implements:
   - **totals**: count files; signal/noise from `role`; sum additions/deletions
   - **reading_order**: signal paths only, sorted by `(additions+deletions)` desc then path asc
   - **flags** (deterministic templates; one flag per matching file/occurrence as specified below):
     - `dependency_manifest` (severity info): basename is `Cargo.toml`, `package.json`, or `Cargo.lock` (also lockfiles even if noise)
     - `deleted_test` (severity warn): status is `deleted` and path looks like a test (`*_test.rs`, contains `.test.`, contains `.spec.`, or path segment `/tests/` / starts with `tests/`)
     - `deleted_check_path` (severity warn): status `deleted` and (path starts with `.github/workflows/` OR basename starts with `verify` under `scripts/` OR path is under `scripts/` and basename contains `verify`)
     - `secret_pattern` (severity critical for `-----BEGIN` / `ghp_` / `AKIA`; else warn): any **added** hunk line (`+` not `+++`) matching conservative substrings: `AKIA`, `ghp_`, `sk-`, `-----BEGIN`, `api_key=` (case-sensitive for prefixes as written; `api_key=` case-insensitive)
     - `permission_widen` (severity warn): any added hunk line containing `chmod 777` or `0o777`
     - `unexpected_path` (severity info): role is Signal and path is NOT under allowlist prefixes: `crates/`, `scripts/`, `.github/`, `crates/ajax-web/web/`, `docs/`, `architecture.md`, `AGENTS.md`, `PRODUCT.md`, `README.md`, `CONTRIBUTING.md`, `Cargo.toml`, `package.json`, `.planning/` — also allow if path equals those root doc names or starts with `web/` (Ajax web frontends). Concrete allowlist: path starts with any of `crates/`, `scripts/`, `.github/`, `docs/`, `.planning/`, `web/` OR path is exactly one of `architecture.md`, `AGENTS.md`, `PRODUCT.md`, `README.md`, `CONTRIBUTING.md`, `Cargo.toml`, `package.json`, `CLAUDE.md`, `deny.toml`, `rustfmt.toml`, `clippy.toml`, `CHANGELOG.md`, `version.txt`, `package-lock.json`, `Cargo.lock`.
3. `TaskDiffProjection` gains `judgment: DiffJudgment`. Both local and PR return paths in `project_task_diff` set `judgment: assess_diff_judgment(&files)` after parse.
4. Unit tests cover: reading_order churn sort; deleted_test; secret_pattern critical; dependency_manifest on Cargo.toml; unexpected_path for e.g. `tmp/scratch.rs`; empty files → empty judgment zeros.

## Constraints

- Do not change `parse_unified_diff` file order or role classification behavior.
- Smallest diff; keep helpers private next to `classify_diff_path`.
- Flag `detail` strings must be short fixed templates like `"deleted test file"` / `"possible secret in added line"` / `"dependency manifest changed"` — not LLM prose.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-core --lib diff_review
      expected: pass (includes new judgment tests)
  broader_checks: []
  reason: Pure core projection logic; focused unit tests validate heuristics.
```

## Stop if

- Need to touch ajax-web or invent browser-side rules
- Heuristics require AST parsing
- Patch would exceed ~400 changed lines

## Code anchors

- `DiffFile` / `TaskDiffProjection` / `project_task_diff` / `parse_unified_diff` / `classify_diff_path` in `crates/ajax-core/src/diff_review.rs`
- Existing tests module at bottom of same file

## Estimated size

~120–180 changed lines in one file.
