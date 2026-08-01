PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`SttConfig` in `crates/ajax-core/src/config.rs` documents seven knobs. Three are
read by nothing: `reconnect_limit`, `provider`, and `language`. Because
`SttConfig` uses `deny_unknown_fields`, a user who sets them gets no error and no
effect. `docs/speech-input.md` presents them as working settings.

Make every surviving key actually load-bearing.

1. **Delete `reconnect_limit`** from `SttConfig`, its `default_reconnect_limit()`
   function, its `Default` impl entry, its assertions in the config tests, and its
   row plus its line in the sample TOML block in `docs/speech-input.md`. Nothing
   reconnects — the socket dropping surfaces "Speech connection closed" and the user
   taps Mic again, which is the reconnect path.

2. **Delete `provider`** from `SttConfig`, its `default_stt_provider()` function, its
   `Default` impl entry, its test assertions, and its documentation row and sample
   TOML line. Only `provider_command` is read. Do **not** touch `provider_command`.

3. **Make `language` server-owned.** It is currently sent by the browser in
   `stt.start` and ignored in favour of nothing, while `config.stt.language` is never
   read.
   - Remove the `language` field from `SttClientMessage::Start` in
     `crates/ajax-web/src/slices/stt.rs`.
   - Remove `language` from the `stt.start` JSON the client builds in `sendControl`
     in `speechTransport.ts`.
   - In `crates/ajax-web/src/runtime.rs`, carry `config.stt.language` onto
     `WebAppState` as `stt_language: String` exactly the way
     `stt_phrase_end_silence_ms` is already carried (field, `Clone` impl, both
     constructors), and pass it into `bridge_task_stt_socket`.
   - Give `bridge_task_stt_socket` a `language: String` parameter and use it when
     building `ProviderSessionConfig` in the `Start` arm, replacing the value that
     came from the client message.

   This mirrors the existing precedent at `MoonshineProvider::start_session`, which
   already overrides `config.phrase_end_silence_ms` from server config.

Removing fields from a `deny_unknown_fields` struct means an existing `[stt]` block
containing `reconnect_limit` or `provider` will fail to parse. That is intended and
acceptable: this feature is unreleased on this branch.

## Allowed files

- `crates/ajax-core/src/config.rs`
- `crates/ajax-web/src/slices/stt.rs`
- `crates/ajax-web/src/adapters/stt_provider.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- `crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`
- `docs/speech-input.md`

## Forbidden changes

- Any file outside `Allowed files`. In particular do not touch `speechState.ts`,
  `TaskTerminal.tsx`, `TerminalComposer.tsx`, or `architecture.rs`.
- Do not remove, rename, or change the semantics of `provider_command`,
  `phrase_end_silence_ms`, `pause_grace_period_ms`, `max_buffered_audio_ms`, or
  `finalization_timeout_ms`. `pause_grace_period_ms` stays unused for now; a
  separate packet wires it.
- Do not change `STT_PROTOCOL_VERSION`.
- Do not touch the `stt.closed` variant, the finalization deadline logic, the
  `pendingFrames` logic, `BoundedAudioBuffer`, or the RMS/VAD logic.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- `SttConfig` has exactly these fields: `provider_command`, `phrase_end_silence_ms`,
  `pause_grace_period_ms`, `language`, `max_buffered_audio_ms`,
  `finalization_timeout_ms`.
- A grep for `reconnect_limit` and for `default_stt_provider` across the repository
  returns no hits.
- `docs/speech-input.md` no longer mentions `reconnect_limit` or `provider`, and its
  sample TOML block parses against the new struct.
- `SttClientMessage::Start` has no `language` field, and the client's `stt.start`
  payload no longer includes one.
- The language used to build `ProviderSessionConfig` originates from
  `config.stt.language`, threaded through `WebAppState` into
  `bridge_task_stt_socket`, not from any client-supplied value.
- Setting `language = "en-GB"` in `[stt]` results in that value reaching
  `ProviderSessionConfig.language`.
- Existing config tests are updated to match the new struct rather than deleted;
  the `stt_configuration_loads_from_documented_toml_shape` test must still assert a
  full round trip over the remaining fields.

## Verification

Run and report actual results for:

- `cargo clippy -p ajax-web -p ajax-core --all-targets --all-features` — must pass.
- `cargo test -p ajax-core config` — must pass.
- `cargo test -p ajax-web stt` — must pass.
- `npx vitest run src/shared/lib/speechTransport.test.ts` from `crates/ajax-web/web`
  — must pass.

Add or adjust a config test proving a non-default `language` in `[stt]` is what
reaches `ProviderSessionConfig`.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Threading `language` through `WebAppState` requires changing the signature of any
  public constructor beyond adding the field alongside the existing
  `stt_phrase_end_silence_ms`.
- Any test outside the allowed files fails as a result of the field removals — report
  which one and stop rather than editing it.
- The patch would exceed roughly 220 changed lines.
