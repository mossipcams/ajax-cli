# Test Quality Refactor Plan

Goal: every test is a small, focused behavioral spec — one meaningful behavior per
test, a clear reason to fail. Remove tests that assert source text, manifest prose,
or documentation wording instead of behavior. Split combined multi-case tests.
No production code changes except where a test seam is explicitly added.

Authorization note: this task deliberately deletes tests, which overrides the
default "never delete test assertions" rule. Scope of the override: only the
tests named in this plan, only for the reasons stated. Behavioral assertions are
never weakened — every deletion is either (a) a non-behavioral source/prose grep,
or (b) replaced by an equal-or-stronger focused spec named in the same task.

Keep list (explicitly NOT touched):
- All `rust_arkitect` rules in `architecture.rs` (all four crates)
- `lifecycle.rs::production_code_does_not_assign_lifecycle_status_outside_authority_module` (whole-crate invariant lint)
- `ajax-tui::palette_has_no_other_hardcoded_colors_in_production` (whole-crate invariant lint)
- Release/CI/config guards: `release_please_*` (5 tests), `workspace_members_inherit_metadata_lints_and_dependencies`, `tui_dependency_uses_audit_clean_ratatui_feature_set`, `cli_manifest_exposes_lightweight_build_without_interactive_dependencies`, `audit_policy_has_no_accepted_warnings`, all of `tests/repo_hooks.rs`
- `ajax-web/src/slices/install.rs` asset-content tests (they assert served bytes — the install slice's actual output contract)
- `crates/ajax-cli/tests/smoke_user_flows.rs` — untouched (hard rule)
- `crates/ajax-cli/tests/live_cli.rs` — untouched (no changes needed)

No files under any `tests/` directory are modified by this plan.

---

## Phase 1 — Delete non-behavioral tests (source-grep / prose-pinning)

### Task 1: `ajax-cli/src/dispatch.rs` — remove all 6 source-grep tests
- Delete: `cli_task_dispatch_uses_core_task_command_kind_without_local_enum`,
  `cli_drop_dispatch_delegates_observed_drop_decision_to_core_operation`,
  `cli_resume_review_dispatch_delegates_execution_to_core_task_command_operation`,
  `cli_ship_dispatch_delegates_execution_to_core_task_command_operation`,
  `cli_repair_dispatch_delegates_execution_to_core_task_command_operation`,
  `cli_task_dispatch_no_longer_owns_legacy_execute_apply_blocks`
- The module becomes empty; remove the `#[cfg(test)] mod tests` block.
- Coverage check before deleting: the behavioral claims are covered by
  `merge_execute_with_yes_marks_task_merged`, `check_execute_success_promotes_active_task_to_reviewable`,
  `drop_execute_*` family, and `task_command_routes_use_core_kinds_without_cli_mapper`
  (behavioral part) in `lib/tests.rs`. Confirm each by running them.
- Verify: `cargo nextest run -p ajax-cli`

### Task 2: `ajax-cli` — remove greps in `execution_dispatch.rs`, `cockpit_actions.rs`, `context.rs`, `supervise.rs`
- Delete from `execution_dispatch.rs`: `cli_start_dispatch_delegates_task_transaction_to_core_operation`,
  `cli_tidy_dispatch_delegates_cleanup_execution_to_core_operation`,
  `web_dispatch_delegates_to_mobile_web_server`, `web_dispatch_with_paths_can_persist_mobile_actions`
  (behavior covered by `new_execute_*`, `sweep_execute_*`, web router tests)
- Delete from `cockpit_actions.rs`: `pending_cockpit_task_actions_use_core_task_command_operations`
  (covered by the `pending_cockpit_*` family in `lib/tests.rs`)
- Delete from `context.rs`: `context_load_uses_store_loader_without_event_mode`
  (covered by `ordinary_context_load_skips_registry_event_history`)
- Delete from `supervise.rs`: `supervise_module_does_not_keep_single_use_event_predicate`
  (covered by `retained_supervisor_events_are_bounded_for_noisy_process_output`)
- Verify: `cargo nextest run -p ajax-cli`

### Task 3: `ajax-cli/src/cockpit_backend.rs` — remove 7 greps; add one focused spec if seam exists
- Delete: `cockpit_backend_live_refresh_delegates_runtime_refresh_to_core`,
  `cockpit_snapshot_build_explicitly_rebuilds_core_projection`,
  `cockpit_backend_does_not_keep_test_only_agent_status_refresh_wrappers`,
  `cockpit_backend_does_not_keep_cockpit_watch_frame_wrapper`,
  `interactive_cockpit_auto_starts_mobile_web_companion`,
  `mobile_web_companion_uses_child_process_and_guard`,
  `mobile_web_companion_preserves_parent_ajax_context_environment`
  (env behavior covered by `mobile_web_companion_preserves_full_dev_runtime_context`)
- Gap to close: the `--no-web` opt-out decision. If a pure decision function or
  command-builder seam exists, add
  `cockpit_entry_skips_mobile_web_companion_when_no_web_flag_set` (one assert path).
  If only the process-spawning path exists, record the gap as a documented
  limitation in this plan instead of writing a process-spawning test.
- DOCUMENTED GAP (resolved as limitation): the `--no-web` branch is a one-line
  inline `if subcommand.get_flag("no-web")` in `render_interactive_cockpit_command`
  (`cockpit_backend.rs:69`), immediately ahead of a TCP bind + process spawn.
  There is no pure seam, and extracting one would add exactly the kind of trivial
  forwarder this repo's standards prohibit. Flag parsing is covered by
  `cockpit_command_accepts_mobile_web_opt_out`; the companion command/env contract
  is covered by `mobile_web_companion_preserves_full_dev_runtime_context` and
  `mobile_web_ports_are_separate_for_stable_and_dev`. The skip-spawn decision
  itself is exercised only manually.
- Verify: `cargo nextest run -p ajax-cli cockpit`

### Task 4: `ajax-cli/src/task_session.rs` and `web_backend.rs` — remove greps
- Delete from `task_session.rs`: `task_session_bridge_has_no_debug_log_environment_hook`,
  `task_operator_terminal_uses_inherited_stdio_instead_of_reopening_dev_tty`,
  `task_session_does_not_keep_screen_sequence_wrappers`
  (keep `task_screen_commands_clear_normal_buffer_without_disabling_scrollback` —
  it pins the actual escape-byte contract sent to terminals)
- Delete from `web_backend.rs`: `cli_web_backend_delegates_pwa_reads_to_ajax_web`,
  `web_supported_filter_lives_in_ajax_web_cockpit_slice`,
  `cli_web_backend_uses_axum_runtime_server`
  (router/action behavior covered by `http_router_*`, `action_endpoint_*`, and
  ajax-web `axum_*` tests; the web-supported-filter behavior is covered by
  `ajax-web/src/action_vocabulary.rs` tests)
- Verify: `cargo nextest run -p ajax-cli`

### Task 5: `ajax-cli/src/lib/tests.rs` — remove pure source-grep tests
- Delete: `cli_does_not_keep_duplicate_conflict_classifier_module`,
  `cli_builder_does_not_keep_trivial_command_forwarders`,
  `web_companion_stays_out_of_cli_backend`,
  `readonly_dispatch_does_not_have_adapter_wiring_placeholder`,
  `snapshot_only_read_dispatch_is_explicitly_named`,
  `cli_context_and_render_logic_live_in_modules`,
  `task_command_kind_uses_operator_review_language`,
  `textual_frontend_files_are_removed`, `textual_startup_scripts_are_removed`
- Verify: `cargo nextest run -p ajax-cli`

### Task 6: `ajax-cli/src/lib/tests.rs` — remove doc-prose tests, slim mixed ones
- Delete (prose-pinning): `architecture_documents_no_legacy_json_state_migration`,
  `agents_documents_no_legacy_code_rule`, `architecture_documents_current_workspace_boundaries`,
  `architecture_documents_current_persistence_and_cockpit_stack`,
  `architecture_documents_current_execution_and_cli_shape`,
  `readme_documents_native_rust_cockpit`,
  `smoke_workflow_script_is_documented_for_release_validation`
- Slim (keep config asserts, drop prose asserts):
  - `release_hygiene_documents_install_config_and_release_process` → keep the
    workspace-manifest assertions (repository URL, version, lints); drop
    README/LICENSE/CHANGELOG/RELEASE wording asserts; rename to
    `workspace_manifest_pins_repository_metadata_and_lints`
  - `workspace_style_files_document_repo_hygiene` → keep clippy.toml/rustfmt.toml/
    rust-toolchain.toml assertions; drop STYLE.md/AGENTS.md prose asserts; rename to
    `workspace_toolchain_and_lint_configs_are_pinned`
- Verify: `cargo nextest run -p ajax-cli`

### Task 7: `ajax-cli/src/lib/tests.rs` — strip grep assertions from mixed behavioral tests
- `repair_command_renders_configured_test_plan`: drop the 2 source-grep asserts,
  keep the 2 rendered-plan asserts
- `task_command_routes_use_core_kinds_without_cli_mapper`: drop the mapper greps,
  keep the four rendered-output asserts and the `OperatorAction::from_label` assert;
  rename to `task_verbs_render_core_operation_titles`
- `failed_pending_new_task_action_marks_state_changed_for_cockpit_recovery`:
  drop the trailing source-grep assert, keep all behavioral asserts
- Verify: `cargo nextest run -p ajax-cli`

### Task 8: `ajax-tui` — remove all source-grep tests
- Delete from `lib.rs`: `active_tui_api_does_not_export_legacy_cockpit_facades`,
  `rendering_helpers_live_in_rendering_module`, `feed_geometry_helpers_live_in_layout_module`,
  `tui_root_does_not_keep_notice_row_forwarder`, `selectable_layout_does_not_build_rendered_feed_items`,
  `tui_does_not_keep_local_evidence_label_mapper`, `lib_does_not_import_terminal_mode_command_mirror`,
  `navigation_module_does_not_keep_single_use_backspace_helper`,
  `layout_module_does_not_keep_selectable_row_ranges_helper`,
  `cockpit_state_does_not_keep_project_repo_forwarders`
- Delete from `actions.rs`: `action_chrome_stores_finished_styles_without_style_builder_methods`
- Delete from `input.rs`: `input_module_does_not_keep_navigation_forwarders`
- Delete from `rendering.rs`: `rendering_does_not_keep_trivial_forwarders`
- Delete from `runtime.rs`: `terminal_mode_tests_do_not_keep_command_mirror`
  (behavioral `terminal_mode_helpers_write_crossterm_commands` stays)
- Keep: `palette_has_no_other_hardcoded_colors_in_production` (whole-crate lint)
- Verify: `cargo nextest run -p ajax-tui`

### Task 9: `ajax-core/src/lib.rs` — remove module-layout greps; replace browser guard with manifest guard
- Delete: `crate_root_does_not_keep_package_identity_wrapper`,
  `avoids_duplicate_cockpit_snapshot_contract`, `crate_root_does_not_export_reconcile_module`,
  `command_doctor_checks_live_in_focused_module`, `command_task_projection_lives_in_focused_module`,
  `command_task_lookup_lives_in_focused_module`, `use_case_contracts_are_not_owned_by_command_facade`,
  `architecture_rules_can_use_rust_arkitect` (vacuous, zero asserts),
  `architecture_rules_are_executable`, `command_review_compatibility_paths_delegate_to_review_slice`
- Replace `core_remains_browser_agnostic` with a focused manifest guard:
  `core_manifest_declares_no_web_dependencies` — assert ajax-core's Cargo.toml
  declares none of `axum`, `rcgen`, `rustls`, `web-push`. Fails when someone adds
  a web dependency to core; that is the actual invariant.
- Verify: `cargo nextest run -p ajax-core`

### Task 10: `ajax-core` — remove remaining greps in `commands.rs`, `attention.rs`, `live.rs`, `registry.rs`, `output.rs`, `registry/sqlite.rs`
- Delete: `commands.rs::teardown_commands_use_force_flag_without_mode_enum`
  (force-flag behavior covered by `teardown_step_result_*` and plan-command asserts),
  `attention.rs::attention_module_does_not_assign_lifecycle_status` and
  `live.rs::live_projection_module_does_not_own_lifecycle_mutation`
  (both subsumed by the kept whole-crate lifecycle-authority lint and by the
  behavioral `live_projection_functions_do_not_mutate_lifecycle_or_substrate`),
  `registry.rs::registry_facade_does_not_write_snapshot_files`,
  `registry.rs::registry_has_no_legacy_json_state_import_surface`,
  `registry.rs::registry_facade_keeps_sqlite_encoding_in_persistence_modules`,
  `registry.rs::registry_facade_does_not_own_json_export_boundary`
  (persistence behavior fully covered by the sqlite.rs suite, including
  `sqlite_registry_store_rejects_legacy_payload_schema_without_migration`),
  `output.rs::output_contracts_do_not_keep_unused_format_wrapper`,
  `registry/sqlite.rs::sqlite_registry_store_batches_task_detail_loads` (pins SQL text)
- Strip the 2 source-grep asserts from
  `commands.rs::inbox_returns_annotation_items_from_task_annotations`, keep its
  5 behavioral asserts
- Verify: `cargo nextest run -p ajax-core`

### Task 11: `ajax-core/src/task_operations.rs` — remove greps, split the kernel test
- Delete (pure grep): `start_operation_execution_uses_shared_operation_kernel`,
  `task_command_operation_returns_plain_execution_result`,
  `sweep_cleanup_operation_returns_plain_execution_result`,
  `drop_operation_returns_plain_execution_result`,
  `drop_operation_plan_does_not_duplicate_confirmation_state`,
  `operation_errors_use_plain_tuples_without_constructor_helpers`
- `operation_kernel_handles_confirmation_blocking_nonzero_and_success`: drop its
  5 grep asserts and split the 4 behavioral cases into 4 focused specs:
  `operation_kernel_requires_confirmation_before_running_risky_plan`,
  `operation_kernel_refuses_blocked_plan_without_running_commands`,
  `operation_kernel_surfaces_nonzero_exit_after_partial_execution`,
  `operation_kernel_returns_outputs_for_successful_plan`
- `task_command_operation_plans_single_task_commands_without_derived_policy_fields`:
  keep the behavioral title assert, drop the 3 greps; rename to
  `task_command_operation_plans_use_operator_titles`
- Verify: `cargo nextest run -p ajax-core task_operations`

### Task 12: `ajax-web` — remove greps and the vacuous no-op test
- Delete from `lib.rs`: `web_crate_declares_vertical_slice_layout`,
  `web_mechanisms_stay_out_of_slice_names`, `architecture_rules_are_executable`
  (slice isolation is already enforced by the kept rust_arkitect rules)
- Delete from `runtime.rs`: `production_server_uses_axum_instead_of_manual_http_loop`,
  `runtime_keeps_custom_connection_serving_out_of_production`
  (axum serving behavior covered by the `axum_*` integration tests)
- Delete from `adapters/server.rs`: `schedule_process_restart_is_no_op_in_tests`
  (zero assertions)
- Verify: `cargo nextest run -p ajax-web`

---

## Phase 2 — Split combined multi-case behavioral tests

One behavior per test; shared setup goes to a helper or `rstest` fixture.

### Task 13: `ajax-core/src/task_operations.rs` — split success/failure pairs
- `ship_task_operation_marks_merged_or_records_merge_failure` →
  `ship_operation_marks_task_merged_on_success` and
  `ship_operation_records_conflict_attention_on_merge_failure`
- `repair_task_operation_marks_check_success_or_failure_in_core` →
  `repair_operation_promotes_task_to_reviewable_on_check_success` and
  `repair_operation_records_tests_failed_on_check_failure`
- `resume_and_review_task_operations_execute_in_core_with_reducers` →
  `resume_operation_executes_plan_and_reports_state_change` and
  `review_operation_returns_diff_output_without_state_change`
- Verify: `cargo nextest run -p ajax-core task_operations`

### Task 14: `ajax-core/src/commands.rs` — split the new-task contract test
- `new_task_contract_preserves_generated_names_and_duplicate_handles` →
  `new_task_plan_slugifies_title_into_branch_session_and_handle` and
  `new_task_plan_rejects_duplicate_visible_handle`
- Verify: `cargo nextest run -p ajax-core commands`

### Task 15: `ajax-cli/src/lib/tests.rs` — split CLI multi-case tests
- `dev_and_stable_invocations_load_and_save_only_their_selected_db` →
  `reads_use_only_the_selected_profile_db` and
  `writes_persist_only_to_the_selected_profile_db`
- `supervise_with_task_requires_existing_visible_task` →
  `supervise_with_task_runs_for_visible_task`,
  `supervise_with_task_rejects_unknown_task`,
  `supervise_with_task_rejects_removed_task`
- Verify: `cargo nextest run -p ajax-cli`

---

## Phase 3 — Fix weak/self-confirming behavioral tests

### Task 16: make new/start open-mode expectations hermetic
- `expected_new_task_open_command` (`lib/tests.rs:558`) calls production
  `current_open_mode()` to build the expectation — self-confirming and dependent
  on ambient `$TMUX` (known CI flake source). Used by 4 tests.
- Fix: route the 4 tests through the existing OpenMode injection seam (the same
  approach used for the earlier CI hermeticity fix via `render_task_command`);
  if the start path lacks a seam, add a minimal one (function parameter defaulting
  to `current_open_mode()`, test-only injection — smallest possible production change,
  TDD: write the failing hermetic test first by asserting a fixed OpenMode under a
  forced `$TMUX`-independent path).
- Verify: `cargo nextest run -p ajax-cli new_execute` (and once with `TMUX` set:
  `TMUX=/tmp/fake,1,0 cargo nextest run -p ajax-cli new_execute`)

### Task 17: tighten fixture-dependent renderer assertion
- `ajax-tui::cockpit_text_renderer_does_not_show_review_lane` asserts
  `!content.contains("review")` (lowercase) — fails if any task title contains
  the word. Tighten to assert the lane header `"Review:"` is absent and keep the
  positive assert. One behavior: the review lane is not rendered.
- Verify: `cargo nextest run -p ajax-tui`

---

## Phase 4 — Final validation

### Task 18: full workspace validation and report
- `cargo fmt --check`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features`
- Report: tests deleted (count + names by file), tests added/split, any
  documented coverage gaps (Task 3), any commands that failed.

---

## Expected outcome
- ~80 non-behavioral tests removed (source greps, prose pins, vacuous tests)
- ~12 new focused behavioral specs (splits + replacements + 1 manifest guard)
- 4 tests made hermetic w.r.t. ambient tmux
- No production behavior changes; at most one minimal injection seam (Task 16)
- Architecture enforcement consolidated on rust_arkitect + the two whole-crate lints
