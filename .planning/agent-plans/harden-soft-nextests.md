# Harden CLI + TUI soft nextests

## Scope

- In: soft-only CLI/TUI tests (typed field / CommandSpec / parsed-row asserts)
- Out: source-scan, architecture greps, web install/CSS, golden full frames

## Delegation decision

`Delegation decision: delegated via model-router` (Waves 1–3 via `cursor-delegate` / `composer-2.5` test-only).
Wave 4 live/smoke soft-only: parent LOCAL (4 tests, smaller than a wave packet).
Residual real soft-only pass: `Delegation decision: delegated via model-router` (packet `harden-soft-nextests-residuals.md`).

## Checklist

### Residual pass 2 (mixed contains → zero, excl source-scan)
- [x] CLI mixed `assert!(...contains(...))` eliminated (excl source-scan; 1 Vec::contains remains)
- [x] TUI mixed `assert!(...contains(...))` eliminated (excl source-scan)
- [x] nextest + fmt + clippy green

### Residual pass (real soft-only → zero, excl source-scan)
- [x] CLI soft-only residuals hardened
- [x] TUI soft-only residuals hardened
- [x] Soft-only real count 0 (excl source-scan)
- [x] nextest + fmt + clippy green

### Ledger
- [x] This file created

### Wave 1 — CLI cockpit + dispatch
- [x] `snapshot_dispatch_module_routes_read_commands`
- [x] `execution_dispatch_module_routes_mutating_commands`
- [x] `cockpit_backend_module_renders_snapshot_frame`
- [x] `cockpit_watch_renders_dashboard_from_backend_state`
- [x] Profile DB soft cases (`reads_use_only_the_selected_profile_db`, related)
- [x] Focused nextest green

### Wave 2 — CLI remaining soft-only
- [x] Supervise / doctor / errors / plan output hardened
- [x] Exact chrome where appropriate
- [x] Source-scan left alone
- [x] Focused nextest green

### Wave 3 — TUI
- [x] `task_rows_from_buffer` (or equivalent) helper
- [x] Soft render/nav tests converted
- [x] Palette/source greps skipped
- [x] `cargo nextest run -p ajax-tui` green

### Wave 4 — live/smoke
- [x] Soft-only touched: `ajax_start_creates_task_like_new`, `ajax_tidy_dispatches_like_sweep`, `smoke_cockpit_reattaches_after_interrupted_attach_client`, `smoke_rooted_orphan_recovery_stays_scoped_to_its_repo`

### Validation
- [x] `cargo nextest run -p ajax-cli -p ajax-tui --all-features` — 540 passed
- [x] `cargo fmt --check` — pass
- [x] `cargo clippy -p ajax-cli -p ajax-tui --all-targets --all-features -- -D warnings` — pass

## Deviations

- Wave 4 smoke reattach: stdout embeds EINTR mid-buffer, so ordered `find` indices used instead of exact line equality.
- Residual pass: supervise tests use filtered event-line `assert_eq!` sequence; clap missing-arg errors compared via trimmed `assert_eq!` on message text; TUI new-task row chrome is `>+ start a new task` (no space between prefix and glyph).
- Parent review: hardened `tui_dependency_uses_audit_clean_ratatui_feature_set` (manifest audit, not in named source-scan list) with `assert_eq!`/`assert_ne!` on `find`/`lines`.
- Residual pass 2: cleared remaining mixed string-`contains` asserts; source-scan tests intentionally keep substring checks; one `Vec<CommandSpec>::contains` membership check remains (not a string soft assert).

## Validation results

```text
cargo nextest run -p ajax-cli -p ajax-tui --all-features
# 540 passed

cargo fmt --check
# pass

cargo clippy -p ajax-cli -p ajax-tui --all-targets --all-features -- -D warnings
# pass

assert!(...contains("...")...) remaining excl source-scan
# ajax-cli tests.rs: 0
# ajax-tui tests.rs: 0
```
