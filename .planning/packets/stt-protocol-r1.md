# STT protocol slice — implementation packet

## Scope

Add only the versioned STT control/event wire types and bounded binary audio
frame helpers in `crates/ajax-web/src/slices/stt.rs`, plus its module export.

## Tests

- JSON round trips preserve `stt.*` type names, version, camelCase session IDs,
  sequence numbers, and error fields.
- Binary audio frames carry a sequence prefix and reject truncated frames.
- Audio frame size is bounded by the configured maximum.

## Constraints

- Use existing `serde`/`serde_json` dependencies and Ajax naming conventions.
- Keep this slice pure and transport-agnostic; do not add a WebSocket route,
  provider implementation, frontend state, or PTY behavior.
- Do not modify `tests/`, terminal keyboard behavior, authentication, or
  configuration outside the already completed `SttConfig` slice.
- No new dependencies and no commits, pushes, branch changes, or generated
  files.

## Verification

```text
rtk cargo test -p ajax-web slices::stt::tests --lib
rtk cargo check -p ajax-web --all-targets
rtk cargo fmt --check
```

## Stop conditions

Stop after the focused tests and checks pass. Report any protocol ambiguity or
required scope expansion instead of implementing the WebSocket/provider layer.
