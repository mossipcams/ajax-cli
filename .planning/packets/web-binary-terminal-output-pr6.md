# TDD Packet: Binary terminal output frames (PR6)

## 1. Goal

Send terminal PTY output as WebSocket binary frames; keep control/error/resize as JSON text. Client accepts binary output (and still accepts legacy JSON base64 output for one release / test compatibility if cheap — prefer binary-only if tests are fully updated).

## 2. Allowed files

- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `crates/ajax-web/web/src/terminalConnection.ts`
- `crates/ajax-web/web/src/terminalConnection.test.ts`
- `.planning/agent-plans/web-mobile-power-optimizations.md` (optional)

## 3. Forbidden changes

- Do not change input path (already binary for keystrokes) or resize JSON.
- Do not remove output batching (PR3) — flush still batches; only the send encoding changes.
- Do not change scrollback filter semantics.
- Do not edit TerminalRawView except if connection tests force a tiny type tweak (prefer not).
- No new dependencies.

## 4. Architecture context

Today: filter → batch → base64 → JSON `{type:"output",data}` → Text WS → client JSON parse → atob → Uint8Array → TextDecoder.

Target: filter → batch → `Message::Binary(bytes)` → client binary → TextDecoder.

Error frames stay JSON text. Resize/input stay as today.

## 5. Code anchors

Server (`terminal_pty.rs`):

```rust
fn output_frame_payload(bytes: &[u8]) -> Option<String> { ... Message::Text ... }
// Change flush sites to send Binary instead of Text for output.
```

Client (`terminalConnection.ts`):

```ts
if (payload.type === "output" && payload.data) {
  const binary = atob(payload.data);
  ...
}
```

Also handle non-JSON / ArrayBuffer / Blob in `onSocketMessage` (there may already be a raw-text fallback).

## 6. Test-first instructions

**Rust:** Update `encode_terminal_output_frame_round_trips_base64_payload` — replace with tests that:

1. `output_frame_bytes_returns_raw_bytes_for_binary_send` (or rename helper to return `Vec<u8>` / Option<&[u8]> path)
2. Existing batch tests still pass
3. Assert flush path helper no longer base64-wraps for output (unit-level)

**TS:** In `terminalConnection.test.ts`:

1. Binary ArrayBuffer/Blob message decodes via TextDecoder and calls `onOutput`
2. Legacy JSON base64 output still works **OR** document removal if you drop compat — prefer keeping a small compat branch for safety during rollout
3. JSON error frames still call `onServerError`
4. Resize still sends JSON text

**Fail first**, then implement.

```bash
cargo test -p ajax-web terminal_output -- --nocapture
npm run web:test -- crates/ajax-web/web/src/terminalConnection.test.ts --run
```

## 7. Production edit instructions

### Server

1. Replace Text output sends with `Message::Binary(drained.into())` (or `Bytes`) after batch take.
2. Remove or stop using `output_frame_payload` for the live path; keep error JSON text helper.
3. Update unit tests accordingly; delete obsolete base64 round-trip if unused.

### Client

1. In `onSocketMessage`, if `event.data` is `ArrayBuffer` / `Blob` / `Uint8Array`, decode as UTF-8 stream and `onOutput` (reuse existing TextDecoder with `{stream:true}`).
2. If string: try JSON parse; handle `error`; optionally keep `output`+base64 compat.
3. Do not JSON-parse binary frames.

## 8. Verification commands

```bash
cargo test -p ajax-web terminal_output filter_scrollback_hostile handle_input_frame -- --nocapture
npm run web:test -- crates/ajax-web/web/src/terminalConnection.test.ts --run
```

## 9. Acceptance criteria

- Server sends binary for terminal output.
- Client renders binary output.
- Error/resize remain JSON text.
- Batching still applies before send.
- Input remains immediate binary/text as today.

## 10. Stop conditions

- Axum/WebSocket binary API unclear and needs dependency change.
- E2E fixtures hard-require JSON output and cannot be updated within Allowed files — stop and report.
