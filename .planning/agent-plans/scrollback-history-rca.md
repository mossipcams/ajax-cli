# Scrollback history RCA + solution ranking

## Scope

Implement **1a**: permanent `scrollOnEraseInDisplay: true` on TaskTerminal, and
remove the server CRLF history pad so attach `CSI 2 J` pushes seeded viewport
lines into scrollback instead of erasing them.

## Non-goals

- Do not implement bootstrap-only (1b) unless 1a pollutes live agent scrollback.
- Do not reintroduce Ghostty / zero-lag overlay.
- Do not change `seed=0` reconnect, capture flags (`-E -1`, `-S -10000`), or
  hostile-filter list (keep stripping `CSI 3 J`).
- Do not commit/push unless asked.

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE:
    - crates/ajax-web/web/src/features/task/TaskTerminal.tsx
    - crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
    - crates/ajax-web/src/adapters/terminal_pty.rs
    - crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
    - crates/ajax-web/web/dist/terminal.js
    - .planning/agent-plans/scrollback-history-rca.md
  REASON: GLM weekly limit on pi-delegate; escalated once to cursor-delegate.
  ESCALATE_IF: [empty diff, scope violation]
```

## Root cause (summary)

History seed (`-E -1`) leaves newest lines in the xterm viewport; tmux attach
sends ED2; default xterm erases those cells. Pad/settle bandaids race geometry.

## Chosen fix (1a)

1. `new Terminal({ scrollOnEraseInDisplay: true, ... })`
2. Stop appending `rows` CRLFs in `captured_history_frame_bytes`; drop dead
   `client_rows` pad plumbing.
3. Update contract: no pad; PuTTY-style ED2 via xterm option.
4. Keep resize settle (still useful for width-correct capture).

## Checklist

- [x] RCA + solution ranking
- [x] Task 1 — READY packet + `scripts/check-packet`
- [x] Task 2 — Delegate implement (GLM limit → cursor-delegate composer-2.5)
- [x] Task 3 — Parent review gate + validation
- [ ] Task 4 — Device follow-up if agent ED2 spam appears

## Approval status

User authorized **Go with A** (permanent `scrollOnEraseInDisplay` + drop pad).

## Deviations

- Round 1 `pi-delegate` / `opencode-go/glm-5.2`: weekly Go usage limit.
- Round 2 `cursor-delegate` / `composer-2.5`: correct scoped diff; report wrapped
  in markdown fence with non-schema fields (`INVALID_STRUCTURED_REPORT`). Parent
  gated on delta + re-ran verification; not trusted on claim alone.

## Validation

Parent-run (accepted):

```bash
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx  # PASS 16
rtk cargo test -p ajax-web captured_history_frame_bytes -- --nocapture  # PASS 2
rtk cargo test -p ajax-web terminal_pty -- --nocapture  # PASS 27
rtk cargo fmt --check  # PASS
rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings  # PASS
npm run web:lint  # PASS
# dist rebuilt by delegate via npm run web:build
```
