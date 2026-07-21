# Notifications + native hooks rewiring

Mode: Behavior Change.
Delegation decision: delegated via model-router (three sequential packets).

## Scope

Rewire attention webhooks onto lifecycle/hook status evidence: expand native
wait/ask hooks where APIs allow, coarsen episode stamp to status class, enrich
webhook body with agent client, align tests/docs.

## Non-goals

- Browser Web Push; curl inside `__agent-event`; inventing Cursor/Pi wait
  signals; pane classification revival; lifecycle semantic changes.

## Checklist

- [x] Round 1: episode stamp class-only + client on AttentionTransition/webhook
- [x] Round 2: Codex PermissionRequest→ask; Cursor pre/postToolUse→working; e2e
- [x] Round 3: architecture.md, README, native-event-adapters, stale comments
- [x] Parent validation after Round 1 and Round 2

## Validation results

- Round 1: `cargo nextest run -p ajax-core attention` 59 passed; `notify` 5 passed; clippy/fmt OK
- Round 2: `agent_event`/`agent_hooks` 10 passed; lifecycle wait e2e 2 passed; clippy/fmt OK
- Round 3 docs: parent-local; final focused suite 59 + 16 passed; fmt OK

## Validation

```bash
cargo nextest run -p ajax-core attention
cargo nextest run -p ajax-cli notify agent_event agent_hooks
cargo fmt --check
cargo clippy -p ajax-core -p ajax-cli --all-targets --all-features -- -D warnings
```

## Deviations

- Round 1: GLM rate-limited; escalated to cursor-delegate composer-2.5. Report envelope invalid but delta in-scope; parent gate ACCEPT after independent validation (attention 59, notify 5, clippy/fmt OK).
- Round 2: same GLM limit → cursor-delegate; report envelope invalid again; briefly touched ledger (restored); parent ACCEPT (agent_event/hooks 10, e2e 2, clippy/fmt OK).
- Round 3: docs-only, parent-local (not delegated).
