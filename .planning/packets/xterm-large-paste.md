# Xterm large paste input frames

## Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Prevent connected large UTF-8 pastes from being dropped by keeping every
binary WebSocket input frame within the Rust bridge's 4096-byte limit while
preserving exact encoded bytes and order.

## Allowed files

- `crates/ajax-web/web/src/terminalConnection.ts`
- `crates/ajax-web/web/src/terminalConnection.test.ts`

## Forbidden changes

- Do not change the Rust PTY limit or protocol.
- Do not edit `TaskTerminal.svelte`, e2e tests, generated assets, dependencies,
  plans, packets, branches, or commits.
- Do not queue input while disconnected or change `sendInput`'s public type.

## Context evidence

- Graphify: `NOT_REQUIRED`; ownership remains in the existing browser
  connection boundary and Rust PTY bridge.
- Serena: `NOT_REQUIRED`; exact source/test symbols and reusable encoder/mock
  patterns are present in the named files.
- ast-grep: `NOT_REQUIRED`; this is one named method and one focused unit test,
  not a structural rewrite.
- Backend contract: `terminal_pty.rs::MAX_INPUT_FRAME_BYTES` is 4096 and binary
  frames larger than it terminate the bridge path.

## Code anchors

- Production: `connectTaskTerminal` return object's `sendInput`, currently one
  `socket.send(inputEncoder.encode(data))` call.
- Test: `terminalConnection.test.ts` `MockWebSocket` plus the existing resize
  send test pattern.

## Test-first instructions

Add one test that sends a payload exceeding 4096 encoded bytes with a multibyte
character crossing a chunk boundary. Capture binary `send` arguments, assert
the current implementation sends one oversized frame (RED), then require every
frame to be at most 4096 bytes and concatenation to exactly equal the original
`TextEncoder` bytes. Also assert ordinary small input remains one frame.

## Edit instructions

Encode once in `sendInput`, then send consecutive `Uint8Array.subarray` views of
at most 4096 bytes. Use one local constant matching the documented backend
limit. Do not decode/re-encode chunks; byte boundaries may split UTF-8 safely
because the PTY consumes the ordered byte stream.

## Verification commands

- `npm run web:test -- --run src/terminalConnection.test.ts`
- `npm run web:check`
- `git diff --check`

## Acceptance criteria

- No input binary frame exceeds 4096 bytes.
- Concatenated bytes exactly equal the original UTF-8 payload.
- Small input still sends one binary frame.
- Closed sockets still send nothing.

## Stop conditions

- The fix requires a protocol or Rust backend change.
- Tests require edits outside the allowed files.
- An unrelated validation failure occurs; report it without broadening scope.
