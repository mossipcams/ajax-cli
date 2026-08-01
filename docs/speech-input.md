# Speech input (Web Cockpit)

Continuous speech-to-text lets an operator dictate into the task terminal
composer from iPhone Safari while recognition runs on the Mac that hosts Ajax.
The phone supplies microphone audio; Ajax owns the authenticated transport,
session supervision, and a replaceable local provider process. Recognition
output never writes to the PTY, tmux, or shell.

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

Install or build your Moonshine-compatible sidecar on the host using whatever
workflow your sidecar documents. Point `provider_command` at the executable you
trust. Ajax only supervises the process you configure; it does not download
models to the browser or verify third-party package names.

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
  composer status region.
- Finalized composer text is preserved; unstable partial text is cleared.
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

Speech is **text composition only**:

1. Tap **Mic** once to start (one active session at a time).
2. Dictate through ordinary pauses; phrase boundaries finalize segments without
   stopping capture.
3. Say standalone **`pause`** (normalized `pause`, `Pause.`, `PAUSE`) to enter a
   nine-second grace period. **Speak to continue** cancels the timer; if it
   expires, the session finalizes and releases the mic.
4. Review and edit text in the **Terminal composer**; partial preview stays
   separate from editable value.
5. Use **Insert transcript** (or your normal send path) to move text into the
   terminal explicitly.

Ajax does **not** auto-press Enter, execute commands, or write recognition
output directly to xterm, tmux, or the PTY. Existing keyboard Ctrl+C and tmux
behavior are unchanged.
