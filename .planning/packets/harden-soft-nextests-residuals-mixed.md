# Residual pass 2: eliminate remaining mixed soft string-contains asserts

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## Goal

In `crates/ajax-cli/src/lib/tests.rs` and `crates/ajax-tui/src/lib/tests.rs`, eliminate **every** `assert!(...contains("...")...)` / `assert!(!...contains("...")...)` except named source-scan tests.

Keep `find(|row| row.contains(...))` as **locators** only when followed by typed/`assert_eq!` on the found row — prefer upgrading locators to parsers where helpers exist.

## Allowed files

- `crates/ajax-cli/src/lib/tests.rs`
- `crates/ajax-tui/src/lib/tests.rs`
- `.planning/agent-plans/harden-soft-nextests.md`

## Skip (source-scan)

`ci_web_job_*`, `cli_manifest_*`, `workspace_manifest_*`, `workspace_members_*`, `workspace_toolchain_*`, `tui_dependency_uses_audit_clean_ratatui_feature_set`, `palette_has_no_other_hardcoded_colors_in_production`

## CLI targets (replace soft contains)

| Test | Replace with |
| --- | --- |
| `cli_error_display_omits_internal_enum_wrapping` | `assert_eq!(error.to_string().find("CommandFailed"), None)` |
| `cockpit_watch_renders_refreshed_live_status_in_frame` | exact line `lines.contains(&"web/fix-login\t...")` or `assert_eq!` on that TSV line |
| `status_command_refreshes_live_state_from_tmux` | same |
| `supervise_with_task_persists_supervisor_state_to_sqlite` | exact event line in filtered Vec / `lines.contains(&"waiting for approval: cargo test")` → prefer `assert_eq!` filtered events |
| `refreshed_read_persists_recovered_ajax_task_without_duplicates` | JSON parse + `assert_eq!(handles, ...)` |
| `state_export_writes_registry_snapshot_without_overwriting` | exact output line + `serde_json` field eqs for repo/handle |
| `new_execute_*` recorded task | `lines.any(\|l\| l == "recorded task: ...")` → also registry/`assert_eq!` handle (already may have); use exact line membership via `assert!(lines.contains(&"..."))` is OK **only if** also has `assert_eq!` — better: `assert_eq!(lines.iter().find(\|l\| l.starts_with("recorded task:")), Some(&"recorded task: web/fix-login"))` |
| drop/sweep/error message contains | exact `assert_eq!(message, "...")` or `assert_eq!(message.find(...), None)` / `starts_with` + `assert_eq!` |
| plan/resume blocked JSON | parse JSON `assert_eq!` on fields |
| cockpit confirm/unknown | exact message `assert_eq!` |

## TUI targets

| Test | Replace with |
| --- | --- |
| `top_level_status_bar_*` | `assert_eq!(status_bar.find("esc/h back"), None)` etc + keep positive `assert_eq!` on `q quit` if present |
| `cockpit_row_renders_probe_failure_*` | parsed row status `assert_eq!`; `assert_eq!(content.lines().find(\|l\| l.contains("unknown")), None)` → `assert!(content.lines().all(\|l\| !l.split_whitespace().any(\|w\| w == "unknown")))` + status `assert_eq!` |
| `inbox_row_uses_two_column_*` | parsed inbox row `assert_eq!` on repo/task; absence via `assert_eq!(find("autosnooze/open-deps"), None)` on joined rows |
| `refresh_snapshot_updates_live_status_*` | `task_rows_from_buffer` / App state `assert_eq!` on handle+status; drop content.contains |
| `delete_is_not_*` / `delete_on_top_level_*` | AppView + exact breadcrumb token `assert_eq!` |
| `selected_project_only_shows_*` | breadcrumb tokens + task_rows handles `assert_eq!` |
| `new_task_title_backspace_*` | AppView + title `assert_eq!` only; drop render contains |

## Acceptance

After edits, this must print empty for both files (excl source-scan names):

```python
# any assert!( ... .contains("...") ) remaining → FAIL
```

## Verification

```bash
cargo nextest run -p ajax-cli -p ajax-tui --all-features
cargo fmt --check
cargo clippy -p ajax-cli -p ajax-tui --all-targets --all-features -- -D warnings
```

## Stop conditions

Production edit needed; unrelated failures.
