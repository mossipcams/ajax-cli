# Official ACP client compliance

**Status:** complete
**Approval:** approved in chat 2026-08-14; operator requested uninterrupted completion
**Branch:** `ajax/older-task-create`
**Protocol baseline:** stable ACP v1; ACP v2 remains draft
**Defect:** [#880](https://github.com/mossipcams/ajax-cli/issues/880)

## Scope

- Replace Ajax's hand-written ACP JSON-RPC framing and wire shapes with the
  official `agent-client-protocol` Rust SDK while preserving Ajax's existing
  task-scoped session hub and browser event contract.
- Use typed ACP v1 initialization, session lifecycle, prompt, cancellation,
  update, config-option, and permission messages.
- Validate protocol negotiation and gate optional methods from advertised agent
  capabilities.
- Prefer `session/resume` when advertised, fall back to `session/load`, then to
  `session/new` when restoration is unavailable or fails.
- Preserve the current uncommitted cancellation, bounded-stderr, and timeout
  behavior (or replace it with equivalent SDK-backed behavior).
- Update the owning Web Cockpit architecture documents.

## Non-goals

- ACP v2 support while it is draft.
- Sharing one ACP subprocess across multiple Ajax tasks.
- Adding filesystem, terminal, elicitation, or MCP capabilities Ajax does not
  expose.
- Changing browser task truth, transcript ownership, or the public WebSocket
  protocol.
- Reworking agent discovery, model catalogs, or harness launch policy beyond
  what the SDK transport requires.

## Existing work to preserve

The worktree already contains user-owned changes in
`web_session_acp/client.rs`, `web_session_acp/hub.rs`,
`slices/web_session/fake_acp_tests.rs`, and `tests/fixtures/fake_acp.js`.
They correct `session/cancel` to a notification and add bounded stderr/timeout
diagnostics. Implementation must inspect and preserve their behavior; it must
not reset or overwrite that delta.

## Implementation checklist

- [x] Task 1 — Track and characterize ACP v1 compliance
  - Test: extend the fake-agent coverage so an unsupported initialize response,
    a nonstandard post-initialize notification, and a standard
    `session/request_permission` exchange are observable; run the focused tests
    and capture the expected failures against the current client.
  - Code: open/link the required Ajax defect issue; add the official SDK as a
    pinned workspace dependency and make the fake agent emit valid ACP v1
    initialization and permission messages. Changes under `tests/` are limited
    to the existing fake fixture explicitly named in this plan.
  - Verify: `cargo test -p ajax-web web_session_acp::client_tests -- --nocapture`
    and the focused fake-agent test target.

- [x] Task 2 — Use the official SDK for connection and transport
  - Test: add a focused spawn test proving newline-delimited stdio traffic,
    malformed-message errors, child exit reporting, bounded stderr context, and
    task-worktree cwd remain observable through the existing client facade; run
    it red before replacing the transport.
  - Code: replace request IDs, pending-response routing, raw JSON serialization,
    and the reader thread with the SDK client/transport connection behind the
    existing `AcpStdioClient` API. Reuse Ajax's existing program resolution and
    Tokio runtime; add only the minimal compatibility dependency the SDK stream
    adapter requires.
  - Verify: run the new focused test plus all `web_session_acp::client_tests`.

- [x] Task 3 — Negotiate and initialize stable ACP v1 correctly
  - Test: require Ajax to send typed ACP v1 client info/capabilities, reject an
    unsupported returned protocol version with an operator-facing error, and
    send no `notifications/initialized` message; run red first.
  - Code: use `InitializeRequest`/`InitializeResponse`, advertise only the
    capabilities Ajax actually implements, validate `protocolVersion == 1`, and
    remove the non-ACP initialized notification.
  - Verify: run the three handshake tests and the focused client suite.

- [x] Task 4 — Use typed session lifecycle and prompt messages
  - Test: cover `session/resume` preference, `session/load` replay fallback,
    `session/new` fallback, typed config options, prompt completion, and the
    existing `session/cancel` notification behavior; run the new restoration
    case red first.
  - Code: use SDK v1 request/response/notification types and advertised
    capabilities, while keeping one in-flight prompt and Ajax's bounded prompt
    queue unchanged.
  - Verify: run focused spawn/resume/prompt/cancel tests.

- [x] Task 5 — Implement standard ACP permission outcomes
  - Test: send a standard `session/request_permission` containing allow/reject
    option IDs and prove browser approve/reject selects the matching ACP option;
    prove cancelling a turn resolves any pending permission with the ACP
    `cancelled` outcome; run red first.
  - Code: keep the SDK responder and advertised options with the pending browser
    decision, map the existing boolean UI choice to the first matching
    allow/reject option, and return typed `RequestPermissionResponse` values.
    Do not add a second permission abstraction or change the browser protocol.
  - Verify: run focused hub, slice-mapping, and fake-agent permission tests.

- [x] Task 6 — Remove superseded protocol code and document the boundary
  - Test: add no new behavior test; first run the full focused ACP/Web Session
    suites to expose any references to the removed raw helpers.
  - Code: delete superseded framing/parsing helpers, keep handwritten Rust files
    below the repository hard limit, and update
    `docs/architecture/web-cockpit.md` and
    `docs/architecture/web-session-behavior.md` to name the official SDK,
    stable-v1 negotiation, restoration order, and permission semantics.
  - Verify: `cargo fmt --check`, `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings`,
    `cargo nextest run -p ajax-web --all-features --test-threads=1`,
    `npm run verify:slice -- web`, and `git diff --check`.

## Acceptance criteria

- No hand-written ACP JSON-RPC envelope, request-correlation map, or untyped ACP
  request/response construction remains in Ajax.
- Ajax negotiates only stable ACP v1 and reports an incompatible agent version.
- Optional restore methods are called only when advertised; resume is preferred
  over replaying load.
- Cancellation remains a notification and settles the live prompt.
- Standard ACP permission requests always receive a standard selected or
  cancelled outcome.
- Existing task-scoped hub, transcript, queue, model selection, and browser wire
  behavior remain intact.
- Current user-owned worktree changes are preserved semantically.

## Validation results

- Red/green regressions captured for protocol-version rejection, malformed
  stdout, initialize stderr diagnostics, resume-to-load fallback, standard
  permission mapping, and selected permission outcomes.
- `cargo test -p ajax-web --all-features`: 367 passed.
- `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings`:
  passed with no issues.
- `cargo nextest run -p ajax-web --all-features --test-threads=1`: 367 passed.
- `npm run verify:slice -- web`: passed (`cargo check` plus 367 nextest tests).
- `cargo test --workspace -- --test-threads=1`: 1,978 passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `client.rs` and `sdk_connection.rs` are 469 and 474 lines, respectively,
  below the repository's 1,000-line hard limit.
- The first parallel `cargo test --workspace` run failed in unrelated
  `ajax-cli::agent_status_cache` global-counter tests (347 passed, 4 failed).
  The focused cache suite passed 10/10 with `--test-threads=1`, and the full
  serialized workspace run passed 1,978/1,978.

## Deviations / changed assumptions

- Tasks 2 and 3 landed together because the official SDK connection owns typed
  initialization as part of establishing the transport; their tests and
  acceptance criteria remained separate.
- The operator explicitly requested uninterrupted completion, so the approved
  per-task continuation pauses were waived while red/green evidence was kept.

## PR preparation and conflict resolution

- Existing PR: [#879](https://github.com/mossipcams/ajax-cli/pull/879); the ACP
  work updates that PR rather than creating a duplicate.
- Strategy: merge `origin/main` into `ajax/older-task-create` so unrelated
  user-owned worktree edits remain in place and existing PR history is not
  rewritten. Safety backup branch created before the merge.
- `bridge.rs`: the PR side added per-harness acquisition plus authoritative
  replay/`busy` state; the base side added immediate outbound flushing after
  inbound messages. The resolution preserves both by routing poll and inbound
  paths through `flush_outbound`, retaining the agent argument and emitting
  `busy` on every `ready` event. Discarding either side would regress harness
  routing or visible chat latency; naively taking both created duplicate poll
  logic and an obsolete `Ready` shape.
- `web/dist/app.js`: both sides changed generated bundle output. It was rebuilt
  from the semantically merged TypeScript sources with `npm run web:build`; no
  minified code was hand-merged.
- Focused post-merge validation: bridge tests 3/3, Web Session tests 97/97,
  and session reducer/chat tests 33/33 passed.
- The ACP commit and merge-resolution commit both passed the complete Husky
  pre-commit gate: embedded web rebuild, staged Rust LOC check, `npm run
  verify`, release `ajax-cli` build, and locked forced install.
