# Packet: local-test-cleanup Wave 1

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 1. Status and task contract

READY tests-only cleanup. Delete duplicate/legacy tests and scope Playwright
`terminal-behavior` to mobile-webkit (keep one desktop-only row on desktop).

## 2. Goal

Reduce redundant local test cost without losing coverage that stronger siblings
already provide.

## 3. Allowed files

- `crates/ajax-cli/tests/live_cli.rs`
- `crates/ajax-web/web/e2e/smoke.test.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/playwright.config.mts` (only if needed for project filter)
- `crates/ajax-web/src/runtime.rs` (test module only — delete two `#[test]` fns)
- `.planning/agent-plans/local-test-cleanup.md` (check off Wave 1 items)

## 4. Forbidden changes

- Any production (non-test) logic in `runtime.rs`
- Deleting `legacyTerminalRemoval.test.ts`, `smoke_user_flows.rs`, or architecture isolation tests
- Deleting the desktop expanded-mode Playwright test (must still run on desktop-chromium)
- Commits, pushes, branch changes
- Editing unrelated files

## 5. Context evidence

- Graphify: NOT_REQUIRED — deletion of named duplicate tests; no architecture boundary change
- Serena: NOT_REQUIRED — exact function/test names already identified
- ast-grep: NOT_REQUIRED — mechanical delete of whole test functions; rg anchors below

## 6. Code anchors

### live_cli.rs deletes

- `fn ajax_parses_new_operator_verbs` (~L481–501): weak `--help contains(verb)`; covered by `live_help_exposes_the_scriptable_command_surface`
- `fn ajax_start_creates_task_like_new` (~L629–676): subset of `live_new_execute_records_task_and_persists_it_to_sqlite_state` (~L528–627)

### smoke.test.ts deletes

- `"new task sheet stays inside the visible band when the keyboard opens"` — duplicate of `layout-scroll.test.ts` `"new task sheet stays inside the simulated keyboard viewport band"`
- `"settings view renders restart and diagnostics controls"` — visibility-only; `actions.test.ts` + `SettingsView.test.ts` cover outcomes

### runtime.rs deletes (inside `#[cfg(test)]`)

- `runtime_tests_do_not_compare_axum_against_old_router` (~L2849)
- `runtime_module_does_not_define_legacy_manual_router_helpers` (~L2872)

### terminal-behavior Playwright scoping

- File: `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- KEEP on desktop-chromium only: `"desktop expanded mode keeps terminal bounded and task details summary reachable"` (~L1905)
- All other tests in this file: run on `mobile-webkit` only (skip when `testInfo.project.name === "desktop-chromium"`)

Preferred pattern: at the start of each mobile-only test (or a shared helper called from each), skip desktop. Do **not** use config `testIgnore` for the whole file (would drop the desktop test). Example helper:

```ts
function skipUnlessMobileWebKit(testInfo: TestInfo) {
  test.skip(testInfo.project.name !== "mobile-webkit", "terminal acceptance is mobile-webkit only");
}
```

Call it as the first line of every test except the desktop expanded-mode one. For the desktop test:

```ts
test.skip(testInfo.project.name !== "desktop-chromium", "desktop expanded layout only");
```

## 7. Test-first instructions

NOT_APPLICABLE — deleting redundant tests; no new behavior.

## 8. Edit instructions

1. Delete the two `live_cli.rs` test functions entirely (and any now-unused helpers only if they become unused and were sole callers — do not delete shared helpers still used).
2. Delete the two named smoke tests from `smoke.test.ts`.
3. Delete the two runtime legacy source-scan tests.
4. Add project skips to `terminal-behavior.test.ts` as specified.
5. Update plan checklist Wave 1 items to `[x]` when done.

## 9. Verification commands

```bash
cargo nextest run -p ajax-cli -p ajax-web --all-features --test-threads=1
npm run web:test -- --run crates/ajax-web/web/src/legacyTerminalRemoval.test.ts
# Optional if Playwright deps available:
npm run web:smoke -- --project=mobile-webkit e2e/terminal-behavior.test.ts
npm run web:smoke -- --project=desktop-chromium e2e/terminal-behavior.test.ts
```

Desktop chromium run of terminal-behavior should execute **only** the desktop expanded-mode test (others skipped). Mobile-webkit should skip the desktop expanded-mode test.

## 10. Acceptance criteria

- Named deletes gone; no production code changed
- Desktop terminal-behavior suite is 1 active test (+ skips); mobile-webkit keeps the rest
- nextest ajax-cli + ajax-web green
- legacyTerminalRemoval vitest still green

## 11. Stop conditions

- Any production compile/logic change required
- Deleting a helper still used by kept tests
- Uncertainty about whether a smoke test is the only browser coverage for a flow
- Diff exceeds the allowed files list
