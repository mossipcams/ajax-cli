PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Stable Ajax web advertises `test_in_stable` on `/api/version` when profile is
`stable` and `AJAX_WEB_RESTART_SCRIPT` exists; Settings shows a confirm-gated
**Test in Stable** button that POSTs `/api/server/test-in-stable` to run
`dev-web-restart.sh --profile stable` (pull origin/main + cargo install).
Remove Settings Reload app / Restart server. Dev profile never sees the button;
POST returns 404 when disabled.

## Allowed files

- `crates/ajax-web/src/adapters/server.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/install.rs` (route-string assert only if needed)
- `crates/ajax-web/web/src/shared/lib/types.ts`
- `crates/ajax-web/web/src/shared/lib/api.ts`
- `crates/ajax-web/web/src/shared/lib/api.test.ts`
- `crates/ajax-web/web/src/shared/lib/polling.ts`
- `crates/ajax-web/web/src/shared/lib/polling.test.ts` (only if timeout constant tested)
- `crates/ajax-web/web/src/features/settings/SettingsView.tsx`
- `crates/ajax-web/web/src/features/settings/SettingsView.test.tsx`
- `crates/ajax-web/web/e2e/actions.test.ts`
- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/dist/app.js`
- `crates/ajax-web/web/dist/app.css`
- `crates/ajax-web/web/dist/app.html`
- `crates/ajax-web/web/dist/terminal.js`
- `.planning/agent-plans/feat-test-in-stable.md`

## Forbidden changes

- Do not change Test in Dev / `dev_deploy.rs` behavior
- Do not remove `POST /api/server/restart`
- Do not add Cargo features or config-file toggles
- Do not edit architecture.md
- Do not commit, push, merge, rebase, or change branches
- Do not edit files outside Allowed files

## Context evidence

- Desired behavior: plan lock — auto-on when profile stable + restart script
  file exists; Settings-only button; pull main via script; remove Reload/Restart.
- Anchors — restart env + schedule no-op in tests:
  `crates/ajax-web/src/adapters/server.rs` L9–66, L99–145
- Anchors — version + restart handlers:
  `crates/ajax-web/src/runtime.rs` L392–393, L629–647, version test L2630–2644,
  restart test L3273–3284, route policy L1830–1847
- Anchors — Settings Restart/Reload UI to delete:
  `crates/ajax-web/web/src/features/settings/SettingsView.tsx` L27–83, L126–140
- Anchors — `VersionResponse` lacks flag:
  `crates/ajax-web/web/src/shared/lib/types.ts` L146–148
- Anchors — `RESTART_TIMEOUT_MS = 30000` too short for cargo install:
  `crates/ajax-web/web/src/shared/lib/polling.ts` L20–21
- Pattern reuse — Test in Dev spawn uses script + profile args:
  `crates/ajax-web/src/slices/dev_deploy.rs` L402–414
- Script default (no `--worktree`) syncs origin/main then cargo install:
  `scripts/dev-web-restart.sh` L124–167, L193–196

## Code anchors

- `server::restart_launch_from_env` / `schedule_process_restart`
- `runtime::axum_version` / `handle_server_restart` / `axum_app` routes
- `SettingsView` restart/reload handlers and Actions buttons
- `fetchVersion` / `restartServer` / `waitForServerOnline`
- e2e `settings Restart confirms then restarts the server`

## Test-first instructions

1. **Rust unit (server.rs)** — add tests:
   - `web_profile_from_env` prefers `AJAX_WEB_RESTART_PROFILE` over `AJAX_PROFILE`
   - `test_in_stable_enabled("stable", Some("/x")) == true`;
     false for `"dev"`, empty script, None
   - `test_in_stable_launch_args(port)` ==
     `["--profile","stable","--port", port]` (default port `"8787"`)
2. **Rust runtime** — update `axum_router_reports_shell_version` to assert
   `test_in_stable` is a boolean (false under default test env).
   Add tests (use env stubs or pure helpers wired into handlers via injectable
   env readers if needed; prefer testing pure helpers + handler wiring that
   calls `test_in_stable_enabled_from_process_env()`):
   - `POST /api/server/test-in-stable` → 404 + error when disabled
   - When helpers report enabled, POST → 200 `{ok:true,restarting:true}`
     (schedule remains no-op under cfg(test))
3. **Vitest Settings** — remove Reload/Restart tests; add:
   - hides Test in Stable when `fetchVersion` returns `test_in_stable: false`
   - shows button when true; first tap confirms; second calls `startTestInStable`
     then `waitForServerOnline` with long timeout; success reloads page
4. **api.test** — `startTestInStable` posts `/api/server/test-in-stable`
5. **e2e** — replace Restart case with Test in Stable (mock version flag + POST)

RED commands:

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests
rtk cargo nextest run -p ajax-web -- test_in_stable
rtk npm run web:test -- --run src/features/settings/SettingsView.test.tsx src/shared/lib/api.test.ts
```

## Edit instructions

1. **server.rs** — add:
   - `AJAX_PROFILE_ENV = "AJAX_PROFILE"`
   - `STABLE_PROFILE = "stable"`
   - `DEFAULT_STABLE_PORT = "8787"`
   - `web_profile_from_env(restart_profile, ajax_profile) -> Option<&str>`
   - `test_in_stable_enabled(profile: Option<&str>, script: Option<&str>) -> bool`
     (profile == stable && script non-empty)
   - `test_in_stable_script_args(port: &str) -> Vec<String>`
   - `test_in_stable_enabled_from_env()` reads script/profile envs; script must
     be non-empty (file existence: `Path::new(script).is_file()` in non-test;
     in tests, treat non-empty path as present OR use a helper that accepts
     `script_exists: bool` for pure testing — keep production check real)
   - `schedule_test_in_stable()` — cfg(test) no-op; else spawn script with
     stable args only (never Respawn), then exit like restart
2. **runtime.rs** — `axum_version` includes `"test_in_stable": server::…enabled…`;
   add route `POST /api/server/test-in-stable`; handler returns 404 JSON when
   disabled else schedule + `{"ok":true,"restarting":true}`; update access-policy
   list and install.rs bundle string list to include new endpoint if grepped.
3. **types.ts** — `VersionResponse { version: string; test_in_stable?: boolean }`
4. **polling.ts** — `TEST_IN_STABLE_TIMEOUT_MS = 900_000` (15m)
5. **api.ts** — `startTestInStable()` POST; optionally pass timeout to
   `waitForServerOnline`
6. **SettingsView.tsx** — delete reload/restart; mount-fetch version; gated
   Test in Stable with confirm; note text; long wait; reload on success
7. **Tests + e2e + fixtures** as above; then `rtk npm run web:build`

## Verification commands

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests
rtk cargo nextest run -p ajax-web -- test_in_stable
rtk cargo nextest run -p ajax-web -- axum_router_reports_shell_version
rtk npm run web:test -- --run src/features/settings/SettingsView.test.tsx src/shared/lib/api.test.ts src/shared/lib/polling.test.ts
rtk npm run web:build
```

## Acceptance criteria

- Version JSON always includes boolean `test_in_stable`
- Enabled only for stable + existing restart script
- POST unavailable → 404; enabled → 200 restarting and schedules script pull-main
- Settings: no Reload/Restart; Test in Stable only when flag true
- Dist rebuilt; no edits outside allowed files

## Stop conditions

- Need auth changes or public unauthenticated mutate beyond existing session model
- Script contract changes required beyond `--profile stable --port`
- Unrelated test failures outside this feature
- Scope expands to Test in Dev or config-file flags
