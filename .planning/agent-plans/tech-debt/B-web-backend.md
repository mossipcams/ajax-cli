# Tech Debt Audit B: ajax-web Rust backend

Scope: `crates/ajax-web/src` and `crates/ajax-web/sidecar`. Read-only audit; no
file changed other than this report.

## Crate size baseline (how counted)

- Total `.rs` lines in `crates/ajax-web/src`: **159,734** (`find -name '*.rs' | xargs cat | wc -l`).
- Per-file name split (`*_tests.rs` / `tests.rs` vs rest): ~118,009 non-test
  lines, ~41,725 test lines (inline tests inside production files are counted
  as prod; this is an approximation).
- `slices/web_session`: 28,362 total (11,113 non-test).
- `adapters/web_session_acp`: 22,298 total (9,629 non-test, 12,669 test).
- `runtime`: 10,160. `adapters/session_shim`: 1,737 (incl. 394 tests).
- `adapters/web_session_store`: 2,823 (incl. 839 tests).
- `sidecar/*.mjs`: 2,080 (5 files; only claude/cursor SDK sidecars exist — no
  Pi-sidecar file; Pi RPC is in-process JSON-RPC).

Context: per the Chat-first section of `architecture.md`, the live chat
session (`TaskSession`), the per-session shim with restart-adopt, headless
auto-resume, and Phase-3 live status are all *current documented behavior*,
not migration leftovers. That ruling is applied to every candidate below.

---

## Candidates (ranked by lines removable)

### B-01 SDK-harness client family: three parallel `*SdkClient`/`PiRpcClient` stacks over one shared core
- Paths: `crates/ajax-web/src/adapters/web_session_acp/{claude_sdk_client,cursor_sdk_client,pi_rpc_client,rpc_client,rpc_session,pi_rpc_handshake,rpc_handshake,pi_rpc_map,claude_sdk_map,cursor_sdk_map}.rs`, `slices/web_session/native_map.rs`
- Prod lines removed: **~800–1,200 max (speculative)** — family is 321 + 453 +
  394 (clients) + 384 (RpcClientCore) + 417 (rpc_session) + 222 + 136
  (handshakes) = 2,327 lines; the three clients/handshakes/maps duplicate a
  lot of shape across Claude/Cursor/Pi. Only consolidation removes real
  duplication; moving code does not count. I cannot prove how much is truly
  redundant without doing the refactor, so this range is an estimate, not a
  measurement.
- Test lines removed: **~2,900 if unification succeeds** (pi_rpc_map_tests
  727 + claude_sdk_map_tests 830 + cursor_sdk_map_tests 446 +
  pi_rpc_client_tests 381 + claude_sdk_client_tests 492 ≈ 2,876; map tests for
  three near-identical mappers are the cleanest part). Only if consolidation
  deletes per-harness maps.
- Evidence: `grep -rn "claude_sdk_client\|cursor_sdk_client\|pi_rpc_client"` shows
  all three clients instantiated in `session_client.rs` (lines 13, 20, 22) and
  `catalog.rs` (97, 128, 203). All three wrap `rpc_client::RpcClientCore`
  (`claude_sdk_client.rs:13`, `pi_rpc_client.rs:13`). All are live product
  transports — `transport_for()` (`session_client.rs:694-716`) resolves Acp /
  PiRpc / ClaudeSdk / CursorSdk per agent via `ajax_core::adapters::acp_launch_for_agent`.
- What replaces it, or why nothing needs to: a single generic SDK-sidecar
  client parameterized by harness dialect (spawn args, map fns). Nothing needs
  the three separate stacks *per se*, but each is currently reachable.
- Behavior or public contract change: none intended; pure internal refactor.
  Model catalogs (`/api/session/models`) must stay byte-equivalent.
- Risk: **high** — this is the live chat execution path for three harnesses;
  a merge regression breaks prompt dispatch, adoption, and model apply.
- Depends on: none.
- Confidence: **speculative** (duplication is proven by reading; removable
  amount is estimated).

### B-02 `/api/actions` route alias of `/api/operations`
- Paths: `crates/ajax-web/src/runtime/mod.rs:119`, `docs/architecture/web-cockpit.md:1032`
- Prod lines removed: **1** (`.route("/api/actions", post(axum_action::<C, B>))`)
- Test lines removed: **0 dedicated** — `/api/actions` appears inside shared
  suites (`crates/ajax-cli/src/web_backend/tests/suite_1.rs`, `runtime/tests/suite_4.rs`,
  `refresh_lane_tests.rs`) that also cover other routes; nothing exists solely for it.
- Evidence: frontend has **zero** callers — `grep -rn "api/actions" crates/ajax-web/web/src`
  returns nothing, while the registered route list (`runtime/mod.rs:60-139`) includes it as
  an alias of `/api/operations` (same handler `axum_action`, `task_routes/live.rs:657`).
  It is still a **documented** public contract (`web-cockpit.md:1032`: "alias of
  `/api/actions`; same handler").
- What replaces it: the existing `/api/operations` route.
- Behavior or public contract change: **PUBLIC** — removes a documented HTTP route.
- Risk: low (mechanical), but it is a public-contract removal → stop-condition territory per AGENTS.md; needs explicit approval.
- Depends on: none.
- Confidence: **proven** (dead in this repo's frontend, but publicly documented).

---

## Totals by confidence

| Confidence | Prod lines | Test lines |
|---|---|---|
| proven | 1 (B-02) | 0 |
| likely | 0 | 0 |
| speculative | ~800–1,200 (B-01) | ~2,900 (B-01) |

**Honest bottom line for this slice:** the ajax-web backend is already lean. I found
no proven dead subsystem ≥150 lines here. Proven removable code is 1 line; the only
realistic reduction is B-01's speculative unification (~1–4k lines total, high risk).
The repo-wide 15% target (38,650 lines) cannot be met from this slice and must come
from other slices.

## Not worth it (looked at, should stay)

- **ACP stdio stack** (`web_session_acp/{client,sdk_connection}.rs`, 768+905=1,673 prod):
  live fallback transport for every agent without an SDK launch (`transport_for` →
  `HarnessTransport::Acp`) and the substrate of restart-adopt
  (`acp_adopted.rs`, `session_shim`). Removing it means dropping ACP harnesses — a product decision, not debt.
- **`adapters/session_shim/`** (1,737): the shim is *the* chat-first restart-survival
  mechanism per architecture.md Chat-first bullet 4; adopted by `shim_adoption.rs`,
  `startup_recovery.rs`. Live core behavior.
- **`adapters/web_session_store/`** (2,823 incl. `legacy.rs` 88): single source of truth
  for transcript/ledger persistence; `slices/web_session/{replay,transcript,session_storage}.rs`
  are thin helpers *on top* of it (`transcript.rs:3` imports its `MAX_LOG_EVENTS`) — no parallel store.
  `legacy.rs` is the one-shot migration from the old transcript format, still read on startup (`mod.rs:64`).
- **`slices/web_session/task_session_phase2_tests.rs`** (830 tests): phase-named, but pins
  live ledger-phase + harness-switch behavior (`task_session_exit::ledger_phase`,
  `PromptPhase::Interrupted`) with real regression coverage (e.g. issue #1189 test). Renaming ≠ reduction.
- **Reliability-named modules** — `acp_drain` (328), `orphan_sweep` (216),
  `execution_ownership` (208), `startup_recovery` (230), `session_cleanup` (35),
  `background_jobs` (138), `headless_dispatch`+`runtime/headless.rs` (82+376): every one has
  live production callers verified by grep (`mod.rs`, `task_session_directory.rs`,
  `runtime/mod.rs:170,202`). Headless dispatch auto-resumes non-interactive chats on web
  start — current product behavior.
- **`adapters/server.rs`** (711): dev-web restart/profile/supervisor machinery behind the
  live `/api/server/restart` and `/api/server/test-in-stable` routes, both called by the frontend.
- **Traits checked for single-implementation speculativeness**: `RuntimeBridge` (1 prod impl + test seams used by 4+ test files), `AcpEventSource` (2 prod impls + fakes), `TerminalChild`, `TrainingCommandRunner` — all are working seams, not speculative extension points.
- **Routes-vs-frontend sweep**: every registered route in `runtime/mod.rs:60-139` has a frontend caller except B-02's alias (`grep -rhoE '"/api/...' crates/ajax-web/web/src`). WS endpoints (`/stt`, terminal, chat) all used.
- **Sidecars** (2,080 `.mjs`): `claude_sdk_sidecar.mjs` and `cursor_sdk_sidecar.mjs` are both embedded via `include_str!` and spawned live; their `.test.mjs` files are the only tests for them.

## Not reached / budget note

- Did not run `cargo build/test` (prohibited) — no dead-code compiler analysis.
- Did not exhaustively prove every pub fn in the 11k-line `web_session` slice has a
  caller; module-level reachability was proven, function-level was not.
- `runtime/state.rs` (993) and `task_session_directory.rs` (994) were read at summary
  level only; they are the live facade for chat status — flagged, not itemized.
