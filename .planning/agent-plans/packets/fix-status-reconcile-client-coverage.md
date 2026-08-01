PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Extend pane-wait reconcile client coverage so Done/Running projections cannot
lie when permission or input chrome is still on the pane.

Today `reconcile_idle` is Claude-only (`runtime_refresh.rs` ~402-404), so
Codex/Cursor can project FullyCompleted / Done while a permission prompt is
visible. `reconcile_running` includes Claude/Codex/Cursor but excludes Pi
(~385-390), so Pi parked at `pi>` / `>` with ActivelyWorking stays AgentRunning.

Mirror the existing Claude idle-reconcile gate for Codex and Cursor, and add Pi
to the running-reconcile match. Keep ack blocking and `prior_actionable_or_running`
unchanged.

## Allowed files

- `crates/ajax-core/src/runtime_refresh.rs`

## Forbidden changes

- Rewriting pane evidence through `reduce_agent_status` / `PaneEvidence` (later packet)
- Changing hook emitters, `AJAX_RUN_ID`, or delegated-run graph
- `agent_status.rs` / `canonical_agent_event.rs` / `pane_fallback.rs` API changes unless a one-line call-site requires it
- Commits, pushes, branch changes
- Unrelated cleanup

## Acceptance

1. `reconcile_idle` is true for `AgentClient::Claude | Codex | Cursor` when phase is `FullyCompleted` and `prior_actionable_or_running` holds (same gate as today, broader clients).
2. `reconcile_running` includes `AgentClient::Pi` alongside Claude/Codex/Cursor when phase is `ActivelyWorking`.
3. Focused tests: (a) Codex or Cursor FullyCompleted + permission-pane text → WaitingForApproval/Input (mirror `fully_completed_claude_idle_reconciles_to_waiting_on_permission_pane`); (b) Pi ActivelyWorking + pane ending in `pi>` / parked input → WaitingForInput.
4. Existing Claude idle-reconcile test still passes.

## Constraints

- Smallest match-arm / boolean change; reuse `reconcile_wait_from_pane` / existing test fixtures in this file.
- Do not change ack comparison or unknown_fallback path.
- Estimated scope ≤ ~80 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-core --lib runtime_refresh -- --nocapture
      expected: new Codex/Cursor idle + Pi running reconcile tests pass; existing Claude idle test passes
    - type: build
      command: cargo check -p ajax-core
      expected: success
  reason: Proves pane reconcile gates cover the missing clients without rewriting the reducer.
```

## Stop if

- Fix requires wiring PaneEvidence through reduce_agent_status
- Permission chrome strings for Codex/Cursor are unknown and cannot reuse pane_fallback recognizers
- Patch would exceed ~400 changed lines
