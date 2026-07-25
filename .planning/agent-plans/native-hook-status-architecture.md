# Native-hook-first status architecture

Planning-only. Architecture pass. **No production code changed in this pass.**
Do not implement until approved.

Delegation decision: not delegated because this is planning/review-only work.

## Scope

Make native client hooks the primary, structured source of agent status; delete
the legacy scalar/pane/tmux-cache status machinery that shadows them; and land
one client-event translation boundary, one agent reducer, and one final
projector.

This builds on the already-landed canonical-agent-events work
(`canonical-agent-events.md`, Phases 0–4a). It does **not** re-litigate the
envelope schema or the JSONL log. It removes the string round-trip and the
duplicate reducer that were left in place during that migration, plus the legacy
inputs that migration was supposed to retire.

### Non-goals (hard scope fence — see §12)

- No hook-health state machine, diagnostic subsystem, or new telemetry.
- No socket/polling redesign (JSONL fold on refresh stays the durable path;
  `notify.sock` stays best-effort as-is).
- No new pane heuristics. Pane classification is **deleted**, not re-scoped.
- No broad DB migration. Existing SQLite schema (v9) and the duplicated-field
  sync approach (§9) are reused.
- No compatibility layer for any removed legacy method.
- No new framework, trait-per-client, or generic "status provider" abstraction.

---

## 1. Current status flow and every competing status source

### 1.1 The pipeline today

```
native hook  ──►  ajax __agent-event --client X --event Y  (stdin JSON)
                    │
                    ├─ translate_native_event()  →  CanonicalAgentEvent{kind,detail}   [agent_event.rs]
                    │
                    ├─ append canonical envelope to {stem}.jsonl                        [KEEP]
                    │
                    └─ project_legacy_value() → "working"/"ask"/"done"/... 
                          └─ write {stem}.json  (AgentEventSnapshot{value:String})       [LEGACY scalar]

refresh  ──►  TmuxAgentStatusSnapshot::from_runtime_cache()                             [agent_status_cache.rs, ajax-cli]
                    ├─ reads ~/.cache/tmux-agent-status/*.status         (session strings) [LEGACY]
                    ├─ reads ~/.cache/tmux-agent-status/panes/*.status   (pane strings)     [LEGACY]
                    ├─ reads agent-runtime/*.json  (wrapper snapshot)    → value string
                    └─ reads agent-events/*.jsonl via fold_and_project_jsonl:
                            fold_envelopes → RunSnapshot → project_snapshot → STRING       [COLLAPSE]
                    → Vec<AgentStatusCacheEntry{ value:String, source, run_id }>

refresh  ──►  runtime_refresh::refresh_runtime_context_with_tier()                      [ajax-core]
                    ├─ builds live::StatusCandidate{ value:String, source }  from entries
                    ├─ live::select_status_observation():                                [DUPLICATE reducer]
                    │     re-parses each String back into StatusObservation
                    │     (classify_agent_status_value / hook_observation_if_eligible)
                    │     → reduce_agent_status()  → StatusProjection → LiveObservation
                    ├─ if !applied && capability allows:
                    │     tmux capture-pane → pane_fallback::maybe_pane_wait()           [LEGACY pane text]
                    └─ GithubChecksAdapter (gh pr checks) → CiFailed / clears

apply    ──►  live::apply_*_observation() writes task.live_status (LiveStatusKind)
                    + side flags (AgentRunning/NeedsInput/…) + agent_status (AgentRuntimeStatus)

project  ──►  ui_state::derive_operator_status(task)
                    lifecycle + substrate + live_status + flags → TaskStatus{Running|Waiting|Idle|Error}
                    → TaskCard.status  (one field, rendered by CLI + TUI + web)          [KEEP]
```

### 1.2 Competing status sources (the problem)

| # | Source | Where | Status |
|---|--------|-------|--------|
| A | Canonical JSONL fold → `RunSnapshot` (structured, correct) | `canonical_agent_event.rs` | **Keep — this is the truth** |
| B | Legacy scalar `{stem}.json` `AgentEventSnapshot{value}` | `agent_event.rs` write; `agent_status_cache.rs` read | **Delete** |
| C | Legacy `~/.cache/tmux-agent-status/*.status` (session) | `agent_status_cache.rs` | **Delete** (nothing in Ajax writes it) |
| D | Legacy pane `~/.cache/tmux-agent-status/panes/*.status` | `agent_status_cache.rs` | **Delete** (nothing writes it) |
| E | `capture-pane` text inference | `pane_fallback.rs` + `runtime_refresh.rs` | **Delete** |
| F | Wrapper runtime snapshot `agent-runtime/*.json` (exit/liveness) | `agent_runtime.rs`, cache | **Keep — terminal/liveness fallback only** |
| G | String reducer `live::select_status_observation` | `live.rs` | **Delete — duplicate of `reduce_agent_status`** |
| H | Structured reducer `agent_status::reduce_agent_status` | `agent_status.rs` | **Keep — single reducer** |
| I | GitHub `gh pr checks` | `adapters/github.rs`, `runtime_refresh.rs` | **Keep — override, extend Pending** |
| J | Substrate/runtime reconciliation | `runtime.rs`, `runtime_refresh.rs` | **Keep** |

**Root problem:** the folded `RunSnapshot` (A) is collapsed to a legacy string
(B), shipped through a string-keyed cache alongside dead inputs (C/D), then
**re-parsed by a second reducer** (G) that re-implements the first (H).
`observations_from_run_snapshot()` — the function that maps `RunSnapshot`
directly to reducer input — already exists but is **dead code outside tests**.
The string round-trip is pure loss: concurrent tools, sticky attention, and
"capability unavailable" all survive in `RunSnapshot` and are flattened to one
of five strings before the reducer ever sees them.

Three enums encode the same "where did this come from" concept:
`AgentStatusCacheSource`, `live::AgentEvidenceSource`,
`agent_status::ObservationSource`.

---

## 2. Target flow

```
native hook ─► __agent-event ─► translate_native_event ─► CanonicalAgentEvent
                                                              │
                                                              └─ append {stem}.jsonl        [ONE write, no scalar]

refresh:  {stem}.jsonl ─► fold_envelopes ─► RunSnapshot(per run)
                                                │
              wrapper agent-runtime/*.json ─────┤  (ProcessExit terminal + liveness only)
                                                ▼
                        observations_from_run_snapshot()  +  wrapper ProcessExit observation
                                                │  (StatusObservation, run_id/parent_run_id preserved)
                                                ▼
                        agent_status::reduce_agent_status()   ── SINGLE agent reducer ──►  AgentPhase + LiveObservation
                                                                                              │
GitHub gh pr checks ──► CiChecksObservation ─────────────────────────────────────────────────┤
runtime substrate  ──► RuntimeHealth / SideFlags / lifecycle ─────────────────────────────────┤
                                                                                              ▼
                        ui_state::derive_operator_status()  ── SINGLE final projector ──►  TaskStatus
                                                                                              │
                                                                              TaskCard.status (CLI + TUI + web)
```

Two stages, made explicit:

- **Stage 1 — Agent-phase reduction:** `native event → CanonicalEvent → (fold) →
  RunSnapshot → AgentPhase`. Owned by `canonical_agent_event.rs` +
  `agent_status.rs`. No GitHub, no substrate, no lifecycle here.
- **Stage 2 — Task-status projection:** `lifecycle + substrate + GitHub +
  AgentPhase → TaskStatus`. Owned by `ui_state.rs`.

`RunPhase` is renamed to **`AgentPhase`** and is the canonical 5-value set
(§5). No new enum is introduced — `RunPhase{Active,Blocked,Settled,Failed,
Unknown}` already is this set.

---

## 3. Verified native-hook capability matrix

Verified against the installer (`agent_hooks.rs`), the translation table
(`agent_event.rs::translate_native_event`), and the declared profiles
(`agent_capability.rs`). "Installed" = Ajax writes this hook; "Translated" =
Ajax maps the event to a canonical kind.

### Claude Code — `~/.claude/settings.json`

| Native event | Installed | Translated → canonical | Capability |
|---|---|---|---|
| `SessionStart` | ✅ | `SessionOpened` | Native |
| `UserPromptSubmit` | ✅ | `TurnStarted` | Native |
| `PreToolUse` | ✅ | `ActivityStarted{Tool}` | Native |
| `PostToolUse` | ✅ | `ActivityFinished{Tool}` | Native |
| `Notification` | ✅ | `AttentionRequested{Permission\|Question}` | Native (permission + question) |
| `Stop` | ✅ | `TurnSettled{Completed}` (or `TurnStarted` if background_tasks) | Native |
| `SessionEnd` | ✅ | `SessionClosed` | Native |
| `SubagentStop` | ❌ **not installed** | — | Subagents = **Unverified** (honest) |
| `PreCompact`/`PostCompact` | ❌ | — | Not modeled (`ActivityKind` has only `Tool`) |

Claude is the most complete client. All installed events are real Claude Code
hooks. ✅ Matrix is accurate.

### Codex — `~/.codex/hooks.json`

| Native event | Installed | Translated → canonical | Capability |
|---|---|---|---|
| `SessionStart` | ✅ | `SessionOpened` | Native |
| `UserPromptSubmit` | ✅ | `TurnStarted` | Native |
| `PreToolUse` | ✅ | `ActivityStarted{Tool}` | Native |
| `PostToolUse` | ✅ | `ActivityFinished{Tool}` | Native |
| `PermissionRequest` | ✅ | `AttentionRequested{Permission}` | Native |
| `Stop` | ✅ | `TurnSettled{Completed}` | Native |
| `SessionEnd` | ✅ | `SessionClosed` | Native |
| question/elicitation wait | ❌ | — | **Unavailable** |
| subagents | ❌ | — | **Unverified** |

⚠️ **VERIFY LIVE (see §"Unsupported/mis-installed hooks").** The installer writes
`.codex/hooks.json` using the **Claude hook shape**
(`{"hooks":{Event:[{"hooks":[{"type":"command","command":…}]}]}}`) via the
shared `merge_hook_entries`. If the Codex CLI does not read this file with this
schema and these event names, every Codex status silently degrades to
wrapper-exit-only. Codex `TurnSettled` completion has always been marked
"native; wrapper exit backup" in `canonical-agent-events.md` for this reason.

### Cursor — `~/.cursor/hooks.json` (`version: 1`)

| Native event | Installed | Translated → canonical | Capability |
|---|---|---|---|
| `sessionStart` | ✅ | `SessionOpened` (+ echoes identity env to stdout) | Native |
| `beforeSubmitPrompt` | ✅ | `TurnStarted` | Native |
| `preToolUse` | ✅ | `ActivityStarted{Tool}` | Native |
| `postToolUse` | ✅ | `ActivityFinished{Tool}` | Native |
| `stop` | ✅ | `TurnSettled{Completed\|Interrupted\|Failed}` (from `status`) | Native |
| `sessionEnd` | ✅ | `SessionClosed` | Native |
| permission wait | ❌ (no such hook) | — | **Unavailable** |
| question wait | ❌ | — | **Unavailable** |
| subagents | ❌ | — | **Unverified** |

Cursor carries turn/tool/session + a rich `stop` outcome (error → Failed). No
wait evidence at all — correctly `Unavailable`, and (post-deletion) it gets **no
pane fallback**; Cursor simply never reports Waiting from a native wait. Identity
resolves via the published cwd-index (`CURSOR_PROJECT_DIR`/`workspace_roots`) +
`sessionStart` env echo-back.

### Pi — `~/.pi/agent/extensions/ajax-agent-events.ts`

| Native event | Installed | Translated → canonical | Capability |
|---|---|---|---|
| `before_agent_start` | ✅ | `TurnStarted` | Native |
| `agent_settled` | ✅ | `TurnSettled{Completed}` | Native |
| session open/close | ❌ | — | session_closed = **Wrapper** |
| tools | ❌ | — | not modeled |
| permission/question | ❌ | — | **Unavailable** |
| subagents | ❌ | — | **Unverified** |

⚠️ Pi is **turn-boundary only**: start + settle, nothing else. Everything between
is inferred from wrapper liveness. `agent_settled` (not `agent_end`) is
deliberately the terminal signal. No tool/session/wait evidence. This is honest
but thin — Pi status is effectively Running (between start and settle) / Done
(after settle) / wrapper-fallback.

### Capability legend

`Native` — client emits it and Ajax translates it. `Wrapper` — only the launch
wrapper's process evidence covers it. `Unavailable` — client has no such event;
Ajax must never fabricate it. `Unverified` — event may exist but is not trusted
for completion. **Absence of an event is never absence of a state.**

---

## 4. Client-event → canonical-event mappings

Single boundary: `agent_event.rs::translate_native_event(client, event,
payload)`. This table is the whole boundary (unchanged from today except the
legacy value projection is removed downstream):

| client | native event | canonical kind | detail |
|---|---|---|---|
| claude | UserPromptSubmit | TurnStarted | — |
| claude | PreToolUse | ActivityStarted | Activity{Tool, id?} |
| claude | PostToolUse | ActivityFinished | Activity{Tool, id?} |
| claude | Notification | AttentionRequested | Attention{Permission if msg~"permission" else Question} |
| claude | Stop | TurnSettled{Completed} — or TurnStarted if `background_tasks` non-empty | Outcome |
| claude | SessionStart / SessionEnd | SessionOpened / SessionClosed | — |
| codex | UserPromptSubmit / PreToolUse / PostToolUse | TurnStarted / ActivityStarted / ActivityFinished | as Claude |
| codex | PermissionRequest | AttentionRequested | Attention{Permission} |
| codex | Stop | TurnSettled{Completed} | Outcome |
| codex | SessionStart / SessionEnd | SessionOpened / SessionClosed | — |
| cursor | beforeSubmitPrompt | TurnStarted | — |
| cursor | preToolUse / postToolUse | ActivityStarted / ActivityFinished | Activity{Tool, id?} |
| cursor | stop | TurnSettled{Completed\|Interrupted\|Failed} | Outcome (from payload `status`) |
| cursor | sessionStart / sessionEnd | SessionOpened / SessionClosed | — |
| pi | before_agent_start | TurnStarted | — |
| pi | agent_settled | TurnSettled{Completed} | Outcome |
| _any other_ | — | `None` (ignored) | — |

`activity_id` comes from `tool_call_id`/`tool_id`/`id`/`tool_name`/`tool`. The
envelope also carries `run_id`/`parent_run_id` from wrapper identity env (or
Cursor cwd-index), preserving run identity (§15).

Notes:
- The wrapper (`agent-runtime/*.json`) supplies **liveness + terminal exit**,
  not activity. It is not a canonical event; it becomes a `ProcessExit`/liveness
  `StatusObservation` directly at reduction (§5, §precedence 5).
- `Heartbeat`, `ChildStarted`, `ChildSettled`, `AttentionCleared` exist in the
  envelope vocabulary and fold correctly; no client emits them today except
  through run-graph identity. Keep the kinds; do not add emitters in this pass.

---

## 5. Canonical-event → AgentPhase rules

Fold (`fold_envelopes`) produces one `RunSnapshot` per `run_id` with open sets
(`active_tools`, `pending_attention`). `RunPhase` is renamed **`AgentPhase`**:

| AgentPhase | Fold condition (open-set aware) |
|---|---|
| **Running** (`Active`) | TurnStarted / any open tool / SessionOpened / ChildStarted, and not settled-with-empty-tools |
| **Waiting** (`Blocked`) | `pending_attention` set (Permission or Question), turn not resolved past it |
| **Done** (`Settled`) | TurnSettled/SessionClosed with no open tools and no pending attention |
| **Error** (`Failed`) | TurnSettled{Failed} (e.g. Cursor `stop status=error`) |
| **Unknown** | no events, or contradictory fresh evidence — never invented |

Open-set invariants (already implemented, kept): one tool finishing ≠ idle while
another is open; parent `Stop` ≠ Done while a non-detached child is active
(`agent_status.rs` parent/child aggregation); a late finish is recorded without
wiping the run; session/process exit closes remaining opens.

`AgentPhase` → reducer input via **`observations_from_run_snapshot()`** (already
written, currently dead): Running→`Working`, Waiting→`WaitingApproval`/
`WaitingInput`, Done→`Done`, Error→`Failed`, Unknown→no observation. These feed
the **single** `reduce_agent_status()` alongside the wrapper `ProcessExit`
observation and child-run observations. The reducer's existing precedence
(ProcessExit > ProviderLifecycle > ProviderHook > ProcessLiveness; expiry;
equal-timestamp cross-source conflict → Unknown; parent/child aggregation) is
retained unchanged.

**`ProviderHook` source tier is deleted** (§8): with legacy hook files gone, the
only structured source is the folded lifecycle (`ProviderLifecycle`) plus wrapper
`ProcessExit`/`ProcessLiveness`. The reducer keeps three tiers, not four.

Waiting is only emitted where the capability profile says the client can prove
it (Claude permission+question; Codex permission). Cursor/Pi never synthesize
Waiting — `Unavailable` + no pane fallback = silence, not a guessed state
(requirement 3).

---

## 6. Final precedence table (Stage 2 — `derive_operator_status`)

Evaluated top-to-bottom; first match wins. `resources_expected` is computed
from lifecycle **first** so substrate absence is only an error while the
lifecycle says the substrate should exist (requirements 8–10).

| # | Condition | Result | Requirement |
|---|---|---|---|
| 0 | `lifecycle == TeardownIncomplete` | **Error** "Teardown incomplete" | 11 |
| 1 | Compute `resources_expected = lifecycle ∉ {Merged, Cleanable, Removing, Removed}` (also `Orphaned` when it means legitimately gone) | — | 1, 10 |
| 2 | `resources_expected` **and** required substrate missing (worktree / branch / tmux / task-window per `RuntimeHealth`+`SideFlag`) | **Error** "<resource> missing" | 2, 8, 9 |
| 3 | `resources_expected` **and** `runtime_projection.observation_error.is_some()` | **Error** "Status unavailable" | 8 |
| 4 | `resources_expected` **and** checkout mismatch (worktree present, wrong/detached branch, not missing) | **Error** "Checkout mismatch" | 8 |
| 5 | GitHub override, relevant + not stale (§7): failed check or merge conflict | **Error** "CI failed" / "Merge conflict" | 3, 6 |
| 6 | GitHub override, relevant + not stale: **pending** checks | **Running** "CI running" | 3, 6 |
| 7 | *(passing checks: clear the override, fall through — passing CI alone is not Done)* | — | 6 |
| 8 | AgentPhase == Error (native Failed, or reducer `CommandFailed`) | **Error** "Agent failed" | 4 |
| 9 | AgentPhase == Running | **Running** ("Agent working"/"Running command"/"Running tests") | 4 |
| 10 | AgentPhase == Waiting, unacknowledged | **Waiting** (native reason) | 4 |
| 11 | AgentPhase == Unknown → wrapper terminal fallback: confirmed exit 0 | **Done**-presentation (Reviewable/Idle path, §note) | 12 |
| 12 | AgentPhase == Unknown → wrapper terminal fallback: confirmed exit ≠ 0 | **Error** "Agent failed" | 12 |
| 13 | lifecycle `Reviewable`/`Mergeable`, unacknowledged | **Waiting** "Ready for review" | — |
| 14 | lifecycle terminal/cleanup ({Merged,Cleanable,Removing,Removed}) | **Idle** | 10 |
| 15 | acknowledged / healthy / nothing actionable | **Idle** | — |
| 16 | no source proved a status | **Unknown** | precedence 6 |

Key deltas from today's `derive_operator_status`:
- **Move the substrate/observation-error/checkout Error checks below the
  lifecycle `resources_expected` gate** (rows 1–4). Today they run *before* the
  terminal-lifecycle idle check, so a `Merged` task with a pruned worktree can be
  reported `Error` instead of staying `Idle`/Done — a requirement-10 violation
  that currently only avoids firing because refresh happens not to set the flags.
  Make the gate explicit.
- **Pending CI → Running** (row 6). Today pending checks only *clear* CI
  evidence; requirement 6 wants them surfaced as Running with a CI explanation.
- **Add `Unknown` to `TaskStatus`** (row 16) for the genuine no-evidence case,
  replacing the unconditional `Idle` default (precedence step 6).
- Wrapper exit is **fallback only** (rows 11–12), reached only when AgentPhase is
  Unknown — process liveness/Running never implies agent Running (requirement 12;
  already enforced in the reducer).

**Note on Done:** `AgentPhase::Done` is a first-class Stage-1 value, but the
operator-facing `TaskStatus` reuses the existing presentation — a completed turn
surfaces as `Waiting "Ready for review"` (Reviewable/Mergeable) and then `Idle`
after acknowledgment. This avoids adding a `TaskStatus::Done` variant and the
churn across CLI/TUI/web renderers. **Open question for approval:** if you want
`Done` as a distinct operator status (not folded into Ready-for-review/Idle),
say so — it is a small enum addition but touches every renderer.

---

## 7. GitHub relevance & staleness rules

Keep `adapters/github.rs` (`gh pr checks <branch> --json name,state,link`, run
at the task worktree path) and `runtime_refresh` wiring. `CiChecksObservation`
is `Healthy | Pending | Failure{check} | Unobservable{reason}`.

Relevance/staleness (requirement 7):
- **Branch-scoped by construction:** the probe runs `gh pr checks <task.branch>`
  in the task's own worktree, so results belong to the task's branch/PR. A task
  with no PR → `Unobservable("no pull request for branch")` → records
  `ci_probe_error`, **never** projects Error (requirement 8 exclusion).
- **Freshness:** rate-limited by `ci_checks_probed_at` — 30 s while the live
  status is a GitHub CI failure, 300 s otherwise. Stale-beyond-window evidence is
  re-probed before it can drive status.
- **Override discipline:** Failure/conflict → Error (rows 5); Pending → Running
  (row 6); Healthy → clear override and reveal AgentPhase (row 7). Passing CI
  never sets Done.
- ⚠️ **Gap to preserve/tighten (not redesign):** there is no head-SHA binding, so
  a just-pushed commit can briefly show the previous run's checks until the next
  probe. This is bounded by the freshness window and is acceptable for this pass.
  Do **not** build a checks-cache-by-SHA subsystem (non-goal). If tightened later,
  bind `ci_checks_probed_at` to `HEAD` sha and invalidate on sha change — one
  field, not a subsystem.

---

## 8. Modules & types: retain / change / add / delete

### Retain (unchanged)
- `ajax-core::canonical_agent_event` — envelope kinds, `fold_envelopes`,
  `RunSnapshot`, **`observations_from_run_snapshot`** (promote from dead → wired).
- `ajax-core::agent_status` — `reduce_agent_status`, `StatusObservation`,
  `ObservationSource` (minus `ProviderHook`), `ProcessLiveness`, parent/child
  aggregation. **Single agent reducer.**
- `ajax-core::agent_capability` — capability profiles (drop `allows_pane_fallback`).
- `ajax-cli::agent_event` — `translate_native_event`, JSONL append, identity
  resolution, socket notify. **Single translation boundary.**
- `ajax-cli::agent_hooks` — installers (see §"mis-installed hooks" for a fix).
- `ajax-core::adapters::github` + `runtime_refresh` GitHub wiring (extend Pending).
- `ajax-core::runtime`, `runtime_refresh` substrate reconciliation.
- `ajax-core::ui_state::derive_operator_status` — **single final projector**
  (re-order + extend per §6).
- `ajax-core::output::TaskCard.status` — one field, rendered by CLI/TUI/web
  (requirement 16 already satisfied — `commands/projection.rs` computes it once).

### Change
- `canonical_agent_event.rs`: rename `RunPhase` → **`AgentPhase`**; delete
  `project_snapshot()` (the `RunSnapshot → &str` collapser) once no caller
  remains.
- `agent_status.rs`: remove `ObservationSource::ProviderHook` and its `rank`
  arm; renumber remaining tiers.
- `ui_state.rs`: re-order precedence (lifecycle gate before substrate Error),
  add `Pending → Running`, add `TaskStatus::Unknown`, wrapper-exit as
  Unknown-only fallback.
- `render.rs` / web / TUI: add the `Unknown` arm to `TaskStatus` display.
- `runtime_refresh.rs`: replace the `AgentStatusCacheEntry{value:String}` →
  `StatusCandidate{value:String}` → `select_status_observation` path with:
  read `{stem}.jsonl` per run → `fold_envelopes` → `observations_from_run_snapshot`
  → collect wrapper `ProcessExit`/liveness observation → `reduce_agent_status`
  → apply. Delete the pane-capture branch.
- `agent_event.rs`: delete legacy scalar write (`project_legacy_value`,
  `should_update_legacy_snapshot`, `write_agent_event`, `AgentEventSnapshot`,
  `translate_agent_event`). Keep `fold`/JSONL/identity/socket.

### Add
- A thin refresh helper (in `runtime_refresh` or a small
  `agent_status_source` fn) that, given the observations for a task, feeds
  `reduce_agent_status`. **One function, not a module/trait.**
- `TaskStatus::Unknown` variant.

### 8b. The ajax-core ↔ ajax-cli status boundary (the one interface decision)

This is the crate boundary the rest of the plan hinges on. **ajax-core never
reads files; ajax-cli does** and passes results across the `AgentStatusCache`
trait. Today that trait carries **strings**, which is why a second reducer
exists in core to re-parse them.

Evidence (verified):
- `agent_status_cache::read_agent_runtime_entry` maps `AgentRuntimeState`
  → `"starting"|"working"|"done"|"failed"`, source `RuntimeWrapper`.
- `agent_status_cache::read_agent_event_entry` → `fold_and_project_jsonl`
  collapses the folded `RunSnapshot` → `"working"|"done"|…`, source `Lifecycle`.
- Both arrive in core as `AgentStatusCacheEntry{ value: String }`;
  `live::classify_agent_status_value` re-parses the string back into a
  `LiveObservation` inside the duplicate reducer.

**Boundary decision:** change what crosses the trait from a *string value* to a
*structured observation*. Two clean options — pick one at implementation time:

- **Option A (preferred): trait yields `Vec<StatusObservation>`.**
  ajax-cli does fold + wrapper mapping and emits reducer-ready observations
  (`observations_from_run_snapshot` for JSONL; a direct `ProcessExit`/liveness
  observation for the wrapper snapshot, preserving `run_id`/`parent_run_id`).
  ajax-core's `runtime_refresh` calls `reduce_agent_status` on them directly.
  `StatusObservation` is already a public `ajax-core` type, so no new shared
  type is needed. This deletes the string, the re-parse, and the duplicate
  reducer in one boundary change.
- **Option B: trait yields `RunSnapshot` per run + a wrapper-exit enum.**
  Keeps folding entirely in ajax-cli but moves the `RunSnapshot → observations`
  step (currently the dead `observations_from_run_snapshot`) to core. Slightly
  more shared surface; only choose if wrapper mapping wants to live in core.

Either way the string (`AgentStatusCacheEntry.value`, `StatusCandidate.value`,
`classify_agent_status_value`) is deleted and the wrapper's
`Starting/Running/Exited*` mapping is expressed **once** as observation
`source`/`kind`, not as a re-parsed English word. The three source enums
(`AgentStatusCacheSource`, `AgentEvidenceSource`, `ObservationSource`) collapse
to the one core enum (`ObservationSource`, minus `ProviderHook`).

This section defines the boundary only. Code is not written in this pass.

### Delete (requirement 13 — all of it)
- `ajax-cli::agent_status_cache.rs` — entire file (tmux-agent-status reads, pane
  `.status` reads, `read_agent_event_entry` scalar reads, `from_runtime_cache`).
- `ajax-core::pane_fallback.rs` — entire file (pane-text inference).
- `runtime_refresh::AgentStatusCache` trait, `AgentStatusCacheEntry`,
  `AgentStatusCacheSource`, `NoAgentStatusCache`, and the capture-pane branch.
- `live::select_status_observation`, `StatusCandidate`, `StatusDecision`,
  `AgentEvidenceSource`, `classify_agent_status_value`,
  `hook_observation_if_eligible`, `drop_hooks_superseded_by_lifecycle`,
  `status_decision_from_projection` — the entire duplicate string reducer.
  (Keep `live::apply_*_observation`, `acknowledge_attention`.)
- `agent_event::project_legacy_value`, `should_update_legacy_snapshot`,
  `write_agent_event`, `AgentEventSnapshot`, `translate_agent_event`,
  `fold_and_project_jsonl` (string projector).
- `canonical_agent_event::project_snapshot` (once callers gone).
- `agent_capability::allows_pane_fallback` + `CapabilitySupport::PaneFallback`.
- `ObservationSource::ProviderHook`.
- All tests, comments, and doc references to the above (grep for
  `tmux-agent-status`, `pane_fallback`, `select_status_observation`,
  `AgentStatusCache`, `project_legacy_value`, `ProviderHook`, `capture-pane`
  status inference).
- `architecture.md`: rewrite the "Legacy provider hook files…", pane-fallback,
  and 4-tier precedence paragraphs to match §5/§6.

---

## 9. Synchronizing duplicated status fields without a persistence redesign

Task carries several redundant status-ish fields: `live_status`
(`LiveStatusKind`), `agent_status` (`AgentRuntimeStatus`), and side flags
(`AgentRunning`, `NeedsInput`, `TestsFailed`, `AgentDead`, …). SQLite schema v9
stores these across `registry_task_workflow` / `registry_task_live_status` /
`registry_task_runtime_projection`.

Approach (no schema change, requirement 17):
- The reducer's `LiveObservation` is applied through the **existing single
  writer** `live::apply_*_observation`, which already updates `live_status`,
  `live_status_observed_at`, side flags, and `agent_status` together, then
  `refresh_cached_annotations(task)` recomputes derived flags. Keep this one
  writer; the sync guarantee comes from routing every status update through it.
- Because status now derives from **one** reducer output rather than two, the
  fields can no longer disagree by construction — the second writer
  (`select_status_observation` path) that could set them independently is
  deleted.
- `TaskCard.status` remains a **pure projection** computed on read in
  `commands/projection.rs` (`derive_operator_status`), never persisted, so it
  can never drift from the stored fields.
- No new columns, no migration. Reuse `migrate_v7_to_current_schema` as-is; the
  deleted legacy `.status`/scalar files were filesystem caches, not DB rows.

---

## 10. Smallest safe implementation sequence

Each step compiles, keeps tests green, and is independently reviewable. TDD per
AGENTS.md: failing test first for behavior steps.

1. **Wire the structured path in parallel (no deletion yet).** In
   `runtime_refresh`, build `StatusObservation`s directly from the JSONL fold via
   `observations_from_run_snapshot` + wrapper observation, feed
   `reduce_agent_status`, apply. Gate behind the same conditions as today so
   output is identical. Prove equivalence with existing `live` / `runtime_refresh`
   / `agent_event` tests. *(characterization)*
2. **Delete the string round-trip on write.** Remove `project_legacy_value` /
   `write_agent_event` / `AgentEventSnapshot`; `__agent-event` writes JSONL only.
   Update `agent_event` tests to assert JSONL only.
3. **Delete the duplicate reducer.** Remove `live::select_status_observation` &
   friends, `StatusCandidate`, `AgentEvidenceSource`; `runtime_refresh` uses the
   step-1 path exclusively.
4. **Delete the legacy inputs.** Remove `agent_status_cache.rs`,
   `AgentStatusCache` trait/entry/source, tmux-agent-status + pane `.status`
   reads.
5. **Delete pane fallback.** Remove `pane_fallback.rs`, the capture-pane branch,
   `allows_pane_fallback`, `CapabilitySupport::PaneFallback`.
6. **Rename `RunPhase → AgentPhase`; drop `ProviderHook` tier; drop
   `project_snapshot`.**
7. **Re-order + extend the projector.** `ui_state`: lifecycle gate before
   substrate Error (req 10), Pending→Running (req 6), add `TaskStatus::Unknown`
   (precedence 6), wrapper-exit Unknown-only fallback. Add `Unknown` render arm
   in CLI/TUI/web.
8. **Fix installer + doc.** Add the Codex-schema guard/verification (§mis-install),
   update `architecture.md` to match §5/§6, update the architecture guard tests.

Steps 1–3 are the core simplification; 4–8 are deletion/cleanup that follow
safely once the single path is proven.

---

## 11. Required tests

**Unit — Stage 1 (fold → AgentPhase), mostly exists in `canonical_agent_event`:**
- two tools, one finishes → Running; attention + tool finish → Waiting;
  settled with open tool → Running, then finish → Done; settled Failed → Error;
  no events → Unknown; parent settled + active child → not Done.
- `observations_from_run_snapshot` feeds `reduce_agent_status` to
  AgentRunning / WaitingForApproval / Done / Failed (extend existing 2).

**Unit — Stage 2 (`ui_state`), new/updated:**
- Merged/Cleanable/Removing task with worktree/branch/tmux missing → **Idle**,
  not Error (requirement 10). *(new — this is the current bug)*
- Active task with worktree missing → Error (requirement 8).
- TeardownIncomplete → Error even when merged-ish (requirement 11).
- Pending CI → Running "CI running" (requirement 6). *(new)*
- Failed CI / conflict → Error; passing CI → reveals AgentPhase, not Done (6).
- AgentPhase Unknown + wrapper exit 0 → Done-presentation; exit ≠ 0 → Error (12).
- AgentPhase Unknown + no wrapper + healthy substrate + non-terminal lifecycle →
  **Unknown** (precedence 6). *(new)*
- Wrapper alive/Running but AgentPhase Unknown → **not** Running (12).

**Integration — `runtime_refresh`:**
- JSONL-only input (no legacy files present) drives live status end-to-end for
  each client (Claude working/wait; Codex working/ask/done; Cursor working/done/
  failed; Pi working/done).
- Cursor/Pi with no wait capability + no pane text → never Waiting (requirement 3).
- Run identity: two runs' JSONL for one task fold independently; parent not Done
  while child active (requirement 15).
- GitHub Pending/Failure/Healthy override interplay with AgentPhase.

**Regression / guard:**
- Grep-guard test (extend existing `architecture.rs` guards): no reference to
  `tmux-agent-status`, `pane_fallback`, `select_status_observation`,
  `AgentStatusCache`, `ProviderHook`, `project_legacy_value` remains in
  non-deleted code (requirement 13 enforced mechanically).
- `TaskCard.status` still computed once in core; web/TUI/CLI render the same
  value (requirement 16).
- Installer round-trips produce valid Claude/Cursor configs; Codex schema
  assertion (§mis-install).

Validation commands (AGENTS.md):
```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-core     # agent_status, canonical_agent_event, live, ui_state, runtime_refresh
cargo nextest run -p ajax-cli      # agent_event, agent_hooks
npm run verify                      # before PR (includes web:smoke per memory)
```

---

## 12. Non-goals (scope fence, restated for the implementer)

- No hook-health state machine; no diagnostic subsystem; no telemetry infra.
- No socket/polling redesign; `notify.sock` stays best-effort, JSONL fold stays
  the durable path.
- No new pane heuristics — pane classification is deleted, not rebuilt.
- No DB migration; reuse schema v9 and the single-writer sync (§9).
- No compatibility shim for any deleted legacy method.
- No new trait/framework/"status provider" abstraction — one translate fn, one
  reducer, one projector.
- No head-SHA CI subsystem (note the gap in §7, don't build it here).
- No `TaskStatus::Done` variant unless explicitly approved (§6 note).

---

## Unsupported / mis-installed hooks found during verification

1. ⚠️ **Codex hook schema is unverified and reuses the Claude shape.**
   `install_codex_hooks` writes `~/.codex/hooks.json` with
   `merge_hook_entries` — the Claude `settings.json` structure
   (`hooks.<Event>[].hooks[].command`). If the Codex CLI expects a different file
   path, top-level key, or event names, none of the Codex hooks fire and Codex
   status silently degrades to wrapper-exit-only, while the capability profile
   still claims `Native` for turn/settle/permission. **Action:** verify against a
   live Codex install; if the schema differs, fix the installer and/or downgrade
   the Codex profile to match reality. Do not ship a profile that claims Native
   for events that never arrive.

2. ⚠️ **Pi is turn-boundary only.** The extension registers exactly
   `before_agent_start` + `agent_settled`; no session, tool, or wait events. Pi
   status between start and settle rests entirely on wrapper liveness. This is
   consistent with the profile (`Wrapper`/`Unavailable`), so it is *honest*, but
   worth stating: Pi cannot report tool activity or any Waiting.

3. ⚠️ **Pi identity has no cwd-index fallback.** Unlike Cursor, the Pi extension
   calls `pi.exec("ajax-cli", …)` and depends on `AJAX_TASK_ID` /
   `AJAX_AGENT_EVENTS_DIR` being present in the extension's process env. If Pi
   runs extensions in an env that doesn't inherit the wrapper's, identity
   resolution fails and events are dropped silently. **Action:** confirm Pi
   passes wrapper env to extensions; if not, add a cwd-index fallback mirroring
   Cursor (out of scope for this pass — note only).

4. ℹ️ **Claude `SubagentStop` / `PreCompact` not installed.** Subagents are
   `Unverified` and compaction is unmodeled (`ActivityKind` has only `Tool`).
   Consistent with the profiles; not a defect, just a known coverage boundary.

5. ℹ️ **Legacy `~/.cache/tmux-agent-status` is read but never written by Ajax.**
   No current Ajax code writes session or pane `.status` files; the reader can
   only pick up files left by an older/external tool. This confirms the reads in
   `agent_status_cache.rs` are safe to delete — they consume a source that no
   longer exists in the system (requirement 13).

---

## Approval

**Status: architecture + boundary APPROVED 2026-07-23. Code implementation NOT
authorized yet** — user scoped this pass to the architecture boundary (§8b), not
the Rust changes.

Resolved:
- §6 note: **keep `Done` folded** into Ready-for-review/Idle — no
  `TaskStatus::Done` variant.
- The core↔cli boundary is defined in **§8b** (Option A preferred:
  `AgentStatusCache` yields `Vec<StatusObservation>`, deleting the string
  round-trip and the duplicate reducer).

### Implementation complete — 2026-07-23

All §10 steps implemented directly (not delegated — see decision below).
Validation (full workspace):
- `cargo fmt --all --check` — clean.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo nextest run --all-features` — **1685 passed, 0 failed**.

Per-crate: ajax-core 807, ajax-cli 361, ajax-web/tui/supervisor 517.

Delivered:
- Native hooks are the primary agent-status evidence; one client-event
  translation boundary (`translate_native_event`), one agent reducer
  (`reduce_agent_status`), one final projector (`derive_operator_status`).
- Boundary rewired to Option A: `AgentStatusSource` yields `Vec<StatusObservation>`
  (+ `ProcessLiveness`); the string round-trip and duplicate reducer are gone.
- `AgentPhase` (renamed `RunPhase`); `ObservationSource::ProviderHook` dropped.
- Projector: lifecycle gate before substrate `Error` (req 7/10), Pending CI →
  Running via `LiveStatusKind::CiPending` (req 6), `TaskStatus::Unknown` for the
  no-evidence case (precedence 6).
- Deleted: `pane_fallback.rs`, `~/.cache/tmux-agent-status` + pane `.status`
  reads, legacy scalar `{stem}.json` snapshot, `select_status_observation` /
  `StatusCandidate` / `AgentEvidenceSource` / `classify_agent_status_value`,
  `project_legacy_value`, `AgentStatusCache`/`Entry`/`Source`, capability
  pane-fallback support. No writer emits removed formats; user cache files are
  untouched.
- Docs: `architecture.md` status/live/projector/module sections rewritten.

Codex hook schema (finding 1): still UNVERIFIED against a live Codex install —
the installer reuses the Claude hook shape for `.codex/hooks.json`. Capability
profile unchanged; flagged for live smoke.

### Implementation verification (2026-07-23) — assumptions hold, no STOP

Verified against checked-out code before editing:
- `AgentStatusCache` trait carries `value: String`; `live.rs` re-parses via
  `classify_agent_status_value` (duplicate reducer confirmed).
- GitHub CI writes into the same `task.live_status` field;
  `apply_github_checks_observation` currently treats `Pending` like `Healthy`
  (clears — `runtime_refresh.rs:607`).
- Wrapper `AgentRuntimeSnapshot` exposes `state` (Starting/Running/
  ExitedSuccess/ExitedFailure) + `exit_code`.
- `derive_operator_status` runs substrate-missing Error *before* the
  terminal-lifecycle idle branch (the requirement-10 bug — confirmed).
- `TaskCard.status` computed once in `commands/projection.rs`; CLI `render.rs`,
  TUI, and web all consume it (requirement 10/16 already structural).

Resolved design points (within approved precedence, not a redesign):
1. **Boundary = Option A.** Source trait yields `Vec<StatusObservation>`.
2. **Wrapper exit stays fed as `ProcessExit` into the single reducer.** This
   already realizes "fallback only": `Starting/Running` → `ProcessLiveness`
   (never Running); `Exited*` → `ProcessExit` terminal, which can only exist
   once native has effectively ended. Equivalent to §6 rows 11–12. §6 rows 11–12
   wording is the loose one; the reducer semantics are authoritative.
3. **`LiveStatusKind::CiPending` (Running class) added** to realize §6 row 6
   (Pending → Running "CI running") in the shared `live_status` field. Enum
   variant only — no schema migration. GitHub-owned kinds (`CiFailed`,
   `CiPending`) are cleared by a Healthy probe, revealing native status.

Delegation decision: **not delegated.** This is one large, tightly-coupled
core-status refactor (cross-crate boundary rewire + duplicate-reducer deletion +
enum rename across crates + projector reorder) whose correctness hinges on the
wrapper-exit and Pending-CI judgments above; splitting into cold bounded packets
risks a literal §5 misread that violates requirement 6. Parent implements
directly under TDD; this is the reviewer/approver anyway.
