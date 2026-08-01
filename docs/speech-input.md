# Speech input (Web Cockpit)

Continuous speech-to-text lets an operator dictate into the active task
terminal from iPhone Safari while recognition runs on the Mac that hosts Ajax.
The phone supplies microphone audio; Ajax owns the authenticated transport,
session supervision, and a replaceable local provider process. Finalized
recognition output is inserted into the active shell line via the existing
paste/PTY input path; Ajax does not auto-press Enter or execute commands.

Design boundaries and ownership live in [`architecture.md`](../architecture.md)
under **Speech Input Architecture**. This page is the operator setup and daily-use
guide.

## Host provider (Moonshine Small Streaming)

Ajax does **not** ship a Moonshine model or a one-line installer. The Mac that
runs `ajax-cli web` must already have a **local sidecar process** that implements
Ajax's supervised STT adapter protocol: bounded PCM audio frames on stdin,
versioned transcript and VAD events on stdout. The initial adapter family is
**Moonshine Small Streaming**, sized for a local MacBook-class host.

Configure the shell command Ajax should launch:

- **`provider_command`** — the supervised local adapter command. Ajax spawns
  this process per speech session, health-checks it, and tears it down on cancel,
  finalization, or failure. Use a full path or a command that is on the server's
  `PATH` when `ajax-cli web` starts. The value is split on whitespace into a
  program and its arguments — there is no shell and no quoting, so a path
  containing spaces will not resolve. Wrap such a command in a small launcher
  script and point `provider_command` at that instead.

### The bundled sidecar

Ajax ships a reference implementation at `scripts/ajax-moonshine-sidecar`. It
speaks the framed protocol below and runs Moonshine ONNX locally. On the Mac
that hosts `ajax-cli web`, run the one-shot installer:

```sh
./scripts/setup-stt.sh
```

That creates `~/.ajax-dev/stt-venv`, installs `useful-moonshine-onnx`, copies
the sidecar to `~/.ajax-dev/bin/ajax-moonshine-sidecar`, and writes a matching
`[stt]` block into **both** stable (`~/.config/ajax/config.toml`) and dev
(`~/.ajax-dev/config.toml`) config files. One venv and sidecar serve both
profiles.

Manual setup (if you prefer not to run the script) uses its own virtualenv so it
never touches your system Python:

```sh
python3 -m venv ~/.ajax-stt-venv
~/.ajax-stt-venv/bin/pip install useful-moonshine-onnx numpy
```

Then point `provider_command` at the interpreter and the script — two
whitespace-separated tokens, since the value is split rather than shell-parsed:

```toml
provider_command = "/Users/you/.ajax-stt-venv/bin/python /path/to/ajax/scripts/ajax-moonshine-sidecar"
```

The model (`moonshine/tiny` by default) downloads from Hugging Face on first
run and is cached afterwards. Override it with `AJAX_STT_MODEL`, and set
`AJAX_STT_LOG=/tmp/stt.log` to capture sidecar diagnostics — stderr is
discarded by the parent.

You may substitute any other executable that speaks the same protocol. Ajax only
supervises the process you configure; it does not download models to the browser
or verify third-party package names.

## `[stt]` configuration

Add a `[stt]` block to `~/.config/ajax/config.toml` (or the file set by
`AJAX_CONFIG`). All keys are centralized in `ajax-core`; timing values are not
hard-coded in the UI.

```toml
[stt]
provider_command = "/usr/local/bin/your-moonshine-sidecar"
phrase_end_silence_ms = 700
pause_grace_period_ms = 9000
language = "en-US"
max_buffered_audio_ms = 2000
finalization_timeout_ms = 5000
```

| Key | Role |
| --- | --- |
| `provider_command` | Command Ajax launches for each session. **Required** for speech. |
| `phrase_end_silence_ms` | Provider phrase-end silence before a final segment (default `700`). |
| `pause_grace_period_ms` | Spoken `pause` grace period before finalization (default `9000`). |
| `language` | Recognition language tag (default `en-US`). |
| `max_buffered_audio_ms` | Bounded audio buffering for transport and provider (default `2000`). |
| `finalization_timeout_ms` | Safe finalization deadline (default `5000`). |

Unset `provider_command` leaves speech unavailable: the Mic control can be used,
but the session enters a recoverable error such as **no STT provider command
configured** instead of starting capture.

## Provider health and terminal safety

Ajax owns the authenticated browser endpoint (`/api/tasks/{handle}/stt`). The
STT service is isolated from the PTY WebSocket and task-session failures.

When the local provider cannot start, crashes, or reports unavailable:

- Speech enters an explicit **recoverable error** with a useful message in the
  Mic status region.
- Already-inserted terminal text is preserved; unstable partial text is cleared.
- The raw terminal, tmux attach, and Cockpit operations keep working. Provider
  failure does **not** take down the terminal or Ajax web runtime.
- After fixing host configuration or the sidecar binary, tap **Mic** again from
  idle or error to start a fresh session.

While a session is **connecting** or **finalizing**, Mic is temporarily disabled
to prevent duplicate activation. **Cancel voice** abandons an active session and
returns to idle.

## Browser transport

Speech uses a **separate authenticated WebSocket**, not the PTY terminal socket.

- **Route:** `GET /api/tasks/{handle}/stt` (same-origin upgrade).
- **Auth:** HttpOnly browser-session cookie plus same-origin `Origin` check,
  matching other Web Cockpit authenticated bridges.
- **Audio:** PCM16, 16 kHz, mono. The browser resamples captured audio and sends
  bounded binary frames with monotonic sequence metadata.
- **Control:** JSON `stt.start`, `stt.stop`, and `stt.cancel` messages; server
  events include `stt.ready`, partial/final transcripts, speech activity, and
  typed errors.

The phone never downloads a speech model, requires WebGPU, or runs provider
inference locally.

## Sidecar protocol

Ajax writes length-prefixed binary frames to the provider's stdin. All integers
are big-endian; PCM is little-endian signed 16-bit mono.

| Frame | Layout |
| --- | --- |
| start | `[0][u32 length][JSON body]` |
| audio | `[1][u32 sequence][u32 length][PCM16 bytes]` |
| finalize | `[2]` |

The start body carries `sessionId`, `sampleRate`, `channels`, `language`, and
`phraseEndSilenceMs`. Every frame is self-delimiting, so a provider can read the
stream without guessing payload boundaries.

The provider replies on stdout with one JSON object per line:

```jsonl
{"type":"stt.speech_started"}
{"type":"stt.partial","sequence":0,"text":"ever tried"}
{"type":"stt.final","sequence":0,"text":"Ever tried, ever failed."}
{"type":"stt.speech_ended"}
```

`sequence` correlates partials with the final that supersedes them. Unknown
event types are reported as provider errors rather than ignored. One process
serves one session; Ajax spawns a fresh one per session and kills it on cancel.

## iOS Safari and installed PWA behavior

Web Cockpit is **Safari-first** on iPhone over the private HTTPS listener. The
supported path is a normal Safari tab. Ajax does not ship a manifest, service
worker, or offline mutation model for speech.

Practical behavior on Safari and on an optional Home Screen installed shell:

- **Permission** — microphone access starts only from the **Mic** tap (user
  gesture). Denial surfaces a recoverable error; the terminal stays usable.
- **Interruptions** — audio-route changes, backgrounding, screen lock, tab
  suspension, or socket loss become an explicit **recoverable interruption**
  error instead of a silent "still listening" state.
- **Resources** — completion, cancel, and error paths stop tracks and release
  browser audio resources.
- **Recovery** — read the status message, return to the task when ready, and tap
  **Mic** again (or use **Cancel voice** during an active session).

## Normal use and transcript safety

1. Tap **Mic** once to start (one active session at a time).
2. Dictate through ordinary pauses; phrase boundaries finalize segments without
   stopping capture. Each finalized segment is auto-inserted into the active
   shell line through the same paste/PTY input path used for manual paste.
3. Say standalone **`pause`** (normalized `pause`, `Pause.`, `PAUSE`) to enter a
   nine-second grace period. **Speak to continue** cancels the timer; if it
   expires, the session finalizes and releases the mic.
4. Say standalone **`start over`** (normalized `start over`, `Start over.`,
   `START OVER`) to delete everything dictated in the current mic session from
   the terminal. The session keeps listening so you can dictate again.
5. Edit or press Enter from the terminal as you normally would.

Ajax does **not** auto-press Enter or execute commands on your behalf. Existing
keyboard Ctrl+C and tmux behavior are unchanged.
