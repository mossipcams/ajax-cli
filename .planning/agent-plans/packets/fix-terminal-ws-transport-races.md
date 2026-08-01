PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Fix two WebSocket transport races in `terminalConnection.ts`.

1. Default `binaryType` is Blob; binary frames are handled in concurrent async
   IIFEs (`onSocketMessage` ~137-151), so `blob.arrayBuffer()` completions can
   reorder and corrupt streaming UTF-8 via `outputDecoder`.
2. `redialNow` / `connect` assign a new WebSocket without closing or ignoring
   the previous one (~169-186, ~183+), so a visibility redial while CONNECTING
   can orphan sockets and double-fire open/close/reconnect.

Set `socket.binaryType = "arraybuffer"` after open (or before listeners), and
track a generation / active socket so prior sockets are closed and their
close/open handlers are no-ops.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/terminalConnection.ts`
- `crates/ajax-web/web/src/shared/lib/terminalConnection.test.ts`

## Forbidden changes

- TaskTerminal.tsx / seed-reset / snapshot / link-menu changes
- Server PTY / terminal_pty.rs changes
- Commits, pushes, branch changes
- Unrelated refactors

## Acceptance

1. New sockets set `binaryType` to `"arraybuffer"` before message handling depends on it.
2. Calling `connect` / `redialNow` while a prior socket exists closes (or detaches) the prior socket; close handlers from superseded sockets do not schedule another reconnect.
3. Unit tests cover: (a) binaryType set on connect; (b) second dial while first still CONNECTING does not leave two live sockets / does not double onOpen; keep existing ArrayBuffer path tests green.
4. Blob decoding path may remain as fallback but must not be the primary unordered concurrent path when binaryType is arraybuffer.

## Constraints

- Keep public `connectTaskTerminal` API and status event shapes.
- Prefer a dial generation counter over a large rewrite.
- Estimated scope ≤ ~100 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run terminalConnection
      expected: new transport tests pass; existing terminalConnection tests pass
    - type: typecheck
      command: npm run web:check
      expected: success
  reason: Proves frame ordering and redial generation without needing a full e2e mobile run.
```

## Stop if

- Fix requires TaskTerminal or server protocol changes
- Cannot mock WebSocket binaryType / close in the existing test harness
- Patch would exceed ~400 changed lines
