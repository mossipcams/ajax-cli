# Plan: Attention notifications (rising-edge Waiting/Error)

Mode: Behavior Change. Status: **v3 — IMPLEMENTED 2026-07-10, all tasks done**.
Delegation decision: delegated via model-router (opencode-go/glm-5.2); the
delegate hung for 24 min with zero output and was killed → implemented locally
under packet constraints per the router fallback rule. Review gate run locally.

## Deviations from packet
- Core detector homed in existing `attention.rs` (typed attention domain), not
  a new `notify.rs` — fewer modules, same boundary.
- T3 (config + wiring) included in the same round since implementation went
  local; a `feat:` PR with unwired dead code would have cut a broken release.
- Three `Config` struct literals in test code needed mechanical `notify: None`
  additions (commands.rs, config.rs, task_operations.rs tests).

## Validation results (2026-07-10)
- Red steps observed (compile failures) before both impl rounds.
- `cargo test -p ajax-core -p ajax-cli`: 1097 passed.
- `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.

## T0 findings (2026-07-09) — supersede the v2 CAS design
- `Task.metadata` is `HashMap<String,String>` on the model
  (`models.rs:250`); sqlite persists it wholesale on snapshot save
  (`registry/sqlite.rs`: DELETE + re-INSERT `registry_task_metadata`,
  `save_if_revision` guard). There is NO per-key SQL API and none is needed.
- Shell saves go through `context.rs::save_context_with_state` /
  `tracked_save_state` with revision tracking + **merge semantics** on
  concurrent saves (see `save_context_merges_independent_same_task_fact_updates`).
- Therefore: drop the SQL CAS requirement. Dedup = rising-edge compare against
  `task.metadata["last_notified_status"]`, stamped through the normal tracked
  save. Residual race window (two processes observe the same first transition
  in the same tick) = one duplicate phone ping, benign.
  `// ponytail: best-effort dedup via metadata stamp; per-key CAS only if dupes annoy`
- Refresh callers (= notify pass sites): `cockpit_backend.rs:402`
  (watch loop) and `web_backend.rs:208/243`.
- Config home: `ajax-core/src/config.rs:206 struct Config`.
- `CommandSpec { program, args, cwd, mode, timeout }`
  (`adapters/command.rs:5`) — curl fits directly; assert-the-spec testing.

v2 rethink: operator is on iOS over SSH (Terminus) — a macOS banner on the host
is invisible to him. **Webhook push (ntfy) is the primary channel; macOS local
is deferred.** No HTTP client exists in the workspace (axum/hyper are
server-side) — deliver via `curl` through the existing `CommandSpec` /
`CommandRunner`, zero new deps. `registry_task_metadata` is key/value —
**no schema migration needed.**

## Problem
Operator must poll `inbox` / `ready` / `next`. Mission is "don't lose track";
mobile-first means push, not pull.

## Scope
- Fire once when a task **transitions into** `TaskStatus::Waiting` or
  `TaskStatus::Error` (rising edge only; re-fires only after leaving the state).
- Delivery v1: single configured webhook — `curl -s -d <body> <url>` as a
  `CommandSpec` (ntfy/Pushover compatible). Off by default.
- Body: `"{repo}/{handle}: {status} — {explanation}"`.

## Non-goals
- macOS osascript/terminal-notifier (deferred — host screen isn't watched).
- No Running/Idle/completion notifications, no batching, no quiet hours.
- No web-push/service-worker channel.

## Anchors (verified)
- Status reduction: `ajax-core/src/ui_state.rs::derive_operator_status(task)`;
  inbox membership is already `Waiting | Error` (architecture.md "Live Status").
- Reconcile hook: `ajax-core/src/runtime_refresh.rs::refresh_runtime_context_with_tier`.
- Dedup storage: `registry_task_metadata (task_id, key, value)` KV table —
  key `last_notified_status`. No `SQLITE_SCHEMA_VERSION` bump.
- Delivery: `adapters/command.rs` `CommandSpec` + `CommandRunner` (curl).
  Core stays OS/network-agnostic: core computes the transition; shell runs curl.

## Design
1. During reconcile, after `derive_operator_status`: if new status is
   Waiting/Error and differs from metadata `last_notified_status`, record a
   typed `AttentionTransition { task_id, status, explanation }` and write the
   metadata key. If new status is Running/Idle, clear/overwrite the key so the
   next entry re-fires.
2. **Dedup must be a compare-and-set in SQLite** (single
   `INSERT ... ON CONFLICT DO UPDATE ... WHERE value <> excluded.value`
   returning changed-row count, or equivalent), because CLI cockpit, web
   backend, and supervisor reconcile concurrently — whichever process wins the
   CAS delivers; losers stay silent. An in-memory flag is wrong here.
3. Shell: map transitions → `curl` CommandSpec, run via existing runner.
   Config: `[notify] webhook_url = "https://ntfy.sh/<topic>"` (absent = off).
   curl missing/non-zero exit → log, never fail the reconcile.

## Tasks (test → impl → verify)
- [x] **T1 edge detection.** `attention::take_attention_transition(&mut Task)`
  + `AttentionTransition` + `LAST_NOTIFIED_STATUS_KEY`. 4 tests in
  `attention.rs` (fires once / re-fires after Idle / Waiting→Error fires /
  Running+Idle silent and clear stamp). Green: 38 attention tests.
- [x] **T2 delivery spec.** `ajax-cli/src/notify.rs::webhook_command` →
  `curl -s --max-time 10 -d <body> <url>`, Capture mode, 10s timeout.
  2 spec-shape tests.
- [x] **T3 config + wiring.** `NotifyConfig { webhook_url }` as
  `Config.notify: Option<_>` (parse round-trip test).
  `notify_attention_transitions(context, runner) -> bool` called from
  `cockpit_backend::refresh_live_context` and web
  `CliRuntimeBridge::refresh_cockpit`; return value ORed into state_changed so
  the stamp persists. 2 pass-level tests (fires once + reports change;
  missing config silent).

## Risks
- **Storms** = the 3am pager. Guarded twice: rising-edge logic (T1) and
  CAS-in-DB so N concurrent reconcilers can't each fire (T1 concurrency test).
- Webhook publishes task titles/explanations to an external service — plan
  notes this; ntfy topic is effectively a password, user picks it. Flag in
  README/config docs.
- curl absent on host: doctor already checks tool availability — add curl to
  doctor only if notify is configured (conditional check, not a hard dep).

## Validation
`cargo test -p ajax-core -p ajax-cli` · `cargo clippy --all-targets -- -D warnings`
· `cargo fmt --check`. Manual: `AJAX_PROFILE=dev`, set `webhook_url` to a test
ntfy topic, force a task to Waiting, confirm exactly one push on the phone.

## Deferred
macOS osascript notifier (only if a desk-based workflow appears); web push.
