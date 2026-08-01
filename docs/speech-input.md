# Speech input (Web Cockpit)

Continuous speech-to-text lets an operator dictate into the task terminal from
iPhone Safari while recognition runs on the Mac that hosts Ajax. The phone
supplies microphone audio; Ajax owns the authenticated transport, session
supervision, and a replaceable local **persistent** provider worker. Finalized
recognition auto-inserts into the active shell line through the same paste/PTY
path as manual paste. Ajax does not auto-press Enter or execute commands.
Partial recognition is session metadata only and is never written to the PTY.

Design boundaries and ownership live in [`architecture.md`](../architecture.md)
under **Speech Input Architecture**. This page is the operator setup and daily-use
guide.

## Host provider (Moonshine v2 Small Streaming)

Ajax does **not** ship a Moonshine model or a one-line installer. The Mac that
runs `ajax-cli web` must already have a **local worker process** that implements
Ajax's supervised STT adapter protocol: bounded PCM audio frames on stdin,
versioned transcript and VAD events on stdout. The initial adapter is
**Moonshine v2** via `moonshine-voice`, defaulting to **Small Streaming**, sized
for a local MacBook-class host. Inference always runs on the Ajax host. Legacy
`useful-moonshine-onnx` / `moonshine/tiny` is not supported.

Configure the shell command Ajax should launch:

- **`provider_command`** — the supervised local worker command. Ajax starts this
  process once (at provider startup or lazy on first Mic use), keeps the model
  resident, and reuses it across speech sessions. Cancel or finalize ends a
  recognition session, not the worker. Ajax tears the worker down when the
  provider shuts down. Use a full path or a command that is on the server's
  `PATH` when `ajax-cli web` starts. The value is split on whitespace into a
  program and its arguments — there is no shell and no quoting, so a path
  containing spaces will not resolve. Wrap such a command in a small launcher
  script and point `provider_command` at that instead.

### The bundled worker

Ajax ships a reference implementation at `scripts/ajax-moonshine-sidecar`. It
speaks the framed protocol below, loads Moonshine Small Streaming once, and
serves multiple sessions. On the Mac that hosts `ajax-cli web`, run the one-shot
installer:

```sh
./scripts/setup-stt.sh
```

That creates `~/.ajax-dev/stt-venv`, installs **moonshine-voice** (Moonshine v2),
copies the worker to `~/.ajax-dev/bin/ajax-moonshine-sidecar`, downloads the
English Small Streaming model, and writes a matching `[stt]` block into **both**
stable (`~/.config/ajax/config.toml`) and dev (`~/.ajax-dev/config.toml`) config
files. One venv and worker serve both profiles. Older `useful-moonshine-onnx`
installs in that venv are uninstalled.

Manual setup (if you prefer not to run the script) uses its own virtualenv so it
never touches your system Python:

```sh
python3 -m venv ~/.ajax-stt-venv
~/.ajax-stt-venv/bin/pip install 'moonshine-voice>=0.1.0' numpy
```

Then point `provider_command` at the interpreter and the script — two
whitespace-separated tokens, since the value is split rather than shell-parsed:

```toml
provider_command = "/Users/you/.ajax-stt-venv/bin/python /path/to/ajax/scripts/ajax-moonshine-sidecar"
```

The default model architecture is **Moonshine v2 Small Streaming**. Override with
`AJAX_STT_MODEL` only to another **streaming** architecture
(`TINY_STREAMING`, `BASE_STREAMING`, `MEDIUM_STREAMING`). Legacy names such as
`moonshine/tiny` are rejected. Set `AJAX_STT_LOG=/tmp/stt.log` to capture worker
diagnostics — stderr is discarded by the parent.

You may substitute any other executable that speaks the same protocol. Ajax only
supervises the process you configure; it does not download models to the browser
or verify third-party package names.

## `[stt]` configuration

Add a `[stt]` block to `~/.config/ajax/config.toml` (or the file set by
`AJAX_CONFIG`). All keys are centralized in `ajax-core`; timing values are not
hard-coded in the UI.

```toml
[stt]
provider_command = "/usr/local/bin/your-moonshine-worker"
phrase_end_silence_ms = 700
pause_grace_period_ms = 9000
language = "en-US"
max_buffered_audio_ms = 2000
finalization_timeout_ms = 5000
```

| Key | Role |
| --- | --- |
| `provider_command` | Command Ajax launches for the persistent worker. **Required** for speech. |
| `phrase_end_silence_ms` | Provider phrase-end silence before a final segment (default `700`). |
| `pause_grace_period_ms` | Spoken `pause` grace period before finalization (default `9000`). |
| `language` | Recognition language tag (default `en-US`). |
| `max_buffered_audio_ms` | Bounded audio buffering for transport and provider (default `2000`). |
| `finalization_timeout_ms` | Safe finalization deadline (default `5000`). |

Unset `provider_command` leaves speech unavailable: the Mic control can be used,
but the session enters a recoverable error such as **no STT provider command
configured** instead of starting capture. Disabled Mic accessibility should
explain that the provider is unavailable.

## Provider health and terminal safety

Ajax owns the authenticated browser endpoint (`/api/tasks/{handle}/stt`). The
STT service is isolated from the PTY WebSocket and task-session failures.

When the local provider cannot start, crashes, or reports unavailable:

- Speech enters an explicit **recoverable error** with a useful message in the
  Mic status region.
- Already-inserted terminal text is preserved; unstable partial metadata is cleared.
- Microphone tracks, audio context, processing, timers, and the STT socket are
  released through the shared teardown path.
- The raw terminal, tmux attach, and Cockpit operations keep working. Provider
  failure does **not** take down the terminal or Ajax web runtime.
- After fixing host configuration or the worker binary, tap **Mic** again from
  idle or error to start a fresh session (the worker reloads only if it crashed).

While a session is **connecting** or **finalizing**, Mic is temporarily disabled
to prevent duplicate activation. **Cancel voice** abandons an active session and
returns to idle without killing the persistent worker.

## Browser transport

Speech uses a **separate authenticated WebSocket**, not the PTY terminal socket.

- **Route:** `GET /api/tasks/{handle}/stt` (same-origin upgrade).
- **Auth:** HttpOnly browser-session cookie plus same-origin `Origin` check,
  matching other Web Cockpit authenticated bridges.
- **Audio:** PCM16, 16 kHz, mono. The browser resamples captured audio and sends
  bounded binary frames with monotonic sequence metadata through a bounded
  client-side queue. Sustained backpressure becomes a visible warning or
  recoverable error; silent frame dropping is not used.
- **Control:** JSON `stt.start`, `stt.stop`, and `stt.cancel` messages; server
  events include `stt.ready` (only after the host model can accept audio),
  partial/final transcripts, speech activity, typed errors, and `stt.closed`
  after successful completion.

The phone never downloads a speech model, never requires WebGPU, and never runs
provider inference locally.

## Worker protocol

Ajax writes length-prefixed binary frames to the worker's stdin. All integers
are big-endian; PCM is little-endian signed 16-bit mono.

| Frame | Layout |
| --- | --- |
| start | `[0][u32 length][JSON body]` |
| audio | `[1][u32 sequence][u32 length][PCM16 bytes]` |
| finalize | `[2]` |
| cancel | `[3]` (optional session cancel without exiting the worker) |

The start body carries `sessionId`, `sampleRate`, `channels`, `language`, and
`phraseEndSilenceMs`. Every frame is self-delimiting, so a provider can read the
stream without guessing payload boundaries.

The worker replies on stdout with one JSON object per line:

```jsonl
{"type":"stt.ready"}
{"type":"stt.speech_started"}
{"type":"stt.partial","sequence":0,"text":"ever tried"}
{"type":"stt.final","sequence":0,"text":"Ever tried, ever failed."}
{"type":"stt.speech_ended"}
{"type":"stt.completed"}
```

- `stt.ready` is emitted only after dependencies and the streaming model are
  loaded and the session can accept audio. Process creation alone is not ready.
- `stt.completed` marks successful session finalization. Ajax maps that path to
  browser `stt.closed` and must not emit `stt.error`.
- Unexpected worker death still surfaces a typed provider error.

`sequence` correlates partials with the final that supersedes them. Unknown
event types are reported as provider errors rather than ignored. One persistent
worker serves many sessions; Ajax does not spawn a fresh model process per Mic
tap.

## iOS Safari and installed PWA behavior

Web Cockpit is **Safari-first** on iPhone over the private HTTPS listener. The
supported path is a normal Safari tab. Ajax does not ship a manifest, service
worker, or offline mutation model for speech.

Practical behavior on Safari and on an optional Home Screen installed shell:

- **Permission** — microphone access starts only from the **Mic** tap (user
  gesture). Denial surfaces a recoverable error; the terminal stays usable.
- **Ready gate** — the UI stays Connecting until host `stt.ready`; it does not
  show Listening before the model can consume audio. If Ready never arrives
  (for example an outdated legacy worker under `~/.ajax-dev/bin/`), Ajax fails
  the session with a recoverable error after a readiness timeout — re-run
  `./scripts/setup-stt.sh` and restart `ajax web` so the Moonshine v2 worker is
  loaded.
- **Interruptions** — audio-route changes, backgrounding, screen lock, tab
  suspension, or socket loss become an explicit **recoverable interruption**
  error instead of a silent "still listening" state.
- **Resources** — completion, cancel, and error paths stop tracks and release
  browser audio resources through one shared teardown.
- **Recovery** — read the status message, return to the task when ready, and tap
  **Mic** again (or use **Cancel voice** during an active session). After an
  Ajax STT upgrade, always re-run `./scripts/setup-stt.sh` and restart
  `ajax web` so the on-disk worker matches the protocol (including `stt.ready`).

## Normal use and transcript safety

1. Tap **Mic** once to start (one active session at a time). Status shows
   Connecting until the host is ready, then Listening.
2. Dictate; each finalized segment is auto-inserted into the active shell line
   in contiguous sequence order.
3. Tap **Mic** again while listening or during the spoken **pause** grace
   period to finalize the session and release the microphone, keeping
   already-inserted terminal text. **Cancel voice** still abandons the session.
4. Say standalone **`pause`** (normalized exact `pause`, including `Pause.`,
   `Pause,`, `Pause!`, `Pause?`) to enter a nine-second grace period. **Speak
   to continue** cancels the timer; if it expires, the session finalizes
   successfully (no error) and releases the mic.
5. Edit the shell line if needed, then press Enter from the terminal yourself
   when you want to submit.

Ajax does **not** auto-press Enter or execute commands on your behalf. Say
standalone **`start over`** or **`start fresh`** (including `Start over.`) to
clear auto-inserted dictation on the current line and keep listening. Sentence
uses of those phrases remain ordinary text. Existing keyboard Ctrl+C and tmux
behavior are unchanged.
