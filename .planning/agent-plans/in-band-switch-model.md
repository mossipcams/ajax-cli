# In-band Switch model change

## Approval

Granted by operator spec (2026-08-19): same-harness Switch must keep the ACP
session and apply the model with `session/set_config_option`. A new session is
only allowed when the harness cannot safely change models in place.

## Problem

Switch (HTTP `POST /api/tasks/{handle}` swap) and WebSocket `set_model` persist
`session_model`, then drop or respawn the ACP child. That wipes harness session
state. ACP already supports changing `configId=model` on the live session; the
new model applies to the next generation.

## Contract (replaces #979 drop-on-Switch)

- Same harness, live slot: persist desired `session_model`, send
  `session/set_config_option` `{ sessionId, configId: "model", value }` using the
  mapped advertised ACP id (never a raw Ajax catalog id — #954). Keep the ACP
  process, `sessionId`, and JSONL transcript. Update `snapshot.model` from the
  harness-reported applied id.
- Same harness, no live slot: persist only. Next attach uses the pin.
- Cross-harness: persist + **context reset on the same public Ajax session**.
  Do not drop the TaskSession / JSONL / WebSocket. Cancel the in-flight turn
  and host queue, discard the old ACP child and its session id (no resume,
  no load, no replay of prior messages to the new harness). Spawn the selected
  harness with empty context (`session/new`). Append host note
  `Client switched harness. Context reset.` (role `note`). Prior turns stay
  visible in the Ajax transcript only. No live slot: persist and clear stored
  `acp_session_id` so the next attach is also `session/new`.
- In-band apply refused, unadvertised, or unprovable: one respawn fallback
  (`session/new`, no resume). Typed error if that still fails.
- Persist failure: typed error, live child unchanged (#931, #962).
- Invalid model: refuse before ACP traffic.

## Non-goals

- Mid-turn cancellation policy (in-band apply may run while a turn is in
  flight; the new model is for the next generation).
- Changing New Task create-time spawn.
- Changing #979 spawn-recovery for a *wrong* model at first handshake.
- Browser catalog / Switch UI chrome (HarnessSwap may keep calling
  `swapTaskAgent` if the host applies in-band).

## Implementation

- Reuse `apply_model_pin` / `resolve_cursor_pin_for_apply` on the live
  `AcpStdioClient` (add a command on the SDK connection actor).
- `set_model`: persist, then in-band apply; respawn only as fallback.
- HTTP swap: if the task's current agent equals the requested agent, do not
  `drop_session`; apply in-band on an existing slot instead.
- Update `web-session-behavior.md` and `web-cockpit.md` in this change.
- Do not grow `client.rs` (~596) or `live.rs` (~614) with new feature bulk;
  extract a small helper if the live-route decision needs more than a few lines.

## Tests

- `set_model` keeps `child_id` / ACP `sessionId` when in-band apply succeeds.
- Same-harness HTTP Switch does not drop the slot; applied model updates.
- Cross-harness swap still drops.
- Persist failure still leaves the child unchanged.
- In-band refusal falls back to one respawn.
- Existing #954/#979/#984 mapping guards still pass (catalog ids are not sent
  as `set_config_option` values).

## Verification

- `cargo test -p ajax-web web_session` (and focused `set_model` / swap tests)
- `cargo test -p ajax-core` mapping tests if catalog mapping is touched
- rustfmt on changed Rust

## Checklist

- [x] Architecture docs updated
- [x] Live `set_config_option` on same-harness Switch / `set_model`
- [x] Cross-harness is a context reset on the same public session (not drop)
- [x] Respawn only as unsafe-change fallback
- [x] Regression tests inverted/added
- [x] Focused verification run

## Status

Same-harness in-band apply and cross-harness context reset are on the branch,
uncommitted. No PR yet.
