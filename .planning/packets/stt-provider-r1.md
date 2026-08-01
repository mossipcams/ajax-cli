# STT provider slice — implementation packet

## Scope

Add the host-side provider boundary and a supervised local Moonshine command
adapter in `crates/ajax-web/src/adapters/stt_provider.rs`; export it from
`adapters/mod.rs`. Keep the implementation independent of the WebSocket route,
React, PTY, and Ajax core task state.

## Required boundary

Define a narrow `SttProvider` interface for health, session start, and clean
shutdown. A returned session must cover bounded `push_audio`, provider event
polling (partial/final/speech activity), finalize, and cancel. Keep provider
events typed and sequence-aware.

The initial implementation is a supervised `MoonshineProvider` that launches
an optional configured local command. Missing/unusable command startup must
return a useful provider error and leave Ajax operational. Keep the command
adapter and its framing isolated so a later Moonshine engine replacement does
not change the rest of Ajax.

## Tests

- No configured command reports provider unavailable without panicking.
- Missing command startup reports a recoverable provider error.
- Audio buffering is bounded and rejects overflow instead of growing without
  limit.
- Provider session exposes typed partial/final/speech events, finalization,
  cancellation, and health/shutdown seams without direct PTY coupling.

## Constraints

- May edit only `crates/ajax-web/src/adapters/stt_provider.rs`,
  `crates/ajax-web/src/adapters/mod.rs`, and this plan.
- Use existing standard library/tokio/serde dependencies; do not add Python or
  cloud STT dependencies.
- Do not expose the provider command as a public listener or alter runtime
  state construction in this slice.
- No unbounded queues, unsafe code, commits, pushes, or branch changes.

## Verification

```text
rtk cargo test -p ajax-web adapters::stt_provider::tests --lib
rtk cargo check -p ajax-web --all-targets
rtk cargo fmt --check
```

## Stop conditions

Stop after the provider boundary and supervised adapter checks pass. Route
integration and authenticated transport belong to later packets.
