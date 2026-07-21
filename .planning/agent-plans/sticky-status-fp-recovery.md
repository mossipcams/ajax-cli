# Plan: Clear sticky status false positives (idle composer + TestsFailed)

Status: **complete — accepted after Composer delegate**
Mode: Behavior Change (operator-visible status recovery)
Delegation decision: `delegated via model-router` → `cursor-delegate` / `composer-2.5`
(user: delegate until finished to Composer)

## Problem (from live stable tasks, 2026-07-21)

1. **`ajax-cli/pwa`** stuck on `CommandFailed` while Claude is idle at a
   filled composer (`❯\xa0watch CI…` + bypass-permissions chrome).
2. **`ajax-cli/statuses-detection`** stuck on `SideFlag::TestsFailed` while
   live is `AgentRunning`, CI green.

## Scope / Non-goals

See `.planning/packets/sticky-status-fp-recovery.md`.

## Approval

- Plan approved; user ordered Composer implement-until-finished (2026-07-21).

## Task checklist

- [x] 1. Test — Claude filled composer is idle (RED proven by delegate)
- [x] 2. Implement — filled-composer recognition
- [x] 3. Test — Healthy clears TestsFailed (RED proven by delegate)
- [x] 4. Implement — Healthy flag clear
- [x] 5. Docs — architecture.md
- [x] 6. Validation — parent re-ran focused tests + clippy

## Review Gate

- Delegate report schema failed extraction (`unknown report schema` — YAML
  fenced inside markers). Raw log showed COMPLETE with RED/GREEN evidence.
- Delta in scope only: `live_recognize.rs`, `runtime_refresh.rs`,
  `architecture.md`. No scope violations.
- Parent verification: PASS (see Validation results).
- Verdict: **ACCEPT**

## Deviations

- Packet VERIFY filter `live_recognize` matches 0 tests; module path is
  `live::recognize::tests`. Delegate used `recognize::tests` (27 passed).
  Parent used the same.

## Validation results

```text
cargo nextest run -p ajax-core claude_filled_composer_with_chrome_is_idle github_healthy
→ 2 passed

cargo nextest run -p ajax-core recognize::tests
→ 27 passed

cargo clippy -p ajax-core --all-targets -- -D warnings
→ exit 0
```

Not committed/pushed (user did not request).
