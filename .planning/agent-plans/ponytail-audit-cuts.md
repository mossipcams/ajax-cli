# Ponytail audit cuts

## Scope

Behavior-preserving cleanup from the full ponytail-audit finding list.
No lifecycle/registry/terminal-model semantics changes. No dependency churn.

## Non-goals

- Rewriting `commands.rs` / `sqlite.rs` production logic
- Deleting architecture boundary *rules* (only dedupe harness)
- Weakening or skipping tests
- Changing Web Cockpit ghostty/raw-tmux contracts

## Delegation decision

`Delegation decision: delegated via model-router`

- Wave 1: MiniMax-M3 (mechanical deletes/wrappers)
- Wave 2–3: GLM 5.2 (trait/module folds)
- Wave 4–5: MiniMax-M3 or GLM as needed
- Wave 6: Cursor Grok 4.5 High (Svelte/TS terminal)

Parent reviews every diff and runs validation; delegates never commit/push.

## Checklist

### Wave 1 — Dead code / thin wrappers

- [x] Delete `CountingCommandRunner`
- [x] Rename `web_companion_backend` → `web_backend`
- [x] Collapse refresh ladder (drop `_with_agent_status_cache`)
- [x] Drop unused TUI `run_interactive` / `_with_flash`
- [x] Drop `ProcessProtocol::parse_stdout_line`
- [x] Replace `AgentAdapter` with free function
- [x] Remove empty `analysis.rs` wrapper
- [x] Focused check/nextest

### Wave 2 — Single-impl traits

- [x] Drop `RegistryStore` trait; methods on `SqliteRegistryStore`
- [x] Collapse `AgentPromptAdapter` / `NullAdapter`
- [x] Focused tests

### Wave 3 — Module folds

- [x] Fold `use_cases` into commands
- [x] Merge `live_application` into `live`
- [x] Fold `slices/review` into `commands/diff`
- [x] Fold `action_vocabulary` into cockpit/actions
- [x] Shrink observation / `run_with_*` helpers where safe
- [x] Sync `architecture.md`

### Wave 4 — Architecture harness

- [x] Dedupe harness boilerplate; update SLICES

### Wave 5 — Test fixtures

- [x] Shared `QueuedRunner` / `context_with_task` helpers
- [x] Optional test file moves only if low churn

### Wave 6 — Web terminal

- [x] Collapse tiny `terminal*.ts` helpers
- [x] Keep ghostty contracts; trim only redundant tests

### Validation

- [x] `cargo fmt --check` (ran `cargo fmt` to apply; then clean)
- [x] `cargo check --all-targets --all-features`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo nextest run --all-features` — 1540 passed
- [x] `npm run web:check && npm run web:test -- --run` — 0 svelte errors; 357 vitest passed

## Deviations

- Wave 5: skipped merging QueuedRunner variants (commands vs task_operations differ on missing-output behavior); web already has test_support.
- Wave 6: folded terminalSelection+terminalTouchScroll into terminalGestures; left terminalRefit separate (distinct scheduler policy + tests).
- OpenCode MiniMax hung ~12m with empty diff; Wave 1 implemented locally (delegate unavailable).


## Validation results

- `cargo fmt` applied; check clean after
- `cargo check --all-targets --all-features` pass
- `cargo clippy --all-targets --all-features -- -D warnings` pass
- `cargo nextest run --all-features` — 1540 passed (8 binaries)
- `npm run web:check` — 0 errors
- `npm run web:test -- --run` — 29 files / 357 tests passed
- `npm run web:smoke` — skipped (not required for this refactor; unit/e2e terminal contracts covered by vitest)
