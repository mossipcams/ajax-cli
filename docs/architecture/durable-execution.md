# Durable Chat Execution

How Ajax Chat persists transcripts, prompt state, and execution checkpoints in
the registry database so that a running chat can survive process crashes and
server restarts. This document describes the persistence and recovery model of
Chat; it is not a general durability framework for Ajax.

## 1. Scope

This change covers **Ajax Chat only**:

- The persisted chat transcript (user, assistant, and tool events).
- Prompt admission, queuing, dispatch, and turn-outcome recording.
- Execution-state tracking for chat runs and tasks.
- Crash-safe persistence of chat state in the registry database, and a fixed restart rule for in-flight prompts (see section 5).

The following are explicitly **not** part of this change:

- Terminals: PTY-backed sessions and terminal state have no durable-execution
  semantics here. Terminal state is out of scope.
- tmux provisioning: session creation, pane management, and tmux lifecycle
  are unchanged and not checkpointed by this mechanism.
- The Claude Agent SDK adapter, the Pi RPC adapter, and the Cursor SDK sidecar
  adapter as execution substrates: their internals are not instrumented with
  checkpoints. Ajax observes them at the boundaries it already has (dispatch,
  events it receives), nothing more.

## 2. Storage

All durable chat state lives in the registry SQLite database, implemented by
`SqliteRegistryStore` (schema `user_version` 10). The relevant tables are:

| Table | Contents |
| --- | --- |
| `agent_runs` | One row per chat run; stores the stored native/ACP session id and model. |
| `execution_tasks` | Execution tasks belonging to a run (parent/child structure). |
| `prompt_submissions` | The prompt queue: admitted prompts, idempotency keys, content. |
| `execution_checkpoints` | Dispatch checkpoints and recorded results per task/prompt. |
| `execution_events` | The durable chat event stream (transcript events). |
| `execution_owner` | The single-host execution lease (see Ownership). |

Key invariants:

- **One run per qualified handle**, with run id `web-chat:<handle>`. A handle
  never produces two live runs; reconnecting to a handle finds the existing
  run.
- **Event sequence numbering is the browser cursor contract**: a browser
  cursor of `n` means "I have events up through sequence `n`", and polling for
  `cursor = n` returns events starting at sequence `n+1`. Sequence numbers are
  dense, monotonic per run, and never reused.
- **Retention**: each run keeps at most 2000 events. Older events are removed
  by `prune_execution_events`, which deletes old rows in place and never reuses
  a sequence number, so existing cursors remain valid after pruning.
- **Superseded streamed versions are rewritten in place as markers.** When a commit appends a streamed `message` event whose non-empty `itemId` already has an earlier real version in the retained transcript, that earlier row's payload is rewritten to an empty marker (a `message` with the same role and `itemId`, no text/content blocks) inside the same transaction. Rows are never deleted or renumbered for this: sequence numbers stay dense and monotonic, cursors stay valid, reads skip markers, and markers count toward the 2000-event retention.

The transcript is the event stream itself: there is no separate prose-only
copy that can drift from what was executed.

## 3. Execution States

An execution task moves through these states:

- `pending` — admitted and waiting to run.
- `running` — dispatched to, or actively executing in, a harness.
- `waiting` — parked while awaiting an external condition.
- `completing` — outcome known, final events being recorded.
- `completed`, `failed`, `cancelled` — terminal states.
- `interrupted` — execution was stopped abnormally (crash or recovery), and the
  task needs a decision before it may run again.

Allowed transitions (authoritative; anything else is rejected):

```text
pending     -> running | cancelled
running     -> waiting | completing | failed | cancelled | interrupted
waiting     -> running | failed | cancelled | interrupted
completing  -> completed | failed | interrupted
interrupted -> pending | cancelled | failed
completed, failed, cancelled: terminal (no outgoing transitions)
```

Parent/child rule: a parent task **cannot** enter `completing` or `completed`
while any child is in a non-terminal state. Parents finish last.

## 4. Atomicity

All durable effects of one logical execution step are written by a single call
to `commit_execution`, which applies everything in **one database transaction**:

- new execution tasks,
- prompt admission into the queue,
- checkpoint results,
- state transitions,
- cancel requests,
- prompt content updates and clears,
- checkpoint intents,
- run updates (for example stored session id / model),
- event payload rewrites (superseded streamed versions), and
- execution events.

Consequences:

- **Idempotency**: if a commit contains a prompt whose idempotency key was
  already admitted, the whole commit is a no-op — nothing in it is applied.
  Retries from clients are safe.
- **Admission is atomic with the transcript**: a prompt's admission and its
  user message event commit together, so the queue and the visible transcript
  can never disagree about what was submitted.
- **Outcome is atomic with final events**: a turn's outcome (transition to a
  terminal state, checkpoint result) and its final assistant/tool events commit
  together, so a run can never show completed without its output being present.
- **No partial visibility**: in-memory execution state and event publication to
  connected browsers happen only *after* the transaction commits. Activity and
  status reporting also follow the transcript commit, including when retained
  events are retried after a persistence failure. Readers see either the complete
  old state or the complete new state, never a mixture.

## 5. Checkpoints and Replay Policy

A checkpoint records that Ajax dispatched a prompt (or observed an operation)
and carries one of three recovery policies:

- **`safe`** — the operation can be replayed after a crash without harmful side
  effects.
- **`unsafe`** *(the default)* — replaying is not safe; a crashed execution of
  this operation must be treated as interrupted.
- **`reconcile`** — recovery requires asking the harness what actually
  happened before deciding.

Crash recovery decides each in-flight task's fate through `resolve_recovery`,
which applies its rules **in this order**:

1. Newer checkpoint schema than this server understands -> **interrupt**
   (unsupported schema; do not guess).
2. A stored result already exists -> **resume from the result**.
3. Cancel was requested -> **cancel**.
4. The stored operation is unrecognised -> **interrupt**.
5. Otherwise, follow the checkpoint's policy:
   - `safe` -> **replay**,
   - `reconcile` -> **reconcile with the harness**,
   - `unsafe` -> **interrupt** (ambiguous; no automatic resend).

**What the host does today.** `resolve_recovery` and the reconcile mapping are implemented and tested as pure functions, and crash-tested against the real kernel store with aborted child processes, but the web host does not yet call them. At restart the chat session applies one fixed rule instead: a prompt that was dispatching becomes interrupted (the transcript shows `Prompt <id> was interrupted by host restart and was not retried.`) and is never resent; queued prompts are kept and dispatched in order. Reconcile and replay are not used for chat yet, because no harness adapter reports that it can reconcile a dispatched prompt.
**Recovery does not wait for an attach.** The first time a host claims the execution owner lease it applies the same restart rule to every chat run owned by a previous boot, without starting any agent: dispatching prompts become `interrupted` and the transcript gets the `Prompt <id> was interrupted by host restart and was not retried.` note, and the task's reported status is moved off "Agent working" to the failed-turn state. Queued prompts are untouched; the per-session acquire path later finds nothing left to interrupt, so the rule is idempotent. Runs whose owner boot id is this host's own are never touched. Queued prompts are then dispatched without a browser: right after that first claim a startup task lists the chats whose queued prompts belong to a previous boot, resolves each to its task, and runs them strictly one chat at a time (concurrency 1) by acquiring the session (which resumes the stored native session and fails closed if the harness cannot resume it), waiting until every prompt of that chat is done or 30 minutes pass, and releasing it. A chat that cannot be resumed keeps its prompts queued for the next attach.

For a `reconcile` task the server queries the harness about the specific
operation, and maps the answer:

- *executed* -> **adopt** the observed result,
- *not executed* -> **replay**,
- *unknown* -> **interrupt**.

Because a dispatched prompt is recorded with an **UNSAFE** checkpoint by
default, a prompt that was handed to a harness and then crashed becomes
`interrupted` on recovery and is **never resent automatically**. This is the
intended behavior: Ajax prefers a visible interruption over a silent duplicate
prompt.

State this plainly: **Ajax checkpoints only what it dispatches or observes.**
It does not checkpoint inside a harness's own turn — it has no visibility into
a model generating text mid-turn or a tool executing halfway through, and it
does not attempt to recover such in-progress work. Recovery operates at the
prompt/task boundary.

## 6. Ownership

Exactly one host executes against a given registry. This is enforced by the
`execution_owner` lease:

- The executing host **heartbeats every 10 seconds**; a lease is considered
  **stale after 30 seconds** without a heartbeat, and a dead owner's lease can
  be taken over.
- Once the host loses its lease (another host took it, or three heartbeats in
  a row failed) it refuses to start or respawn an agent, refuses to admit a
  prompt, and refuses to dispatch a queued prompt; queued prompts stay queued.
  The refusal is not permanent: at each 10 s heartbeat tick, and on the next
  acquire, the host tries to claim the lease again and resumes as soon as it is
  free, stale, or held by a process that is gone. Agents that were already
  running when the lease was lost are not stopped by that host itself; the new
  owner's first-claim orphan sweep ends them instead, targeting every run whose
  `owner_boot_id` is foreign and whose recorded child start-time token still
  matches the live pid, including live-but-stale children left by the previous
  owner.
- **Owner fence.** Execution commits carry the host's lease `boot_id` (a
  store-level default set after the host's first successful claim, or a
  per-commit override). `commit_execution` checks `execution_owner.boot_id`
  inside the IMMEDIATE transaction and rejects the write with `Fenced` when the
  boot id differs or the row is gone. A displaced host's late writes therefore
  never reach history: they surface as persist errors and, for transcript
  events, land in the in-memory unpersisted backlog as a durability fault.
- A dead owner is detected with an in-process `kill(pid, 0)` probe and counts as
  dead **only on positive evidence** that the process is gone (`ESRCH`, no such
  process); a failed or inconclusive check does not authorize takeover. The
  probe runs in-process rather than through `ps` because `/bin/ps` is setuid and
  the nono agent sandbox refuses to exec it, so the same code runs in sandboxed
  test sessions.
- **Attach and read paths are not gated** by the lease: any client can attach
  to, and read state from, a registry whose execution lease is held elsewhere.
- **Harness children run as process-group leaders.** The host starts every harness child (ACP stdio, Pi RPC, Claude SDK and Cursor SDK sidecars) in its own process group, records each child's pid and start-time token before the attach succeeds (if the token cannot be recorded or is unreadable, the attach fails and the staged client is discarded, so every live child is findable by a later sweep), and kills the whole group when it tears the session down, so tool subprocesses cannot outlive a session. A host does not acquire, respawn, or reset while a previous owner's child for that chat is still recorded and alive. When a new host claims the lease it runs an orphan sweep; the sweep and startup recovery run until complete (an incomplete run is retried on the next heartbeat tick, and the headless startup scan is retried until it completes or a bounded number of attempts), so recovered turns whose activity report failed are re-reported by the heartbeat. For each run whose `owner_boot_id` is foreign and whose recorded start-time token still matches the live pid, the sweep sends SIGTERM to the child's process group (falling back to the pid for children recorded before groups existed), polls up to 5 s for exit, escalates to SIGKILL after re-checking the token so a reused pid is never killed, polls up to 2 s more, and clears the recorded child identity only once the process is observed gone (unreaped zombies count as gone); a survivor leaves the row and the sweep incomplete so the heartbeat retries. A mismatched or unreadable token is never signalled. The start-time token is the kernel's process start time (`seconds.microseconds` from `proc_pidinfo` on macOS, the boot id plus `starttime` ticks from `/proc/<pid>/stat` on Linux, so a token from a previous boot never matches), so a token recorded by a build that used `ps lstart` never matches and is never signalled.
- **Agents can start with no browser attached.** After a restart the host starts harnesses for chats that still have queued prompts (see section 5); this is the only case where the host starts an agent without an operator action, and it only happens after the host holds the execution lease.

## 7. Migration

Existing on-disk chat files are imported into the database exactly once:

- The legacy transcript `<handle>.jsonl` is read and its events imported as
  `execution_events`. After import the source file is **renamed to
  `<handle>.jsonl.migrated`**: it is kept (not deleted), remains mode `0600`,
  and if a `.migrated` name already exists, a numbered suffix is used instead.
- The legacy prompt ledger `<handle>.prompt-ledger.json` is imported into
  `prompt_submissions`. After its import commits, the source file is **deleted** —
  except when the database already contained prompts for that handle, in which
  case it is renamed (preserved) rather than deleted.

After migration completes, the database is the only transcript source; the
legacy files are never read again on normal operation.

## 8. Confidentiality

- The registry **database file is created mode `0600`**.
- SQLite **`secure_delete` is enabled**, so overwritten pages do not retain
  recoverable copies within the database file.
- **Prompt content is cleared in the same transaction** that completes or
  withdraws the prompt; committed history no longer needs the raw prompt body,
  and no window exists where a completed prompt's content lingers.
- **Session delete removes the database rows and every legacy file** for that
  handle (including `.jsonl.migrated` remainders), so deleting a session does
  not leave an on-disk copy behind.

Residual risk, stated plainly: **SQLite rollback journals may briefly hold old
page images** (for example a page containing prompt content) between the moment
a transaction rolls back and the journal is deleted. This is transient — the
journal file exists only for the duration of write transactions — but it means
"deleted from the database" is not instantly "gone from disk" at every instant.

## 9. Guarantees NOT Provided

To prevent over-reading this document, these are explicitly out of scope:

- **No replay of in-flight model or tool work.** If a crash happens mid-turn —
  while a model is generating, or a tool call is executing — nothing is
  re-executed, rolled back, or completed on the harness's behalf. The task
  becomes `interrupted` (under the default UNSAFE policy) and waits for an
  explicit decision.
- **A resumed native session does not imply an interrupted tool finished.**
  Reattaching to a live harness session (native/ACP) restores the conversation
  context only; it is not evidence that work in flight at crash time completed,
  and Ajax makes no attempt to infer completion from session state.
- **PTY/terminal state is out of scope.** Terminal scrollback, tmux panes, and
  any interactive terminal contents are neither checkpointed nor recovered by
  this mechanism.
