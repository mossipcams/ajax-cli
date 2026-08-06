# Packet: restore Test in Stable button via worktree-layout discovery

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact
```

## Task

Restore Settings **Test in Stable** when the stable web process runs with
`--profile stable` but cwd is a deleted/trashed path under
`ajax-cli__worktrees` (so #771 cwd walk finds no scripts) and
`AJAX_WEB_RESTART_SCRIPT` is unset.

Live evidence: PID listens with argv `ajax-cli --profile stable web ...`,
cwd=`.../ajax-cli__worktrees/.ajax-trash/...` (gone), no restart env →
`test_in_stable: false` → button hidden. Main checkout
`/Users/matt/Desktop/Projects/ajax-cli/scripts/{dev-web-restart,test-in-stable}.sh`
exists.

## Scope

### Allowed

- `crates/ajax-web/src/adapters/server.rs` (discovery helpers, resolve, tests)
- `crates/ajax-web/src/runtime/tests/suite_*.rs` only if an existing
  `test_in_stable` integration test must be updated
- `.planning/agent-plans/fix-test-in-stable-button.md` checklist updates only

### Forbidden

- Frontend Settings UI redesign
- Shell script rebuild/git semantics changes
- Commits, pushes, branch switches
- Reading `config.toml` / adding OnceLock globals unless the path heuristic
  cannot work (prefer path inference)

## Acceptance

1. When `AJAX_WEB_RESTART_SCRIPT` is unset and cwd (or a provided search root)
   sits under a directory named `ajax-cli__worktrees`, discovery also tries the
   sibling checkout `…/ajax-cli` (path prefix before `__worktrees` + `ajax-cli`).
2. Enablement still requires stable profile (CLI/`AJAX_WEB_RESTART_PROFILE`/
   `AJAX_PROFILE` order unchanged from #771) **and** both
   `dev-web-restart.sh` + sibling `test-in-stable.sh` to exist.
3. Unit tests cover: trashed/empty worktree-style cwd still resolves via sibling
   main checkout; missing wrapper still disables; existing cases still pass.
4. Focused `ajax-web` nextest for `adapters::server::tests` / `test_in_stable`
   passes.

## Constraints

- Prefer a small pure `infer_…` helper + multi-root try in `resolve_restart_script`.
- Keep `cfg(test)` no-op for `schedule_test_in_stable`.
- Do not expand beyond allowed paths.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
      expected: pass
  broader_checks: []
  reason: Gate lives in server.rs; unit tests are the right lock.
```

## Stop if

- Fix needs architecture/config coupling beyond path inference
- Verification fails after one focused attempt

## Code anchors

- `crates/ajax-web/src/adapters/server.rs` —
  `discover_dev_web_restart_script`, `resolve_restart_script`,
  `resolve_test_in_stable_config`, `process_test_in_stable_config`,
  existing `mod tests`
- Live layout: `…/ajax-cli__worktrees/<name>` → sibling `…/ajax-cli`
