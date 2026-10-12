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
- No durability claim for PTY shell state across an Ajax process crash.
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

- [ ] 1. Kernel domain in `ajax-core::execution`: ids, states, transition
      table, replay policy, recovery resolution, checkpoint schema version.
      Pure, unit + property tested.
- [ ] 2. Kernel persistence: migration adding the five tables + owner row;
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
- [ ] 9. Terminal: direct shell PTY in the worktree; drop `tmux attach`.
- [ ] 10. Task lifecycle: `start` provisions worktree only; agent session is
      created on first Chat attach or prompt; drop/tidy stop the run instead of
      killing tmux. Remove tmux plan steps, probes, preflight, config.
- [ ] 11. Delete `adapters/tmux.rs`, `tmux_task_session`, tmux evidence
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

(none yet)

## Validation log

(appended per stage)
