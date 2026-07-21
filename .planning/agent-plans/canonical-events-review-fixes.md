# Canonical agent events — review conclusion fixes

## Scope

Address cross-cutting REVISE findings before merge:

1. **HIGH** — anon open-set leak (`ActivityStarted` without id + fold)
2. **HIGH** — pane wait re-apply churn (`live_status_unchanged` vs `pane_now`)
3. Socket: bound read + stop claiming immediate refresh delivery
4. Docs: plan non-goals / Codex SessionClosed matrix / architecture socket wording

## Non-goals

- Full socket → cockpit wake/fan-out (document honesty instead)
- Broad pane TTL redesign beyond the unchanged-kind skip
- New transports or schema_version bump

## Approval

User: “Address the conclusion” (2026-07-21).

**Delegation decision:** delegated via model-router → `cursor-delegate` /
`composer-2.5` (backend would prefer GLM; prior Pi/GLM rate-limits → escalate
to Cursor).

## Task checklist

- [x] **T1** Failing tests: anon leak + Claude background Stop → TurnStarted;
      pane kind-unchanged skip
- [x] **T2** Fold: no `anon-*` insert without id; ActivityFinished without id
      closes leftover anons if any
- [x] **T3** `claude_stop` background → `TurnStarted`; activity_id prefers unique ids
- [x] **T4** Pane path: skip apply when `kind` unchanged (drop `>= pane_now`)
- [x] **T5** Socket: bound `read_line`; docs honesty
- [x] **T6** Parent validation of focused suites

## Validation

```bash
cargo test -p ajax-core canonical_agent_event -- --nocapture   # parent: 7 passed
cargo test -p ajax-core pane_fallback_same_kind_skips_reapply  # parent: 1 passed
cargo test -p ajax-core pane_fallback -- --nocapture           # parent: 7 passed
cargo test -p ajax-cli agent_event -- --nocapture              # parent: 14 passed
cargo test -p ajax-cli agent_event_notify -- --nocapture       # parent: 1 passed
cargo test -p ajax-core runtime_refresh -- --nocapture         # parent: 47 passed
cargo fmt --check                                              # parent: 0
cargo clippy -p ajax-core -p ajax-cli --all-targets -- -D warnings  # parent: 0
```

## Deviations

- `scripts/run-delegate` exited FAILED (missing report envelope); delegate still
  wrote the diff. Parent recovered COMPLETE report from raw log and re-validated.
- Cockpit wake/fan-out not wired — docs honesty per review “or stop claiming”.

## Re-review (2026-07-21)

Three parallel subagents on the uncommitted fix diff:

| Lane | Verdict |
| --- | --- |
| fold / Claude (`canonical_agent_event` + `agent_event`) | ACCEPT |
| pane / socket / docs | ACCEPT |
| cross-cutting integration | ACCEPT (MEDIUM test/probe gaps only) |

Prior HIGHs (anon open-set leak, pane re-apply churn) treated as closed.
MEDIUM follow-ups only: JSONL two-Stop integration test, legacy `anon-*`
clearance test, capture-pane call-count assert, oversized socket-line test,
stale `.json` fallback when `.jsonl` present but unreadable.
