# Wave 1 packet: Harden ajax-cli cockpit/dispatch soft tests

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## Goal

Replace soft `contains` asserts in Wave 1 CLI cockpit/dispatch tests with typed JSON/`TaskCard`/`CommandSpec`/registry `assert_eq` so wrong handle, status, or commands fail.

## Allowed files

- `crates/ajax-cli/src/lib/tests.rs`
- `.planning/agent-plans/harden-soft-nextests.md` (check off Wave 1 items only)

## Forbidden changes

- Any production/src non-test code
- Deleting tests or weakening coverage
- Touching source-scan tests (`ci_web_job_runs_mobile_webkit_smoke`, manifest feature greps)
- Editing the Cursor plan file under `.cursor/plans/`

## Context evidence

- Graphify: NOT_REQUIRED — tests-only assert hardening; no architecture boundary change
- Serena: NOT_REQUIRED — anchors collected by parent from source
- ast-grep: NOT_REQUIRED — named test functions are exact edit anchors

## Code anchors

1. `snapshot_dispatch_module_routes_read_commands` (~L51) — currently `contains("\"tasks\"")` + handle
2. `execution_dispatch_module_routes_mutating_commands` (~L117) — `state_changed` + `contains("recorded task:...")`; `RecordingCommandRunner` available; start execute records commands
3. `cockpit_backend_module_renders_snapshot_frame` (~L145) — `contains("Ajax")` / handle; prefer `build_cockpit_snapshot` field eqs (see strong pattern at `cockpit_snapshot_excludes_stale_tasks...` ~L153)
4. `cockpit_watch_renders_dashboard_from_backend_state` (~L898) — multi-contains; assert snapshot fields via `build_cockpit_snapshot` AND/OR parse that frame still includes exact chrome with `assert_eq!(output.matches("Ajax Cockpit").count(), 1)` plus snapshot field eqs on same context
5. `reads_use_only_the_selected_profile_db` (~L662) — dual contains; use `--json` + `serde_json` field eqs on handles (mirror `cockpit_json_returns_single_startup_snapshot` ~L969) OR parse tasks table lines into handles list and `assert_eq!`
6. `writer_entrypoint_uses_selected_runtime_paths` (~L736) — already `--json`; replace contains with `assert_eq!` on parsed `qualified_handle` list

Strong in-file pattern:

```rust
let snapshot = crate::cockpit_backend::build_cockpit_snapshot(&context);
assert_eq!(snapshot.cards[0].qualified_handle, "web/fix-login");
```

JSON pattern:

```rust
let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
// or nested: parsed["tasks"]["tasks"][0] depending on command shape — inspect existing tests
```

For `execution_dispatch`: after execute, assert `rendered.state_changed`, registry contains `web/fix-logout`, and `runner.commands` is non-empty with expected program names (at least git/tmux-style start commands — inspect what `execute_start_task_operation` records; prefer `assert!(runner.commands.iter().any(|c| c.program == "git"))` style field checks, or full `assert_eq!` on a known prefix if stable). Also keep an exact output line assert if useful: `assert!(rendered.output.lines().any(|l| l == "recorded task: web/fix-logout"))` or `assert_eq!` on the relevant line.

## Test-first instructions

`NOT_APPLICABLE: tests-only hardening of existing tests; PRODUCTION_EDIT FORBIDDEN.`

## Edit instructions

1. Harden the six anchors above only (plus any tiny shared test helper in the same file if needed).
2. Do not change production modules.
3. Update `.planning/agent-plans/harden-soft-nextests.md` Wave 1 checkboxes when done.

## Verification commands

```bash
cargo nextest run -p ajax-cli --all-features -E 'test(/snapshot_dispatch_module_routes|execution_dispatch_module_routes|cockpit_backend_module_renders|cockpit_watch_renders_dashboard|reads_use_only_the_selected|writer_entrypoint_uses_selected/)'
```

## Acceptance criteria

- Named tests no longer rely on loose multi-`contains` for success
- Typed/JSON/`CommandSpec`/registry asserts would fail if wrong handle or empty commands
- Focused nextest green
- No production diff

## Stop conditions

- Need production seam → stop and report
- Unrelated test failures → stop
- Scope expands beyond Wave 1 list → stop
