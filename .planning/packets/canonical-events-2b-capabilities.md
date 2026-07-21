# Packet: capability profiles (Phase 2b)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Add an explicit per-client `AgentCapabilityProfile` in ajax-core so Ajax can
declare which facts each client supplies. Cursor marks `permission_wait` and
`question_wait` as `Unavailable`. Provide helpers used later by pane fallback
and status projection: `allows_pane_fallback`, `has_native`. No pane
recognizer revive in this packet; no socket.

## Allowed files

- `crates/ajax-core/src/agent_capability.rs` (new)
- `crates/ajax-core/src/lib.rs` (mod + pub use)
- `crates/ajax-cli/src/agent_event.rs` (optional: attach `client` string →
  profile lookup helper only if needed for a test; prefer core-only)

## Forbidden changes

- Do not revive `live_recognize` / pane classification.
- Do not edit live.rs, runtime_refresh.rs, agent_status_cache.rs in this packet.
- No new dependencies. No commits.

## Context evidence

- Plan matrix: `.planning/agent-plans/canonical-agent-events.md` capability
  profiles + Cursor gaps.
- `AgentClient` at `crates/ajax-core/src/models.rs:23` (Claude|Codex|Other).
- Cursor/Pi launch as `Other` today (`adapters/agent.rs`).
- Hook client strings: `claude`|`codex`|`cursor`|`pi` in `agent_event.rs`.

## Code anchors

1. New `agent_capability.rs`:

```rust
pub enum CapabilitySupport {
    Native,
    Wrapper,
    PaneFallback,
    Unavailable,
    Unverified,
}

pub struct AgentCapabilityProfile {
    pub turn_started: CapabilitySupport,
    pub turn_settled: CapabilitySupport,
    pub permission_wait: CapabilitySupport,
    pub question_wait: CapabilitySupport,
    pub subagents: CapabilitySupport,
    pub session_closed: CapabilitySupport,
}

pub fn profile_for_agent_client(client: AgentClient) -> AgentCapabilityProfile
pub fn profile_for_hook_client(client: &str) -> AgentCapabilityProfile
```

2. Defaults (v1):
   - Claude: turn_* Native; permission_wait Native; question_wait Native;
     subagents Unverified; session_closed Native
   - Codex: turn_* Native; permission_wait Native; question_wait Unavailable
     (or Unverified if limited — use Unavailable for invent-wait ban);
     subagents Unverified; session_closed Native (SessionEnd) / Wrapper ok
   - Cursor (`Other` when named cursor, and hook client `"cursor"`): turn_*
     Native; permission_wait Unavailable; question_wait Unavailable;
     subagents Unverified; session_closed Native
   - Pi (hook `"pi"`, and Other default when unknown): turn_* Native;
     permission/question Unavailable; subagents Unverified; session_closed
     Wrapper
   - Unknown Other: all Unverified except turn_settled Wrapper

3. Helpers:
   - `profile.supports_native(fact) -> bool`
   - `profile.allows_pane_fallback(fact) -> bool` true only when support is
     `Unavailable` or `Unverified` (never when Native).

4. Export from `lib.rs`.

## Test-first instructions

Red: `cargo test -p ajax-core agent_capability -- --nocapture`

1. `cursor_profile_marks_wait_capabilities_unavailable`
2. `claude_profile_has_native_permission_and_question`
3. `pane_fallback_allowed_only_when_unavailable_or_unverified` — Cursor
   question → allows_pane_fallback true; Claude question → false

Implement until green.

## Edit instructions

New module + lib.rs export. Keep enums Copy where possible. Match Ajax naming.

## Verification commands

```bash
cargo test -p ajax-core agent_capability
cargo clippy -p ajax-core --all-targets -- -D warnings
cargo fmt -p ajax-core -- --check
```

## Acceptance criteria

- Profiles match matrix above; helpers tested.
- No pane/live/cache edits.

## Stop conditions

- Would require changing AgentClient variants (stop — use hook client string
  + Other mapping instead).
- Patch > ~250 lines.
