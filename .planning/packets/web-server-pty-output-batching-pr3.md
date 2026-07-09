# TDD Packet: Server PTY output batching (PR3)

## 1. Goal

Batch filtered PTY output bytes for up to 16ms or 16 KiB before one base64 JSON WebSocket `output` frame, without delaying browser→PTY input or resize handling.

## 2. Allowed files

**Production / tests (same module)**

- `crates/ajax-web/src/adapters/terminal_pty.rs`

**Plan (optional checklist only)**

- `.planning/agent-plans/web-mobile-power-optimizations.md`

## 3. Forbidden changes

- Do not change WebSocket frame schema (`type`/`data` base64 JSON) — binary frames are a later PR.
- Do not batch, delay, or coalesce input / resize / binary inbound frames.
- Do not change scrollback hostile filter semantics.
- Do not change ephemeral tmux session attach/teardown.
- Do not edit frontend TS/Svelte, polling, or other crates.
- Do not add new dependencies.
- No drive-by refactors outside the output send path.

## 4. Architecture context

`ajax-web` is a presentation adapter. The PTY bridge in `terminal_pty.rs` reads tmux-attached PTY bytes, filters scrollback-hostile sequences, base64-encodes, and sends JSON text frames to the browser. Batching only coalesces *when* frames are sent; bytes and filter order must be preserved. Input remains immediate (typing latency).

## 5. Code anchors

Constants near top of `terminal_pty.rs`:

```rust
pub const MAX_INPUT_FRAME_BYTES: usize = 4096;
const PTY_READ_BUFFER_BYTES: usize = 8192;
```

Add:

```rust
const TERMINAL_OUTPUT_FLUSH_MS: u64 = 16;
const TERMINAL_OUTPUT_MAX_BYTES: usize = 16 * 1024;
```

Output send path today (per chunk — must become batched):

```rust
output = output_rx.recv() => {
  Some(bytes) => {
    let filtered = filter_scrollback_hostile_sequences(...);
    if filtered.is_empty() { continue; }
    let encoded = base64::...encode(&filtered);
    let frame = TerminalOutputFrame { frame_type: "output", data: &encoded };
    let payload = serde_json::to_string(&frame)?;
    socket.send(Message::Text(payload.into())).await?;
  }
  None => break,
}
```

`TerminalOutputFrame` and `filter_scrollback_hostile_sequences` stay as-is.

Existing tests module at bottom of the same file — add unit tests there (no new test file required).

## 6. Test-first instructions

In `#[cfg(test)] mod tests` inside `terminal_pty.rs`, add:

1. `terminal_output_batch_pushes_until_max_bytes_then_take_drains`
2. `terminal_output_batch_take_on_empty_returns_none_or_empty`
3. `encode_terminal_output_frame_round_trips_base64_payload` (optional but useful)
4. `terminal_output_flush_constants_match_targets`

Recommended pure helper (exact names preferred):

```rust
struct TerminalOutputBatch {
    buf: Vec<u8>,
}

impl TerminalOutputBatch {
    fn new() -> Self;
    fn push(&mut self, bytes: &[u8]);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    /// true when buffered bytes >= TERMINAL_OUTPUT_MAX_BYTES
    fn should_flush_by_size(&self) -> bool;
    /// Drain all buffered bytes (empty → empty Vec).
    fn take(&mut self) -> Vec<u8>;
}
```

Test 1: push `b"abc"`, assert not should_flush; push a slice that brings len to `TERMINAL_OUTPUT_MAX_BYTES`, assert should_flush; `take()` returns concatenated bytes and leaves empty.

Test 2: `take()` on empty yields empty; `is_empty()` true.

Test 4: assert `TERMINAL_OUTPUT_FLUSH_MS == 16` and `TERMINAL_OUTPUT_MAX_BYTES == 16 * 1024`.

**Fail first:**

```bash
cargo test -p ajax-web terminal_output_batch -- --nocapture
```

(or the exact test filter matching the new test names)

Then implement helper + wire loop. Re-run:

```bash
cargo test -p ajax-web --lib
```

If full lib is heavy, at minimum:

```bash
cargo test -p ajax-web filter_scrollback_hostile terminal_output_batch handle_input_frame -- --nocapture
```

## 7. Production edit instructions

### Helper

Add `TerminalOutputBatch` (private) in `terminal_pty.rs` as above. Keep it sync/pure so unit tests do not need a WebSocket.

Optional small helper:

```rust
fn output_frame_payload(bytes: &[u8]) -> Option<String> {
  if bytes.is_empty() { return None; }
  let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
  serde_json::to_string(&TerminalOutputFrame { frame_type: "output", data: &encoded }).ok()
}
```

### Select loop wiring (minimal safe path)

Inside the existing `loop { tokio::select! { ... } }` that serves the terminal:

1. Keep `TerminalOutputBatch` local: `let mut output_batch = TerminalOutputBatch::new();`
2. Keep a flush deadline: `let mut flush_deadline: Option<tokio::time::Instant> = None;` or `tokio::time::Sleep` pinned — use the smallest clear pattern already idiomatic in this crate. Prefer:

```rust
let mut flush_delay = std::pin::pin!(tokio::time::sleep(Duration::from_millis(TERMINAL_OUTPUT_FLUSH_MS)));
flush_delay.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(86400 * 365)); // inert until armed
// OR: Option + `biased` select with `if flush_deadline = Some(deadline) = ...`
```

Simplest recommended pattern (avoid year-long sleep hacks):

```rust
let mut output_batch = TerminalOutputBatch::new();
let mut flush_deadline: Option<tokio::time::Instant> = None;

loop {
  let flush_wait = flush_deadline
    .map(tokio::time::sleep_until)
    .unwrap_or_else(|| tokio::time::sleep(Duration::from_secs(86400 * 365))); // only if Option pattern awkward
  tokio::pin!(flush_wait);
  tokio::select! {
    _ = &mut flush_wait, if flush_deadline.is_some() => {
      flush_deadline = None;
      // take batch, encode, send; if send err break
    }
    output = output_rx.recv() => { ... }
    incoming = socket.recv() => { ... } // UNCHANGED immediacy
  }
}
```

Prefer the `if flush_deadline.is_some()` guard form so the sleep branch is disabled when idle.

On `Some(bytes)` from `output_rx`:

1. Filter with existing `filter_scrollback_hostile_sequences`.
2. If filtered empty, continue (do not arm timer).
3. `output_batch.push(&filtered)`.
4. If `should_flush_by_size()`, clear deadline, take, encode, send immediately.
5. Else if deadline is `None`, set `flush_deadline = Some(Instant::now() + Duration::from_millis(TERMINAL_OUTPUT_FLUSH_MS))`.

On timer branch: take batch (if empty, no-op), encode once, send one Text frame; clear deadline.

On `output_rx` `None` (reader ended): flush any remaining batch once, then `break`.

On loop exit paths that `break` from send errors: OK to drop remaining batch.

**Do not** flush the output batch on every input frame — that would defeat batching under typing. Input handling stays as today.

Preserve: filter carry across chunks; base64 JSON schema; ping/pong; resize; binary input.

## 8. Verification commands

```bash
cargo test -p ajax-web terminal_output -- --nocapture
cargo test -p ajax-web filter_scrollback_hostile handle_input_frame -- --nocapture
```

Optional broader:

```bash
cargo test -p ajax-web --lib
```

## 9. Acceptance criteria

- New batch unit tests fail before helper exists, pass after.
- Constants are 16ms and 16 KiB.
- Select loop sends at most one WS output frame per flush (timer or size), not per PTY read chunk.
- Input/resize path still handled immediately in the same select loop.
- Existing filter / resize / cleanup tests still pass.
- Frame format unchanged (JSON text + base64 `data`).

## 10. Stop conditions

- Need a new crate dependency or change the WS protocol.
- Select-loop timer wiring conflicts with existing structure in a way that needs >~150 lines of rewrite — stop and report a narrower approach.
- Any edit outside `terminal_pty.rs` (except optional plan checklist).
