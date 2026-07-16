# Wave 3 packet: Harden ajax-tui soft-only render/nav tests

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## Goal

Add a test-only row parse helper and convert soft TUI tests from loose `content.contains(...)` to `assert_eq!` on parsed row fields and/or `App`/`AppView` state.

## Allowed files

- `crates/ajax-tui/src/lib/tests.rs`
- `.planning/agent-plans/harden-soft-nextests.md`

## Forbidden

- Production code under `crates/ajax-tui/src/` outside the tests module in `lib/tests.rs`
- Palette/source greps: `palette_has_no_other_hardcoded_colors_in_production` (and similar source-scan)
- Commits/push/branch; `.cursor/plans/`

## Context evidence

- Graphify/Serena/ast-grep: NOT_REQUIRED
- Existing helpers in same file: `render_to_string`, `render_buffer`, `row_text_finder`, `app_in_project_view`, pipe convention in `task_row_separates_columns_with_pipe`

## Helper to add (near `render_to_string` / `render_buffer`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedTaskRow {
    handle: String,
    status: String,
    action: String,
}

fn parse_task_row(row: &str) -> Option<ParsedTaskRow> {
    // Trim padding; find handle like "web/..." then split on '|' into handle|status|action
    // Match the actual rendered shape from task_row_separates_columns_with_pipe:
    //   "...web/task-0|<status>|Resume..."
}

fn task_rows_from_buffer(buffer: &ratatui::buffer::Buffer) -> Vec<ParsedTaskRow> { ... }
fn task_rows_from_content(content: &str, width: u16) -> Vec<ParsedTaskRow> {
    // If content is flattened symbols from render_to_string, prefer using render_buffer + row_text_finder instead
}
```

Prefer converting tests to use `render_buffer` + per-row text (already used) rather than flattened `render_to_string` soup when asserting columns.

## Convert these soft-only tests (priority order)

### Render / status / chrome (must use parsed rows or exact header lines)
- `task_row_separates_columns_with_pipe` — assert_eq on ParsedTaskRow fields
- `task_rows_render_live_status_when_present`
- `cockpit_row_uses_canonical_status_instead_of_annotation_label`
- `cockpit_row_renders_probe_failure_status_verbatim`
- `task_row_renders_primary_action_label_and_chrome`
- `inbox_section_renders_labeled_header_with_count` — exact `assert_eq!` on header line text
- `cockpit_header_summarizes_review_and_cleanup_pressure`
- `project_rows_summarize_operator_work_by_project`
- `inbox_row_uses_two_column_repo_task_layout`
- `header_shows_repo_count_right_aligned`
- `cockpit_text_renderer_does_not_show_review_lane`
- `top_level_status_bar_does_not_advertise_nested_back_action` — exact status bar line eq
- `projects_view_omits_attention_banner_when_inbox_visible`
- `cockpit_brand_does_not_render_in_header`
- `cockpit_renders_backend_snapshot`
- `main_page_renders_task_statuses_without_opening_project`
- `feed_inbox_items_render_handle_reason_and_action`
- `selected_row_renders_chevron_prefix`
- `feed_uses_named_section_headers_between_groups`
- `help_page_lists_cockpit_shortcuts`
- `project_view_lists_new_task_first_then_tasks`
- `interactive_cockpit_renders_to_narrow_buffer`

### Navigation / view state (prefer AppView matches over contains)
- `activating_project_opens_project_workflow`
- `top_level_back_stays_in_cockpit`
- `top_level_backspace_stays_in_cockpit`
- `top_level_back_variants_stay_in_cockpit`
- `nested_back_returns_to_parent_without_exit`
- `nested_backspace_returns_to_parent_without_exit`
- `delete_in_task_title_input_erases_without_closing_ajax`
- `nested_views_advertise_immediate_back_keys`
- `help_back_returns_to_previous_view`
- `help_escape_returns_to_previous_view`
- `project_view_has_no_reconcile_action`
- `selected_project_only_shows_that_projects_tasks`
- `project_new_task_row_opens_title_input`
- `escape_from_new_task_input_returns_to_ajax_main_menu`
- `rendering_module_exposes_screen_renderer` — if only contains, harden or leave if it's a module path check

### Leave / skip
- `cockpit_render_uses_orange_yellow_palette` — color/style asserts; only harden if currently soft-contains; keep style eq asserts
- `palette_has_no_other_hardcoded_colors_in_production` — source-scan, skip

## Recipe

| Kind | Prefer |
| --- | --- |
| Task row status/action | `assert_eq!(parsed, ParsedTaskRow { handle: ..., status: ..., action: ... })` |
| Headers | find row via `row_text_finder`, `assert_eq!(row.trim(), "-- inbox (3) --")` |
| Nav | `matches!(app.view, AppView::...)` / `assert_eq!(app.selected, ...)` / `assert!(app.expanded_task.is_none())` |
| Negative chrome | `assert!(!rows.iter().any(|r| r == "..."))` or exact absence of a full line |

## Test-first

`NOT_APPLICABLE: tests-only.`

## Verification

```bash
cargo nextest run -p ajax-tui --all-features
```

## Acceptance

- Helper exists and is used by multiple former soft render tests
- Soft-only count in tui tests drops sharply (nav uses AppView; render uses parsed rows)
- Full ajax-tui nextest green
- No production diff

## Stop conditions

- Need production renderer change to expose columns → stop and report
- Unrelated failures
