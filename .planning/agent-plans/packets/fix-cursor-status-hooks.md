# Packet: first-class Cursor/Pi status + live hook gate

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Stop Ajax from treating Cursor (and Pi) as anonymous `Other` for status, and
stop ambient Cursor IDE hooks (stale `AJAX_TASK_ID`) from writing events after
the Ajax-launched agent runtime has exited.

Concrete outcomes:

1. `agent_from_name("cursor"|"pi")` → `AgentClient::Cursor` / `AgentClient::Pi`
2. `profile_for_agent_client(Cursor|Pi)` uses existing `cursor_profile` /
   `pi_profile` (not `unknown_other_profile`)
3. Cursor/Pi do **not** ignore provider hooks the way `Other` does
4. Cursor/Pi pane fallback does **not** use Claude/Codex prompt recognizers
   (return no wait hint until Cursor/Pi-specific chrome exists)
5. `__agent-event` no-ops when the sibling `agent-runtime` snapshot is missing
   or not live (`Starting`/`Running`), except a short post-exit window for
   settle/session-close only

## Allowed files

- `crates/ajax-core/src/models.rs` (`AgentClient` enum)
- `crates/ajax-core/src/commands/new_task.rs` (`agent_from_name` + tests)
- `crates/ajax-core/src/agent_capability.rs`
- `crates/ajax-core/src/live.rs` (hook eligibility for Cursor/Pi)
- `crates/ajax-core/src/pane_fallback.rs`
- `crates/ajax-core/src/adapters/agent.rs`
- `crates/ajax-core/src/adapters.rs` (cursor launch test expectation)
- `crates/ajax-core/src/agent_prompt.rs` (exhaustive match only)
- `crates/ajax-core/src/registry/sqlite.rs` (`string_codec!` AgentClient list + parse tests)
- `crates/ajax-cli/src/execution_dispatch.rs` (`supervisor_agent_for_task`)
- `crates/ajax-cli/src/agent_event.rs` (live-runtime gate + tests)
- `crates/ajax-cli/src/agent_runtime.rs` (small helper to read snapshot / accept hooks)
- `architecture.md` (only the sentence(s) that still say Cursor/Pi register as `Other`)
- Any other Rust file that fails to compile **solely** due to a newly
  non-exhaustive `AgentClient` match: add arms that preserve prior `Other`
  behavior for Cursor/Pi unless this packet specifies different behavior above.
  Do not drive-by refactor those files.

## Forbidden changes

- Do not migrate existing SQLite rows from `Other` to `Cursor`/`Pi`.
- Do not invent Cursor/Pi pane chrome recognizers.
- Do not change Claude/Codex profiles, hook translation tables, or Codex
  supervise defaults beyond Cursor/Pi match arms.
- No commits, pushes, branch switches, or web UI edits.
- Do not delete `AgentClient::Other`.

## Context evidence

| Category | Finding | Anchor |
|---|---|---|
| Desired | Cursor/Pi collapsed to Other → wrong profile, hooks ignored, Codex/Claude pane waits | `new_task.rs` `agent_from_name`; `live.rs` `hook_observation_if_eligible` `agent == Other → None`; `pane_fallback.rs` `Other => (claude, codex)` |
| Desired | Cursor capability profile already exists for hook client strings | `agent_capability.rs` `cursor_profile` / `pi_profile` / `profile_for_hook_client` |
| Desired | Ambient IDE env keeps writing Cursor events after agent exit | Shell had `AJAX_TASK_ID=ajax-cli/history` while runtime snapshot `exited_success`; `history.json` value `working` |
| Identity | Runtime snapshot lives beside events dir | `agent_runtime.rs`: `events_dir = state_root.parent()/agent-events`; snapshot `{stem}.json` under `agent-runtime` |
| Identity | Events require env identity today | `agent_event.rs` `read_agent_event_identity` (`AJAX_TASK_ID`, `AJAX_AGENT_EVENTS_DIR`) |
| Architecture | Cursor/Pi as Other lose ProviderHook; Cursor has no native wait/ask | `architecture.md` status precedence; `.planning/agent-plans/canonical-agent-events.md` |
| Pattern | SQLite string codec lists enum variants | `registry/sqlite.rs` `string_codec!(..., [Claude, Codex, Other,])` |
| Pattern | Supervisor maps Other→Cursor today | `execution_dispatch.rs` `supervisor_agent_for_task` |

## Code anchors

1. `AgentClient` in `crates/ajax-core/src/models.rs` (~line 23): add `Cursor`, `Pi`.
2. `agent_from_name` in `new_task.rs` (~551): map `"cursor"` / `"pi"` (case-fold like existing).
3. `profile_for_agent_client` in `agent_capability.rs` (~65): Cursor→`cursor_profile()`, Pi→`pi_profile()`.
4. `hook_observation_if_eligible` / `hook_freshness_window` in `live.rs`: keep ignore/`None` **only** for `Other`; Cursor/Pi use same freshness path as Claude/Codex.
5. `recognize_prompt` in `pane_fallback.rs` (~89): `Cursor | Pi => return None` (no Claude/Codex recognizers).
6. `agent_launch_spec` args in `adapters/agent.rs`: `AgentClient::Cursor` (and cursor program) gets `["agent"]` like today’s `Other if program == "cursor"`.
7. `supervisor_agent_for_task`: `Cursor | Other => SupervisorAgent::Cursor`; `Pi` → `SupervisorAgent::Cursor` is acceptable (no Pi supervisor variant); Claude/Codex unchanged.
8. `run_agent_event` in `agent_event.rs`: after identity+translate, before append — if `!runtime_hooks_accepted(...)`, return Ok(()) without writing.
9. Helper in `agent_runtime.rs` (preferred): given `events_dir` + `task_id` + whether event is post-exit-settle (`TurnSettled` | `SessionClosed`), read `events_dir.parent()/agent-runtime/{stem}.json`:
   - missing / unreadable → reject
   - `starting` | `running` → accept
   - `exited_success` | `exited_failure` → accept **only** if settle/session-close **and** snapshot `observed_at_unix_millis` within **15_000** ms of `now_millis()`; else reject

## Test-first instructions

Red commands (expect failure before production edits):

```bash
cargo test -p ajax-core new_task_plan_cursor_agent -- --nocapture
cargo test -p ajax-core pane_fallback -- --nocapture
cargo test -p ajax-core agent_capability -- --nocapture
cargo test -p ajax-cli agent_event -- --nocapture
```

Add/adjust tests first:

1. **`new_task.rs`**: assert `agent_from_name` path — extend existing
   `new_task_plan_cursor_agent_command_uses_agent_subcommand` (or sibling) so
   created task `selected_agent == AgentClient::Cursor`. Add Pi analogue or
   unit assert on `agent_from_name` if private — prefer public plan/create
   assertion. Today cursor test expects `Other`; flip to `Cursor`.

2. **`agent_capability.rs`**: `profile_for_agent_client(AgentClient::Cursor)`
   matches `profile_for_hook_client("cursor")` on wait capabilities
   (Unavailable). Same for Pi.

3. **`pane_fallback.rs`**: Codex/Claude permission or idle chrome that
   currently matches `Other` must return `None` for `AgentClient::Cursor`
   and `AgentClient::Pi` from `maybe_pane_wait` / `recognize_wait_hint`.
   Keep existing `Other` behavior tests.

4. **`live.rs`** (optional focused): Cursor agent accepts a fresh Hook
   candidate (`working`) where `Other` still ignores — only if easy via
   existing `decide` helpers; else capability+pane tests suffice with
   comment.

5. **`agent_event.rs`**: 
   - With temp `agent-events` + sibling `agent-runtime` snapshot `running`,
     `run_agent_event` appends jsonl for `cursor`/`beforeSubmitPrompt`.
   - With snapshot `exited_success` older than 15s (or observed_at far past),
     `preToolUse` does **not** append.
   - With fresh `exited_success` (now), `stop` **does** append turn_settled.
   - Missing runtime snapshot → no append.

## Edit instructions

1. Tests first until red for the right reason.
2. Add enum variants + sqlite codec list + `agent_from_name`.
3. Wire capability, live hook eligibility, pane_fallback, adapters,
   supervisor mapping.
4. Implement `runtime_hooks_accepted` (name flexible) and gate
   `run_agent_event`.
5. Fix exhaustive matches minimally.
6. Update `architecture.md` only where it claims Cursor/Pi register as
   `Other` for ProviderHook loss — state they are first-class clients;
   `Other` remains for unknown agents and still ignores hooks.

## Verification commands

```bash
cargo test -p ajax-core commands::new_task -- --nocapture
cargo test -p ajax-core pane_fallback -- --nocapture
cargo test -p ajax-core agent_capability -- --nocapture
cargo test -p ajax-core live -- --nocapture
cargo test -p ajax-cli agent_event -- --nocapture
cargo check -p ajax-core -p ajax-cli --all-targets
cargo clippy -p ajax-core -p ajax-cli --all-targets -- -D warnings
```

## Acceptance criteria

- New Cursor/Pi tasks store `selected_agent` as `Cursor`/`Pi`, not `Other`.
- Cursor/Pi status profiles match hook-client profiles (waits Unavailable).
- Cursor/Pi never emit pane wait from Claude/Codex chrome.
- Hook writes require live (or briefly-exited settle) agent-runtime snapshot.
- `Other` still ignores hooks; Claude/Codex unchanged.
- Focused tests green; crates check clean.

## Stop conditions

- Need schema migration beyond TEXT enum string values.
- Cursor IDE hooks cannot be gated without breaking in-session Ajax
  `cursor agent` hooks during `Running`.
- Exhaustive match cleanup expands into behavior changes outside this packet.
- Validation failures unrelated to AgentClient/hooks that cannot be isolated.
