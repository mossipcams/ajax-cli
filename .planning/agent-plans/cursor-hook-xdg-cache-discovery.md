# Plan: discover XDG `~/.cache/ajax` for Cursor hook identity

## Problem

Cursor hooks sometimes inherit `AJAX_*` from `__agent-runtime` (writes work)
and sometimes spawn without that env. Fallback discovery missed the stable
cache root `~/.cache/ajax/agent-events` (only checked `.ajax-dev`, `.ajax`,
project-local, `AJAX_HOME`). Live probe: without env → no write; with env → write.

## Scope

- Add `{HOME}/.cache/ajax/agent-events` to Cursor identity discovery roots
- Focused test that resolves via that root

## Non-goals

- Changing stop→done / Idle projection
- Profile/cache path unification beyond discovery

## Delegation decision

`Delegation decision: not delegated because one-line discovery root + focused
test is smaller than a work order (AGENTS tiny-change exception).`

## Checklist

- [x] Failing test: resolve via `home/.cache/ajax/agent-events` cwd-index
- [x] Add XDG root to `cursor_identity_discovery_roots`
- [x] Validate focused tests + install ajax-cli

## Validation

```text
cargo test -p ajax-cli cursor_resolves_identity_from_xdg_cache → pass
cargo test -p ajax-cli cursor_event_resolves_identity_from_cwd_index → pass
cargo clippy -p ajax-cli → clean
live probe without AJAX_* + CURSOR_PROJECT_DIR against ~/.cache/ajax → writes
cargo install --path crates/ajax-cli --locked --force → installed
```

## Delegation decision

`Delegation decision: not delegated because one-line discovery root + focused
test is smaller than a work order (AGENTS tiny-change exception).`
