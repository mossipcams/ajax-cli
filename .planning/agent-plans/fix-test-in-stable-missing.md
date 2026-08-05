# Fix: Test in Stable button missing on stable Web Cockpit

## Scope

Restore Settings **Test in Stable** when the stable web server runs with
`--profile stable` but without `AJAX_WEB_RESTART_*` env (or with a leaked
`AJAX_PROFILE=dev` from the tmux server).

## Diagnosis (live 2026-08-05)

- Stable `:8787` PID 98055: `ajax-cli --profile stable web ...`
- Env: `AJAX_PROFILE=dev` (tmux global), **no** `AJAX_WEB_RESTART_SCRIPT` /
  `AJAX_WEB_RESTART_PROFILE`
- `GET /api/version` → `{"test_in_stable":false,...}` → Settings hides button
- Code + bundle still contain the feature; gate is wrong

`test_in_stable_enabled_from_env` only checks restart/AJAX profile env and
requires `AJAX_WEB_RESTART_SCRIPT`. It ignores CLI `--profile stable` and does
not discover `scripts/dev-web-restart.sh` + `scripts/test-in-stable.sh` from
cwd (scripts exist in the process cwd worktree).

## Non-goals

- Frontend Settings redesign
- Changing Test in Stable rebuild semantics
- Forcing every bare start through a full `dev-web-restart.sh` rebuild

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Persistent plan + READY packet
- [x] Harden profile + script resolution in `server.rs` (+ tests)
- [x] Wire enablement + `schedule_test_in_stable` through shared resolve
- [x] Parent Review Gate + focused validation
- [x] Ops: restart stable so live button returns (via script or fixed binary)

## Validation

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
rtk cargo nextest run -p ajax-web -- test_in_stable
```

Parent results (2026-08-05):
- `adapters::server::tests test_in_stable` → PASS (17 passed)
- `test_in_stable` → PASS (8 passed under filter)
- Live env relaunch: `GET /api/version` → `test_in_stable: true`

## Deviations

- Updated `suite_3.rs` integration test to use temp script files (required after
  removing `#[cfg(test)]` path-existence stub; real file checks only).
- Delegate report extractor failed (`MISSING_STRUCTURED_REPORT`); parent reviewed
  diff and re-ran validation — ACCEPT.
- Live restore used correct `AJAX_WEB_RESTART_*` with the current cargo binary;
  durable CLI/`cwd` resolve fix is uncommitted in this worktree and still needs
  install/merge to protect bare `--profile stable` starts.
