# Status & Notification Engine Redesign

## Context

Ajax reduces six evidence vocabularies (lifecycle, live status, side flags, agent runtime status, runtime health, annotations) to a canonical 4-value operator status (`Running`/`Waiting`/`Idle`/`Error`) in `ajax-core::ui_state::derive_operator_status`. PR #419 bolted a minimal ntfy webhook onto this: `attention.rs::take_attention_transition` (rising-edge detector, dedup via `task.metadata["last_notified_status"]`) → `ajax-cli::notify.rs` (curl).

Three problems, confirmed with Matt:

1. **Wrong visible status — "Waiting while agent works."** Pane classification transiently flips a busy task to Waiting; UI shows it, phone gets pinged, then it flips back to Running.
2. **Notification false positives / too often.** The dedup stamp is *removed* whenever status returns to Running/Idle, so every Waiting→Running→Waiting flap re-fires.
3. **No background delivery.** Notifications fire only inside a CLI read command, the TUI stream tick, or a browser `GET /api/cockpit` poll. No browser tab → no notifications. There is no daemon; ajax-supervisor is observation-only.

Agreed scope: keep the visible 4-status set and all JSON/web contracts unchanged; fix Waiting confirmation at the root with **one mechanism serving both UI and notifications**; add a server-side notify tick to the always-running `ajax web` process (no new daemon); bounded unification of duplicated LiveStatusKind→attention-class mappings. Out of scope: new visible statuses, richer ntfy payloads, per-event config, `classify_agent_pane` pattern rewrites, supervisor changes, frontend changes.

**Verified facts the design relies on** (checked in source, not just summaries):
- `task.metadata` persists via `registry_task_metadata` (sqlite.rs:1181 write, :942 load) — dedup/candidate stamps survive SQLite round-trips and cross-process reloads.
- Only the pane path flaps: hooks use `apply_authoritative_observation_at`, wrapper uses `apply_trusted_observation_at`; both bypass the ordinary apply path (runtime_refresh.rs:380-385).
- **Trap:** runtime_refresh.rs short-circuits (`continue`) when a new observation equals current `live_status` (pane path ~:426-433, hook/wrapper path ~:373). During a busy/waiting flap the *busy* samples never reach the apply path — a deferred candidate must be explicitly cleared here or it confirms falsely later (Step 3).
- Web: `axum_cockpit` (runtime.rs:531-577) is the only web refresh trigger — cache TTL (750ms), `cockpit_refresh_lock` single-flight, revision-checked commit. `serve_axum_web` builds a multi-thread tokio runtime; a spawned tick task fits directly. `CliRuntimeBridge::refresh_cockpit` (web_backend.rs:236-252) already reloads SQLite on mtime/revision change, notifies, and persists.

## Design decisions

- **Confirmation window lives in the apply path (`live_application.rs`), not derive.** `derive_operator_status` is pure/clock-free and widely called; `apply_observation_at` already receives a timestamp. Gating at apply fixes every downstream consumer (ui_state, attention, annotations, JSON, web DTOs) at once — a waiting flap never mutates `live_status`, `agent_status`, or `SideFlag::NeedsInput` until confirmed.
- **Only busy→waiting is gated.** Gate fires when the observation is waiting-class AND the task currently shows running-class evidence. Idle→waiting stays immediate (matches the reported bug, keeps nearly all existing tests green). Errors stay immediate. Trusted hook/wrapper paths stay ungated.
- **Candidate state = one `task.metadata` key** (`waiting_candidate_since`, unix secs), same precedent as `last_notified_status`. No schema migration. Malformed value = absent.
- **Dwell: `WAITING_CONFIRMATION_DWELL = 4s`** compared against incoming `observed_at`. At 1s poll cadence a genuine wait shows in ~4–5s; at a 30s background tick it confirms on the second tick. Candidate cleared whenever any non-waiting observation applies — no expiry needed.
- **`take_attention_transition` unchanged.** The flap direction the classifier is biased against is spurious-Running-while-waiting; the confirmation gate kills the actual bug (spurious-Waiting-while-running). Genuine Waiting→Running(minutes)→Waiting cycles *should* re-notify, which stamp-removal-on-Running provides.
- **Unification: `LiveStatusKind::class() -> LiveStatusClass { Running, Waiting, Error, MissingSubstrate, Neutral }`** in models.rs, mirroring today's exact membership (Waiting includes `Done`). Consumed by the gate, `ui_state::live_evidence_is_acknowledged`, and `attention::annotation_kind_for_live_status` (with `Done → Reviewable` kept explicit). No table engine, no new module.
- **Background delivery: tokio tick inside `serve_axum_web`.** Factor the `axum_cockpit` body into `refresh_cockpit_and_cache(state)`; handler and tick both call it — same lock, same TTL, same revision-checked commit, zero new notify code. Config: `[notify] poll_seconds` (optional u64; default 30 when `[notify]` present; `0` disables; no `[notify]` → no tick). Cross-process dedup is free via the persisted stamp.

**Delegation decision** (AGENTS.md): delegated via model-router, one bounded step per delegation (`tdd-implementation-packet` per step), with local review + validation between steps. On approval, this plan is copied to `.planning/agent-plans/status-notification-redesign.md` as the execution ledger.

## Steps (TDD: failing test first per step)

### Step 1 — `LiveStatusClass` (behavior-preserving enabler)
- Test first (ui_state tests): for every `LiveStatusKind`, `canonical_waiting_explanation(kind).is_some() == (kind.class() == Waiting)`; same for error/running lists — prevents future divergence.
- `crates/ajax-core/src/models.rs`: add `LiveStatusClass` + `LiveStatusKind::class()`.
- Rewire `ui_state.rs::live_evidence_is_acknowledged` (:123) and `attention.rs::annotation_kind_for_live_status` (:181) onto `class()`.
- Existing ui_state rstest truth table + attention tests must pass unchanged.

### Step 2 — Confirmation gate in `crates/ajax-core/src/live_application.rs`
- Failing tests (via `apply_observation_at`, explicit timestamps):
  1. Busy task + `WaitingForInput` → live_status still `AgentRunning`, no `NeedsInput`, candidate key set.
  2. Second waiting obs ≥ dwell later → applied, candidate removed.
  3. Second waiting obs < dwell → still deferred, first-seen kept.
  4. Candidate then `AgentRunning` applied → candidate cleared; later waiting starts fresh.
  5. Non-busy task + waiting → applies immediately.
  6. Trusted/authoritative waiting on busy task → applies immediately (ungated).
  7. Confirmed waiting newer than acknowledgment → `derive_operator_status` = Waiting (ack semantics survive).
- Implement: `WAITING_CANDIDATE_SINCE_KEY`, `WAITING_CONFIRMATION_DWELL` (ponytail comment: constant; config knob only if real agents need tuning). In `apply_observation_at`: waiting-class + running-class task → stamp candidate & return, or return if < dwell, else fall through. Clear candidate on any non-waiting apply and on confirmed apply. Helper `has_pending_waiting_candidate(task)` re-exported via `live.rs`.

### Step 3 — Route flap busy samples past the `runtime_refresh.rs` short-circuit
- Without this, a busy sample equal to current live_status skips apply and a stale candidate confirms falsely.
- Failing test (runtime_refresh tests): task with `AgentRunning` live status + pre-set candidate key; refresh classifies busy pane; assert candidate cleared.
- Change: add `&& !live::has_pending_waiting_candidate(task)` to the `continue` conditions in the pane path (~:432) and hook/wrapper path (~:373). Persistence is free (`changed |= *task != previous`; metadata participates in `PartialEq`).

### Step 4 — End-to-end regression test for the original bug
- No production change. Attention-level test: busy task + single waiting obs → `take_attention_transition` = `None`; dwell-confirmed second obs → fires exactly once. Confirm `notify.rs` tests still pass.

### Step 5 — `[notify] poll_seconds` config knob
- Failing test (config.rs): parses with/without `poll_seconds`; existing configs stay valid.
- `NotifyConfig` (config.rs:265): add `#[serde(default)] pub poll_seconds: Option<u64>`. Update struct literals at config.rs:496 and notify.rs:158.

### Step 6 — Web-server background tick (`crates/ajax-web/src/runtime.rs`)
- Failing tests (inline `#[tokio::test]` + existing `TestBridge` pattern):
  1. `notify_poll_interval(config)`: no `[notify]` → `None`; default → 30s; `0` → `None`; `90` → 90s.
  2. `refresh_cockpit_and_cache(&state)` → bridge refresh_count 1, cache populated; second call within TTL → still 1 (tick and handler share single-flight/cache; existing TTL test at :1718 covers the handler side).
- Implement: extract `axum_cockpit` body (:536-576) into `refresh_cockpit_and_cache`; in `serve_axum_web` before `axum::serve`, if interval is `Some(period)`, `tokio::spawn` an `interval` loop (skip immediate first tick) calling it and ignoring errors. `DEFAULT_NOTIFY_POLL_SECONDS = 30` (ponytail comment: one redundant refresh per period while a browser polls — cheap; gate on cache age if it ever matters).

### Step 7 — Docs (same change, per AGENTS.md)
- `architecture.md`: Live Status section (~:304) — dwell-confirmed waiting candidates, trusted/error evidence immediate, candidate is Ajax-owned metadata. ajax-web runtime section (~:658) — optional notify tick reusing the `/api/cockpit` refresh path. Clarify the "Notifications are out of scope" paragraph (~:562): it forbids browser Web Push; server-side webhook via the CLI notify adapter is the supported channel.
- Document `poll_seconds` under `[notify]` where `webhook_url` is documented (README).

## Blast radius

- Unaffected: ui_state truth table, attention tests (side-flag driven), notify.rs tests, live_cli cockpit fixture (not busy-class), smoke_user_flows, web cockpit slice tests, web_backend reload tests, frontend/e2e (no rendering change).
- Must touch: `NotifyConfig` literals (config.rs:496, notify.rs:158); scan runtime_refresh tests for pane running→waiting sequences during Step 3 (runtime_refresh uses `SystemTime::now()`, so dwell confirmation is only testable at the live_application level with explicit timestamps — keep any such tests on the deferred assertion).
- Ack interaction: gate is upstream of acknowledgment; `acknowledge_attention` semantics unchanged (Step 2 test 7). Reviewable/Mergeable "Ready for review" is lifecycle-driven — untouched.

## Risks

1. **Short-circuit interplay** (highest): a missed skip-path leaves stale candidates → false confirmation. Mitigated by Step 3 tests on both paths.
2. **Latency of genuine waits**: ~5s browser-open; tick-only worst case ≈65s. Single constants if tuning needed.
3. **Dual writers racing** (CLI + web notify in same window): pre-existing best-effort (noted in attention.rs ponytail comment); tick uses the same reload-then-persist bridge, no widening.
4. **Blocking refresh on tokio from the tick**: identical to today's handler behavior on a multi-thread runtime.

## Validation

```bash
cargo nextest run -p ajax-core live_application
cargo nextest run -p ajax-core ui_state attention runtime_refresh
cargo nextest run -p ajax-cli
cargo nextest run -p ajax-web
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Manual (per memory: verify classifier-adjacent changes against real panes): run `ajax web` with `[notify]` configured, no browser attached; drive a task to a genuine waiting prompt → expect one ntfy delivery within ~2 poll intervals; confirm no delivery from transient waiting text while an agent streams.

## Execution ledger

Approval: plan approved by Matt (plan mode, 2026-07-10). Delegation decision: delegated via model-router.

Delegation A — waiting confirmation gate (Steps 1–4, ajax-core):
- [x] Step 1: LiveStatusClass + rewire ui_state/attention (tests first)
- [x] Step 2: confirmation gate in live_application.rs (7 tests first)
- [x] Step 3: runtime_refresh short-circuit fall-through (test first)
- [x] Step 4: attention-level end-to-end regression test
- [x] Validation: cargo nextest run -p ajax-core (live_application, ui_state, attention, runtime_refresh) + ajax-cli

Delegation B — background delivery (Steps 5–6, ajax-core config + ajax-web):
- [x] Step 5: NotifyConfig.poll_seconds (test first)
- [x] Step 6: refresh_cockpit_and_cache extraction + notify tick (tests first)
- [x] Validation: cargo nextest run -p ajax-web + config tests

Direct (non-delegated, docs-only):
- [x] Step 7: architecture.md + README notify docs

Final:
- [x] cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings
- [x] cargo nextest run --all-features
- [ ] Manual verification per plan

Deviations: (none yet)

Deviation (Delegation A): opencode-go/glm-5.2 hung (SIGTERM after ~10min, 0-byte output, empty diff). Known failure mode. Fallback per model-router: implemented locally under packet-a-waiting-gate.md constraints, review gate still applied.

Deviation (Delegation B): same glm-5.2 lane unavailable (hung on A); implemented locally under packet-b-notify-tick.md constraints, review gate applied.

Validation results: cargo fmt --check OK; cargo clippy --all-targets --all-features -D warnings OK; cargo nextest run --all-features 1568/1568 passed (2026-07-10).
Remaining: manual live-pane verification (run `ajax web` with [notify], no browser; expect one ntfy ping per genuine wait, none during agent streaming).
