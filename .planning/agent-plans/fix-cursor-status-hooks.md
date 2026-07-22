# Plan: first-class Cursor/Pi status + live hook gate

## Scope

- Add `AgentClient::Cursor` and `AgentClient::Pi` as first-class clients
- Wire capability, live hooks, pane fallback, adapters, supervisor mapping
- Gate `__agent-event` writes on sibling agent-runtime snapshot liveness
- Update architecture.md sentence about hook eligibility

## Non-goals

- SQLite row migration from `Other` to `Cursor`/`Pi`
- Cursor/Pi pane chrome recognizers
- Commits, pushes, branch changes

## Delegation decision

`Delegation decision: delegated via model-router` — GLM weekly limit, escalated
to Cursor `composer-2.5`. Parent reviewed diff and re-ran validation (delegate
report envelope missing).

## Task checklist

- [x] Add/adjust failing tests (new_task, agent_capability, pane_fallback, agent_event)
- [x] Add `Cursor`/`Pi` enum variants + sqlite codec + `agent_from_name`
- [x] Wire capability, pane_fallback, adapters, supervisor (`live.rs` unchanged: only `Other` ignores hooks)
- [x] Implement `runtime_hooks_accepted` + gate `run_agent_event`
- [x] Fix exhaustive `AgentClient` matches (minimal)
- [x] Update `architecture.md`
- [x] Parent review + verification

## Validation results

Parent re-ran (2026-07-21); all passed:

```text
cargo test -p ajax-core pane_fallback -- 9 passed
cargo test -p ajax-core agent_capability -- 5 passed
cargo test -p ajax-core new_task_plan_cursor / new_task_plan_pi -- passed
cargo test -p ajax-core live -- 136 passed
cargo test -p ajax-cli agent_event -- 18 passed
cargo check --all-targets --all-features -- exit 0
cargo clippy -p ajax-core -p ajax-cli --all-targets -- -D warnings -- exit 0
```

## Deviations

- Husky verify caught `agent_status_cache::jsonl_fold_prefers_ask_over_stale_working_json_snapshot`
  failing under the live-runtime gate; parent seeded a Running runtime snapshot in that test.

Branch: `ajax/fix-cursor-status`.
