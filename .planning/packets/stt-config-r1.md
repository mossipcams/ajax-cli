PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Add centralized STT configuration to `ajax-core` so Web Cockpit speech timing,
provider selection, buffering, reconnect, and finalization limits are loaded
from the existing TOML configuration system.

## Scope

Allowed:

- `crates/ajax-core/src/config.rs`
- `crates/ajax-core/src/commands.rs` and `crates/ajax-core/src/task_operations.rs`
  only where existing `Config` test fixtures need `..Config::default()` after
  adding the defaulted STT field
- `crates/ajax-core/src/config.rs` unit tests already added for this task
- `.planning/agent-plans/continuous-speech-to-text.md`

Forbidden:

- WebSocket routes, provider processes, frontend files, PTY behavior, and
  terminal shortcut code
- New dependencies
- Changes to existing test assertions except making struct construction include
  the new defaulted field
- Commits, pushes, rebases, branch changes, or unrelated formatting

## Acceptance

- `Config` has a defaulted `stt: SttConfig` field and existing TOML remains
  backward compatible.
- `SttConfig` is serializable/deserializable with unknown fields rejected.
- Defaults are exactly provider `moonshine-small-streaming`, no provider command,
  phrase silence 700 ms, pause grace 9000 ms, language `en-US`, maximum buffered
  audio 2000 ms, reconnect limit 3, and finalization timeout 5000 ms.
- The focused tests for defaults and documented TOML parsing pass.
- Existing config tests continue to pass.

## Constraints

- Reuse existing serde/config conventions and concrete structs.
- Keep timing names centralized in Rust config; do not add environment-variable
  parsing or duplicate constants.

## Verification

verification:
  methods:
    - type: test
      command: `rtk cargo test -p ajax-core config::tests::stt_ --lib`
      expected: focused STT configuration tests pass
    - type: existing_test
      command: `rtk cargo test -p ajax-core config::tests --lib`
      expected: all config tests pass
    - type: build
      command: `rtk cargo check -p ajax-core --all-targets`
      expected: ajax-core compiles without warnings/errors
  broader_checks:
    - `rtk cargo fmt --check`
  reason: The change is isolated to the typed TOML configuration model and its
    existing unit-test module.

## Stop if

- Any file outside the allowed scope must change.
- Existing config parsing requires a public behavior change beyond adding the
  optional defaulted `[stt]` section.
- A dependency or environment-specific provider implementation is required.
