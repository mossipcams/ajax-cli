# Packet: fix Test in Stable missing on stable web

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact
```

## Task

Make Web Cockpit advertise and run **Test in Stable** when the process is
actually stable (`--profile stable`) even if `AJAX_WEB_RESTART_*` is unset or
`AJAX_PROFILE` is a leaked non-stable value (e.g. `dev` from tmux).

Live bug: `:8787` runs `ajax-cli --profile stable web` with `AJAX_PROFILE=dev`
and no restart-script env → `/api/version` returns `test_in_stable: false` →
Settings hides the button. Feature code and `scripts/test-in-stable.sh` exist.

## Scope

### Allowed

- `crates/ajax-web/src/adapters/server.rs` (resolve helpers, enablement, schedule)
- `crates/ajax-web/src/adapters/server.rs` unit tests (same file `mod tests`)
- `crates/ajax-web/src/runtime/tests/suite_*.rs` only if an existing
  `test_in_stable` integration test must be updated for new resolve behavior
- `.planning/agent-plans/fix-test-in-stable-missing.md` checklist updates only

### Forbidden

- Frontend Settings UI redesign
- Changing Test in Stable rebuild/git semantics in shell scripts
- Unrelated restart reliability changes
- Commits, pushes, branch switches

## Acceptance

1. Profile resolution order for Test in Stable: `AJAX_WEB_RESTART_PROFILE` →
   CLI `--profile` from `std::env::args()` → `AJAX_PROFILE`.
2. Restart script resolution: non-empty `AJAX_WEB_RESTART_SCRIPT` if that path
   exists → else walk ancestors of `std::env::current_dir()` for
   `scripts/dev-web-restart.sh`. Enable only when sibling `test-in-stable.sh`
   also exists (same rule as today).
3. `test_in_stable_enabled_from_env()` / schedule path share that resolve so the
   button is not advertised when spawn would fail for missing scripts.
4. Unit tests cover: CLI `--profile stable` wins over `AJAX_PROFILE=dev`;
   discovery finds scripts under a temp dir tree; missing wrapper still
   disables; existing stable+script cases still pass.
5. Focused `ajax-web` nextest for `test_in_stable` / `adapters::server::tests`
   passes.

## Constraints

- Prefer small pure helpers over new traits/modules.
- Do not `set_var` process-wide env unless unavoidable; prefer explicit resolve
  used by enablement and `schedule_test_in_stable`.
- Keep `cfg(test)` no-op for `schedule_test_in_stable`.
- Port for schedule: `AJAX_WEB_RESTART_PORT` → CLI `--port` → `DEFAULT_STABLE_PORT`.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
      expected: pass
    - type: test
      command: rtk cargo nextest run -p ajax-web -- test_in_stable
      expected: pass
  broader_checks: []
  reason: Gate and schedule live in server.rs; existing endpoint tests cover HTTP.
```

## Stop if

- Fix needs architecture changes outside ajax-web adapters
- Script discovery would require hard-coded machine paths outside cwd walk
- Verification fails after one focused fix attempt

## Code anchors

- `crates/ajax-web/src/adapters/server.rs` — `test_in_stable_enabled_from_env`,
  `web_profile_from_env`, `schedule_test_in_stable`, existing unit tests
- `crates/ajax-web/src/runtime/mod.rs` — `axum_version` /
  `handle_server_test_in_stable` (call sites; prefer not changing unless needed)
- `scripts/dev-web-restart.sh` — sets `AJAX_WEB_RESTART_*` when launching correctly
- `scripts/test-in-stable.sh` — sibling wrapper required beside restart script

## Context evidence

- Live `GET /api/version` with session cookie:
  `{"test_in_stable":false,"version":"0.11.0-..."}`
- Process argv: `ajax-cli --profile stable web --host 0.0.0.0 --port 8787`
- Process env includes `AJAX_PROFILE=dev`, no `AJAX_WEB_RESTART_SCRIPT`
- Cwd has `scripts/dev-web-restart.sh` and `scripts/test-in-stable.sh`
- Ajax core profile resolve is `cli_profile.or(env_profile)`; Test in Stable
  gate must match that precedence when restart profile env is absent
