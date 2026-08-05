# Plan: Ajax web server reliability

## Scope

Harden the host-native Web Cockpit **process** against:

1. Self-exit without a successor
2. Soft-wedge / false “backend unreachable”
3. Opaque liveness (no readiness / drain signal)
4. STT worker crash-loops
5. Persist-then-lose-CAS → false HTTP `409` / bad `request_id` replay

## Non-goals

- Public-internet auth model changes
- Per-task mutation concurrency / task-granular commits
- Blue/green dual-bind listener (note as follow-up after R0–R2)
- Frontend redesign; only minimal client hooks needed for health/restart semantics
- Rewriting terminal PTY or speech UI

## Delegation decision

`Delegation decision: delegated via model-router` (`cursor-delegate` / `composer-2.5`) for R0, R2, R4.

## Approval status

**Approved 2026-08-04** for the three Highs: **R0, R2, R4**.  
Private-network perimeter is given (not in scope). R1/R3 deferred unless requested.

| Wave | Status | Why |
| --- | --- | --- |
| R0 Restart exit discipline | **Approved — implement** | High: self-exit without successor |
| R1 Health readiness + drain flag | Deferred | Nice-to-have messaging; not one of the three Highs |
| R2 Refresh offload + tick tier | **Approved — implement** | High: soft-dead under lane/refresh |
| R3 STT restart backoff | Deferred | Medium / contained |
| R4 Lost-CAS recovery | **Approved — implement** | High: false 409 after durable persist |

Ship order: **R0 → R2 → R4**.

---

## Problem → fix map

| # | Failure mode | Fix wave |
| --- | --- | --- |
| A | `schedule_process_restart` / `schedule_test_in_stable` always `exit(0)` even if spawn fails | R0 |
| B | Phone / polls treat slow lane as crash; health always `{"ok":true}` | R1 + R2 |
| C | Cockpit refresh / push tick run sync substrate work on async workers | R2 |
| D | Push tick always `RefreshTier::Full` even when browser connected | R2 |
| E | Moonshine worker crash → unbounded `ensure_worker` respawn | R3 |
| F | Persist on clone, process-local CAS loses → `409` + replay stores lie | R4 |

---

## Wave R0 — Restart exit discipline

### Intent

Never kill the listener unless a successor process (or restart script) was successfully spawned.

### Files

- `crates/ajax-web/src/adapters/server.rs` — `schedule_process_restart`, `schedule_test_in_stable`
- Tests in same module / `runtime/tests` as needed

### Design

```text
sleep(RESTART_DELAY)
match launch_restart(...) {
  Ok(()) => std::process::exit(0),
  Err(e) => {
    log error (tracing + eprintln)
    // stay alive; clear any drain flag if R1 already landed
    return
  }
}
```

Same rule for Test-in-Stable: if wrapper script spawn fails, **do not exit**.

### Acceptance

- [x] Unit/characterization: injectable launch failure stays in-process (test cfg seam or `RestartLaunch` test double)
- [x] Successful spawn path still exits (existing restart JSON `restarting: true` unchanged)
- [x] Comment documents: supervisor-managed kill remains external; this only fixes self-exit-without-successor

### Validation

```bash
cargo nextest run -p ajax-web -- adapters::server
```

Result: passed (`adapters::server` focused run).

### Risks

- Operators who relied on “broken restart still exits so launchd restarts me” lose that accident. Prefer explicit non-zero exit **only** if we confirm a supervisor always restarts on non-zero; default = stay up and log.

**Decision in plan:** stay up on spawn failure (safest for solo WireGuard host without assuming launchd semantics).

---

## Wave R1 — Readiness / drain on `/api/health`

### Intent

Split **liveness** (accept loop up) from **readiness** (safe to treat as normal Cockpit) so the phone can show “Updating…” / “Busy…” instead of “backend unreachable” when the process is intentionally draining or the lane is held.

### Contract (additive)

`GET /api/health` remains **public** and **HTTP 200** whenever the accept loop can answer (preserves reachability checks and `waitForServerOnline` TCP/HTTP liveness).

Body becomes:

```json
{
  "ok": true,
  "state": "ready" | "busy" | "updating"
}
```

| `state` | Meaning |
| --- | --- |
| `ready` | Default; operate normally |
| `busy` | `control_lane` currently held (best-effort atomic flag or `try_lock` probe that does not wait) |
| `updating` | Restart / Test-in-Stable scheduled; process will exit soon **if** spawn succeeds |

Rules:

- Absent `state` in old clients → treat as ready (forward compatible)
- Set `updating` **before** spawning the delayed restart thread work (or at schedule entry)
- Clear `updating` if R0 spawn fails and process stays up
- `busy` is observational only; never block health on the lane

### Files

- `crates/ajax-web/src/runtime/mod.rs` — `axum_health`; process-global/drain flag on `WebAppState` or `server` module
- `crates/ajax-web/src/runtime/state.rs` — optional `lane_busy` / drain `AtomicBool`s
- `crates/ajax-web/src/adapters/server.rs` — set/clear drain around schedule
- `docs/architecture/web-cockpit.md` — health contract paragraph
- Minimal client: `checkHealth` / connection banner
  - `crates/ajax-web/web/src/shared/lib/api.ts`
  - `ConnectionStatus` / cockpit error mapping only if needed

### Client policy (minimal)

- `waitForServerOnline`: `response.ok` still enough (process answered). Optional later: require `state !== "updating"` **and** version bump — defer unless restart flake remains.
- Cockpit poll failure while last health was `busy` or `updating`: prefer “Busy” / “Updating” copy over “backend unreachable” (small UI string change).

### Acceptance

- [ ] Health returns `state: "ready"` by default
- [ ] After `POST /api/server/restart` (test: schedule no-op under cfg(test) but set drain flag), health shows `updating`
- [ ] R0 spawn failure clears `updating`
- [ ] Health handler never awaits `control_lane`
- [ ] Existing tests that assert `{"ok":true}` updated to allow/require `state`
- [ ] Arch doc updated

### Validation

```bash
cargo nextest run -p ajax-web -- health state updating
npm run web:test -- --run src/shared/lib/api.test.ts src/shared/ui/ConnectionStatus.test.tsx
```

### Risks

- Overselling `busy` (flicker). Mitigate: only report busy if lane held; UI debounces if needed.
- Do not return HTTP 503 for `updating` — that breaks “liveness” and confuses `waitForServerOnline` during the brief pre-exit window.

---

## Wave R2 — Soft-wedge: refresh offload + tick tier

### Intent

Keep lane serialization (snapshot semantics) but stop blocking Tokio worker threads on substrate probes. Stop the push tick from running Full refresh while an interactive browser is present.

### Design

1. **`refresh_cockpit_and_cache`**: after `control_lane.lock().await`, run `refresh_cockpit_and_cache_locked` inside `tokio::task::spawn_blocking` (pattern already used by Diff/start). Preserve cache TTL, revision check, push delivery side effect.
2. **`axum_cockpit` try_lock path**: same — if `try_lock` wins, do locked refresh via `spawn_blocking` + `blocking_lock` **or** drop try_lock and always use async lock + spawn_blocking. Prefer one code path: always `lock().await` then `spawn_blocking` body that assumes lane already held (pass nothing; hold guard across… **cannot** hold `MutexGuard` across await into spawn_blocking easily).

**Lane + spawn_blocking pattern (match start/action):**

```text
spawn_blocking {
  let _lane = state.control_lane.blocking_lock();
  refresh_cockpit_and_cache_locked(...)
}
```

For `axum_cockpit`:

```text
if cache hit → return
spawn_blocking { blocking_lock; refresh_locked Live }
```

Stale fallback when lane busy: keep today’s “return current projection without waiting” by using `try_lock` **inside** spawn_blocking, or `try_lock` on async side first:

```text
if cache hit → return
if try_lock fails → return current projection (unchanged)
spawn_blocking { /* lane already held via transfer? */ }
```

Tokio `Mutex` guard is not `Send` across spawn_blocking. So:

**Chosen approach:**

- Remove async `lock().await` from refresh entrypoints used by HTTP.
- Use only `blocking_lock` / `try_lock` inside `spawn_blocking` for refresh (same as mutate).
- Async `refresh_cockpit_and_cache` for push tick: `spawn_blocking { blocking_lock; refresh_locked }`.
- Cockpit GET: `spawn_blocking { match try_lock { Ok → refresh_locked; Err → serialize current view } }` plus cache check before spawn.

3. **Push tick** (`spawn_push_tick` in `runtime/mod.rs`):

```text
if browser_connected() {
  // optional: skip entirely, or Live refresh without deliver
  continue or refresh Live with deliver_notifications=false
} else if has_subscriptions() {
  Full + deliver
} else {
  skip or Live without deliver  // avoid Full with no subscribers
}
```

**Chosen:** If `browser_connected()` → **skip tick body** (browser polls already refresh Live). Else if subscriptions → Full + deliver. Else skip.

### Files

- `crates/ajax-web/src/runtime/task_routes/cockpit.rs`
- `crates/ajax-web/src/runtime/mod.rs` — `spawn_push_tick`
- Existing characterization tests in `runtime/tests/suite_3.rs` / `suite_4.rs` (health during refresh, lane busy)

### Acceptance

- [x] `axum_health_stays_responsive_during_slow_cockpit_refresh` still passes (strengthen: refresh uses spawn_blocking)
- [x] New/adjusted test: push tick does not call Full refresh while `browser_connected()`
- [x] `axum_cockpit_returns_current_projection_while_control_lane_is_busy` still passes
- [x] Projection JSON shape unchanged

### Validation

```bash
cargo nextest run -p ajax-web -- refresh_cockpit axum_health axum_cockpit browser_connected push
```

Result: passed (focused suite including refresh/tick/busy-lane).

### Risks

- Blocking pool saturation under storm of cockpit polls — cache TTL (750ms) + single-flight via lane still serialize work; OK.
- Skipping Full while browser connected delays orphan discovery until browser leaves — acceptable; Live is the interactive tier by design.

### Docs

Updated `docs/architecture/web-cockpit.md` push-tick paragraph.

---

## Wave R3 — STT worker restart backoff

### Intent

Contain a crash-looping Moonshine sidecar without taking down Ajax.

### Design

In `MoonshineProvider` (`adapters/stt_provider/mod.rs`):

- Track `last_spawn_at`, `consecutive_failures` (or spawn attempts in a window)
- On `ensure_worker` when previous worker dead: if within backoff window, return `ProviderError::Unavailable` with clear message instead of immediate respawn
- Caps (ponytail defaults): e.g. max 3 spawns / 60s, then require 30s cool-down; reset counter on successful `stt.ready` / healthy session
- Do not kill process; do not affect PTY

### Files

- `crates/ajax-web/src/adapters/stt_provider/mod.rs`
- `crates/ajax-web/src/adapters/stt_provider/tests.rs`

### Acceptance

- [ ] Simulated rapid worker death → third+ `start_session` returns Unavailable until cool-down
- [ ] After cool-down, spawn allowed again
- [ ] Existing overflow / cancel tests still pass

### Validation

```bash
cargo nextest run -p ajax-web -- stt_provider
```

---

## Wave R4 — Persist-then-CAS recovery (architecture-adjacent)

### Intent

HTTP success means “durable outcome is visible in shared state,” not “process-local revision CAS won.” SQLite remains cross-process authority.

### Hazard (restate)

`run_optimistic` may persist via `CliRuntimeBridge` on a clone, then lose `shared.revision` CAS to terminal ack or Diff `run_read`, return `409`, and `OperationCoordinator` may store that `409` for `request_id` replay.

### Design

1. **Detect durable success before CAS**  
   Operate/start already return `OperateOutcome { state_changed, ... }` wrapped into HTTP inside the closure. Prefer restructuring so `run_optimistic` learns whether disk was written:

   Option A (preferred, smallest):  
   `run_optimistic` takes closure returning `(Response, CommitHint)` where `CommitHint = { persisted: bool }` derived from operate success / `state_changed` / bridge save.

   Option B: parse response JSON for `ok: true` — brittle; avoid.

2. **On CAS loss with `persisted: true`:**  
   - Call bridge `reload_context_if_stale` / new `RuntimeBridge::reload_from_disk` into **shared** (not discarded clone)  
   - Bump `shared.revision`  
   - Clear cockpit cache  
   - Rebuild success response with `browser_cockpit_view(&shared.context)` (and original output if still available)  
   - Return **200**, not 409

3. **On CAS loss with `persisted: false`:** keep today’s 409.

4. **Idempotent replay:**  
   - Only `store_completed_response` for final returned response after recovery  
   - Never store a lost-race 409 when `persisted: true`  
   - Optional: on Replay hit, if stored was 409 and disk revision advanced, re-resolve — only if needed after (2)

5. **Tests (mandatory):**  
   - Hold mutate in `spawn_blocking` mid-operate; bump revision via `operator_input_sink` or Diff `run_read` metadata path; assert HTTP 200 and shared registry contains operate effect  
   - Duplicate `request_id` replays 200, not stale 409

### Files

- `crates/ajax-web/src/runtime/state.rs` — `run_optimistic`
- `crates/ajax-web/src/runtime/task_routes/live.rs` — start/action call sites
- `crates/ajax-web/src/runtime/bridge.rs` — maybe `CommitHint` / reload hook
- `crates/ajax-cli/src/web_backend.rs` — `reload_context_if_stale` exposure via trait
- `docs/architecture/web-cockpit.md` — optimistic commit paragraph

### Acceptance

- [x] Race test green (mutate + concurrent ack)
- [x] True conflicts (two mutates) still 409 via `OperationCoordinator`
- [x] Diff `run_read` metadata bump does not turn a successful ship into client-visible failure
- [x] Arch doc states: after durable persist, lost process-local CAS recovers via SQLite reload

### Validation

```bash
cargo nextest run -p ajax-web -- run_optimistic cas recover request_id
cargo nextest run -p ajax-cli -- web_backend
```

Result: ajax-web focused recovery/CAS tests passed; ajax-cli `web_backend` 20/20; `cargo check -p ajax-web -p ajax-cli` ok.

### Risks

- Double-apply on blind client retry of non-idempotent actions — mitigated by `request_id` replay of **success** after recovery  
- Reload must use same merge/save_state rules as refresh  
- Do not weaken `OperationCoordinator` single-mutation gate

---

## Sequencing

```text
R0 ─────────────────────────────────────────────►
R1 ────────── (after or parallel with R0) ──────►
R2 ────────── (after R1 health busy flag optional) ►
R3 ────────── parallel anytime ─────────────────►
R4 ────────── after R0–R2 validated ────────────►
```

Suggested ship order: **R0 → R2 → R1 → R3 → R4** if health UI copy can wait; or **R0 → R1 → R2** if phone messaging is the priority. R2 is the main soft-wedge fix; R1 makes the remaining wedge legible.

**Recommended ship order:** R0 → R2 → R1 → R3 → R4.

---

## Follow-ups (out of scope now)

- Blue/green / keep-old-binary-until-health for Test-in-Stable
- Graceful `axum` shutdown on SIGTERM with drain
- Cap parallel `capture-pane` inside core Live refresh
- Unify Diff metadata writes onto control lane (alternative to R4; heavier)

---

## Validation summary (full after all waves)

```bash
cargo nextest run -p ajax-web
cargo nextest run -p ajax-cli -- web_backend
npm run web:test -- --run src/shared/lib/api.test.ts
# after R1/R2: optional focused e2e connection banner if UI copy changed
```

Before any PR: `npm run verify` / husky gate per `AGENTS.md`.

---

## Checklist (execution ledger)

- [x] User approved waves: **R0, R2, R4** (private network given; R1/R3 deferred)
- [x] R0 implemented + validated (`adapters::server` 10/10)
- [x] R2 implemented + validated (refresh/tick + health isolation; revise fixed try_lock drop)
- [ ] R1 deferred
- [ ] R3 deferred
- [x] R4 implemented + validated (ajax-web recovery tests + ajax-cli web_backend 20/20 + arch doc)
- [x] Deviations noted below

## Deviations

- **R2 first delegate** acquired `try_lock` on the async side then dropped the guard before `spawn_blocking`/`blocking_lock`, regressing busy-lane stale fallback. **Resume revise** moved `try_lock` inside `spawn_blocking` and holds the guard across refresh. Parent re-validated.
- Delegate reports often failed schema validation (`INVALID_STRUCTURED_REPORT`) while still landing correct diffs; parent gated on delta + verification commands, not the YAML report envelope.
- **Delegation decision:** delegated via model-router (`cursor-delegate` / `composer-2.5`) for R0, R2, R4.
- **Bugbot (post-R4):** durable CAS recovery returned success after no-op reload when `paths` is `None`. Fix wave: `reload_registry_from_disk` → `Result<bool>`; on `Ok(false)` install operate clone. Packet `web-reliability-r4-cas-no-disk.md`.

## Checklist (follow-up)

- [x] R4 no-disk CAS recovery fix (Bugbot medium)
