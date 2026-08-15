# ACP chat reliability

**Status:** complete
**Scope:** Make the existing ACP-backed Web Cockpit chat reliable across
browser reconnects, host-side processing, persistence pressure, model changes,
and stable ACP update/capability negotiation.
**Non-goals:** ACP v2, a new browser state model, replacing the existing task
authority, or changing terminal/task lifecycle behavior.

## Implementation checklist

- [x] Task 1 — Make prompt delivery acknowledged and idempotent.
  - Test: add a browser transport test for a client prompt ID and acceptance
    acknowledgement; add a host test proving retrying the same ID does not
    dispatch a second ACP prompt.
  - Code: add client-generated prompt IDs, a durable accepted/duplicate result,
    and server acknowledgement before marking the starter brief delivered.
  - Verify: focused WebSession transport, bridge, and hub tests.
  - Result: browser transport tests 9/9; ACP hub tests 26/26.

- [x] Task 2 — Keep ACP sessions processing when no browser socket is connected.
  - Test: queue prompts, release the last socket, and prove the fake ACP agent
    completes the turn and advances the queue without a reconnect.
  - Code: add one host-owned background pump for live session slots; keep one
    ACP child and one event drain per task.
  - Verify: focused ACP hub tests and serialized ajax-web tests.
  - Result: focused ACP tests 65/65; `cargo check -p ajax-web` passed.

- [x] Task 3 — Remove per-token full-transcript rewrites and synchronous disk stalls.
  - Test: incrementally append streamed events, restart/load the transcript,
    and compact past the event cap without losing metadata or cursor offsets.
  - Code: use append-oriented persistence with bounded compaction/batching;
    keep disk work out of the hot session lock and coalesce streamed chunks
    where the browser contract permits it.
  - Verify: store/hub tests plus the focused ACP suite.
  - Result: store tests 6/6; focused ACP tests 67/67.

- [x] Task 4 — Preserve the selected model across reconnects.
  - Test: change the model, force a transport reconnect, and assert the new
    model is used in the next session URL without reverting to the old value.
  - Code: read the current model preference for each reconnect and preserve the
    task/session model as the host fallback.
  - Verify: focused browser transport/session tests.
  - Result: hook, SessionChat, and transport tests 16/16.

- [x] Task 5 — Keep ACP updates typed and cover stable update kinds.
  - Test: exercise message, thought, tool, plan, status, config, session-info,
    usage, and unknown update handling with the official ACP schema types.
  - Code: stop serializing typed `SessionNotification` values into generic JSON
    before mapping; preserve useful stable updates and safely retain unknown
    extensions without inventing conversation state.
  - Verify: mapper tests and ajax-web compilation against the pinned SDK.
  - Result: typed mapper coverage passed; ACP suite 69/69.

- [x] Task 6 — Implement or explicitly negotiate client filesystem/terminal capabilities.
  - Test: prove initialization advertises only implemented capabilities and that
    supported client requests are handled within the task worktree boundary;
    prove unsupported requests fail clearly.
  - Code: add the smallest safe worktree-scoped handlers needed by the selected
    ACP harnesses, or make the unsupported capability boundary explicit in the
    session error path. Update security documentation for any handler added.
  - Verify: fake-agent capability/request tests, focused ACP tests, and security
    boundary checks.
  - Result: initialization capability test passed; filesystem and terminal remain unadvertised.

- [x] Task 7 — Update architecture docs and run verification.
  - Test: no new behavior test; run the documented focused and broader checks.
  - Code: update the Web Cockpit/session behavior docs for acknowledgements,
    host-side processing, persistence, reconnect model behavior, and capability
    negotiation.
  - Verify: `cargo fmt --check`, focused ajax-web tests, web tests, clippy/checks,
    architecture slice verification, and `git diff --check`.
  - Result: docs updated; Rust 375/375, web 845/845 (9 skipped), check/clippy/fmt/diff checks passed.

## Approval

Approval: received in chat 2026-08-15. Task-by-task continuation approval is
still required by the repository workflow.
