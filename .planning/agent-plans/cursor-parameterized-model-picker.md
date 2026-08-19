# Plan: Cursor ACP parameterized model picker

## Scope

Launch the operator-selected Cursor model over ACP without Cursor collapsing
the session to Composer 2.5 Fast. Ajax must advertise Cursor’s parameterized
model picker on `initialize`, then apply `model` / `effort` / `fast` as
separate `session/set_config_option` values.

GitHub: [#979](https://github.com/mossipcams/ajax-cli/issues/979).

Non-goals: catalog UI, native `<select>`, changing spawn argv mapping unless
required, claiming filesystem/terminal client capabilities, rewriting
`cli-config.json`, vendoring Cursor’s model list.

## Approval

Approved in chat 2026-08-19 (“yes split it”).

## Contract

- `initialize` advertises `clientCapabilities._meta.parameterizedModelPicker: true`
  (keep filesystem and terminal capabilities false).
- After handshake, when Cursor advertises separate `fast` / `effort` options,
  apply a non-Fast catalog pin as split options: `model=<base>`,
  `effort=<level>` when advertised, `fast=false` unless the catalog id is
  Fast (`*-fast` / `fast=true`).
- Reconstruct the applied id from those options
  (e.g. `grok-4.6[effort=high,fast=false]`) so `snapshot.model` and pin
  matching stay honest. Do not treat bare `grok-4.6` as satisfying
  `cursor-grok-4.6-high`.
- `cursor-grok-4.6-high` must not be satisfied by
  `grok-4.6[effort=high,fast=true]` or `composer-2.5[fast=true]`.
- Existing #954/#979/#984 guards remain: never send Ajax catalog ids through
  `session/set_config_option`.
- `apply_model.rs` is 932 lines (hard max 1000). New parameterized apply logic
  belongs in a sibling module, not more bulk in that file.

## Task checklist

- [x] Advertise `parameterizedModelPicker` on ACP initialize
- [x] Apply Cursor pins as split `model` + `effort` + `fast` when advertised
- [x] Reconstruct applied id from split config options
- [x] Focused #979 regression: parameterized handshake + Grok High is not Fast / Composer Fast
- [x] Update `docs/architecture/web-session-behavior.md` and `web-cockpit.md`
- [x] Focused #979 regression: parameterized Auto/unspecified clears Fast (non-Fast grok-4.6)
- [x] `client_tests.rs` stays at or under 1000 lines (Grok High spawn test in `spawn_tests.rs`)

## Validation

- `cargo test -p ajax-web --lib client_capabilities`
- `cargo test -p ajax-web --lib adapters::web_session_acp::apply_model`
- `cargo test -p ajax-web --lib adapters::web_session_acp::client_tests`
- rustfmt on changed Rust
