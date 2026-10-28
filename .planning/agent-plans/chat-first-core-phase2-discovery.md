# Phase 2 discovery: per-session shim (factual map)

Scope: read-only map of how harness children are spawned, supervised, recovered and restarted today, for `.planning/agent-plans/chat-first-core.md` Phase 2. All paths are relative to the repo root. Line numbers were read from this worktree (`ajax-architectural-flaws`, branch `ajax/architectural-flaws`, HEAD `92c9a088`). Items marked **UNVERIFIED** are inferences or external behavior that the code cannot prove here.

Crate and file shorthand used below:
- `web_session/` = `crates/ajax-web/src/slices/web_session/`
- `acp/` = `crates/ajax-web/src/adapters/web_session_acp/`

---

## 1. Where the harness child is spawned today

### 1.1 Two layers: the slice owns a `SessionClient`, the adapter owns the OS child

The slice never calls `Command` for a harness. It holds `TaskSessionState.acp.client: Option<SessionClient>` (`web_session/task_session.rs:372-373`, initialized to `None`). Spawning is delegated to `SessionClient::spawn_with_operator_pin` (`acp/session_client.rs:165-191`), which dispatches on `transport_for(agent)` to one of four transports:

| Transport (`HarnessTransport`) | Spawn function | Child program | Stdio plumbing |
|---|---|---|---|
| `Acp` (Codex, Claude via ACP, others) | `AcpStdioClient::spawn_with_operator_pin` (`acp/client.rs:371`) → `spawn_internal` (`client.rs:417`) → `spawn_acp_process` (`client.rs:824`) | ACP agent binary from `acp_candidates` (`client.rs:762-782`); npx fallback | raw `std::process::Child`, stdin/stdout/stderr piped (`client.rs:850-908`) |
| `ClaudeSdk` | `spawn_claude_sdk` (`acp/claude_sdk_spawn.rs:64`) → `ClaudeSdkClient::spawn` (`acp/claude_sdk_client.rs:55`) | `node --input-type=module -e <embedded sidecar> -- --sdk-module <path>` (`claude_sdk_spawn.rs:8-41`, `acp/rpc_handshake.rs:82-95`) | `JsonlProcess::spawn` (`claude_sdk_client.rs:63`) |
| `CursorSdk` | `spawn_cursor_sdk` (`acp/cursor_sdk_spawn.rs:151`) → `CursorSdkClient::spawn` (`acp/cursor_sdk_client.rs:125`) | same `node -e` shape with the cursor sidecar (`cursor_sdk_spawn.rs:81-92`) | `JsonlProcess::spawn` (`cursor_sdk_client.rs:139`) |
| `PiRpc` | `SessionClient::spawn_pi_rpc` (`session_client.rs:193`) → `PiRpcClient::spawn` (`acp/pi_rpc_client.rs:99`) | `pi --mode rpc -na [--session-id <id>]` (`pi_rpc_client.rs:103-107`); program from `pi_program_and_args` (`session_client.rs:575-586`) | `JsonlProcess::spawn` (`pi_rpc_client.rs:113`) |

### 1.2 The Command builder, sandbox wrapping, env, cwd

**ACP path** (`acp/client.rs:824-916`, `spawn_acp_process`):
- Program resolution: `acp_candidates(launch)` (`client.rs:762-782`) yields the native ACP binary (only if `native_acp_advertised`, which runs `<program> --help` and caches for 300 s, `client.rs:784-822`), then fixed candidates, then `npx -y <package>`.
- Args: `acp_args_for_program` (`client.rs:750-760`).
- Wrapping: `crate::adapters::program::sandboxed_harness_command(&program, &args, worktree_path)` (`client.rs:894-898`) → `resolve_program` (`program.rs:113-129`) → `sandboxed_program` (`program.rs:131-149`) → `sandbox_launch` (`program.rs:151-169`) → ajax-core `sandbox_command` (`crates/ajax-core/src/adapters/sandbox.rs:223-276`) → `wrap_argv` (`sandbox.rs:179-221`).
- Resulting argv: `nono -s run --allow-cwd --no-rollback-prompt --profile <ajax-agent.json> --workdir <worktree> [--read <common .git>] [--allow <gitdir/objects/refs/logs>] [--allow-file <packed-refs>] -- <program> <args>`. A bare or absolute `codex` also gets `-c sandbox_mode="danger-full-access"` (`sandbox.rs:215-218`).
- Sandbox toggle: `AJAX_SANDBOX` off/0/false disables it (`sandbox.rs:21-30`). If nono is missing and the sandbox is on, spawn fails (`sandbox.rs:88-110`, message at 107).
- `current_dir(worktree_path)` (`client.rs:904`). Env: the inherited web env plus `PATH` prefixed with the login-shell PATH (`program.rs:171-181`, cached `shell_path` at `program.rs:82`). No other env is set in the production path (`FAKE_ACP_STATE_DIR` is test-only, `client.rs:829-886`).
- Spawn leader: `process_group::spawn_leader` → `process_group(0)` (`acp/process_group.rs:4-9`), so pgid == pid.

**JSONL path** (used by Claude SDK, Cursor SDK, Pi): `JsonlProcess::spawn` (`acp/jsonl_process.rs:33-36`) calls `crate::adapters::program::sandboxed_program` (so every JSONL harness is sandboxed) then `spawn_unsandboxed` (`jsonl_process.rs:38-75`): `current_dir(cwd)`, stdin/stdout/stderr piped, `own_group` (`jsonl_process.rs:46`, which is `process_group(0)`). The only unsandboxed spawns are cursor login (`acp/cursor_login.rs:103`, `:138`).

Env for JSONL: inherited; no `PATH` merge (the `apply_shell_path` call is only in `harness_command` / `sandboxed_harness_command`, `program.rs:107-129`). Note the sidecar `node` itself is resolved by `resolve_program("node")` (`claude_sdk_spawn.rs:23`, `cursor_sdk_spawn.rs:121`-ish, `resolve_node_and_sdk`).

### 1.3 How stdio is consumed (types and framing)

Concrete types, not async wrappers:
- ACP: `child.stdin.take()` / `child.stdout.take()` / `child.stderr.take()` on `std::process::Child` (`client.rs:426-438`). The ACP connection takes `std::process::{ChildStdin, ChildStdout}` directly (`acp/sdk_connection.rs:133-134` in `RunOptions`, `:362-363` in `traced_transport`), wrapped in `futures::io::BufReader::new(Unblock::new(stdout))` (`sdk_connection.rs:369`) and `Unblock::new(stdin)` with `write_all(line) + "\n" + flush` (`sdk_connection.rs:380-383`). The connection runs on its own thread (`client.rs:453-469`, `sdk_connection::run` at `sdk_connection.rs:162`) with a current-thread tokio runtime (`sdk_connection.rs:163`).
- JSONL: `ChildStdin` held in `JsonlProcess.stdin: Option<ChildStdin>` (`jsonl_process.rs:25`). Stdout is read on a thread named `jsonl-stdout` (`jsonl_process.rs:53-58`) by `read_stdout_lines_bounded` (`jsonl_process.rs:188-251`): `\n`-delimited, strips `\r`, 16 MiB cap (`jsonl_process.rs:15`), invalid JSON becomes `JsonlRecord::Error(text)` (`jsonl_process.rs:213-216`), oversize lines are discarded with an error record (`jsonl_process.rs:221-245`). Records go over `mpsc` to `recv`/`try_recv` (`jsonl_process.rs:85-91`). Writes: `serde_json` + `\n` (`jsonl_process.rs:135-150`), `send` assigns `ajax-rpc-<n>` ids (`jsonl_process.rs:98-108`).
- stderr: drained on its own thread into a 4 KiB rolling tail (`jsonl_process.rs:11`, `:60-66`, `:253-271`) for JSONL, and into an unbounded `String` sink for ACP (`client.rs:434-438`, `drain_stderr` at `client.rs:194`).

### 1.4 How exit is detected

- ACP: the connection thread emits `AcpClientEvent::Exited` after stdout EOF or connection error (`sdk_connection.rs:358`), after sending any stderr tail as an `Error` event (`sdk_connection.rs:354-357`). Separately, `AcpStdioClient::host_exited` polls `child.try_wait()` (`client.rs:578-584`).
- JSONL: `read_stdout_lines_bounded` sends `JsonlRecord::Exited` on EOF (`jsonl_process.rs:250`). The native RPC layer maps that to `host_exited = true` (`acp/rpc_client.rs:209-211`, `RpcStep::Exited`). `host_exited()` = `g.host_exited || g.session.is_none()` (`rpc_client.rs:229-232`).
- Slice pump: every 50 ms, `TaskSessionState::pump` drains events (`web_session/task_session.rs:296-339`); `host_exit_from_drain` (`task_session.rs:319`) feeds `reconcile_unexpected_child_exit` (`web_session/task_session_exit.rs:101`). The poll interval is at `task_session.rs:432`, and the pump call is at `task_session.rs:448-455`.
- `is_acp_exit_error` (`web_session/task_session_exit.rs:13-19`) matches the synthetic strings `"ACP process exited"` / `"Agent process exited"` (`task_session_exit.rs:10-11`; produced at `acp_drain.rs:192,198` and `task_session_exit.rs:113`). It does **not** parse stderr.

### 1.5 How kill / cancel / idle eviction work

- **Cancel (turn):** `web_session/task_session_exit.rs:655-682` (`cancel`) calls `client.cancel()` (ACP sends `ClientCommand::Cancel`, `client.rs:636-`; native sends `cancel` via the RPC session). The process is not killed on cancel.
- **Teardown primitive, ACP:** `AcpStdioClient::tear_down(close)` (`client.rs:547-576`): optional `session/close` with a 10 s timeout (`client.rs:346`, `:553-571`), then `ClientCommand::Shutdown`, then `process_group::kill_tree(&mut self.child)` (`client.rs:573`), then `child.wait()` (`client.rs:574`). So `detach()` (`client.rs:543`) also SIGKILLs the group. Only `close` differs.
- **Teardown primitive, JSONL:** `JsonlProcess::drop` (`jsonl_process.rs:157-181`): close stdin, poll `try_wait` for up to `DROP_GRACE` = 2 s (`jsonl_process.rs:13`), then `kill_group(pid)` (SIGKILL to `-pid`), `child.kill()`, `child.wait()`. Native `shutdown()` takes the session out of the mutex, which drops it (`rpc_client.rs:218-227`), so shutdown and detach both end in this drop. `SessionClient::detach` for native calls `shutdown` (`session_client.rs:257-262`).
- **Kill primitive:** `process_group::kill_tree` = `kill -KILL -- -<pid>` (via `/bin/kill`, `process_group.rs:17-25`) then `Child::kill` (`process_group.rs:12-15`).
- **Slice-level close:** `spawn_task_session` loop exit (`web_session/task_session.rs:460-487`): if `Shutdown{close}` arrives or the channel drops, it calls `cancel()` if the host is alive, then `shutdown()` (close=true) or `detach()` (close=false).
- **Release/acquire holders:** `release` only marks `last_released` (`web_session/task_session_directory.rs:354-366`). No process change.
- **Idle eviction:** `TaskSessionDirectory::evict_idle_over_limit` (`task_session_directory.rs:246-317`). Candidates must have been released longer than `IDLE_RELEASE_GRACE` = 15 min (`web_session/transcript.rs:8`) and be `evictable` (`task_session.rs:632`, `state.is_idle()`). Only the oldest-released are removed to keep `MAX_IDLE_SESSIONS` = 8 (`transcript.rs:6`, `task_session_directory.rs:270-278`). Eviction sends `Shutdown { close: false }` (`task_session_directory.rs:312`), which means detach, so the harness group is SIGKILLed on eviction.
- **Drop session (chat Drop):** `task_session_directory.rs:368-388` sends `Shutdown { close: true }` and clears the stored ACP session id. `cleanup_session` also deletes the transcript (`task_session_directory.rs:405-412`).
- **Replacement / respawn / harness switch:** `release_live_client` (`web_session/task_session_spawn.rs:405-486`) cancels, then `shutdown` or `detach` (`:470-474`), then `state.acp.client.take()`. Respawn: `respawn` (`task_session_spawn.rs:236-268`). Harness switch: `reset_harness_context` (`:270-333`). Model/config change: `apply_config_option` (`:150-234`), and a dead host triggers respawn (`:159-166`).

### 1.6 Every place that touches the child handle or pid

| # | Location | What it does |
|---|---|---|
| 1 | `acp/client.rs:424-438` | Takes stdin/stdout/stderr from `Child` |
| 2 | `acp/client.rs:475-476`, `:486-487` | Startup failure: `kill_tree` + `wait` |
| 3 | `acp/client.rs:510-521` | Stores `child` in `AcpStdioClient` |
| 4 | `acp/client.rs:573-574` | `tear_down`: `kill_tree` + `wait` |
| 5 | `acp/client.rs:578-584` | `host_exited` via `try_wait` |
| 6 | `acp/client.rs:587-594` | `kill_host_for_test` (cfg test), `child_id` (pub(crate)) |
| 7 | `acp/client.rs:730-732` | `Drop` → `detach()` |
| 8 | `acp/jsonl_process.rs:47-51`, `:77-83`, `:157-181` | spawn, `child_id`/`pid`, Drop (grace, kill group, kill, wait) |
| 9 | `acp/session_client.rs:247-291` | `shutdown`, `detach`, `host_exited`, `child_pid` (ACP arm `Some(client.child_id())`), cfg-test `kill_host_for_test` |
| 10 | `acp/rpc_client.rs:218-237` | native `shutdown` (drops session), `host_exited`, `child_pid` |
| 11 | `acp/claude_sdk_client.rs:52-53`, `:276-` | `child_pid`, `shutdown` |
| 12 | `acp/cursor_sdk_client.rs:210-211`, `:410-` | `child_pid`, `shutdown` |
| 13 | `acp/pi_rpc_client.rs:174-175`, `:329-` | `child_pid`, `shutdown` |
| 14 | `acp/rpc_session.rs:36` | owns `process: JsonlProcess` |
| 15 | `web_session/task_session.rs:219-241` | `report_committed_events`: `child_pid()` used as the activity-probe root (`:226`) |
| 16 | `web_session/task_session.rs:300-319`, `:453-455` | `pump` drains events, reads `host_exited` |
| 17 | `web_session/task_session.rs:468-487` | exit-time cancel/shutdown/detach |
| 18 | `web_session/task_session.rs:610-623` | `ChildId` reply (`client.child_id()`), `KillHostForTest` (cfg test, `:615-617`) |
| 19 | `web_session/task_session_child.rs:6-34` | `record_child_identity`: `child_pid()` → `os_process::start_token` → `web_session_store::record_child_identity` (`task_session_child.rs:23-30`) |
| 20 | `web_session/task_session_spawn.rs:86`, `:312`, `:381` (and `task_session_replacement.rs:94`) | calls `record_child_identity` after each spawn |
| 21 | `web_session/task_session_spawn.rs:88`, `:103`, `:302`, `:371` | `discard_staged_client` on identity/persist failure |
| 22 | `web_session/task_session_spawn.rs:405-486` | `release_live_client`: cancel, shutdown/detach, `take()` |
| 23 | `web_session/task_session_spawn.rs:492-514` | `reject_foreign_live_child`: refuses spawn if the latest run's child is live under another boot id |
| 24 | `web_session/background_jobs.rs:108-110` | `child_pid()` for background job probing |
| 25 | `web_session/task_session_directory.rs:690-697`, `:699-703` | `child_id(handle)`, `kill_host_for_test` (cfg test) |
| 26 | `web_session/orphan_sweep.rs:65-68`, `:77-115` | pid/pgid/start-token from the DB row; TERM/KILL; clear |

---

## 2. Transports on top of the byte stream

- **Newline-delimited over stdio of ONE child:** yes for all four.
  - Codex / Claude-via-ACP and any other ACP harness: one agent child, ACP JSON-RPC lines over stdin/stdout (`sdk_connection.rs:369-383`). The `agent_client_protocol` crate is used on top (`sdk_connection.rs` imports, and `acp/native_*` modules name it). Malformed JSON on stdout emits an error (`sdk_connection.rs:374-376`).
  - Claude SDK, Cursor SDK: one `node` sidecar child each, JSONL over stdio (`claude_sdk_client.rs:63`, `cursor_sdk_client.rs:139`). The sidecar source is embedded with `include_str!` (`claude_sdk_client.rs:20`, `cursor_sdk_client.rs` analog) and passed as `node -e`, so there is no separate sidecar file on disk.
  - Pi: one `pi --mode rpc` child, JSONL over stdio (`pi_rpc_client.rs:99-113`).
- **Extra fds:** none found. No `pass_fd`, no `dup2`, no extra pipes beyond stdin/stdout/stderr. Only `Stdio::piped()` for the three standard fds in every spawn.
- **Extra processes:**
  - Probes, not held: `<acp program> --help` (`client.rs:798-814`, cached 300 s), `node --version` for Cursor (`cursor_sdk_spawn.rs:22-35`), `resolve_program` probes (`program.rs`), `git rev-parse` for linked worktrees (`ajax-core sandbox.rs:140-176`).
  - Kill helpers: `kill` is spawned as a subprocess (`process_group.rs:17-25`, `orphan_sweep.rs:161-172`, `:174-186`, `process_identity.rs:58-68`).
  - Sidecar internals: a `node` sidecar may itself start the vendor agent or SDK. **UNVERIFIED**: the sidecar sources are not in the Rust tree (`claude_sdk_sidecar.mjs`, `cursor_sdk_sidecar.mjs` under `crates/ajax-web/sidecar/`, not read here). If the SDK spawns grandchildren, they are in the sidecar's process group because the sidecar is the group leader (`jsonl_process.rs:46`).
- **Stderr-dependent logic:** none that gates control flow. Stderr text is appended to user-facing error strings (`client.rs:477-490`, `sdk_connection.rs:354-357`, `rpc_session.rs:177-192`, `pi_rpc_handshake.rs:73-104`, `rpc_handshake.rs:46-66`). Exit classification uses synthetic messages, not stderr (`task_session_exit.rs:13-19`).

**Implication for a socket-attached session:** ACP `sdk_connection::run` and `traced_transport` take concrete `ChildStdin`/`ChildStdout` (`sdk_connection.rs:133-134`, `:362-363`). `JsonlProcess` owns a concrete `Child` and its Drop kills the group (`jsonl_process.rs:157-181`). `host_exited`, `child_pid`, `tear_down` and Drop all assume a local `Child` (see table §1.6). A shim-attached client therefore needs an attachment mode that does not own a `Child`.

---

## 3. Startup recovery, orphan sweep, ownership, heartbeat

### 3.1 Identity and the lease

- **Boot id:** random 16 bytes per `ExecutionOwnership::new` (`web_session/execution_ownership.rs:18-27`, `random_boot_id` at `:183-189`).
- **Lease:** DB row via `claim_execution_owner(boot_id, pid, now, STALE_AFTER_MS)` (`execution_ownership.rs:55-58`, `STALE_AFTER_MS` = 30 000 at `:15`). Heartbeat every `HEARTBEAT_EVERY` = 10 s (`execution_ownership.rs:14`), driven by `runtime/mod.rs:198` → `task_session_directory.rs:123-126` → `execution_ownership.rs:113-160`.
- Takeover: if the holder is this pid or `!process_alive(holder)`, it force-claims with staleness `-1` (`execution_ownership.rs:58-77`). `process_alive` = `os_process::exists` (`execution_ownership.rs:191-200`).
- Release on drop: `release_execution_owner` (`execution_ownership.rs:165-170`).
- Each acquired claim runs `orphan_sweep::sweep_once` and records recovered handles (`execution_ownership.rs:61-70`, `:81-90`).

### 3.2 What marks runs interrupted (and when)

- **Trigger:** `ensure_owner` → `try_claim` → `sweep_once` (`execution_ownership.rs:51-80`). `ensure_owner` is called by `startup_dispatch_handles` (`task_session_directory.rs:75-77`, `:213` in `ensure_entry_for_acquire`) and therefore by the headless startup dispatch (`runtime/headless.rs:82`, `claim_startup_handles` ~`:95-110`). So recovery runs on the first claim after boot, not at process boot.
- **`sweep_once`** (`web_session/orphan_sweep.rs:13-33`) runs once per process (`SWEPT_ONCE` at `:10`, set only when complete at `:29-31`). It calls `sweep` (`:40-42`, grace 5 s) then `recover_interrupted_prompts`.
- **`recover_interrupted_prompts`** (`web_session/startup_recovery.rs:30-101`): for every task whose latest run row has `owner_boot_id != current boot` (`:47-61`), it calls `ledger.recover_after_restart()`. Prompts that were dispatching become `Interrupted` and get the note `"Prompt <id> was interrupted by host restart and was not retried."` (`startup_recovery.rs:15-28`, persisted at `:86`).
- **`settle_running_chats`** (`startup_recovery.rs:103-140`): a persisted running chat (`persisted_chat_running`, `:142-152`) whose latest run is not owned by this boot is added to `recovered_handles` (`:138`). The live status is settled later (`task_session_exit.rs:21` `settle_running_activity_on_end`).
- **Run row owner:** `record_child_identity` writes `owner_boot_id` together with the child pid and token (`adapters/web_session_store/mod.rs:167-192`).

### 3.3 What identifies an orphan, and what gets killed

- **Source:** `store.runs_with_children()` (`orphan_sweep.rs:53`), which reads `agent_runs.child_pid`, `child_pgid`, `child_started`, `owner_boot_id` (`ajax-core registry/sqlite/execution_recovery.rs:145-163` with `WHERE child_pid IS NOT NULL`; migration column at `registry/sqlite/migrations.rs:210`; update at `registry/sqlite/execution_store.rs:451`).
- **Skip rule:** rows owned by the current boot are skipped (`orphan_sweep.rs:61-64`).
- **Identity check:** `judge(recorded_child_started, start_token(pid))` (`orphan_sweep.rs:77-80`, `process_identity.rs:21-27`). `start_token` on macOS = `proc_pidinfo` `pbi_start_tvsec.usec`; on Linux = `/proc/sys/kernel/random/boot_id:starttime` (`web_session/os_process.rs:17-25` and `:112-122`).
- **Dead pid:** if `pid_alive` is false (`orphan_sweep.rs:69-76`, `os_process::exists` and not zombie, `:150-155`), only the identity is cleared.
- **Live and same token:** `kill -TERM -- -<pid>` (group), else `kill -TERM <pid>` (`orphan_sweep.rs:170-186`), wait up to 5 s (`:41`, `:86`), then `-KILL` if the token still matches (`:87-93`), wait up to 2 s (`:94`). On failure it retains the identity and marks the sweep incomplete (`:95-98`). On success it clears the identity (`:101-103`, `clear_child_identity` at `:132-148`).
- **Live, different token (pid reused) or gone:** row left unchanged, debug log only (`orphan_sweep.rs:107-112`).
- `process_identity::terminate_if_same` (`process_identity.rs:83-113`) is a separate TERM/KILL helper marked `#[allow(dead_code)]`, and `Verdict`, `Termination`, `judge` are also dead-code allowed. **UNVERIFIED** whether anything calls `terminate_if_same` in production (grep found no caller outside its own file).
- `foreign_live_child` (`process_identity.rs:29-46`) treats a row as live only if the owner boot differs from the host boot and the start token matches.

### 3.4 Execution ownership guard on spawn

`reject_foreign_live_child` (`task_session_spawn.rs:492-514`) refuses any spawn, acquire, respawn or reset while the latest run's child is live under another boot id, with the error "previous host's agent for this chat is still shutting down; retry shortly". This is the first check a shim-adoption path must bypass or satisfy.

---

## 4. Web state dir and existing per-session files

- **State dir:** the parent directory of the registry DB file. `companion_session_storage` uses `state_file.parent()` (`crates/ajax-cli/src/web_backend.rs:220-224`), and `SessionStorage::under(state_dir)` sets `registry_db = state_dir/ajax.db` (`web_session/session_storage.rs:13-19`). `state_db` defaults to `<home>/ajax.db` (`crates/ajax-core/src/config.rs:173`). Dev profile home is `~/.ajax-dev` (`config.rs:123`, and test at `config.rs:401-403`). The stable profile uses `ConfigPaths::for_home` (`config.rs:157-161`) **UNVERIFIED**: the concrete stable path was not read.
- **Plan vs code:** the plan says `~/.ajax*/sessions/<h>.log`. The existing per-session transcript directory is **`<state_dir>/web-session/`** (`adapters/web_session_store/mod.rs:11`, `:323`, `:374`, `runtime/headless.rs:212`). Files are `<encode_handle(handle)>.jsonl` (`mod.rs:399-404` `session_path`, `encode_handle` at `mod.rs:307`), and a migrated copy `<encoded>.jsonl.migrated` (`mod.rs:347`, `:372`). `sessions/` does not exist in the code.
- **Other per-session state (not files):** the prompt ledger is stored in the registry DB under kind `prompt_ledger` (`adapters/web_session_store/prompt_ledger.rs:10`, `:369`, `:379`), not in a JSONL. Run rows and child identity are in `ajax.db` (`agent_runs`, see §3.3).
- **Other state under the same dir:** `operation-outcomes.json` (`runtime/operation_outcomes.rs:14`), the TLS identity via `tls::load_or_create_identity(&state.state_dir)` (`runtime/mod.rs:153`), push and browser session files (`runtime/state.rs:773-774`), `agent-events/` dir created next to the state root by `__agent-runtime` (`crates/ajax-cli/src/agent_runtime.rs:130-135`, `publish_cwd_index`).

---

## 5. CLI definition and dependency edges

- **Clap root:** `build_cli()` at `crates/ajax-cli/src/cli.rs:13`. Subcommands are added at `cli.rs:25-68`. Hidden internal subcommands use `Command::new(...).hide(true)`: `__agent-event` (`cli.rs:158`), `__agent-runtime` (`cli.rs:175-176`). `web` is at `cli.rs:220`, with `service` at `cli.rs:244`.
- **Dispatch:** `crates/ajax-cli/src/lib.rs` has two matchers that must both be extended: the value-returning one at `lib.rs:112-114` (`__agent-runtime`) and the writer one at `lib.rs:174-176`. `agent_runtime::run_agent_runtime_command` is the precedent for a hidden child-supervisor subcommand (`crates/ajax-cli/src/agent_runtime.rs`, spawn at `:148`, `Stdio::inherit` for all three fds at `:150-152`, env `AJAX_TASK_ID` etc. at `:149-151`).
- **Where `session-shim` would go:** a `session_shim_command()` next to `agent_runtime_command` in `cli.rs`, a new module under `crates/ajax-cli/src/`, and one dispatch arm in each of the two `lib.rs` matchers.
- **Dependency edges:** `ajax-cli` depends on `ajax-core` and `ajax-web` (`crates/ajax-cli/Cargo.toml:17`, `:20`). `ajax-cli` already imports `ajax_web::slices::...` (`web_backend.rs:9`).
- **Crate features:**
  - `ajax-core`: no tokio, no nix, no libc (`crates/ajax-core/Cargo.toml` `[dependencies]`: rusqlite, serde, serde_json, toml, tracing, tracing-subscriber).
  - `ajax-cli`: tokio with `rt-multi-thread`, `net`, `time` (`crates/ajax-cli/Cargo.toml:25`); nix (workspace, `Cargo.toml:30`); clap.
  - `ajax-web`: tokio with `io-util`, `macros`, `process`, `rt-multi-thread`, `net`, `time` (`crates/ajax-web/Cargo.toml:28`); nix; getrandom.
  - Workspace nix: `features = ["fs", "poll", "process", "signal", "term"]` (`Cargo.toml:30`). `setsid` is `nix::unistd::setsid` under `process`, so it is available without a Cargo change. `nix::libc` is used via re-export (`web_session/os_process.rs:2-29`). **libc is not a direct dependency** of any crate (grep found none).
  - Unix sockets: `tokio::net::UnixListener` and `UnixStream` are behind tokio's `net` feature, which `ajax-web` and `ajax-cli` already enable. No new dependency needed for sockets. **UNVERIFIED** until compiled.
  - `setsid` placement: spawning with `pre_exec` needs `unsafe` (AGENTS: avoid). The shim can instead call `setsid()` itself as its first action, which has a short window where it is still in web's group. **Design choice, not verified.**

---

## 6. Existing tests that pin current behavior we would change

Names are as they appear in the source. "Pins" means the test asserts the behavior named.

**A. Runs marked interrupted on startup or restart**
- `web_session/startup_recovery_tests.rs:79` `foreign_boot_dispatching_prompt_is_interrupted_with_note`: foreign owner, no shim, asserts Interrupted + one note. Must still pass when no shim is alive; a new shim-alive case must not break it.
- `web_session/startup_recovery_tests.rs:109` `recover_is_idempotent`
- `web_session/startup_recovery_tests.rs:218`, `:223`, `:227` `sweep_once_retries_ledger_load_failure_issue_1326`, `sweep_once_retries_ledger_persist_failure_issue_1326`, `recovery_retries_failure`
- `web_session/startup_recovery_tests.rs:292` `running_chat_without_dispatching_prompt_is_recovered_issue_1439`
- `web_session/session_activity_directory_tests.rs:214` `issue_1176_turn_interrupted_by_restart_clears_agent_working`
- `web_session/session_activity_directory_tests.rs:476` `issue_1439_running_chat_without_dispatching_prompt_settles_on_startup`
- `web_session/session_activity_directory_tests.rs:520` `issue_1439_recovered_handle_with_a_live_actor_is_not_settled`
- `web_session/task_session_tests.rs:556` `recover_dispatching_prompt_is_interrupted_without_retry`
- `web_session/task_session_restore_tests.rs:271` `issue_1179_ajax_web_restart_restores_without_context_reset_note` (restart restores via ACP resume today; the new path must keep "no context reset note")
- `web_session/prompt_ack_tests.rs:21` `interrupted_prompt_is_acknowledged_once_and_not_again_on_reconnect`
- `web_session/task_session_commit_tests.rs:41` `admitted_prompt_and_user_bubble_survive_a_restart`

**B. Children killed on web drop, shutdown, or replacement**
- `adapters/web_session_acp/jsonl_process_tests.rs:286` `dropping_the_process_reaps_the_child`
- `adapters/web_session_acp/jsonl_process_group_tests.rs:25` `dropping_the_process_also_kills_its_grandchildren`
- `adapters/web_session_acp/client_tests.rs:645` `drop_without_close_still_resumes_when_close_advertised`
- `adapters/web_session_acp/client_tests.rs:698` `host_exited_and_kill_host_for_test`
- `adapters/web_session_acp/client_process_group_tests.rs:34` `acp_spawn_failure_kills_grandchildren_of_the_acp_child`
- `adapters/web_session_acp/claude_sdk_client_tests.rs:302` `shutdown_returns_the_session_id_once_then_none`; `adapters/web_session_acp/claude_sidecar_tests.rs:497` `shutdown_responds_then_child_exits_once`
- `adapters/web_session_acp/pi_rpc_client_tests.rs:169` `shutdown_returns_session_id_once_then_host_exited`
- `web_session/task_session_phase2_tests.rs:230` `replacement_reaps_prior_child_before_installing_new_one`
- `web_session/task_session_idle_eviction_tests.rs:40`, `:93`, `:148`, `:220`, `:306`, `:380`, `:439`, `:520` (the eviction family; eviction currently kills the group, see §1.5)
- `web_session/session_close_tests.rs:160` `advertised_close_skipped_on_detach_and_session_resumes`, `:253` `advertised_close_on_drop_session_prevents_resume`
- `runtime/tests/unowned_cleanup.rs:4` `cockpit_refresh_that_drops_task_shuts_down_its_live_session`
- `web_session/session_cleanup_tests.rs:26`, `:53`, `:84` (startup prune of unowned persisted sessions; drop cleanup)

**C. Orphan sweep kill behavior**
- `web_session/orphan_sweep_tests.rs:72` `term_ignoring_child_is_killed_before_identity_is_cleared_issue_1327`
- `web_session/orphan_sweep_tests.rs:334` `matching_token_terminates_the_whole_process_group_issue_1298`
- `web_session/orphan_sweep_tests.rs:417` `sweep_once_interrupts_dispatching_prompts_of_dead_hosts`
- `web_session/orphan_sweep_tests.rs:446` `sweep_once_retries_listing_failure_issue_1326`
- `web_session/execution_ownership_tests.rs:70` `takeover_fences_directory_and_recovery_writes_issue_1327`
- `web_session/execution_ownership_tests.rs:290` `lost_lease_recovers_when_the_foreign_host_dies`
- `web_session/execution_ownership_tests.rs:456` `healthy_heartbeat_retries_an_incomplete_startup_sweep_issue_1326`

**D. Host crash and respawn (must still pass, probably with new fixtures)**
- `web_session/host_crash_tests.rs:161` `crash_and_restart`, with `:330` `acp_host_crash_recovers`, `:335` `pi_rpc_host_crash_recovers`, `:340` `cursor_sdk_host_crash_recovers`, `:345` `claude_sdk_host_crash_recovers`. These kill the child via `kill_host_for_test` (`task_session_directory.rs:699`). **UNVERIFIED** whether the bodies assert interruption. Not read.
- `web_session/task_session_child_tests.rs:8` `acquire_records_the_child_identity`, `:40` `respawn_updates_the_recorded_pid`
- `web_session/task_session_phase2_tests.rs:741` `issue_1327_cold_acquire_fails_when_child_identity_cannot_be_recorded`, `:788` `issue_1327_replacement_acquire_fails_when_child_identity_cannot_be_recorded`

**E. Contradictions with the required behavior:** none found by name and by the read bodies. Tests A and C use fake run rows with no live shim, so they still describe the no-shim path. Tests B pin *web-side Drop kills the group*. That is a decision point, not a contradiction: the plan keeps `Drop`/eviction semantics unless we decide otherwise. Needs Matt's call on eviction of shim-owned sessions (do we keep the harness alive for an evicted, idle chat?).

---

## 7. Dev deploy, `ajax web service`, and restart paths

### 7.1 Dev deploy (`scripts/dev-web-restart.sh`, called from `crates/ajax-web/src/slices/dev_deploy/`)
- `dev_deploy/mod.rs:371-376` resolves `scripts/dev-web-restart.sh`; `restart_command` spawns it (`mod.rs:386-391`); `run_test_in_dev_job` streams its stdout (`mod.rs:392-`).
- The script reads the PID file (`PID_FILE="$RUN_DIR/${PROFILE}-web.pid"`, `:104`), then:
  - `kill "$old_pid"` then `kill -9 "$old_pid"` if still alive (`:323-326`). **It signals only the single pid, not the process group.**
  - `launchctl bootout gui/<uid>/<label>` (`:332`).
  - `tmux kill-session -t =<TMUX_SESSION>` (`:341`), used when the web runs in tmux (profile-specific).
  - Foreground start: `exec "$bin_path" --profile "$PROFILE" web ...` (`:353`).
  - Fallback pid discovery: `lsof -iTCP:<port> -sTCP:LISTEN` (`:365`, `:423`).
- Consequence: the setsid'd shim is not in the web's process group and is not the recorded pid, so the script leaves it alone. **Verified by reading; the actual kill behavior is by the kernel.**

### 7.2 `ajax web service` (launchd)
- Plist: `KeepAlive` true (`crates/ajax-cli/src/web_service/mod.rs:118-119`), **`AbandonProcessGroup` true** (`:120-121`), `ThrottleInterval` 10 (`:122-123`).
- `install`: `bootout` then `bootstrap` (`web_service/mod.rs:247-253`). `start`: `kickstart` without `-k` if loaded, else `bootstrap` (`:330`, `:332`). `stop`: `bootout` (`:344`). `uninstall`: `bootout` and remove plist (`:300-307`).
- **`AbandonProcessGroup` semantics:** per the launchd.plist(5) documentation as I understand it, launchd does not kill the job's remaining process group when the job's main process exits. **UNVERIFIED here**: `man launchd.plist` returned nothing in this sandbox, so this was not confirmed on this machine. Also **UNVERIFIED** whether `bootout` SIGKILLs setsid'd descendants. A setsid'd shim is outside the job's process group by construction, which should keep it out of launchd's group kill, but this is not proven.
- Plist changes must be applied by Matt (per the plan, nono blocks `launchctl bootstrap`, see `nono_blocks_launchctl_bootstrap` memory). No plist change is expected for Phase 2 as scoped.

### 7.3 In-process self-restart
- `handle_server_restart` (`crates/ajax-web/src/runtime/mod.rs:662-663`) calls `server::schedule_process_restart()` (`adapters/server.rs:79-95`), which sleeps `RESTART_DELAY` 400 ms (`server.rs:5`), then `launch_restart(restart_launch())` and `std::process::exit(0)` (`server.rs:85-90`).
- The plan `RestartLaunch`: under `AJAX_WEB_SUPERVISOR=launchd` with no restart script it is `Exit` (launchd respawns), otherwise `Respawn` (new process spawned with `Command::new(current_exe)`, `server.rs:119-128`) or `Script` (`server.rs:110-117`). `restart_launch_for_supervisor` is at `server.rs:~36-45`.
- **No signal handler exists** in `ajax-web` or `ajax-cli` (grep for `ctrl_c`, `SIGTERM`, `signal::unix`, `with_graceful_shutdown` found nothing). So SIGTERM kills the web without running Drop. `process::exit(0)` also skips destructors. Consequence (by code reading): a web restart today does **not** kill harness children. They keep running in their own process groups (`process_group(0)`, §1.2), and the next boot's orphan sweep TERM/KILLs them if their start token still matches (§3.3). That is how "the turn gets interrupted on restart" happens today.

### 7.4 What is UNVERIFIED in §7
- Whether a harness child reacts to its stdin pipe closing (web death) by exiting. This determines whether children exit on their own on web death today. **UNVERIFIED** (depends on each agent).
- Whether nono `exec`s the harness or forks it. This determines whether the recorded pid is the nono wrapper or the harness. **UNVERIFIED** (no nono source in this repo).

---

## 8. Consequences for the design (factual inputs, not decisions)

1. **Identity stored today is the spawned pid** (`client.child_id()` → `JsonlProcess`/ACP `Child::id`, `task_session_child.rs:10-14`). With a shim, the pid that matters for the orphan sweep changes: the shim or the harness. The sweep kills `-pid` groups (`orphan_sweep.rs:174-176`). A shim-owned harness is in the shim's group, so `kill -TERM -- -<harness pid>` only works if the harness pid is also a group leader, which it is not (it is in the shim's group). Choosing the shim pid as the recorded child is the consistent option. This needs a decision.
2. **The sweep does not know about sockets.** Today "alive with matching token" means "orphan, kill". With shims, a live shim socket must mean "not orphan". `startup_recovery` and `orphan_sweep` would need that check before they interrupt or kill.
3. **`reject_foreign_live_child`** (`task_session_spawn.rs:492-514`) would block adoption. Adoption needs its own path that runs before this check.
4. **`JsonlProcess` and `AcpStdioClient` both own a `Child` and kill on drop or teardown.** A socket-attached mode must not kill on `detach`, must not `try_wait` a child it does not own, and needs a liveness signal from the socket (EOF or connect failure) for `host_exited`.
5. **Plan text vs code:** `sessions/` vs `web-session/` (§4). The per-session transcript (`web-session/<enc>.jsonl`) already exists and already holds the outbound event log. The shim's byte log would be a second log of the same bytes. Decision needed on whether the shim log replaces or duplicates `web-session/*.jsonl`.
6. **Size limits:** `task_session_directory.rs` is 896 lines (over the ~600 target, under the 1000 hard max). `task_session_exit.rs` is 745. `acp/client.rs` is 916 and near the hard max. New behavior should go in new modules, not in these files (AGENTS: do not grow over-limit files). `acp/client.rs` also needs a pure extraction first (see Slice 0).

---

## 9. Proposed slicing of Phase 2 (packets of at most ~4 files, each compiles and tests alone)

Every packet is a separate commit. Each names the test that should be listed as passed in nextest. Per-crate runs: `env -u RUST_LOG cargo nextest run -p ajax-web`, `-p ajax-cli`. Each packet runs clippy with `-D warnings` on its crate.

**Slice 0 (pure move, prerequisite): shrink `acp/client.rs`.** Files: `acp/client.rs`, new `acp/acp_launch.rs`, `acp/mod.rs`. Move `acp_candidates`, `native_acp_advertised`, `spawn_acp_process` (`client.rs:762-916`, about 155 lines) and their cfg-test helpers. Verify by a line-multiset diff, not grep (memory: `sqlite_migrations_module`). No behavior change. Tests: the existing `client_process_group_tests` and `client_spawn_model_tests` must pass unchanged (listed). Packet size: 3 files.

**Slice 1: byte log with offsets (pure).** Files: new `ajax-web/src/slices/web_session/session_shim_log.rs`, new `session_shim_log_tests.rs` (with a `#[cfg(test)]` line before `mod session_shim_log_tests;` in `web_session/mod.rs`), `web_session/mod.rs`. Append lines to `<state_dir>/web-session/<encoded>.shim.log`, offsets as byte positions, replay from offset. No process, no socket. Test: `append_then_replay_from_offset_returns_exact_suffix`. Packet size: 3 files.

**Slice 2: socket relay protocol (pure over `Read`/`Write`).** Files: new `web_session/session_shim_relay.rs`, `session_shim_relay_tests.rs`, `web_session/mod.rs`. Framing: a request line `REPLAY <offset>` or `SEND`, then JSONL. No tokio. Test: `replay_request_streams_from_offset_then_live_lines`. Packet size: 3 files.

**Slice 3: CLI subcommand skeleton (no spawn).** Files: `ajax-cli/src/cli.rs`, `ajax-cli/src/lib.rs` (both matchers, `lib.rs:112` and `:174`), new `ajax-cli/src/session_shim.rs` (parse `--handle`, `--state-root`, program and args, call a stub in ajax-web). Test: a `cli.rs` parse test listed for `session-shim`. Packet size: 3 files.

**Slice 4: shim process (spawn under nono, setsid, hold stdio, fan-out).** Files: new `ajax-web/src/adapters/session_shim.rs` (or a `slices/web_session/` module, if the ajax-web architecture tests prefer), `ajax-web/src/adapters/mod.rs`, `ajax-cli/src/session_shim.rs` (wire to the adapter), plus `ajax-web/src/architecture_web_session.rs` only if a new module needs an allow-list entry (check first). Uses `program::sandboxed_program` (`adapters/program.rs:131`, currently `pub(crate)`) and `setsid` via `nix::unistd::setsid`. Test: spawn a `sh -c 'echo hi'` harness, assert the log has `hi` and the shim survives the parent's exit (uses a child `sleep`). Packet size: 3–4 files.

**Slice 5: ACP stdio generalization.** Files: `acp/sdk_connection.rs` (`ChildStdin`/`ChildStdout` at `:133-134`, `:362-363` → generic `AsyncRead`/`AsyncWrite` bounds), `acp/client.rs` (only the `spawn_internal` call sites), new `acp/shim_attach.rs`. Test: existing `sdk_connection` tests plus a new `shim_attach_round_trips_initialize` listed as passed. Packet size: 3 files. Depends on Slice 0.

**Slice 6: JSONL attach mode (no child ownership).** Files: `acp/jsonl_process.rs` (add an attach constructor that takes a socket instead of `Child`; `Drop` does not kill for attached mode), `acp/rpc_session.rs` or `acp/rpc_client.rs` (only if the attached `host_exited` needs a value), `acp/jsonl_process_tests.rs`. Test: `attached_drop_leaves_the_shim_running` listed as passed, and the existing `dropping_the_process_reaps_the_child` still passes for owned children. Packet size: 3 files.

**Slice 7: adoption on web startup.** Files: `web_session/startup_recovery.rs` (skip interrupt when the shim socket answers), `web_session/orphan_sweep.rs` (do not TERM/KILL a live shim's harness), `web_session/task_session_spawn.rs` (adopt before `reject_foreign_live_child`, `:492`), `web_session/task_session_child.rs` (record the shim pid). Tests: `foreign_boot_dispatching_prompt_is_interrupted_with_note` must still pass with no shim; new `live_shim_is_adopted_not_interrupted` listed as passed. Packet size: 4 files, the largest allowed. Split into 7a (sweep + recovery, 2 files) and 7b (spawn/adopt, 2 files) if the diff is large.

**Slice 8: live check and docs.** Files: `architecture.md` or the Web Cockpit doc for session behavior (`docs/architecture/web-session-behavior.md`, per AGENTS), `.planning/agent-plans/chat-first-core.md` (Matt edits, not us), and the live-check notes. Live check per plan: start a chat turn, restart the dev web, confirm the turn finishes and the browser reconnects with no interrupted note. Packet size: 2 files.

**Open decisions for Matt before Slice 4:** (a) shim log path: `web-session/<enc>.shim.log` (reuse the dir) or `sessions/` as in the plan (§8.5); (b) recorded child pid: shim pid or harness pid (§8.1); (c) eviction of an idle shim-owned session: keep the harness alive (requires a new eviction policy) or kill as today (§5, §1.5); (d) whether `setsid` runs in the shim's first action (short window) or via `pre_exec` (unsafe, §5).

---

## Verification performed for this file

- Read-only: `cat`/`sed`/`grep` only. No `cargo` build or test was run, and no source file was edited. No git commands that change state were run.
- Line numbers were taken from `grep -n` and `sed -n` outputs in this worktree. Ranges marked `~` were read from excerpts and not re-checked by line.
