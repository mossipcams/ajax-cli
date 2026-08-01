# STT provider configuration wiring packet

## Scope

Make the existing centralized `SttConfig.phrase_end_silence_ms` value reach the
supervised provider session without changing terminal or WebSocket semantics.

## Required change

- Add `phrase_end_silence_ms` to `ProviderSessionConfig`.
- Include it as `phraseEndSilenceMs` in the local sidecar start metadata.
- Store/pass the configured value through `MoonshineProvider`, `WebAppState`,
  and `bridge_task_stt_socket`.
- Update existing Rust struct literals and focused tests.

## Boundaries

- Do not add a second timing constant or change frontend behavior.
- Do not alter authentication, PTY handling, provider process supervision, or
  protocol framing beyond the sidecar start metadata.

## Acceptance

- The red provider test `sidecar_start_frame_carries_phrase_end_silence_configuration`
  passes.
- `cargo fmt --check` and focused ajax-web provider/runtime checks pass.
