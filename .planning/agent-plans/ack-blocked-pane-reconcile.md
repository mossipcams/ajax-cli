# Fix ack-blocked pane reconcile fallthrough

## Scope

When pane reconcile sees wait chrome but attention ack blocks re-applying
Waiting, `continue` instead of falling through to apply lifecycle Working/Done
(Bugbot high on #711 follow-up).

Also land Cursor Notification wait hooks (already implemented, uncommitted).

## Non-goals

- Changing ack semantics / dwell
- Broad pane detector work

## Delegation decision

`Delegation decision: not delegated because smaller than the work order — one
continue path + one regression test on the just-reviewed diff`

## Checklist

- [ ] Failing test: ack-blocked pane wait chrome must not become AgentRunning
- [ ] Fix: continue when pane wait observed but blocked_by_ack
- [ ] Parent validation
- [ ] Commit Cursor hooks + fix; PR against main

## Validation

```bash
cargo test -p ajax-core --lib -- ack_blocked_pane running_lifecycle_reconciles cursor_running
cargo test -p ajax-cli --lib -- agent_hooks agent_event
```
