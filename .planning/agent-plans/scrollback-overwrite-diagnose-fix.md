# Scrollback overwrite: diagnose then fix

## Scope

Pin why scrollback still shows overwritten/mashed words on current main
(post-#666), then apply the smallest fix.

## Non-goals

- Do not reintroduce server CRLF pad / doubled pad / `-E -`
- Do not restore Ghostty/wterm
- Do not change `seed=0` reconnect policy or architecture ownership
- Do not edit the Cursor plan file

## Checklist

- [x] Task 1 — Reproduce/classify symptom with evidence
- [x] Task 2 — Choose fix + READY packet
- [x] Task 3 — Delegate implement; parent review gate + dist rebuild
- [x] Task 4 — Focused verification

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
    - crates/ajax-web/web/src/shared/lib/scrollbackOverwriteProbe.test.ts
    - crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md
    - crates/ajax-web/web/dist/terminal.js
    - .planning/agent-plans/scrollback-overwrite-diagnose-fix.md
  REASON: Multi-file TaskTerminal seed-window latch; frontend exceeds MiniMax bounds.
  ESCALATE_IF: [empty diff, scope violation, reintroduces pad]
```

Packet: `.planning/packets/scrollback-overwrite-bootstrap-erase.md`

## Approval status

User approved attached plan “Scrollback overwrite: diagnose then fix”.

## Evidence log

### Live tmux attach bytes
Attach prefix includes `\x1b[?1049h` then `\x1b[H\x1b[2J`. Ajax hostile
filter strips `1049h` and keeps `ED2`, so clears hit the **normal** buffer
(where the history seed lives).

### xterm probe (`scrollbackOverwriteProbe.test.ts`)
- ED2 + `scrollOnEraseInDisplay: true` → seed markers survive above live screen
- Permanent scrollOnErase → many near-duplicate FRAME-N screens in scrollback
- Bootstrap latch (true → first ED2 → false) → seed survives; later ED2 does not
  dump prior live frame

### Classification
**Cause C / RCA 1b:** permanent `scrollOnEraseInDisplay: true` (#666) fixed
attach seed wipe, but with alt-screen stripped, every live agent/tmux ED2 dumps
the viewport into scrollback. Scrolling history shows stacked near-duplicate
frames that look like words overwriting each other.

### Chosen fix
Gate scrollOnErase to the seeded-open window only (no server pad).

## Deviations

- cursor-delegate wrote a correct scoped diff but failed structured report
  (`MISSING_STRUCTURED_REPORT`). Parent gated on delta and re-ran verification.
- Delegate trimmed the diagnostic permanent-dump probe case; seed preserve +
  bootstrap-latch buffer proofs remain.

## Validation

Parent-run (accepted):

```bash
npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/shared/lib/scrollbackOverwriteProbe.test.ts
# PASS 35
npm run web:lint   # PASS
npm run web:check  # PASS
# dist/terminal.js newer than TaskTerminal.tsx; scrollOnEraseInDisplay present in dist
```

### IWDP (simulator Safari WebKit) — 2026-08-03

- Restarted this worktree onto dev `:8788` via `dev-web-restart.sh --worktree`.
- Attached IWDP to `https://localhost:8788/#/t/ajax-cli%2Ftest` (SIMULATOR only;
  no physical device on port 9221).
- Live xterm after open: `scrollOnEraseInDisplay === false` (latched off).
- In-page ED2 probe on that Terminal instance:
  - latched-off: only latest frame kept (`f0/f1/f3=0`, `f4>0`) → **PASS**
  - permanent-on (old bug): prior frames retained (`f0/f1/f3/f4>0`) → **PASS**
    (reproduces overwrite dump; confirms latch is the fix)
- Physical-phone busy-agent scroll feel still optional follow-up.

## Follow-up: File LOC peel (PR #749)

`Delegation decision: not delegated because R-SIZE-SPLIT`

Peel ledger: `.planning/agent-plans/task-terminal-file-loc-peel.md`
