# Fix: restore Test in Stable Settings button when cwd is dead

## Scope

Settings hides **Test in Stable** when `/api/version` returns
`test_in_stable: false`. Live stable (`--profile stable`) has no
`AJAX_WEB_RESTART_*` and a deleted `__worktrees` cwd, so #771 cwd discovery
fails. Extend script discovery to also try the sibling main `ajax-cli`
checkout inferred from `ajax-cli__worktrees` paths.

## Non-goals

- Frontend Settings redesign
- Changing Test in Stable rebuild/git semantics
- Requiring config.toml reads for enablement

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
EXECUTION:
  AGENT: cursor
  MODEL: composer-2.5
  RISK: low
  SCOPE:
    - crates/ajax-web/src/adapters/server.rs
    - crates/ajax-web/src/runtime/tests/suite_*.rs (only if an existing test_in_stable test must update)
    - .planning/agent-plans/fix-test-in-stable-button.md
  VERIFY:
    - rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
  FALLBACK: STOP
  REASON: Localized enablement/discovery fix in one adapter file with existing unit-test pattern.
```

## Task checklist

- [x] Persistent plan + READY packet
- [x] Infer main ajax-cli checkout from `ajax-cli__worktrees` paths; use as discovery fallback
- [x] Unit tests: fallback finds scripts when cwd has none; missing scripts still disable
- [x] Parent Review Gate + focused validation
- [x] Ops: restart stable with `AJAX_WEB_RESTART_*` so live button returns

## Validation

```bash
rtk cargo nextest run -p ajax-web -- adapters::server::tests test_in_stable
```

Parent result: pass — 19 passed, 230 skipped (exit 0)

Live ops (2026-08-06): restarted `ajax-web-stable` with
`AJAX_WEB_RESTART_SCRIPT`/`PROFILE`/`PORT` set → `/api/version` should
advertise `test_in_stable: true` on current binary. Worktree discovery fix
still needs install/merge to cover bare starts with trashed cwd.

## Deviations

- Immediate live restore used restart-env relaunch (no cargo install); durable
  path-layout fallback remains uncommitted until user asks to commit/PR.
