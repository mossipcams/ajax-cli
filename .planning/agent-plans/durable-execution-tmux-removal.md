# Durable agent execution and tmux removal

Approval: user requested immediate implementation (2026-10-12). No wait gate.
Branch: `ajax/durability`. Orchestrated through the model router; one
`EXECUTION` per stage below. Each stage is one PR-sized delta (≤1500 changed
lines, ≤600 per file) that leaves the workspace green.

## Scope

Replace tmux-backed agent supervision with a SQLite-backed durable execution
kernel owned by Rust, native harness runtimes behind one capability-aware
interface, and a directly managed PTY terminal. Reference for invariants only:
Pi Durable (`earendil-works/pi/packages/durable`, `docs/spec.md`). It is not a
dependency.

## Non-goals

- No general workflow engine: no user-defined workflows, timers, or signals.
- Terminal shells survive an `ajax-web` restart or crash (user requirement,
  2026-10-12, supersedes the original "not guaranteed" wording). They do not
  survive a machine reboot or the death of their own keeper process — the same
  bound tmux had. Nothing stronger is claimed.
- No fabricated model/tool checkpoints for external harnesses. Ajax checkpoints
  only what it dispatches or can observe: prompt admission, prompt dispatch,
  turn completion, permission answers, session create/restore, cancel.
- No rewrite of Git/worktree/PR/CI slices, Cockpit layout, or the browser
  WebSocket protocol v2 envelope.

## Audit (2026-10-12, at 04877e33)

| Today | Where | Replacement |
| --- | --- | --- |
| Transcript JSONL per handle | `ajax-web/src/adapters/web_session_store/mod.rs` | `execution_events` rows |
| Prompt ledger JSON per handle (`Queued/Dispatching/Interrupted/Completed`) | `web_session_store/prompt_ledger.rs` | `prompt_submissions` rows |
| Stored ACP session id + model in JSONL meta | same | `agent_runs.native_session_id`, `model` |
| Per-task actor, queue, restore, eviction | `ajax-web/src/slices/web_session/task_session*.rs`, `task_session_directory.rs` | same actor, driven by kernel + `HarnessRuntime` |
| ACP stdio for all four harnesses | `ajax-web/src/adapters/web_session_acp/` | kept for Cursor and Codex only |
| tmux session per task, send-keys launch, probes | `ajax-core/src/adapters/tmux.rs`, `commands/*`, `runtime_refresh/*`, `task_operations/*`, `ajax-cli/src/tmux_task_session/` | removed; process and run evidence from kernel |
| `tmux_session`, `tmux_status`, `TmuxMissing`, `TmuxProbe`, `registry_task_tmux_evidence`, `registry_task_window_evidence` | `ajax-core/src/models/*`, `registry/sqlite/*` | legacy columns kept readable, no longer written or required |
| Browser terminal = `portable-pty` running `tmux attach` | `ajax-web/src/adapters/terminal_pty/` | `portable-pty` running the user shell in the worktree |
| tmux in 107 Rust files (1269 mentions), plus frontend, docs, scripts | repo-wide | stages 9–11 |

Registry schema lives in `ajax-core/src/registry/sqlite/migrations.rs` with a
single `user_version`; the kernel tables join that database and that version
sequence. No second database, no second store type.

## Architecture

```text
ajax-core::execution            pure domain: ids, states, transitions, replay
                                policy, checkpoint schema version, recovery
                                resolution (no I/O)
ajax-core::registry::sqlite     execution tables + transactional operations on
  ::execution                   SqliteRegistryStore's connection
ajax-web adapters::harness_runtime
                                trait HarnessRuntime + Capabilities
    acp        (Cursor, Codex)  wraps existing AcpStdioClient
    claude_sdk (Claude)         Node worker using @anthropic-ai/claude-agent-sdk,
                                JSONL over stdio
    pi_rpc     (Pi)             `pi --mode rpc` JSONL over stdio
ajax-web slices::web_session    TaskSession actor = execution owner; scheduler
                                recovers unfinished work at host start
ajax-web adapters::terminal_pty direct shell PTY, no tmux
```

Single owner: the `ajax-web` host process. Ownership is a row
(`execution_owner`: boot id, pid, heartbeat) taken in a transaction; a second
host refuses to schedule while a live owner exists. Harness children run in
their own process group and are killed as a group on cancel, shutdown, and
orphan sweep (pids recorded in `agent_runs`).

### Entities (SQLite)

- `agent_runs` — run id, task id, harness, model, native session id,
  capabilities JSON, status, owner boot id, child pid/pgid, schema version.
- `prompt_submissions` — id, run id, idempotency key (existing
  `clientMessageId`, unique per run), ordinal, phase, content JSON.
- `execution_tasks` — id, run id, parent id, kind, state, cancel requested,
  outcome JSON.
- `execution_checkpoints` — task id, version, operation, replay policy,
  external operation id, intent JSON, result JSON (null until step 3).
- `execution_events` — run id, seq (monotonic per run), payload JSON. The
  committed seq is the browser cursor.

### States

`pending → running → waiting → completing → completed | failed | cancelled`,
plus `interrupted` (recovery required, operator action) reachable from
`running`, `waiting`, and `completing`. A parent cannot enter `completing`
while a child is non-terminal.

### Effect sandwich

1. Transaction: checkpoint row with intent + policy, state transition.
2. External call outside any transaction.
3. Transaction: checkpoint result, next state, events, child wakeups.
4. Publish to subscribers only after step 3 commits.

### Replay policy on recovery (checkpoint has intent, no result)

- `safe` — re-execute.
- `reconcile` — query the harness by external id / session state, then decide.
- `unsafe` (default, and for unknown operation or newer schema version) —
  mark task `interrupted`, never re-execute.

| Operation | Policy |
| --- | --- |
| session restore, state query, history fetch | safe |
| prompt dispatch | reconcile if the adapter reports `execution_reconciliation`, else unsafe |
| permission answer, cancel | unsafe |
| queued prompt not yet dispatched | no effect started; stays queued |

### Recovery guarantees by harness (target)

| Harness | Conversation restore | Dispatched-prompt reconcile | Interrupted tool |
| --- | --- | --- | --- |
| Claude (Agent SDK) | yes, `resume` by session id | no — surfaced as interrupted | never assumed complete |
| Pi (RPC) | yes, session file | yes when `get_state`/messages show the turn; otherwise interrupted | never assumed complete |
| Cursor (ACP) | when `session/resume` or `loadSession` is advertised | no | never assumed complete |
| Codex (ACP) | same as Cursor | no | never assumed complete |

## Stages

- [x] 1. Kernel domain in `ajax-core::execution`: ids, states, transition
      table, replay policy, recovery resolution, checkpoint schema version.
      Pure, unit + property tested.
- [x] 2. Kernel persistence: migration adding the five tables + owner row;
      transactional ops (admit prompt, begin effect, commit effect, complete
      child + wake parent, request cancel, load unfinished). Fault-injection
      test: a failed transaction leaves no partial rows.
- [ ] 3. `HarnessRuntime` trait + `Capabilities`; ACP adapter over
      `AcpStdioClient`; mock runtime binary for crash tests.
- [ ] 4. `TaskSession` writes through the kernel: prompt admission, dispatch
      checkpoint, events, completion. Publish after commit. One-time import of
      JSONL transcripts and prompt ledgers (ids and cursors preserved, source
      files kept as `.migrated`). Remove `web_session_store`.
- [ ] 5. Recovery scheduler at host start: ownership, orphan process-group
      sweep, bounded concurrency, replay-policy resolution, interrupted
      surfaced to Chat with Retry / Start fresh.
- [ ] 6. Claude Agent SDK worker + adapter; Claude leaves ACP.
- [ ] 7. Pi native RPC adapter; Pi leaves ACP.
- [ ] 8. Status projection from committed events: idle, generating, tool
      executing, subagent working, waiting for input, recovering, interrupted,
      failed, completed; frontend mapping.
- [ ] (PARKED) 9. Terminal: one detached keeper process per task (`ajax terminal-keeper`,
      own session via `setsid`, not a child of `ajax-web`) owns the PTY master
      and the shell in the worktree, keeps a bounded scrollback ring, and serves
      input / output / resize over a Unix socket under `state_dir` (0600, same
      user only). `ajax-web` connects to the socket on attach and starts a
      keeper only when none answers; a host restart reconnects to the running
      shell and replays scrollback. Keeper pid + socket path are recorded so
      drop/tidy can stop it and startup can sweep keepers whose task is gone.
      9a keeper + protocol, 9b web bridge onto the keeper, 9c drop `tmux
      attach` and the ephemeral-session reaper.
- [ ] (PARKED) 10. Task lifecycle: `start` provisions worktree only; agent session is
      created on first Chat attach or prompt; drop/tidy stop the run instead of
      killing tmux. Remove tmux plan steps, probes, preflight, config.
- [ ] (PARKED) 11. Delete `adapters/tmux.rs`, `tmux_task_session`, tmux evidence
      writes, `TmuxMissing` paths; migration tolerating legacy tmux columns;
      docs (`architecture.md`, `core-subsystems.md`, `web-cockpit.md`,
      `web-session-behavior.md`, `AGENTS.md`, README).
- [ ] 12. Crash-recovery integration suite with real process kill/restart
      over the mock runtime (all 14 boundaries in the request), run with tmux
      absent from `PATH`.

## Stop conditions already covered by the request

Replacing the terminal model, changing runtime authority, and removing the
tmux public behavior are explicitly requested. Deleting user data is not:
legacy rows, transcripts, and worktrees are never deleted by a migration.

## Deviations

- 2026-10-12 (user, latest instruction): scope narrowed to **Ajax web chat
  only, not terminals**. In scope: stages 1, 2, 4, 5, 8, 12 (chat prompt
  admission, queue, transcript, checkpoints, recovery, status, crash tests) and
  the chat runtimes via #1264. **Parked, not started:** stage 9 (terminal
  keeper), stage 10 (task provisioning without tmux), stage 11 (tmux code and
  evidence removal). Ajax Terminal and tmux provisioning stay exactly as they
  are on main; nothing on this branch touches them.

- 2026-10-12: open PR #1264 (`feat/claude/sdk-integration`, 11 commits, not on
  main) already adds a `SessionClient` abstraction, a Claude Agent SDK client,
  and a Pi native RPC client under `ajax-web/src/adapters/web_session_acp/`.
  Stages 3, 6, and 7 are therefore NOT reimplemented on this branch. They
  become: land #1264, then add `Capabilities` reporting and kernel checkpoint
  wiring on top of its `SessionClient`. Stages 1, 2, 9, 10, 11 do not touch
  those files and proceed here. Stages 4, 5, 8, 12 touch the session slice and
  are sequenced after #1264 merges to avoid a 200-file conflict.
- Stage 2 split into 2a (schema + atomic commit) and 2b (recovery load, owner
  lease, cancel intent).
- Prompt phase is not stored separately: a prompt's phase is the state of its
  execution task (`prompt_submissions.task_exec_id`), so there is one source of
  truth for queued / dispatching / interrupted / completed.

## Validation log

- Stage 1 (ca518073): `cargo fmt --check`, `cargo clippy -p ajax-core
  --all-targets --all-features -- -D warnings`, `cargo nextest run -p ajax-core
  execution architecture` — 51 passed. Rerun by the orchestrator.
- Stage 2a: Qwen timed out at 900s with partial work; escalated to Codex,
  which finished it (its report failed the router schema, so acceptance rests
  on the delta and an orchestrator rerun). `cargo fmt --check`; `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`; `cargo nextest run
  -p ajax-core` 1021 passed; `cargo nextest run -p ajax-cli -p ajax-web` 1152
  passed. Existing-test edits: six `assert_eq!(version, 9)` literals became 10
  in `registry/sqlite/tests/suite_2.rs` and `suite_3.rs`; nothing else.
- Stage 2b (Codex): duplicate admission is now a whole-commit no-op; cancel
  intent, run lookup, history import, unfinished-execution load, event prune,
  per-task delete, owner lease. Orchestrator rerun: fmt, workspace clippy,
  `cargo nextest run -p ajax-core` 1030 passed (9 new recovery tests).
- Stage 4a-1 (6971212c): registry database path threaded into chat sessions
  (`SessionStorage`). 1153 web+cli tests passed.
- Stage 4a-2: chat transcript + stored session id + model now live in the
  kernel tables (`web_session_store` rewritten; JSONL reader kept only as the
  import source, `legacy.rs`). Codex started it and ran out of quota; per user
  instruction the rest went to Qwen in six small packets. Orchestrator review
  changed four things in the adapter (raw-JSON import, undecodable row skipped
  not fatal, `.migrated.N` instead of an error, no capabilities-field hack) and
  reversed one of its own calls: issue #1240 needs the unreadable-row count to
  persist, now `agent_runs.unreadable_history_rows`. Core `open()` now creates
  the parent directory. Orchestrator rerun: fmt, workspace clippy,
  `npm run verify:arch`, `cargo nextest run --workspace` 2522 passed.
  Existing tests changed (setup/inspection only unless noted):
  `web_session_store/tests.rs` — `torn_last_line_is_skipped` (now seeds a
  legacy file; renamed `..._on_import`), `append_past_the_byte_cap_...`
  (byte cap no longer exists; now asserts the MAX_LOG_EVENTS bound),
  `incremental_appends_keep_the_transcript_file_identity` (inode assertions
  removed, ordering assertion kept; renamed), filename/delete tests seed a
  legacy file; `task_session_durability_tests.rs` — fault helpers target the
  database / a test-only meta-write switch, #1240 seeds a legacy file;
  `session_cleanup_tests.rs` — #977 inspects through `load` instead of
  `is_file()`. **These need Matt's sign-off.**
- Known ceiling: the adapter opens the registry (and runs the migration check)
  on every append; marked `ponytail:` in `load_run`.
- Security review of 046d6ae7 (background reviewer; finding body arrived
  empty, title only): confirmed and fixed in 38a48a56 — session delete now
  removes legacy files and ledger even when the database delete fails, and
  deletes/lists numbered `.jsonl.migrated.N` leftovers. Not addressed: SQLite
  does not zero deleted rows (`secure_delete` is off), so deleted transcript
  text can remain in free pages of the registry database.
- Stage 4b (Qwen, packets K1/A1a/A1b/A1c): prompt ledger stored as kernel
  prompt tasks. Every ledger save is one `commit_execution`; a dispatch opens a
  `PromptDispatch` checkpoint with replay policy `unsafe`, completion closes
  it. Legacy `prompt-ledger.json` imported once, renamed `.migrated`. A re-used
  prompt id that was already withdrawn fails closed. Registry database is now
  chmod 0600 on open (prompt text used to sit in a 0600 file; the database was
  0644) — this also tightens existing users' databases. Orchestrator rerun:
  fmt, workspace clippy, verify:arch, `cargo nextest run --workspace` 2543/2544;
  the one failure, `web_session_acp::spawn_tests::
  live_cursor_initialize_advertises_load_session`, spawns the real cursor CLI,
  is untouched by this branch, and passed when rerun alone (load flake).
  Existing tests changed: `prompt_ledger/tests.rs::round_trip_atomic_ledger`
  (0600 assertion now on the database file) and
  `delete_ledger_removes_sidecar_file` (seeds the legacy file by hand).
- Not yet done in stage 4: prompt admission and its user-message transcript
  event are still two commits (4c); nothing reads the checkpoints yet (stage 5).
- Still owed from 4a-2's list: a slice-level restart test and an import test
  with `dropped > 0`; folded into 4b.
- Resolved in 2b: a commit carrying a duplicate prompt must be a complete
  no-op (today its events still append, which would duplicate the user bubble).
