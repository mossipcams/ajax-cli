# Phase 2 adoption: how a restarted web re-attaches to a live shim

Scope: answers the six questions in the Phase 2 adoption brief for `.planning/agent-plans/chat-first-core.md` (Phase 2 design decisions, 2026-10-27). Read-only. Paths are relative to the repo root; `ajax-web/src/` is shortened to `web/`, and `slices/web_session/` to `ws/`. Line numbers come from `grep -n` and `sed -n` in this worktree (`ajax-architectural-flaws`, branch `ajax/architectural-flaws`). Items marked **UNVERIFIED** are inferences or behavior the code cannot prove here (sidecar sources, the pinned ACP schema, or runtime behavior). Discovery facts already in `chat-first-core-phase2-discovery.md` are cited there and not repeated.

---

## Summary

- **Pure line → event mapping already exists for every transport.** The JSONL mappers (`map_sdk_message`, `map_record`, `map_cursor_message`) and the ACP update mapper (`map_acp_session_update`) are pure over `&Value`. What is not pure is the per-run state that wraps them (`RpcSession`) and the ACP turn-end path, which depends on the in-memory request id.
- **Turn end is detectable from the line alone for Claude, Cursor and Pi.** For ACP it is only detectable by shape (`result.stopReason`), not by request id.
- **Committed events carry a durable `seq`** (SQLite `execution_events`, keyed by run and seq). But no shim byte offset is recorded anywhere, so a replay from an old offset would duplicate text. The fix is to commit the consumed shim offset in the same transaction as the events.
- **Restart today uses ACP `session/resume` (or `session/load`) with the stored session id.** Handshake-time history is suppressed, so it never reaches the transcript. The browser sees the SQLite transcript, not harness history.
- **Adoption would install an attached client at `TaskSessionState.acp.client`** (`ws/task_session.rs:372-373`) on the same path as the cold acquire (`ws/task_session_spawn.rs:77-86`), before `reject_foreign_live_child` (`ws/task_session_spawn.rs:82`).

---

## 1. Raw stdout line → web events, and whether a pure function exists

### 1.1 Where the conversion happens, per transport

| Transport | Raw line parsed at | Turned into events at | Stateful wrapper |
|---|---|---|---|
| ACP (Codex, Claude-via-ACP, …) | the `agent_client_protocol` connection (`web/adapters/web_session_acp/sdk_connection.rs:162-178`, `run_async`), not by our code | notification handler `sdk_connection.rs:210-227` builds `AcpClientEvent::{ConfigOptionsUpdated, AvailableCommandsUpdated, SessionInfoUpdated, SessionUpdate}`; responses become `AcpClientEvent::RequestFinished` (`sdk_connection.rs:766-776`) | `command_loop` (`sdk_connection.rs:648`) and the `busy` flag |
| Claude SDK | `JsonlProcess` reader thread → `JsonlRecord::Record(Value)` (`web/adapters/web_session_acp/jsonl_process.rs:188-251`) | `RpcSession::next_step` (`web/adapters/web_session_acp/rpc_session.rs:196`) → `step_for_record` (`rpc_session.rs:246`) → `RecordMapper` (`rpc_session.rs:16`) = `map_sdk_message` (`web/adapters/web_session_acp/claude_sdk_map.rs:6`) | `RpcSession` fields (`rpc_session.rs:34-47`) |
| Cursor SDK | same | `RpcSession::step_for_record` → `map_cursor_message` (`web/adapters/web_session_acp/cursor_sdk_map.rs:8`) | same |
| Pi RPC | same | `RpcSession::step_for_record` → `map_record` (`web/adapters/web_session_acp/pi_rpc_map.rs:6`) | same |

The ACP side is split in two. The adapter emits `AcpClientEvent` (`web/adapters/web_session_acp/client.rs:209-239`). The slice maps those into `SessionServerEvent` in `ws/acp_drain.rs` (`drain_acp_events_with_prompt_cancel`, `acp_drain.rs:88`; `AcpClientEvent::SessionUpdate` branch at `acp_drain.rs:122-131`; `RequestFinished` branch at `acp_drain.rs:161-170`).

### 1.2 Is there a pure function already?

**Yes, for the event content. No, for the whole conversion.**

- ACP content: `map_acp_session_update(update: &Value) -> Vec<SessionServerEvent>` (`ws/acp_map.rs:12`) is pure. It takes the raw `params` JSON. The live path uses the typed `map_acp_session_notification(&SessionNotification)` (`ws/acp_map.rs:54`) instead. **UNVERIFIED** that the two produce the same events for every update kind. The replay path would have to use the `Value` one.
- ACP turn end: `map_request_finished(method, result, request_id, deduper, cancel_requested)` (`ws/acp_drain.rs:267-300`) is pure but needs the `method` string (`"session/prompt"`) and `request_id`. A raw line has neither, so this is where the gap is (see §2).
- JSONL content: `map_sdk_message`, `map_record`, `map_cursor_message` are pure `fn(&Value, &str) -> RpcMapping` (`rpc_session.rs:16-22`). They do not read any state. `RpcMapping` is `{ events, finished, ended_aborted }`.
- JSONL run state: `RpcSession::step_for_record` (`rpc_session.rs:246-274`) and `prompt_response_step` (`rpc_session.rs:276-318`) read and write `prompt_id`, `run_active`, `ended_aborted`, `pending_error`, `deferred_finish`, and `tool_inputs`. The helper functions `carry_tool_inputs` (`rpc_session.rs:359`) and `fold_pending_error` (`rpc_session.rs:340`) are pure over those fields.

### 1.3 Smallest extraction that makes a pure line fold

**Proposal (P2 below):** move the seven fields that `step_for_record` and `prompt_response_step` touch into a `RecordFold` struct with one method, `fold(&mut self, value: &Value, mapper: RecordMapper) -> Option<RpcStep>`. `RpcSession` keeps `process`, `queued`, `session_id`, `image_type_tag` and owns a `RecordFold`. Replay then constructs a `RecordFold` directly and feeds it logged `Value`s. Nothing else moves, and no behavior changes.

For ACP, the smallest pure function is a new `classify_acp_line(value: &Value) -> AcpLine` that returns one of `Update(params)`, `Response { id, result }`, `Request { id, method, params }`, or `Other`. Replay then calls `map_acp_session_update` for `Update` and `map_request_finished` for `Response`, after the turn-end shape check described in §2. This is P1 below.

---

## 2. Which raw line signals end of turn

| Transport | Line that ends the turn | Detectable from the line alone? | Evidence |
|---|---|---|---|
| ACP | JSON-RPC **response** to `session/prompt`: `{"jsonrpc":"2.0","id":N,"result":{"stopReason":"…"}}`. Built as `RequestFinished { method: "session/prompt" }` (`sdk_connection.rs:766-776`), mapped to `TurnEnd` at `acp_drain.rs:275-285`. | **Partly.** The line has no method name. `result.stopReason` is the only signal, and it is only safe if no other ACP response carries `stopReason`. **UNVERIFIED** against the pinned `agent_client_protocol` schema. Error responses (`{"id":N,"error":…}`) carry no `stopReason`, so they cannot be told apart from other errors without the request id. | `sdk_connection.rs:766-776`; `acp_drain.rs:275-285` |
| Claude SDK | record `type == "result"` → `map_result` (`claude_sdk_map.rs:299`), which sets `finished`. Sub-agent records are filtered by `parent_tool_use_id` (`claude_sdk_map.rs:9-17`). | **Yes**, from the record type alone. **UNVERIFIED** that the sidecar emits exactly one `result` per run (sidecar source is not in the tree). | `claude_sdk_map.rs:6`, `:299` |
| Cursor SDK | record `type == "result"` → `map_result` (`cursor_sdk_map.rs:177`), `finished = true`, `ended_aborted` when `status == "cancelled"`. | **Yes**, from the record alone. | `cursor_sdk_map.rs:8-33`, `:177-190` |
| Pi RPC | record `type == "agent_settled"` sets `finished` (`pi_rpc_map.rs:6`, arm `"agent_settled" => true`). A `response` whose `id` equals the in-flight prompt id also ends the run (`rpc_session.rs:276-318`), but that needs the id. | **Yes** for `agent_settled`. The response path needs the prompt id, which replay does not have unless it is persisted (§5). | `pi_rpc_map.rs:6-56`; `rpc_session.rs:276-318` |

**Summary for the design:** Claude, Cursor and Pi can be finalized from the line alone. ACP needs the `stopReason` shape check and a persisted in-flight prompt id (the prompt ledger, §5) so that `RequestFinished` can be matched to the active prompt.

---

## 3. How streamed chunks are committed, and whether replay would duplicate

### 3.1 The commit path

1. `pump` (`ws/task_session.rs:296`) drains `AcpClientEvent`s (`:305-313`).
2. Events are normalized (`normalize_session_events` → `StreamNormalizer::normalize_batch`, `ws/normalize.rs:35`; `ws/task_session.rs:326`). The normalizer merges stream continuations (`normalize.rs:164-177`) and keeps lane state (`normalize.rs:113-121`).
3. `append_to_log` (`ws/task_session.rs`, called at `:327`) → `commit_ledger_with_events` (`ws/task_session_commit.rs:4-37`). This persists the prompt ledger and the events in one call, `prompt_ledger::try_persist_with_events_and_rewrites` (`web/adapters/web_session_store/prompt_ledger.rs:304`). On failure the events stay in `unpersisted` and are retried (`ws/task_session.rs:327-328`).
4. The events are written to SQLite as `execution_events` rows (`web/adapters/web_session_store/mod.rs:258-288`, `try_append_events_with_rewrites`), through `ExecutionCommit.events` (`ajax-core/src/registry/sqlite/execution_store.rs:92`). Superseded entries are rewritten by seq (`mod.rs:283-285`, `seq: cursor + 1`).

### 3.2 Identifiers on committed events

- **Durable sequence:** each `execution_events` row has `seq`. Load reads rows `execution_events_since(run_id, 0)` and derives `dropped` from the first seq (`mod.rs:130-134`). This is a per-run monotonic sequence, not a per-chunk id.
- **In-memory cursor:** `TranscriptLog` cursor = `dropped + events.len()` (`ws/transcript.rs:57-59`), with trimming at `MAX_LOG_EVENTS` (`transcript.rs:61-68`).
- **Item id:** `SessionServerEvent::Message` carries `item_id` and optional `message_id` (`ws/mod.rs`, `Message` variant); the normalizer allocates item ids (`normalize.rs:117-125`). These identify a lane, not a line.
- **Shim byte offset:** **none.** Nothing in the committed transcript or the run row records which harness stdout line or byte offset produced it.

### 3.3 Would replay from an earlier offset duplicate text?

**Yes, unless the consumed offset is committed.** A committed event has no link back to a line, so a replayed text chunk would be appended again. Chunk merge (`normalize.rs:164-177`) would only merge with the tail of the seeded log (`task_session_spawn.rs:117`, `StreamNormalizer::seeded_from`), so a duplicate chunk would not be deduped.

**Required:** commit the shim offset in the same transaction as the events, and advance it only at commit time, not at drain time. Drained-but-uncommitted lines (the `unpersisted` retry case) must be re-read on adoption, which is correct because a new web process starts with an empty `unpersisted`.

**UNVERIFIED risk:** `StreamNormalizer` may hold pending text across batches (`normalize.rs:35-112`). If so, the committed offset must be the offset after the last line whose text is fully emitted, not simply the last line drained. This needs a read of `normalize.rs:18-112` before P3 is written.

---

## 4. Today's restore after a restart

### 4.1 What is spawned

1. A new web process runs the cold acquire (`ws/task_session_spawn.rs:31`). For a dead or missing slot it reads the stored session with `web_session_store::try_load` (`task_session_spawn.rs:67-75`). The resume id is `stored.acp_session_id`, but only when the stored harness matches (`task_session_spawn.rs:78-81`).
2. `reject_foreign_live_child` runs (`task_session_spawn.rs:82`), then `spawn_acp(agent, worktree, model, resume_id)` (`task_session_spawn.rs:83`) → `SessionClient::spawn_with_operator_pin` (`web/adapters/web_session_acp/session_client.rs:165`).
3. For ACP the connection sends `session/resume` (`sdk_connection.rs:545-566`, `send_resume`) if advertised, else `session/load` (`sdk_connection.rs:568-589`, `send_load`). If neither is advertised it fails with `RestoreFailure::Unsupported` (`sdk_connection.rs:441-450`). If resume is rejected it falls back to load (`sdk_connection.rs:452-470`). Without a resume id it sends `session/new` (`sdk_connection.rs:494-505`).
4. Claude, Cursor and Pi receive the session id as a launch argument or handshake request (`pi_rpc_client.rs:103-107` for `--session-id`; discovery §1.1 for Claude and Cursor), and handshake-time records are queued (`rpc_handshake.rs:43-55`, `pi_rpc_handshake.rs:31-77`, consumed through `RpcSession::queued`, `rpc_session.rs:320-322`).

### 4.2 What is replayed to the browser, and does history enter the transcript?

- **ACP history is suppressed.** During resume/load, `suppress_handshake_transcript` is set (`sdk_connection.rs:434-438`) and the notification handler drops every `session/update` except config, commands, session info and current-mode updates (`sdk_connection.rs:217-224`, `is_resume_load_capability_update` at `sdk_connection.rs:122-131`). Unparseable updates are dropped too (`sdk_connection.rs:262-268`). So **session/load history does not end up in the transcript.** The flag is cleared after the handshake (`sdk_connection.rs:661`; exact condition **UNVERIFIED**).
- **JSONL handshake records are not suppressed.** They are queued and mapped like any other record (`rpc_session.rs:320-322`). Whether any handshake record is transcript-relevant is **UNVERIFIED**.
- **The browser transcript is the SQLite log.** `adopt_stored_session` loads the stored events into `TranscriptLog` and seeds the normalizer (`task_session_spawn.rs:111-129`). The browser reads from this log, not from the harness.
- **Context reset:** when resume fails, a context-reset note is written (`ws/transcript.rs:210`, `context_reset_needed` at `transcript.rs:240`).

### 4.3 Test that pins this

`ws/task_session_restore_tests.rs:271-309` (`issue_1179_ajax_web_restart_restores_without_context_reset_note`): first acquire, record a seed message, detach, new `BlockingSessionDirectory`, acquire. It asserts the stored `acp_session_id` is unchanged, no `CONTEXT_RESET_NOTE` in the log, and `session_new_count == 1` (resume, not a second `session/new`). The adoption design must keep this test passing.

---

## 5. Client state from initialize / session-new, and what is persisted

### 5.1 What the client holds after the handshake

`ConnectionReady` (`sdk_connection.rs:520-538`) carries: `session_id`, `session_new_result`, `config_options`, `prompt_capabilities` (`sdk_connection.rs:533-535`), `close_advertised`, `load_session_advertised`, `restore_advertised`, `resumed`, `applied_model`, `model_apply_error`. `LiveSession` (`sdk_connection.rs:179-200` region, `live` set at `:255-262`) keeps `session_id`, `session_result`, `config_options`, `agent`.

The slice copies these into `TaskSessionState.acp` through `apply_spawn_capabilities` (`ws/task_session_spawn.rs:21-27`): `restore_advertised`, `session_config_options`, `session_prompt_capabilities`, and the cleared command list.

### 5.2 What is persisted, and where

| State | Persisted? | Where |
|---|---|---|
| ACP session id (resume key) | **Yes** | `StoredSession.acp_session_id` (`web/adapters/web_session_store/mod.rs:16-23`), loaded at `task_session_spawn.rs:67-81` |
| Model | **Yes** | `StoredSession.model`; run row `RunUpdate.model` (`execution_store.rs:73`) |
| Native session id on the run row | **Yes**, through the `RunUpdate` shape | `RunUpdate.native_session_id` (`execution_store.rs:72`) |
| Harness name | **Yes** | `StoredSession.harness`; `RunUpdate.harness` (`execution_store.rs:71`) |
| Prompt ledger (in-flight prompt ids, dispatching state) | **Yes** | registry kind `prompt_ledger` (`web/adapters/web_session_store/prompt_ledger.rs:10`), persisted with events (`prompt_ledger.rs:304`) |
| Transcript events and seq | **Yes** | `execution_events` (`mod.rs:258-288`) |
| `capabilities_json` | **Column exists, no non-test writer found in `crates/ajax-web/src`** | `RunUpdate.capabilities_json` (`execution_store.rs:73`). **UNVERIFIED** whether ajax-core writes it. |
| `config_options`, `prompt_capabilities`, `restore_advertised`, `close_advertised` | **No** (in memory only) | `TaskSessionState.acp` (`task_session_spawn.rs:21-27`) |
| Available commands | **No** (re-sent by the harness on the resume response or `AvailableCommandsUpdate`) | `sdk_connection.rs:244-249` |

### 5.3 Can an adopted client restore without a handshake?

**Partly.**

- Session id, model, harness and in-flight prompt id: yes, from the stored session and the ledger. An adopted client does not need `session/new` or `session/resume`.
- `prompt_capabilities`, `restore_advertised`, `close_advertised`: not persisted. They gate later behavior (for example `close` on shutdown, `sdk_connection.rs:419-423`). The adopted client can either default them conservatively (no `close`, which falls back to a detach that still kills the group, `client.rs:543`) or persist them in the same run-row update as the shim offset. **Recommendation:** persist them in the offset commit (§6, P3), because the SHUTDOWN-then-resume policy (§7) makes them transient anyway.
- `config_options` and `applied_model`: not persisted. They are only used by the UI and `apply_config_option`. An adopted client can serve the stored values, or the next acquire can re-read them through the resume path after shutdown. Acceptable for the first cut.

---

## 6. Where an adopted client would be installed, and what the pump expects

### 6.1 Install point

- `TaskSessionState.acp.client: Option<SessionClient>` (`ws/task_session.rs:372-373`).
- Cold path (`ws/task_session_spawn.rs:77-101`): `state.acp.client = Some(client)`, then `record_child_identity` (`task_session_spawn.rs:86`), then `apply_spawn_capabilities` (`:91`), `adopt_stored_session` (`:95-96`), `finish_first_acquire`.
- Entry: `TaskSessionDirectory::ensure_entry_for_acquire` (`ws/task_session_directory.rs:209-240`) → `spawn_task_session` (`directory.rs:227`). Startup dispatch calls `ensure_owner` and recovery (`task_session_directory.rs:75-77`, `:211`).
- Startup order problem: `ensure_owner` → `sweep_once` → `recover_interrupted_prompts` (`web/slices/web_session/startup_recovery.rs:30`, `execution_ownership.rs:51-80`). Adoption must run **before** `recover_interrupted_prompts` marks prompts `Interrupted` (`startup_recovery.rs:47-61`), or it must skip those tasks when a shim answers.

**Proposed install:** in `acquire` (`task_session_spawn.rs:31`), before the cold path's `reject_foreign_live_child` at `:82`, and in the dead-slot branch before `:60`. A new function `adopt_live_shim(state) -> Result<bool, SessionError>` returns `Ok(true)` when it installed an attached client. Only then does `acquire` skip spawning. `reject_foreign_live_child` (`task_session_spawn.rs:492-514`) stays for the no-shim case.

### 6.2 What the pump expects from a client

`pump` (`ws/task_session.rs:296-340`) and the exit paths use:

- `AcpEventSource`: `session_new_result` and `poll_event` (`web/adapters/web_session_acp/session_client.rs:56-63`), and `take_available_commands`.
- `SessionClient` methods: `host_exited` (`session_client.rs:264`, read at `task_session.rs:320` via `host_gone`), `child_pid` (`session_client.rs:281`, used as the activity probe root at `task_session.rs:226`), `cancel` (`session_client.rs:325`), `shutdown` and `detach` (`session_client.rs:247`, `:257`, used by the spawn loop at `task_session.rs:460-487` and by `release_live_client`, `task_session_spawn.rs:405-486`).
- Prompting and respond paths: `begin_prompt` (`session_client.rs:307`), `prompt_in_flight` (`:340`), `respond_client_request` (`:347`), `respond_elicitation` (`:366`), `apply_config_option` (`:468`), `apply_model_pin` (`:435`).

**Design rule:** an attached `SessionClient` must implement these with the same signatures. `detach` must not kill (no `Child`). `host_exited` must return true on socket EOF or connect failure. `child_pid` returns the recorded shim pid (§8, decision b).

### 6.3 What adoption cannot cover yet (gaps)

- **Pending agent→client requests** (ACP `session/request_permission`, elicitation, Pi dialogs) are in-memory: `PendingPermissions` (`sdk_connection.rs:117`-ish) and `ActivePrompt`. If the old web died while the harness waited for an answer, the adopted client must rebuild them from the log and answer with the original JSON-RPC id. **UNVERIFIED:** how the existing `PendingPermission` maps are keyed, and whether the log holds enough to rebuild them.
- **The shim log must hold outbound lines only** (per plan). Inbound lines (web→harness) are needed for the ACP request id check and for "was the prompt written". The shim should log inbound `session/prompt` lines too, or the ledger must record that the prompt was written before the shim accepted it.

---

## 7. Proposed minimal adoption design

### 7.1 Invariants

1. **No prompt is resent.** Adoption never writes `session/prompt` (or the Claude/Cursor/Pi prompt record). The attached client only reads, and writes only control lines (`REPLAY`, `SHUTDOWN`, permission answers). Enforced by a test with a fake shim that records inbound lines.
2. **No duplication.** The shim offset is committed with the events it produced (same SQLite transaction), and adoption replays from that committed offset.
3. **Not interrupted.** `recover_interrupted_prompts` and `orphan_sweep` skip any run whose shim socket answers `REPLAY`. The no-shim path is unchanged (`startup_recovery_tests.rs:79`).
4. **After the adopted turn ends,** the shim is told `SHUTDOWN` and the next acquire takes the existing resume path (`task_session_spawn.rs:77-83`). Chosen over "keep using the adopted shim" for the first cut because it reuses the path that `issue_1179` already covers, and it keeps the adopted client short-lived.

### 7.2 Flow

1. Web starts. `ensure_owner` runs (`execution_ownership.rs:51-80`). `sweep_once` skips any run whose socket answers.
2. For each recovered run with a live shim: connect `s-<hash>.sock`, send `REPLAY <committed_offset>`, read the replayed lines through the pure folds (§1.3, §2).
3. Replayed lines go through `map_acp_session_update` / `map_request_finished` (ACP) or `RecordFold` (JSONL) and are committed through `commit_ledger_with_events`, advancing the committed offset in the same transaction (§3.3).
4. The active prompt is restored from the ledger (no resend). Its terminal is captured when the replayed turn end arrives (`ws/acp_drain.rs:161-170`, `ws/task_session.rs:314-318`).
5. On turn end, the web sends `SHUTDOWN`, drops the attached client (`release_live_client` path, `task_session_spawn.rs:405-486`), and clears the run's child identity. A later acquire resumes with the stored session id (`task_session_spawn.rs:77-83`).
6. If the socket is gone before turn end, the run is treated as today (interrupted or swept), because no shim answers.

### 7.3 Packets (each ≤ 4 files, each a separate commit)

Packet order is the dependency order. Each packet's named test must be listed as passed by nextest. None touches a file in `tests/` (repo rule). Every new test module is declared in its parent `mod.rs` with a `#[cfg(test)]` line directly above it.

**P1 — Pure ACP line classifier (no behavior change).**
- Files: new `web/slices/web_session/acp_replay.rs` (`classify_acp_line`, `acp_turn_end(&Value) -> Option<Option<String>>`), new `web/slices/web_session/acp_replay_tests.rs`, `web/slices/web_session/mod.rs` (add `#[cfg(test)] mod acp_replay_tests;`).
- Tests: `acp_response_with_stop_reason_is_turn_end`, `acp_update_replay_matches_live_mapping` (compares `map_acp_session_update` with the typed mapper on the same fixtures).
- Size: 3 files. Depends on: nothing.

**P2 — Extract `RecordFold` out of `RpcSession` (pure move).**
- Files: new `web/adapters/web_session_acp/rpc_record_fold.rs` (fields from `rpc_session.rs:34-47` that `step_for_record` and `prompt_response_step` touch, plus `fold`), `web/adapters/web_session_acp/rpc_session.rs` (delegate to `RecordFold`), new `web/adapters/web_session_acp/rpc_record_fold_tests.rs`, `web/adapters/web_session_acp/mod.rs` (declare modules).
- Tests: existing `rpc_session_tests.rs` and `rpc_session_request_tests.rs` must pass unchanged (listed). New `fold_replays_claude_result_as_run_finished`.
- Size: 4 files. Depends on: nothing. Verify by a line-multiset diff of the moved code, not grep.

**P3 — Commit the shim offset with the events.**
- Files: `ajax-core/src/registry/sqlite/execution_store.rs` (add `shim_offset` to `RunUpdate` or `ExecutionCommit`), `web/slices/web_session/task_session_commit.rs` (thread the committed offset through `commit_ledger_with_events`), `web/adapters/web_session_store/prompt_ledger.rs` (persist it with the ledger), `web/slices/web_session/task_session.rs` (advance the offset at commit time only).
- **Decision needed:** a schema column needs a migration (`ajax-core/src/registry/sqlite/migrations.rs`), which is a core change. Matt should sign off. Alternative with no migration: store the offset in the prompt-ledger blob (`prompt_ledger.rs:10`), which is already transactional with events (`prompt_ledger.rs:304`). **UNVERIFIED** that the ledger write is one transaction with `commit_execution`. Read `prompt_ledger.rs` before choosing.
- Tests: `committed_offset_advances_only_with_persisted_events` (fails the persist, asserts the offset did not move).
- Size: 4 files, which is the limit. If the migration is chosen, split: P3a core column + migration (≤ 2 files), P3b web wiring (≤ 3 files).

**P4 — Attach mode for JSONL (no child ownership).**
- Files: new `web/adapters/web_session_acp/shim_attach.rs` (socket reader/writer producing `JsonlRecord`s, `REPLAY`/`SHUTDOWN` writes, EOF → `Exited`), `web/adapters/web_session_acp/jsonl_process.rs` (attach constructor; `Drop` does not kill for attached mode, `jsonl_process.rs:157-181` unchanged for owned children), `web/adapters/web_session_acp/jsonl_process_tests.rs`.
- Tests: `attached_drop_leaves_the_shim_running`, existing `dropping_the_process_reaps_the_child` and `dropping_the_process_also_kills_its_grandchildren` still pass.
- Size: 3 files. Depends on: P2 (for `RecordFold` use in the attached path, optional), and on the shim socket existing (plan Slices 1–4).

**P5 — Attach mode for ACP (generic stdio).**
- Files: `web/adapters/web_session_acp/sdk_connection.rs` (`ChildStdin`/`ChildStdout` at `:133-134` and `:362-363` become generic `AsyncRead`/`AsyncWrite`), `web/adapters/web_session_acp/client.rs` (only the `spawn_internal` call sites and `Child` becomes optional), new `web/adapters/web_session_acp/shim_attach_acp.rs` (or extend P4's module).
- Tests: existing `sdk_connection` and `client_tests.rs` pass; new `shim_attach_round_trips_initialize`.
- Size: 3 files. Depends on: the plan's Slice 0 (pure move out of `client.rs`, which is 760 lines, under the 1000 max but over the ~600 target). **Do not grow `client.rs`:** the attach variant belongs in the new module.

**P6 — Attached `SessionClient` variant.**
- Files: `web/adapters/web_session_acp/session_client.rs` (add `Attached` arm to the match in `transport`/`transport_mut` and each method in §6.2; the file is 645 lines, so put the new arms in a helper module), new `web/adapters/web_session_acp/session_client_attached.rs`, `web/adapters/web_session_acp/mod.rs`.
- Tests: `attached_client_host_exited_on_socket_eof`, `attached_detach_does_not_kill`.
- Size: 3 files.

**P7a — Startup: skip interrupt and sweep for live shims.**
- Files: new `web/slices/web_session/shim_probe.rs` (connect + `REPLAY` probe returning live/none, with a short timeout), `web/slices/web_session/startup_recovery.rs` (skip `recover_interrupted_prompts` for a live shim, `:30-101`), `web/slices/web_session/orphan_sweep.rs` (do not TERM/KILL a live shim's group, `:61-112`).
- Tests: `live_shim_is_not_interrupted_on_startup` (new); existing `foreign_boot_dispatching_prompt_is_interrupted_with_note` (`startup_recovery_tests.rs:79`) must still pass with no shim.
- Size: 3 files. Depends on: P4 or P5 (the probe needs the shim socket to answer).

**P7b — Adopt: install the attached client in `acquire`.**
- Files: new `web/slices/web_session/task_session_adopt.rs` (`adopt_live_shim`: restore session id, model and ledger prompt, committed offset, then `REPLAY`), `web/slices/web_session/task_session_spawn.rs` (call `adopt_live_shim` before `:60` and `:82`; file is in the ~500-line range, check size first), `web/slices/web_session/task_session_child.rs` (record the shim pid as child identity, decision b).
- Tests: `live_shim_is_adopted_not_interrupted` (new), `adopted_session_never_resends_prompt` (fake shim records inbound lines, asserts no `session/prompt`), `issue_1179_ajax_web_restart_restores_without_context_reset_note` unchanged and passing.
- Size: 3 files. Depends on: P3, P6, P7a.

**P8 — Adopted turn end: SHUTDOWN, then the existing resume path.**
- Files: `web/slices/web_session/task_session_exit.rs` (on the replayed turn end of an adopted client, send `SHUTDOWN` and clear the client; `task_session_exit.rs` is 745 lines, so put the helper in a new module), new `web/slices/web_session/task_session_adopt_end.rs`, `web/slices/web_session/task_session_adopt_end_tests.rs`, `web/slices/web_session/mod.rs`.
- Tests: `adopted_turn_end_shuts_shim_then_next_acquire_resumes` (asserts the next acquire's spawn uses the stored `acp_session_id`, and `session_new_count` stays 1).
- Size: 4 files.

**P9 — Live check and docs.**
- Files: `docs/architecture/web-session-behavior.md` (per AGENTS.md, for session behavior), and the live-check notes. The plan file is Matt's to edit, not ours.
- Live check (plan Phase 2): start a chat turn, restart the dev web (`scripts/dev-web-restart.sh`), confirm the turn finishes and the browser reconnects with no interrupted note. Matt must run the launchd part (nono blocks `launchctl bootstrap`).
- Size: 2 files.

---

## 8. Open decisions for Matt (kept from the discovery file, plus new ones)

- **(a) Shim log path:** `web-session/<enc>.shim.log` or `sessions/` (plan). This proposal assumes `web-session/`.
- **(b) Recorded child pid:** shim pid (this proposal) or harness pid. The shim pid keeps the group-kill sweep working.
- **(c) Eviction of an idle shim-owned session:** this proposal keeps today's behavior (eviction kills the group, `task_session_directory.rs:246-317`, `Shutdown { close: false }` at `:312`). Keeping the harness alive for an evicted chat needs a new policy.
- **(d) `setsid` placement:** the shim's first action (short window, no `unsafe`) or `pre_exec`.
- **(e) New:** offset storage. A schema column (core migration, needs sign-off) or a field in the prompt-ledger blob (no migration, needs the transactionality check in P3).
- **(f) New:** whether the adopted run should persist `prompt_capabilities` and `close_advertised` (recommended yes, §5.3).

---

## 9. What is UNVERIFIED (consolidated)

- The pinned `agent_client_protocol` schema: that only `PromptResponse` carries `stopReason` (§2).
- That the sidecars (`claude_sdk_sidecar.mjs`, `cursor_sdk_sidecar.mjs`) emit exactly one `result` record per run (§2). Sidecar sources are not read here.
- That `map_acp_session_update` and `map_acp_session_notification` agree on all update kinds (§1.2).
- Whether `StreamNormalizer` holds text across batches (§3.3). Needs `normalize.rs:18-112` read before P3.
- That the prompt-ledger write is one SQLite transaction with `commit_execution` (§8e, P3).
- Whether ajax-core writes `capabilities_json` (§5.2).
- The exact condition that clears `suppress_handshake_transcript` (`sdk_connection.rs:661`, §4.2).
- Whether any JSONL handshake record is transcript-relevant (§4.2).
- How `PendingPermission` maps are keyed, and whether the log holds enough to rebuild them (§6.3).
- Whether `ExecutionCommit` / `prompt_ledger.rs` line counts stay under the 600-line target after P3 (not measured).
- `RpcSession` struct line (`rpc_session.rs:34`) and the `LiveSession` / `PendingPermissions` line numbers are approximate (read from `sed` excerpts, not re-checked by line).

No cargo command was run and no source file was edited. Scope was this file only.
