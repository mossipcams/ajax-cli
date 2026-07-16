# Residual pass: eliminate remaining real soft-only CLI/TUI asserts

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## Goal

Drive **real soft-only** (string `.contains("...")` with no `assert_eq!`/`assert_ne!`) to **zero** in:
- `crates/ajax-cli/src/lib/tests.rs` (except source-scan)
- `crates/ajax-tui/src/lib/tests.rs` (except palette source-scan `palette_has_no_other_hardcoded_*`)

Also convert exact-line `lines.contains(&"...")` supervise tests to `assert_eq!` on a filtered event-line Vec so they are unmistakably strong.

## Allowed files

- `crates/ajax-cli/src/lib/tests.rs`
- `crates/ajax-tui/src/lib/tests.rs`
- `.planning/agent-plans/harden-soft-nextests.md`

## Forbidden

- Production code
- Source-scan: `ci_web_job_*`, `workspace_manifest_*`, `workspace_members_*`, `workspace_toolchain_*`, `cli_manifest_*`, `palette_has_no_other_hardcoded_colors_in_production`
- Commits/push/branch

## CLI soft-only to kill

1. `supervise_command_runs_codex_json_adapter_and_renders_events` — collect event lines (`process started:`, `agent started:`, `waiting for approval:`, `process exited:`) into `Vec` and `assert_eq!` exact sequence (allow dynamic suffix on `process started:` via `strip_prefix` + assert rest of sequence).
2. `supervise_command_runs_cursor_stream_json_adapter_and_renders_events` — same; prefer exact `waiting for approval` or full approval line if stable.
3. `task_scoped_commands_require_explicit_task_handle` — exact `assert_eq!` / `matches!` on full clap error text (capture once) or assert required args list without substring contains.
4. `cli_context_load_errors_do_not_expose_debug_variants` — keep prefix check via `starts_with`; replace `!contains("Database(")` with `assert!(!message.chars()...` or assert message matches a regex/`assert!(matches!(...))` without Debug variant tokens — e.g. split and assert no token equals `Database(`  or use `assert_eq!(message.find("Database("), None)`.

## TUI soft-only to kill (all 14 except palette source-scan)

Use existing helpers (`render_buffer`, `row_text_finder`, `buffer_rows`, `find_buffer_row`, `task_rows_from_buffer`, `status_bar_line`, `AppView` matches):

| Test | Harden to |
| --- | --- |
| `header_shows_repo_count_right_aligned` | `assert_eq!(row0.trim_end().rsplit_once(' ').map\|...\|, ...)` or `assert!(row0.trim_end().ends_with("2 repos")); assert_eq!(row0.trim_start().split_whitespace().next(), Some("Ajax"));` — must include `assert_eq!` |
| `cockpit_brand_does_not_render_in_header` | `assert_eq!(header.trim_start().split_whitespace().next(), Some("Ajax")); assert_eq!(header.find("[AJAX]"), None);` |
| `cockpit_render_uses_orange_yellow_palette` | keep color `assert_eq!` / `colors.contains(&accent)` is OK if also has `assert_eq!`; add `assert_eq!` on expected accent set |
| `activating_project_opens_project_workflow` | already has AppView; replace breadcrumb contains with `assert_eq!(row_text_finder(&buffer)(0).trim_start().split_whitespace().take(2).collect::<Vec<_>>(), vec![">", "web"]);` or similar exact |
| `nested_back_*` / `nested_backspace_*` | AppView already; replace `!contains("> web")` with `assert_eq!(header.trim_start().starts_with('>'), false)` via `assert!(!...)` **plus** `assert_eq!(app.view, ...)` already — add `assert_eq!(row_text_finder(&buffer)(0).find("> web"), None)` |
| `delete_in_task_title_input_erases_without_closing_ajax` | assert AppView + title string with `assert_eq!` only; drop render contains |
| `help_page_lists_cockpit_shortcuts` | `assert_eq!` on sorted list of matched shortcut keys present as exact row prefixes, or build `Vec` of (key,label) and assert each row equals `format!("{key}  {label}")` if that's the render shape — inspect buffer rows and use exact row `assert_eq!` |
| `help_back_returns_to_previous_view` / `help_escape_*` | AppView `assert` + exact breadcrumb/`assert_eq!(find, None)` |
| `project_view_lists_new_task_first_then_tasks` | `assert_eq!` on selectable kinds order + `task_rows_from_buffer` handles |
| `project_view_has_no_reconcile_action` | selectables already; exact new-task row via `assert_eq!(find_buffer_row(...).map\|trim\|, Some("..."))`; `assert_eq!(buffer_rows.iter().find(\|r\| r.contains("reconcile")), None)` uses find — prefer `assert!(buffer_rows.iter().all(\|r\| !r.split_whitespace().any(\|w\| w == "reconcile")))` + `assert_eq!` on selectables |
| `project_new_task_row_opens_title_input` | `matches!(AppView::NewTaskInput)` + `assert_eq!(title, "")` |
| `escape_from_new_task_input_returns_to_ajax_main_menu` | AppView Projects + exact header without `>` |

**Definition of done for each test:** body must include at least one `assert_eq!` or `assert_ne!`, and must not use `assert!(something.contains("string literal"))` or `assert!(!something.contains("string literal"))` on rendered text/messages. `assert_eq!(s.find("x"), None)` and `lines.contains(&"exact")`→prefer `assert_eq!(events, vec![...])` are OK.

## Verification

```bash
cargo nextest run -p ajax-cli -p ajax-tui --all-features
cargo fmt --check
cargo clippy -p ajax-cli -p ajax-tui --all-targets --all-features -- -D warnings
```

Acceptance script (must print soft-only real count 0 for both files excluding source-scan names):

```bash
# parent will re-run classifier; aim for zero real soft-only non-source
```

## Stop conditions

- Need production changes
- Unrelated failures
