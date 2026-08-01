PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`MoonshineSession` in `crates/ajax-web/src/adapters/stt_provider.rs` has three
coupled defects in its sidecar I/O:

1. `write_frame` calls blocking `write_all` + `flush` directly on `ChildStdin`, and
   it is reached from the async `bridge_task_stt_socket`. A sidecar that stops
   reading blocks a tokio worker thread.
2. `BoundedAudioBuffer` is dead ceremony. `push_audio` pushes a frame, writes it,
   then immediately pops it, so the buffer never holds more than one frame. Its
   capacity — derived from `max_buffered_audio_ms` — can never reject anything,
   because a single frame is capped at `MAX_SIDECAR_AUDIO_PCM_BYTES` (640) while the
   capacity is tens of thousands of bytes.
3. `poll_event` is `self.events.try_recv().ok()`, which collapses
   `TryRecvError::Disconnected` into the same `None` as `Empty`. A crashed sidecar is
   indistinguishable from silence, so the session hangs until its deadline with no
   error reported.

Replace all three with a single writer-thread design.

- Give `MoonshineSession` a bounded `std::sync::mpsc::SyncSender<Vec<u8>>` for
  outbound frames and a writer `JoinHandle`. The writer thread owns the `ChildStdin`,
  loops on `recv()`, and does `write_all` + `flush` per frame, exiting cleanly when
  the channel closes or a write fails.
- Channel capacity must represent roughly `max_buffered_audio_ms` worth of audio.
  One audio frame is 20 ms of PCM16, so a frame-count bound of
  `max(1, max_buffered_audio_ms / 20)` is correct. Thread the value in from
  `MoonshineProvider::start_session`, which already holds `max_buffered_audio_ms`.
- `write_frame` becomes a non-blocking `try_send`. Map `TrySendError::Full` to
  `ProviderError::AudioBufferOverflow` and `TrySendError::Disconnected` to
  `ProviderError::SessionClosed`. This is now the real backpressure.
- Route the start frame and the finalize frame through the same channel so ordering
  with audio frames is preserved. Both are sent when the channel is effectively
  empty, so neither can be dropped in practice.
- **Delete `BoundedAudioBuffer` entirely**, along with its `audio` field, the
  `push`/`pop`/`clear`/`buffered_bytes` calls in `push_audio`, `finalize`, and
  `cancel`, and the `audio_buffer_rejects_overflow` test. Delete
  `buffer_capacity_bytes` if it becomes unused.
- `poll_event` must distinguish disconnection:
  `Ok(event) => Some(event)`, `Err(Empty) => None`, `Err(Disconnected) =>` emit
  `ProviderEvent::Error { message: "stt sidecar exited" }` **exactly once**. Latch it
  behind a new `sidecar_ended: bool` field so repeated polls after the child dies
  return `None` rather than re-emitting the error every 20 ms.
- `stop_child` must drop the sender and join the writer thread in addition to its
  existing child kill/wait and reader join.

## Allowed files

- `crates/ajax-web/src/adapters/stt_provider.rs`

## Forbidden changes

- Any file outside `Allowed files`.
- Do not change the wire protocol, `SttClientMessage`, `SttServerEvent`, the
  `stt.closed` variant, or `STT_PROTOCOL_VERSION`.
- Do not change the finalization deadline logic in `bridge_task_stt_socket`.
- Do not change the sidecar frame encoders `encode_sidecar_audio_frame`,
  `encode_sidecar_start_frame`, `encode_sidecar_finalize_frame`, or
  `parse_sidecar_event_line`.
- Do not change `MAX_SIDECAR_AUDIO_PCM_BYTES` or `SIDECAR_EVENT_QUEUE_BOUND`.
- Do not remove the `SttProvider` / `SttProviderSession` traits — a separate packet
  handles those.
- Do not add async/tokio to `MoonshineSession`; the writer is a plain OS thread.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- `BoundedAudioBuffer` no longer exists; a repository grep returns no hits.
- No `write_all` or `flush` call on `ChildStdin` remains reachable from
  `bridge_task_stt_socket`; all such calls happen on the writer thread.
- `push_audio` returns `ProviderError::AudioBufferOverflow` once the bounded channel
  is full, instead of blocking.
- A session whose sidecar never reads stdin does not block the caller: filling the
  channel yields overflow errors rather than a hang.
- When the sidecar exits, the next `poll_event` yields exactly one
  `ProviderEvent::Error`, and every subsequent `poll_event` yields `None`.
- Frame ordering is preserved: the start frame reaches the sidecar before any audio
  frame, and the finalize frame after all audio frames sent before it.
- `cancel()` and `Drop` still tear down the child, the reader thread, and now the
  writer thread without panicking or leaking.
- Existing tests keep passing, including
  `finalize_leaves_the_session_open_to_drain_final_events` and the
  `provider_startup_failure_is_recoverable` / unavailable-command tests.

## Verification

Run and report actual results for:

- `cargo clippy -p ajax-web --all-targets --all-features` — must pass.
- `cargo test -p ajax-web stt` — must pass.

Add tests in the existing `mod tests` covering: (a) a full channel produces
`AudioBufferOverflow` rather than blocking, and (b) sidecar exit surfaces exactly one
error event and then `None`. Use a real short-lived child command such as `cat` or
`true` the way the existing tests already do; do not add new test infrastructure
outside this file.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Preserving start/audio/finalize ordering is not achievable through a single
  channel without adding a second synchronization primitive beyond the sender and
  the join handle.
- The patch would exceed roughly 250 changed lines.
- A test would require spawning a process that is not already used in this file's
  tests.
