# Wave 2 packet: Harden remaining ajax-cli soft-only tests

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## Goal

Harden remaining soft-only tests in `crates/ajax-cli/src/lib/tests.rs` (Wave 1 already done). Replace loose `contains` with typed/exact asserts. Leave source-scan alone.

## Allowed files

- `crates/ajax-cli/src/lib/tests.rs`
- `.planning/agent-plans/harden-soft-nextests.md`

## Forbidden changes

- Production code
- Source-scan tests: `cli_manifest_compiles_tui_and_supervisor_unconditionally`, `ci_web_job_runs_mobile_webkit_smoke`, `workspace_toolchain_and_lint_configs_are_pinned`, `workspace_manifest_pins_repository_metadata_and_lints`, `workspace_members_inherit_metadata_lints_and_dependencies`
- Deleting tests / weakening coverage
- Commits/push/branch
- `.cursor/plans/`

## Context evidence

- Graphify/Serena/ast-grep: NOT_REQUIRED — tests-only; exact fn names below

## Soft-only targets (harden all of these)

### Supervise / adapters
- `supervise_command_runs_codex_json_adapter_and_renders_events`
- `supervise_command_runs_cursor_stream_json_adapter_and_renders_events`
- `supervise_command_keeps_stderr_context_on_agent_exit`
- `supervise_with_task_runs_for_visible_task`

Prefer: assert exact rendered lines via `output.lines().any(|l| l == "...")` for known event lines; or collect matching lines into Vec and `assert_eq!`. For errors: `assert_eq!` / `matches!` on full message (see existing `supervise_with_task_rejects_removed_task`).

### Errors / doctor / help / chrome
- `binary_prints_cli_errors_with_display_formatting`
- `read_only_cockpit_rejects_interactive_mode_before_navigation_only_tui`
- `help_output_is_successful`
- `bare_command_reports_missing_subcommand_as_error`
- `readonly_context_rejects_supervise_instead_of_reporting_placeholder_success`
- `doctor_reports_context_path_health`
- `task_scoped_commands_require_explicit_task_handle`
- `new_command_requires_task_title`
- `readonly_context_rejects_execute_before_running_external_commands`
- `cli_context_load_errors_do_not_expose_debug_variants`
- `new_execute_rejects_existing_task_before_native_provisioning`
- `new_execute_requires_task_title_before_native_provisioning`
- `external_command_failure_uses_operator_facing_message`

Prefer exact `assert_eq!(error, ...)` or `matches!(..., message == "...")` or `assert!(stderr.lines().any(|l| l == "..."))`.

### Plan / render / JSON outputs
- `read_command_skips_live_pane_probe_when_cached_runtime_is_fresh`
- `new_command_renders_plan_without_json_panic`
- `repos_command_renders_human_output`
- `tasks_command_renders_json_output`
- `open_command_renders_command_plan`
- `merge_command_renders_json_plan`
- `repair_command_renders_configured_test_plan`
- `review_command_renders_diff_summary_plan`
- `ready_command_renders_review_queue`
- `cli_loads_context_from_config_and_state_files`
- `cli_missing_config_and_state_files_use_empty_context`
- `cli_rejects_legacy_json_state_without_migration`
- `task_verbs_render_core_operation_titles`
- `drop_execute_hard_remove_survives_subsequent_tasks_read`
- `agent_runtime_command_runs_without_loading_ajax_context`

Prefer `--json` + field `assert_eq!`, registry handle lists, or exact plan lines. Mirror Wave 1 patterns already in this file.

### Cockpit action messaging
- `cockpit_new_task_action_guides_operator_to_project_input`
- `cockpit_known_actions_never_return_command_hints`
- `removed_cockpit_task_actions_are_unknown`
- `cockpit_unknown_action_does_not_suggest_shell_command`
- `pending_new_task_action_requires_completed_title`
- `pending_new_task_action_does_not_run_without_title`
- `cockpit_remove_action_requires_confirmation_before_running`
- `pending_cockpit_removed_actions_are_rejected`

Prefer `assert_eq!` on ActionOutcome / exact message strings / enum matches rather than substring contains.

## Test-first

`NOT_APPLICABLE: tests-only hardening.`

## Edit instructions

1. For each target: replace soft `contains`/`!contains` with typed/exact asserts without reducing what is checked.
2. If a test is already strong enough after Wave 1 patterns exist nearby, still eliminate soft-only classification (must have `assert_eq!`/`assert_ne!`/`matches!` on values).
3. Update Wave 2 checkboxes in the agent plan when done.

## Verification

```bash
cargo nextest run -p ajax-cli --all-features -E 'test(/supervise_command|doctor_reports|help_output|bare_command|new_command_renders|tasks_command_renders|repos_command_renders|ready_command|open_command_renders|merge_command|repair_command|review_command|cockpit_.*action|task_scoped|readonly_context|external_command_failure|cli_loads|cli_missing|cli_rejects_legacy|cli_context_load|drop_execute|agent_runtime|task_verbs|read_command_skips|binary_prints|read_only_cockpit/)'
```

Also ensure the full lib test binary still compiles:
```bash
cargo nextest run -p ajax-cli --all-features -E 'test(workspace_manifest)' # source-scan still passes
```

## Acceptance

- Listed soft-only tests hardened or confirmed already eq-strong
- Source-scan untouched
- Focused nextest green
- No production diff

## Stop conditions

- Production edit needed
- Unrelated failures
- Scope beyond listed tests
