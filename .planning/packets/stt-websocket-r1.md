# Authenticated STT WebSocket — implementation packet

## Scope

Add the task-scoped authenticated STT WebSocket route to the existing Ajax
runtime. Reuse the browser-session middleware and same-origin policy; keep the
PTY terminal socket unchanged. Route `/api/tasks/{handle}/stt` through the
existing wildcard task handler and bridge it to the typed STT protocol and
provider boundary already present.

## Tests

- Missing browser session returns 401.
- Non-upgrade requests return 400.
- Cross-site WebSocket origins return 403.
- Same-origin upgrade reaches the STT handler and does not use PTY input.
- Start controls create only one provider session; stale/wrong session IDs,
  malformed controls, oversized/malformed audio, stop, and cancel are rejected
  or closed safely with typed error events.

## Implementation

- Add `MoonshineProvider` to shared `WebAppState` using centralized
  `Config.stt`; keep it behind `Arc<Mutex<...>>` and do not make the provider
  or route part of Ajax core task truth.
- Add a small async WebSocket loop that accepts JSON control frames and binary
  sequence-prefixed PCM frames, polls provider events, enforces bounded frame
  sizes, and performs safe finalization/cancellation.
- Include session IDs in every JSON event and ignore/close stale or duplicate
  session control. Keep auth/origin checks before upgrade.

## Constraints

- May edit `crates/ajax-web/src/runtime.rs`, this plan, and only directly
  necessary provider/protocol glue files.
- Do not change PTY WebSocket framing, keyboard/Ctrl+C behavior, task lifecycle,
  browser auth cookie policy, or frontend files.
- No public STT endpoint, no cloud dependency, no new dependencies, no commits
  or branch changes.

## Verification

```text
rtk cargo test -p ajax-web runtime::tests::axum_task_stt --lib
rtk cargo test -p ajax-web slices::stt::tests adapters::stt_provider::tests --lib
rtk cargo check -p ajax-web --all-targets
rtk cargo fmt --check
```

## Stop conditions

Stop after the authenticated route and focused transport tests pass. Frontend
capture/VAD and composer integration belong to later packets.
